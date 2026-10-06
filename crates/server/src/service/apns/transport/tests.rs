use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    body::Bytes,
    http::{HeaderMap, StatusCode, Version},
    response::IntoResponse,
    routing::post,
};
use ring::signature::{ECDSA_P256_SHA256_FIXED, KeyPair, UnparsedPublicKey};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use super::*;
use crate::{service::apns::ApnsService, test_utils::setup_test_db};

fn signing_key() -> String {
    let der = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
        .unwrap();
    format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        STANDARD.encode(der.as_ref())
    )
}

fn config(private_key: &str, sandbox: bool) -> ApnsConfig<'_> {
    ApnsConfig {
        key_id: "ABC123DEFG",
        team_id: "TEAM123456",
        private_key,
        bundle_id: "app.serverbee",
        sandbox,
    }
}

fn verify_jwt(key: &EcdsaKeyPair, authorization: &str, expected_iat: u64) {
    let jwt = authorization.strip_prefix("bearer ").unwrap();
    let parts: Vec<_> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3);
    assert!(!jwt.contains('='));
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(header, json!({ "alg": "ES256", "kid": "ABC123DEFG" }));
    assert_eq!(claims, json!({ "iss": "TEAM123456", "iat": expected_iat }));
    let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    assert_eq!(signature.len(), 64);
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, key.public_key().as_ref())
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .unwrap();
}

#[tokio::test]
async fn jwt_is_verified_reused_and_refreshed_before_expiry() {
    let pem = signing_key();
    let transport = ApnsHttpTransport::new(&config(&pem, false)).unwrap();
    let issued_at = transport.token.lock().await.issued_at;
    let first = transport.authorization_at(issued_at).await.unwrap();
    verify_jwt(&transport.key, first.to_str().unwrap(), issued_at);
    assert!(first.is_sensitive());
    assert_eq!(
        transport
            .authorization_at(issued_at + JWT_LIFETIME_SECS - 1)
            .await
            .unwrap(),
        first
    );
    let refreshed = transport
        .authorization_at(issued_at + JWT_LIFETIME_SECS)
        .await
        .unwrap();
    assert_ne!(refreshed, first);
    verify_jwt(
        &transport.key,
        refreshed.to_str().unwrap(),
        issued_at + JWT_LIFETIME_SECS,
    );
    let backwards = transport.authorization_at(issued_at - 1).await.unwrap();
    verify_jwt(&transport.key, backwards.to_str().unwrap(), issued_at - 1);
}

#[test]
fn configured_environment_uses_the_correct_apple_host() {
    let pem = signing_key();
    for (sandbox, expected) in [
        (false, "api.push.apple.com"),
        (true, "api.sandbox.push.apple.com"),
    ] {
        let transport = ApnsHttpTransport::new(&config(&pem, sandbox)).unwrap();
        assert_eq!(transport.endpoint.host_str(), Some(expected));
        assert_eq!(transport.endpoint.scheme(), "https");
    }
}

