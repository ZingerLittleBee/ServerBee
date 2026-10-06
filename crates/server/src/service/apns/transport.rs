use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use reqwest::{Client, ClientBuilder, Url, header::HeaderValue};
use ring::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::ApnsConfig;
use crate::error::AppError;

const JWT_LIFETIME_SECS: u64 = 50 * 60;
const MAX_PAYLOAD_BYTES: usize = 4096;
const MAX_ERROR_BYTES: usize = 4096;

pub struct LegacyApnsRequest<'a> {
    pub device_token: &'a str,
    pub topic: &'a str,
    pub payload: Value,
}

pub struct LegacyApnsResponse {
    pub status: u16,
    pub reason: Option<String>,
}

/// Only the external Apple transport is replaceable. Recipient selection,
/// pre-send migration checks, payload construction and cleanup stay real.
#[async_trait::async_trait]
pub trait LegacyApnsTransport: Send + Sync {
    async fn send(&self, request: LegacyApnsRequest<'_>) -> Result<LegacyApnsResponse, AppError>;
}

struct CachedToken {
    authorization: HeaderValue,
    issued_at: u64,
}

pub(super) struct ApnsHttpTransport {
    client: Client,
    endpoint: Url,
    key: EcdsaKeyPair,
    key_id: String,
    team_id: String,
    token: Mutex<CachedToken>,
}

fn credential_error() -> AppError {
    AppError::Internal("Failed to create APNs client: invalid signing credentials".into())
}

fn unix_seconds() -> Result<u64, AppError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| AppError::Internal("APNs signing clock is before the Unix epoch".into()))
}

fn sign_token(
    key: &EcdsaKeyPair,
    key_id: &str,
    team_id: &str,
    now: u64,
) -> Result<CachedToken, AppError> {
    let header = URL_SAFE_NO_PAD.encode(json!({ "alg": "ES256", "kid": key_id }).to_string());
    let claims = URL_SAFE_NO_PAD.encode(json!({ "iss": team_id, "iat": now }).to_string());
    let input = format!("{header}.{claims}");
    // JWT ES256 uses the fixed-width r || s signature, not ASN.1 DER.
    let signature = key
        .sign(&SystemRandom::new(), input.as_bytes())
        .map_err(|_| credential_error())?;
    let mut authorization = HeaderValue::from_str(&format!(
        "bearer {input}.{}",
        URL_SAFE_NO_PAD.encode(signature.as_ref())
    ))
    .map_err(|_| credential_error())?;
    authorization.set_sensitive(true);
    Ok(CachedToken {
        authorization,
        issued_at: now,
    })
}

impl ApnsHttpTransport {
    fn client_builder() -> ClientBuilder {
        Client::builder()
            .use_rustls_tls()
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .https_only(true)
            .http2_prior_knowledge()
            .redirect(reqwest::redirect::Policy::none())
            // The legacy client connected directly and used a single 20-second
            // request deadline. Keep those deployment/network semantics.
            .no_proxy()
            .timeout(Duration::from_secs(20))
            .pool_idle_timeout(Duration::from_secs(600))
    }

    pub(super) fn new(config: &ApnsConfig<'_>) -> Result<Self, AppError> {
        Self::with_client(config, Self::client_builder())
    }

    fn with_client(config: &ApnsConfig<'_>, builder: ClientBuilder) -> Result<Self, AppError> {
        let pem = config.private_key.trim();
        let encoded = pem
            .strip_prefix("-----BEGIN PRIVATE KEY-----")
            .and_then(|pem| pem.strip_suffix("-----END PRIVATE KEY-----"))
            .ok_or_else(credential_error)?;
        let der = STANDARD
            .decode(encoded.split_whitespace().collect::<String>())
            .map_err(|_| credential_error())?;
        let key =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &SystemRandom::new())
                .map_err(|_| credential_error())?;
        let token = sign_token(&key, config.key_id, config.team_id, unix_seconds()?)?;
        let endpoint = Url::parse(if config.sandbox {
            "https://api.sandbox.push.apple.com"
        } else {
            "https://api.push.apple.com"
        })
        .map_err(|_| AppError::Internal("Invalid APNs endpoint".into()))?;
        let client = builder
            .build()
            .map_err(|_| AppError::Internal("Failed to create APNs client".into()))?;
        Ok(Self {
            client,
            endpoint,
            key,
            key_id: config.key_id.to_owned(),
            team_id: config.team_id.to_owned(),
            token: Mutex::new(token),
        })
    }

    async fn authorization_at(&self, now: u64) -> Result<HeaderValue, AppError> {
        let mut token = self.token.lock().await;
        // Reuse within each batch/connection and refresh before Apple's one-hour
        // expiry, including when a long dispatch crosses that boundary.
        if now < token.issued_at || now - token.issued_at >= JWT_LIFETIME_SECS {
            *token = sign_token(&self.key, &self.key_id, &self.team_id, now)?;
        }
        Ok(token.authorization.clone())
    }
}

#[derive(Deserialize)]
struct AppleError {
    reason: String,
}

#[async_trait::async_trait]
impl LegacyApnsTransport for ApnsHttpTransport {
    async fn send(&self, request: LegacyApnsRequest<'_>) -> Result<LegacyApnsResponse, AppError> {
        let body = serde_json::to_vec(&request.payload)
            .map_err(|_| AppError::BadGateway("Failed to encode APNs payload".into()))?;
        if body.len() > MAX_PAYLOAD_BYTES {
            return Err(AppError::BadGateway(
                "APNs payload exceeds 4096 bytes".into(),
            ));
        }
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .map_err(|_| AppError::Internal("Invalid APNs endpoint".into()))?
            .clear()
            .extend(["3", "device", request.device_token]);
        let mut response = self
            .client
            .post(url)
            .header(
                "authorization",
                self.authorization_at(unix_seconds()?).await?,
            )
            .header("apns-topic", request.topic)
            .header("apns-priority", "10")
            .header("apns-push-type", "alert")
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            // reqwest errors contain the URL/device token. Keep them out of logs.
            .map_err(|_| AppError::BadGateway("APNs request failed".into()))?;
        let status = response.status().as_u16();
        if status == 200 {
            return Ok(LegacyApnsResponse {
                status,
                reason: None,
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AppError::BadGateway("Failed to read APNs response".into()))?
        {
            if bytes.len() + chunk.len() > MAX_ERROR_BYTES {
                return Err(AppError::BadGateway(
                    "APNs error response exceeds 4096 bytes".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let reason = if bytes.is_empty() {
            None
        } else {
            Some(
                serde_json::from_slice::<AppleError>(&bytes)
                    .map_err(|_| {
                        AppError::BadGateway("APNs returned an invalid error response".into())
                    })?
                    .reason,
            )
        };
        Ok(LegacyApnsResponse { status, reason })
    }
}

#[cfg(test)]
mod tests;
