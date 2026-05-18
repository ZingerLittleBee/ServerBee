//! Pinned-source 自升级:来源推导、防降级、TLS 加固。

use std::sync::Arc;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

/// 返回当前构建对应的 release asset 文件名(与 release.yml 命名一致)。
pub fn current_asset_name() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { "serverbee-agent-linux-amd64" }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { "serverbee-agent-linux-arm64" }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { "serverbee-agent-darwin-amd64" }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { "serverbee-agent-darwin-arm64" }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { "serverbee-agent-windows-amd64.exe" }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    { "serverbee-agent-unsupported" }
}

/// 由 release base + 版本号推导 (binary_url, checksums_url)。
/// base 必须 https://。version 不带前导 v。
pub fn derive_urls(base: &str, version: &str) -> anyhow::Result<(String, String)> {
    let base = base.trim_end_matches('/');
    if !base.starts_with("https://") {
        anyhow::bail!("release_repo_url must be https://, got: {base}");
    }
    let asset = current_asset_name();
    let binary = format!("{base}/download/v{version}/{asset}");
    let checksums = format!("{base}/download/v{version}/checksums.txt");
    Ok((binary, checksums))
}

/// 仅当 target 严格大于 current 时返回 Ok。预发布版按 §2.5 决策接受(semver 序)。
pub fn ensure_upgrade(current: &str, target: &str) -> anyhow::Result<()> {
    let cur = semver::Version::parse(current.trim_start_matches('v'))
        .map_err(|e| anyhow::anyhow!("invalid current version {current}: {e}"))?;
    let tgt = semver::Version::parse(target.trim_start_matches('v'))
        .map_err(|e| anyhow::anyhow!("invalid target version {target}: {e}"))?;
    if tgt > cur {
        Ok(())
    } else {
        anyhow::bail!("refusing non-upgrade: target {tgt} is not greater than current {cur}")
    }
}

/// 从 `sha256sum` 风格的 checksums.txt 文本中取指定 asset 的小写 hex 哈希。
pub fn checksum_for(checksums: &str, asset_name: &str) -> anyhow::Result<String> {
    for line in checksums.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        // sha256sum 二进制模式前缀 '*'
        let name = name.strip_prefix('*').unwrap_or(name);
        if name == asset_name {
            return Ok(hash.to_lowercase());
        }
    }
    anyhow::bail!("asset {asset_name} not found in checksums.txt")
}

/// 规范化并校验配置的 SPKI pin:去首尾空白、转小写,必须 64 位 hex。
/// 返回 None 表示空串(未启用);Err 表示非法(调用方应 fail-fast)。
pub fn normalize_spki_pin(raw: &str) -> anyhow::Result<Option<String>> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() {
        return Ok(None);
    }
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        anyhow::bail!(
            "release_cert_spki_sha256 must be 64 lowercase hex chars, got {} chars",
            s.len()
        );
    }
    Ok(Some(s))
}

/// 从 leaf 证书 DER 提取 SubjectPublicKeyInfo DER 并返回其 SHA-256 小写 hex。
pub fn spki_sha256_hex(cert_der: &[u8]) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let (_, cert) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|e| anyhow::anyhow!("parse cert: {e}"))?;
    let spki_der = cert.tbs_certificate.subject_pki.raw;
    Ok(hex::encode(Sha256::digest(spki_der)))
}

#[derive(Debug, PartialEq, Eq)]
pub enum RedirectAction {
    Follow,
    StopNonHttps,
    StopTooMany,
}

/// 纯决策:重定向目标 scheme + 已发生跳数 → 动作。上限 10 跳。
pub fn redirect_decision(next_scheme: &str, hops: usize) -> RedirectAction {
    if next_scheme != "https" {
        RedirectAction::StopNonHttps
    } else if hops >= 10 {
        RedirectAction::StopTooMany
    } else {
        RedirectAction::Follow
    }
}

/// 包裹标准 WebPkiServerVerifier:先完整链校验,成功后再比对 leaf SPKI SHA-256。
#[derive(Debug)]
struct SpkiPinVerifier {
    inner: Arc<WebPkiServerVerifier>,
    want: String, // 64 hex
}