struct TlsListener {
    tcp: TcpListener,
    acceptor: TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (tcp, addr) = self.tcp.accept().await.unwrap();
            if let Ok(tls) = self.acceptor.accept(tcp).await {
                assert_eq!(tls.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
                return (tls, addr);
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.tcp.local_addr()
    }
}

struct CapturedRequest {
    version: Version,
    headers: HeaderMap,
    body: Value,
    token: String,
}

struct AppleFixture {
    endpoint: Url,
    requests: mpsc::UnboundedReceiver<CapturedRequest>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for AppleFixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn apple_fixture(status: StatusCode, body: Value) -> AppleFixture {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(
                include_bytes!("testdata/server.der").to_vec(),
            )],
            PrivatePkcs8KeyDer::from(include_bytes!("testdata/server-key.der").to_vec()).into(),
        )
        .unwrap();
    tls.alpn_protocols = vec![b"h2".to_vec()];
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Url::parse(&format!(
        "https://127.0.0.1:{}",
        tcp.local_addr().unwrap().port()
    ))
    .unwrap();
    let (tx, requests) = mpsc::unbounded_channel();
    let app = Router::new().route(
        "/3/device/{token}",
        post(
            move |axum::extract::Path(token): axum::extract::Path<String>,
                  version: Version,
                  headers: HeaderMap,
                  bytes: Bytes| {
                let tx = tx.clone();
                let body = body.clone();
                async move {
                    let redirected = token == "redirect-target";
                    tx.send(CapturedRequest {
                        version,
                        headers,
                        token,
                        body: serde_json::from_slice(&bytes).unwrap(),
                    })
                    .unwrap();
                    if status == StatusCode::TEMPORARY_REDIRECT {
                        if redirected {
                            return (StatusCode::GONE, Json(json!({ "reason": "Unregistered" })))
                                .into_response();
                        }
                        return (
                            status,
                            [("location", "/3/device/redirect-target")],
                            Json(body),
                        )
                            .into_response();
                    }
                    (status, Json(body)).into_response()
                }
            },
        ),
    );
    let listener = TlsListener {
        tcp,
        acceptor: TlsAcceptor::from(Arc::new(tls)),
    };
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    AppleFixture {
        endpoint,
        requests,
        server,
    }
}

fn fixture_transport(pem: &str, fixture: &AppleFixture) -> ApnsHttpTransport {
    let ca = reqwest::Certificate::from_der(include_bytes!("testdata/ca.der")).unwrap();
    let mut transport = ApnsHttpTransport::with_client(
        &config(pem, false),
        ApnsHttpTransport::client_builder()
            .no_proxy()
            .add_root_certificate(ca),
    )
    .unwrap();
    transport.endpoint = fixture.endpoint.clone();
    transport
}

#[tokio::test]
async fn real_tls_http2_dispatch_preserves_alert_headers_payload_and_authentication() {
    let (db, _tmp) = setup_test_db().await;
    super::super::tests::seed_token(&db, "wire").await;
    let pem = signing_key();
    let mut fixture = apple_fixture(StatusCode::OK, Value::Null).await;
    let transport = fixture_transport(&pem, &fixture);
    ApnsService::send_push_with_transport(
        &db,
        &config(&pem, false),
        "Title \"quoted\"",
        "Unicode: 你好",
        Some("server-1"),
        Some("rule-1"),
        &transport,
    )
    .await
    .unwrap();
    let request = tokio::time::timeout(Duration::from_secs(5), fixture.requests.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(request.version, Version::HTTP_2);
    assert_eq!(request.token, "token-wire");
    assert_eq!(request.headers["apns-topic"], "app.serverbee");
    assert_eq!(request.headers["apns-priority"], "10");
    assert_eq!(request.headers["apns-push-type"], "alert");
    assert_eq!(request.headers["content-type"], "application/json");
    // Preserve the existing omission of expiration/collapse headers.
    assert!(!request.headers.contains_key("apns-expiration"));
    assert_eq!(
        request.body,
        json!({
            "aps": { "alert": { "title": "Title \"quoted\"", "body": "Unicode: 你好" },
                "sound": "default", "badge": 1, "mutable-content": 0 },
            "server_id": "server-1", "rule_id": "rule-1",
        })
    );
    verify_jwt(
        &transport.key,
        request.headers["authorization"].to_str().unwrap(),
        transport.token.lock().await.issued_at,
    );
}

#[tokio::test]
async fn only_terminal_unregistered_response_deletes_the_current_token() {
    use crate::entity::device_token;
    use sea_orm::EntityTrait;
    let pem = signing_key();
    for (status, reason, deleted) in [
        (StatusCode::GONE, "Unregistered", true),
        (StatusCode::BAD_REQUEST, "BadDeviceToken", false),
        (StatusCode::GONE, "OtherReason", false),
        (StatusCode::TOO_MANY_REQUESTS, "TooManyRequests", false),
        (StatusCode::SERVICE_UNAVAILABLE, "ServiceUnavailable", false),
    ] {
        let (db, _tmp) = setup_test_db().await;
        super::super::tests::seed_token(&db, "cleanup").await;
        let mut fixture =
            apple_fixture(status, json!({ "reason": reason, "timestamp": 123 })).await;
        let transport = fixture_transport(&pem, &fixture);
        ApnsService::send_push_with_transport(
            &db,
            &config(&pem, false),
            "T",
            "B",
            None,
            None,
            &transport,
        )
        .await
        .unwrap();
        let remaining = device_token::Entity::find().all(&db).await.unwrap();
        assert_eq!(remaining.is_empty(), deleted, "{status}: {reason}");
        let request = fixture.requests.recv().await.unwrap();
        assert!(request.body.get("server_id").is_none());
        assert!(request.body.get("rule_id").is_none());
    }
}

#[tokio::test]
async fn redirects_malformed_and_oversized_responses_do_not_delete_tokens() {
    use crate::entity::device_token;
    use sea_orm::EntityTrait;
    let pem = signing_key();
    for (status, body) in [
        (
            StatusCode::TEMPORARY_REDIRECT,
            json!({ "reason": "Unregistered" }),
        ),
        (StatusCode::GONE, json!({ "unexpected": "Unregistered" })),
        (
            StatusCode::GONE,
            json!({ "reason": "Unregistered", "padding": "x".repeat(MAX_ERROR_BYTES) }),
        ),
    ] {
        let (db, _tmp) = setup_test_db().await;
        super::super::tests::seed_token(&db, "retained").await;
        let mut fixture = apple_fixture(status, body).await;
        let transport = fixture_transport(&pem, &fixture);
        ApnsService::send_push_with_transport(
            &db,
            &config(&pem, false),
            "T",
            "B",
            None,
            None,
            &transport,
        )
        .await
        .unwrap();
        assert_eq!(
            device_token::Entity::find().all(&db).await.unwrap().len(),
            1
        );
        let request = fixture.requests.recv().await.unwrap();
        assert_eq!(request.token, "token-retained");
        assert!(
            fixture.requests.try_recv().is_err(),
            "redirect must not be followed"
        );
    }
}

#[tokio::test]
async fn untrusted_tls_and_cleartext_fail_without_disclosing_device_token() {
    let pem = signing_key();
    let fixture = apple_fixture(StatusCode::OK, Value::Null).await;
    for endpoint in [
        fixture.endpoint.clone(),
        Url::parse("http://127.0.0.1:1").unwrap(),
    ] {
        let mut transport = ApnsHttpTransport::with_client(
            &config(&pem, false),
            ApnsHttpTransport::client_builder().no_proxy(),
        )
        .unwrap();
        transport.endpoint = endpoint;
        let err = transport
            .send(LegacyApnsRequest {
                device_token: "sensitive-device-token",
                topic: "app.serverbee",
                payload: json!({ "aps": {} }),
            })
            .await
            .err()
            .unwrap();
        assert_eq!(err.to_string(), "APNs request failed");
    }
}

#[tokio::test]
async fn oversized_payload_is_rejected_before_entering_the_network() {
    let pem = signing_key();
    let mut fixture = apple_fixture(StatusCode::OK, Value::Null).await;
    let transport = fixture_transport(&pem, &fixture);
    let err = transport
        .send(LegacyApnsRequest {
            device_token: "token",
            topic: "app.serverbee",
            payload: json!({ "aps": { "alert": "x".repeat(MAX_PAYLOAD_BYTES) } }),
        })
        .await
        .err()
        .unwrap();
    assert!(err.to_string().contains("payload exceeds"));
    assert!(fixture.requests.try_recv().is_err());
}