impl ServerCertVerifier for SpkiPinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let verified =
            self.inner
                .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)?;
        let got = spki_sha256_hex(end_entity.as_ref())
            .map_err(|e| rustls::Error::General(format!("spki extract: {e}")))?;
        if got == self.want {
            Ok(verified)
        } else {
            Err(rustls::Error::General(
                "SPKI pin mismatch for release host".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(m, c, d)
    }

    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(m, c, d)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

fn webpki_root_store() -> rustls::RootCertStore {
    rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    }
}

/// 升级专用 reqwest client:webpki 根证书库 + 可选 SPKI pin + 每跳强制 https。
/// `spki_pin`: 已规范化的 64-hex(None=不启用 pin)。
pub fn build_upgrade_client(spki_pin: Option<&str>) -> anyhow::Result<reqwest::Client> {
    let roots = webpki_root_store();
    let tls = if let Some(pin) = spki_pin {
        let inner = WebPkiServerVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|e| anyhow::anyhow!("build webpki verifier: {e}"))?;
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SpkiPinVerifier {
                inner,
                want: pin.to_string(),
            }))
            .with_no_client_auth()
    } else {
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };

    let client = reqwest::Client::builder()
        .use_preconfigured_tls(tls)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            match redirect_decision(attempt.url().scheme(), attempt.previous().len()) {
                RedirectAction::Follow => attempt.follow(),
                RedirectAction::StopNonHttps => attempt.error("non-https redirect blocked"),
                RedirectAction::StopTooMany => attempt.error("too many redirects"),
            }
        }))
        .user_agent("ServerBee-Agent")
        .timeout(std::time::Duration::from_secs(crate::reporter::UPGRADE_DOWNLOAD_TIMEOUT_SECS))
        .build()?;
    Ok(client)
}

#[cfg(test)]
const TEST_CERT_DER: &[u8] = include_bytes!("testdata/test_cert.der");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_is_known_non_empty() {
        let name = current_asset_name();
        assert!(name.starts_with("serverbee-agent-"));
        assert_ne!(name, "serverbee-agent-unsupported");
    }

    #[test]
    fn derive_urls_builds_github_layout() {
        let (bin, sums) = derive_urls(
            "https://github.com/ZingerLittleBee/ServerBee/releases/",
            "1.2.3",
        )
        .unwrap();
        assert!(bin.contains("/download/v1.2.3/serverbee-agent-"));
        assert_eq!(
            sums,
            "https://github.com/ZingerLittleBee/ServerBee/releases/download/v1.2.3/checksums.txt"
        );
    }

    #[test]
    fn derive_urls_rejects_non_https() {
        assert!(derive_urls("http://example.com/releases", "1.0.0").is_err());
    }

    #[test]
    fn ensure_upgrade_strictly_greater() {
        assert!(ensure_upgrade("0.9.2", "0.9.3").is_ok());
        assert!(ensure_upgrade("0.9.2", "0.9.2").is_err()); // 等于拒绝
        assert!(ensure_upgrade("0.9.2", "0.9.1").is_err()); // 降级拒绝
        // 预发布版且 > current:按 §2.5 决策接受
        assert!(ensure_upgrade("0.9.2", "1.0.0-rc.1").is_ok());
        assert!(ensure_upgrade("0.9.2", "garbage").is_err());
    }

    #[test]
    fn checksum_for_finds_asset() {
        let body = "abc123  serverbee-server-linux-amd64\n\
                    DEADBEEF *serverbee-agent-linux-amd64\n";
        assert_eq!(
            checksum_for(body, "serverbee-agent-linux-amd64").unwrap(),
            "deadbeef"
        );
        assert!(checksum_for(body, "missing").is_err());
    }

    #[test]
    fn normalize_spki_pin_rules() {
        assert_eq!(normalize_spki_pin("  ").unwrap(), None);
        let ok = "a".repeat(64);
        assert_eq!(normalize_spki_pin(&ok.to_uppercase()).unwrap(), Some(ok));
        assert!(normalize_spki_pin("xyz").is_err());
        assert!(normalize_spki_pin(&"a".repeat(63)).is_err());
    }

    #[test]
    fn spki_hash_is_stable_64_hex() {
        let h = spki_sha256_hex(TEST_CERT_DER).unwrap();
        assert_eq!(h.len(), 64);
        assert!(h.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(h, spki_sha256_hex(TEST_CERT_DER).unwrap());
    }

    #[test]
    fn redirect_decision_rules() {
        assert_eq!(redirect_decision("https", 0), RedirectAction::Follow);
        assert_eq!(redirect_decision("http", 0), RedirectAction::StopNonHttps);
        assert_eq!(redirect_decision("https", 9), RedirectAction::Follow);
        assert_eq!(redirect_decision("https", 10), RedirectAction::StopTooMany);
    }

    #[test]
    fn build_client_without_pin() {
        assert!(build_upgrade_client(None).is_ok());
    }

    #[test]
    fn build_client_with_pin() {
        let pin = "a".repeat(64);
        assert!(build_upgrade_client(Some(&pin)).is_ok());
    }
}
