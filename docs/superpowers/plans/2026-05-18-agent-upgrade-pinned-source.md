# Agent 自升级 Pinned-Source 安全加固 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Agent 自升级的下载来源从不可信的 Server 收回到宿主机配置链,使被攻陷/MITM 的 Server 无法注入恶意二进制。

**Architecture:** Server 的 `Upgrade` 消息只提供 `version`;Agent 用编译期默认 + 配置文件/env/CLI 决定的 `release_repo_url` 自行推导下载与 checksums URL,严格防降级,专用 HTTP client 用 webpki 根证书库 + 可选 SPKI pin。废弃 Server 的 `download_url`/`sha256`(发空串),不做向后兼容(存量旧 Agent 人工重装一次)。

**Tech Stack:** Rust(reqwest 0.12 + rustls 0.23 + tokio)、新增 `semver`/`x509-parser`/`webpki-roots`/`rustls`,React(前端 label)。

**Spec:** `docs/superpowers/specs/2026-05-18-agent-upgrade-pinned-source-design.md`

---

## File Structure

- `crates/agent/src/upgrade.rs` — **新建**。纯逻辑 + TLS client:asset 名映射、URL 推导、checksums 解析、防降级比较、SPKI hex 规范化、SPKI 提取、`SpkiPinVerifier`、`build_upgrade_client`、重定向决策。集中放便于单测与隔离。
- `crates/agent/src/config.rs` — 新增 `UpgradeConfig` + `Default` + SPKI hex 校验。
- `crates/agent/src/main.rs` — 接入 `--release-repo` 覆盖;启动时配置文件权限 warn(Unix)。
- `crates/agent/src/reporter.rs` — `perform_upgrade` 重写为调用 `upgrade.rs`;`Upgrade` arm 透传 upgrade 配置、忽略 `download_url`/`sha256`。
- `crates/agent/src/lib.rs` 或 `main.rs` mod 声明 — 注册 `mod upgrade;`。
- `crates/common/src/protocol.rs` — `Upgrade.download_url`/`sha256` 加 `#[serde(default)]` + 废弃 doc。
- `crates/server/src/router/api/server.rs` — `trigger_upgrade` 简化为只发 `version`。
- `crates/server/src/service/upgrade_release.rs` — 删 `resolve_asset` / `ReleaseAsset`。
- `apps/web/src/components/server/agent-version-section.tsx` — latest 文案改 advisory。
- `ENV.md`、`apps/docs/content/docs/{en,cn}/configuration.mdx`、`CHANGELOG.md` — 文档。

---

## Task 1: 锁定并新增依赖

**Files:**
- Modify: `crates/agent/Cargo.toml`

- [ ] **Step 1: 加依赖**

在 `crates/agent/Cargo.toml` 的 `[dependencies]` 段加入(紧邻现有 `hex = "0.4"` 之后):

```toml
semver = "1"
x509-parser = "0.16"
rustls = "0.23"
webpki-roots = "0.26"
```

- [ ] **Step 2: 验证版本与 reqwest 的 rustls 对齐**

Run: `cargo tree -p serverbee-agent -i rustls 2>/dev/null | head`
Expected: 解析出的 `rustls` 版本为 `0.23.x`(与 reqwest 0.12 一致,无双版本)。若出现两个 rustls 版本,调整本 crate 的 `rustls` 版本号到与 reqwest 传递依赖一致后重试。

- [ ] **Step 3: 编译**

Run: `cargo build -p serverbee-agent`
Expected: 编译通过(仅引入依赖,无代码改动)。

- [ ] **Step 4: Commit**

```bash
git add crates/agent/Cargo.toml Cargo.lock
git commit -m "build(agent): add deps for pinned-source upgrade (semver, x509-parser, rustls, webpki-roots)"
```

---

## Task 2: 新建 upgrade 模块 + asset 名映射

**Files:**
- Create: `crates/agent/src/upgrade.rs`
- Modify: `crates/agent/src/main.rs`(加 `mod upgrade;`)

- [ ] **Step 1: 写失败测试**

创建 `crates/agent/src/upgrade.rs`,内容:

```rust
//! Pinned-source 自升级:来源推导、防降级、TLS 加固。

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_is_known_non_empty() {
        let name = current_asset_name();
        assert!(name.starts_with("serverbee-agent-"));
        assert_ne!(name, "serverbee-agent-unsupported");
    }
}
```

在 `crates/agent/src/main.rs` 顶部模块声明区(`mod config;` 附近)加一行:

```rust
mod upgrade;
```

- [ ] **Step 2: 运行测试,确认失败/通过状态**

Run: `cargo test -p serverbee-agent upgrade::tests::asset_name_is_known_non_empty -- --nocolor`
Expected: 在当前开发机(macOS arm64 或 Linux)上 PASS。若 CI 跑在不支持平台会命中 `unsupported` 分支 FAIL —— 这是有意的,表示该平台不发布二进制。

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/upgrade.rs crates/agent/src/main.rs
git commit -m "feat(agent): add upgrade module with release asset name mapping"
```

---

## Task 3: URL 推导

**Files:**
- Modify: `crates/agent/src/upgrade.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/agent/src/upgrade.rs`(`#[cfg(test)] mod tests` 之前)追加:

```rust
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
```

在 `mod tests` 内追加:

```rust
    #[test]
    fn derive_urls_builds_github_layout() {
        let (bin, sums) = derive_urls(
            "https://github.com/ZingerLittleBee/ServerBee/releases/",
            "1.2.3",
        )
        .unwrap();
        assert!(bin.ends_with("/download/v1.2.3/serverbee-agent-"[..].trim_end_matches('-'))
            || bin.contains("/download/v1.2.3/serverbee-agent-"));
        assert_eq!(
            sums,
            "https://github.com/ZingerLittleBee/ServerBee/releases/download/v1.2.3/checksums.txt"
        );
    }

    #[test]
    fn derive_urls_rejects_non_https() {
        assert!(derive_urls("http://example.com/releases", "1.0.0").is_err());
    }
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent upgrade::tests::derive_urls -- --nocolor`
Expected: 两个测试 PASS。

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/upgrade.rs
git commit -m "feat(agent): derive pinned download/checksums URLs from release base"
```

---

## Task 4: 防降级(严格 semver 大于)

**Files:**
- Modify: `crates/agent/src/upgrade.rs`

- [ ] **Step 1: 写失败测试**

在 `upgrade.rs` 追加(tests 之前):

```rust
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
```

在 `mod tests` 追加:

```rust
    #[test]
    fn ensure_upgrade_strictly_greater() {
        assert!(ensure_upgrade("0.9.2", "0.9.3").is_ok());
        assert!(ensure_upgrade("0.9.2", "0.9.2").is_err()); // 等于拒绝
        assert!(ensure_upgrade("0.9.2", "0.9.1").is_err()); // 降级拒绝
        // 预发布版且 > current:按 §2.5 决策接受
        assert!(ensure_upgrade("0.9.2", "1.0.0-rc.1").is_ok());
        assert!(ensure_upgrade("0.9.2", "garbage").is_err());
    }
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent upgrade::tests::ensure_upgrade_strictly_greater -- --nocolor`
Expected: PASS。

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/upgrade.rs
git commit -m "feat(agent): strict semver anti-downgrade gate for upgrades"
```

---

## Task 5: checksums.txt 解析

**Files:**
- Modify: `crates/agent/src/upgrade.rs`

- [ ] **Step 1: 写失败测试**

在 `upgrade.rs` 追加:

```rust
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
```

在 `mod tests` 追加:

```rust
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
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent upgrade::tests::checksum_for_finds_asset -- --nocolor`
Expected: PASS。

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/upgrade.rs
git commit -m "feat(agent): parse checksums.txt for pinned asset hash"
```

---

## Task 6: SPKI hex 规范化 + 从证书 DER 提取 SPKI 哈希

**Files:**
- Modify: `crates/agent/src/upgrade.rs`

- [ ] **Step 1: 写失败测试**

在 `upgrade.rs` 追加:

```rust
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

#[cfg(test)]
const TEST_CERT_DER: &[u8] = include_bytes!("testdata/test_cert.der");
```

生成测试证书 fixture:

```bash
mkdir -p crates/agent/src/testdata
openssl req -x509 -newkey ed25519 -keyout /tmp/k.pem -nodes \
  -subj "/CN=test" -days 1 -outform der -out crates/agent/src/testdata/test_cert.der
```

在 `mod tests` 追加:

```rust
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
        // 同输入稳定
        assert_eq!(h, spki_sha256_hex(TEST_CERT_DER).unwrap());
    }
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent upgrade::tests::normalize_spki_pin_rules upgrade::tests::spki_hash_is_stable_64_hex -- --nocolor`
Expected: PASS。若 `x509-parser` 字段路径报错,用 `cargo doc -p x509-parser --open` 确认 `TbsCertificate::subject_pki` 与 `SubjectPublicKeyInfo::raw` 的当前名并修正(0.16 为此名)。

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/upgrade.rs crates/agent/src/testdata/test_cert.der
git commit -m "feat(agent): SPKI pin normalization and cert SPKI hashing"
```

---

## Task 7: 重定向决策(每跳强制 https)

**Files:**
- Modify: `crates/agent/src/upgrade.rs`

- [ ] **Step 1: 写失败测试**

在 `upgrade.rs` 追加:

```rust
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
```

在 `mod tests` 追加:

```rust
    #[test]
    fn redirect_decision_rules() {
        assert_eq!(redirect_decision("https", 0), RedirectAction::Follow);
        assert_eq!(redirect_decision("http", 0), RedirectAction::StopNonHttps);
        assert_eq!(redirect_decision("https", 10), RedirectAction::StopTooMany);
    }
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent upgrade::tests::redirect_decision_rules -- --nocolor`
Expected: PASS。

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/upgrade.rs
git commit -m "feat(agent): https-only redirect decision for upgrade downloads"
```

---

## Task 8: SPKI pin verifier + 专用 TLS client 构建

**Files:**
- Modify: `crates/agent/src/upgrade.rs`

- [ ] **Step 1: 写实现 + 构建测试**

在 `upgrade.rs` 顶部 `use` 区加:

```rust
use std::sync::Arc;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
```

追加:

```rust
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
        // 1) 标准链/主机名/时间校验(不可绕过)
        let verified =
            self.inner
                .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)?;
        // 2) 追加 SPKI pin
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
        .timeout(std::time::Duration::from_secs(super::reporter::UPGRADE_DOWNLOAD_TIMEOUT_SECS))
        .build()?;
    Ok(client)
}
```

> 注:`UPGRADE_DOWNLOAD_TIMEOUT_SECS` 当前在 `reporter.rs` 内私有。Step 2 处理可见性。

在 `mod tests` 追加:

```rust
    #[test]
    fn build_client_without_pin() {
        assert!(build_upgrade_client(None).is_ok());
    }

    #[test]
    fn build_client_with_pin() {
        let pin = "a".repeat(64);
        assert!(build_upgrade_client(Some(&pin)).is_ok());
    }
```

- [ ] **Step 2: 暴露 timeout 常量**

在 `crates/agent/src/reporter.rs` 找到 `const UPGRADE_DOWNLOAD_TIMEOUT_SECS`,改为 `pub(crate) const UPGRADE_DOWNLOAD_TIMEOUT_SECS`。在 `upgrade.rs` 中用 `crate::reporter::UPGRADE_DOWNLOAD_TIMEOUT_SECS` 替换上面的 `super::reporter::...`。

- [ ] **Step 3: 运行测试**

Run: `cargo test -p serverbee-agent upgrade::tests::build_client_with_pin upgrade::tests::build_client_without_pin -- --nocolor`
Expected: PASS。若 rustls 0.23 API 名(`WebPkiServerVerifier::builder`、`dangerous()`、`pki_types`)与代码不符,以 `cargo build` 报错为准对照 rustls 0.23 文档修正方法名,逻辑结构(先 inner 校验再 pin)保持不变。

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/upgrade.rs crates/agent/src/reporter.rs
git commit -m "feat(agent): dedicated upgrade TLS client with webpki roots and optional SPKI pin"
```

---

## Task 9: UpgradeConfig 配置结构

**Files:**
- Modify: `crates/agent/src/config.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/agent/src/config.rs` 的结构定义区追加:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpgradeConfig {
    #[serde(default = "default_release_repo")]
    pub release_repo_url: String,
    #[serde(default)]
    pub release_cert_spki_sha256: String,
}

fn default_release_repo() -> String {
    option_env!("SERVERBEE_RELEASE_REPO")
        .unwrap_or("https://github.com/ZingerLittleBee/ServerBee/releases")
        .to_string()
}

impl Default for UpgradeConfig {
    fn default() -> Self {
        Self {
            release_repo_url: default_release_repo(),
            release_cert_spki_sha256: String::new(),
        }
    }
}
```

在 `AgentConfig` 结构体字段区(`ip_change` 之后)加:

```rust
    #[serde(default)]
    pub upgrade: UpgradeConfig,
```

在 `config.rs` 的 `#[cfg(test)] mod tests` 追加:

```rust
    #[test]
    fn upgrade_config_defaults_to_official_releases() {
        let c = UpgradeConfig::default();
        assert_eq!(
            c.release_repo_url,
            "https://github.com/ZingerLittleBee/ServerBee/releases"
        );
        assert!(c.release_cert_spki_sha256.is_empty());
    }

    #[test]
    fn agent_config_has_upgrade_section_by_default() {
        let c: AgentConfig = figment::Figment::new()
            .merge(figment::providers::Serialized::defaults(
                serde_json::json!({ "server_url": "https://x" }),
            ))
            .extract()
            .unwrap();
        assert!(c.upgrade.release_repo_url.starts_with("https://"));
    }
```

> 若 `serde_json` / `figment::providers::Serialized` 未在 dev-deps,改用现有测试里已有的 TOML 字符串方式构造 `AgentConfig`(参考 `config.rs` 现有 `rebind` 相关测试模式)。

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent config::tests::upgrade_config_defaults_to_official_releases config::tests::agent_config_has_upgrade_section_by_default -- --nocolor`
Expected: PASS。

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/config.rs
git commit -m "feat(agent): add [upgrade] config section with pinned release defaults"
```

---

## Task 10: CLI `--release-repo` 覆盖

**Files:**
- Modify: `crates/agent/src/upgrade.rs`
- Modify: `crates/agent/src/main.rs`

- [ ] **Step 1: 写失败测试**

在 `upgrade.rs` 追加:

```rust
/// 从进程参数解析 `--release-repo <url>`(或 `--release-repo=<url>`),返回覆盖值。
pub fn parse_release_repo_arg<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--release-repo" {
            return it.next();
        }
        if let Some(v) = a.strip_prefix("--release-repo=") {
            return Some(v.to_string());
        }
    }
    None
}
```

在 `mod tests` 追加:

```rust
    #[test]
    fn parse_release_repo_arg_forms() {
        let v = vec![
            "bin".into(),
            "--release-repo".into(),
            "https://m.example/releases".into(),
        ];
        assert_eq!(
            parse_release_repo_arg(v),
            Some("https://m.example/releases".into())
        );
        let v2 = vec!["bin".into(), "--release-repo=https://x/releases".into()];
        assert_eq!(parse_release_repo_arg(v2), Some("https://x/releases".into()));
        assert_eq!(parse_release_repo_arg(vec!["bin".into()]), None);
    }
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent upgrade::tests::parse_release_repo_arg_forms -- --nocolor`
Expected: PASS。

- [ ] **Step 3: 接入 main.rs**

在 `crates/agent/src/main.rs` 的 `let capability_overrides = parse_capability_args(std::env::args())?;` 之后插入:

```rust
    if let Some(repo) = crate::upgrade::parse_release_repo_arg(std::env::args()) {
        tracing::info!("release_repo_url overridden by --release-repo CLI flag");
        config.upgrade.release_repo_url = repo;
    }
```

- [ ] **Step 4: 编译**

Run: `cargo build -p serverbee-agent`
Expected: 通过。

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/upgrade.rs crates/agent/src/main.rs
git commit -m "feat(agent): support --release-repo CLI override (highest precedence)"
```

---

## Task 11: 启动时配置文件权限 warn(Unix)

**Files:**
- Modify: `crates/agent/src/upgrade.rs`
- Modify: `crates/agent/src/main.rs`

- [ ] **Step 1: 写失败测试**

在 `upgrade.rs` 追加:

```rust
/// Unix 权限位 → 是否 group/world 可写。
pub fn is_group_or_world_writable(mode: u32) -> bool {
    mode & 0o022 != 0
}
```

在 `mod tests` 追加:

```rust
    #[test]
    fn perm_writable_detection() {
        assert!(is_group_or_world_writable(0o644)); // o+? no; 0o644 has g=r only -> 0o020? check
        // 0o644 = rw-r--r-- : group/other 无写位
        assert!(!is_group_or_world_writable(0o600));
        assert!(!is_group_or_world_writable(0o644));
        assert!(is_group_or_world_writable(0o664)); // group 可写
        assert!(is_group_or_world_writable(0o646)); // other 可写
    }
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-agent upgrade::tests::perm_writable_detection -- --nocolor`
Expected: PASS。

- [ ] **Step 3: 接入 main.rs(Unix only,非致命 warn)**

在 `crates/agent/src/main.rs` 的 `AgentConfig::load()` 成功之后、其它逻辑之前插入:

```rust
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in ["agent.toml", "/etc/serverbee/agent.toml"] {
            if let Ok(meta) = std::fs::metadata(path) {
                let mode = meta.permissions().mode();
                if crate::upgrade::is_group_or_world_writable(mode) {
                    tracing::warn!(
                        "SECURITY: {path} is group/world-writable (mode {:o}); \
                         another local user could tamper release_repo_url. \
                         Run: chmod 600 {path}",
                        mode & 0o777
                    );
                }
            }
        }
    }
```

- [ ] **Step 4: 编译**

Run: `cargo build -p serverbee-agent`
Expected: 通过。

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/upgrade.rs crates/agent/src/main.rs
git commit -m "feat(agent): warn when agent.toml is group/world-writable"
```

---

## Task 12: 重写 perform_upgrade 为 pinned-source 流程

**Files:**
- Modify: `crates/agent/src/reporter.rs`

- [ ] **Step 1: 改写 Upgrade arm 透传配置、忽略 url/sha256**

在 `crates/agent/src/reporter.rs` 的 `ServerMessage::Upgrade { version, download_url, sha256, job_id }` arm 中:

1. 解构改为忽略废弃字段:`ServerMessage::Upgrade { version, job_id, .. }`(去掉 `download_url`/`sha256` 绑定)。
2. 把日志 `"Upgrade requested: v{version} from {download_url}"` 改为 `"Upgrade requested: v{version} (pinned source)"`。
3. 在 `tokio::spawn` 之前捕获:`let upgrade_cfg = self.config.upgrade.clone();`
4. spawn 内调用改为:
   `if let Err(e) = perform_upgrade(&version, &upgrade_cfg, job_id, tx.clone()).await {`

- [ ] **Step 2: 重写 perform_upgrade 签名与函数体**

将 `async fn perform_upgrade(version, download_url, sha256, job_id, tx)` 整个函数替换为:

```rust
/// Pinned-source 升级:Server 仅提供 version;来源由本地 upgrade 配置决定。
async fn perform_upgrade(
    version: &str,
    upgrade_cfg: &crate::config::UpgradeConfig,
    job_id: Option<String>,
    tx: mpsc::Sender<AgentMessage>,
) -> anyhow::Result<()> {
    use crate::upgrade::{
        build_upgrade_client, checksum_for, current_asset_name, derive_urls, ensure_upgrade,
        normalize_spki_pin,
    };
    use sha2::{Digest, Sha256};
    use std::io::Write;

    macro_rules! fail {
        ($stage:expr, $msg:expr) => {{
            let msg: String = $msg;
            emit_upgrade_failure(&tx, job_id.clone(), version.to_string(), $stage, msg.clone(), None)
                .await;
            anyhow::bail!(msg);
        }};
    }

    emit_upgrade_progress(&tx, job_id.clone(), version, UpgradeStage::Downloading).await;

    // 1. 防降级
    let current = serverbee_common::constants::VERSION;
    if let Err(e) = ensure_upgrade(current, version) {
        fail!(UpgradeStage::Downloading, format!("anti-downgrade: {e}"));
    }

    // 2. SPKI pin 规范化(非法在此即报错;启动时已 warn,这里再防御一次)
    let spki = match normalize_spki_pin(&upgrade_cfg.release_cert_spki_sha256) {
        Ok(v) => v,
        Err(e) => fail!(UpgradeStage::Downloading, format!("invalid SPKI pin: {e}")),
    };

    // 3. 推导 URL(忽略 Server 的 download_url/sha256)
    let (binary_url, checksums_url) =
        match derive_urls(&upgrade_cfg.release_repo_url, version) {
            Ok(v) => v,
            Err(e) => fail!(UpgradeStage::Downloading, format!("derive url: {e}")),
        };

    // 4. 专用 client
    let client = match build_upgrade_client(spki.as_deref()) {
        Ok(c) => c,
        Err(e) => fail!(UpgradeStage::Downloading, format!("tls client: {e}")),
    };

    tracing::info!("Downloading agent v{version} from pinned source {binary_url}");

    // 5. 拉 checksums.txt
    let checksums = match client.get(&checksums_url).send().await {
        Ok(r) if r.status().is_success() => match r.text().await {
            Ok(t) => t,
            Err(e) => fail!(UpgradeStage::Downloading, format!("read checksums: {e}")),
        },
        Ok(r) => fail!(
            UpgradeStage::Downloading,
            format!("checksums HTTP {}", r.status())
        ),
        Err(e) => fail!(UpgradeStage::Downloading, format!("fetch checksums: {e}")),
    };
    let asset = current_asset_name();
    let want_hash = match checksum_for(&checksums, asset) {
        Ok(h) => h,
        Err(e) => fail!(UpgradeStage::Verifying, format!("{e}")),
    };

    // 6. 下载二进制
    let bytes = match client.get(&binary_url).send().await {
        Ok(r) if r.status().is_success() => match r.bytes().await {
            Ok(b) => b,
            Err(e) => fail!(UpgradeStage::Downloading, format!("read binary: {e}")),
        },
        Ok(r) => fail!(
            UpgradeStage::Downloading,
            format!("binary HTTP {}", r.status())
        ),
        Err(e) => fail!(UpgradeStage::Downloading, format!("fetch binary: {e}")),
    };

    emit_upgrade_progress(&tx, job_id.clone(), version, UpgradeStage::Verifying).await;

    // 7. 校验哈希(对照已从 pinned 源取得的 checksums)
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != want_hash {
        fail!(
            UpgradeStage::Verifying,
            format!("checksum mismatch: expected {want_hash}, got {actual}")
        );
    }
    tracing::info!("Checksum verified against pinned checksums.txt");

    // 8. 落盘 + 替换 + 重启(沿用原逻辑)
    let current_exe = std::env::current_exe()?;
    let tmp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("bak");
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))?;
    }
    emit_upgrade_progress(&tx, job_id.clone(), version, UpgradeStage::Installing).await;
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }
    std::fs::rename(&current_exe, &backup_path)?;
    std::fs::rename(&tmp_path, &current_exe)?;
    tracing::info!("Agent binary replaced. Restarting...");
    emit_upgrade_progress(&tx, job_id, version, UpgradeStage::Restarting).await;
    let args: Vec<String> = std::env::args().collect();
    let mut cmd = std::process::Command::new(&current_exe);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    cmd.spawn()?;
    std::process::exit(0);
}
```

- [ ] **Step 3: 编译 + 现有升级单测回归**

Run: `cargo build -p serverbee-agent && cargo test -p serverbee-agent reporter -- --nocolor`
Expected: 编译通过;`reporter` 相关既有测试通过(若有断言旧 `perform_upgrade` 签名/`download_url` 日志的测试,按新行为更新断言,不得保留对废弃字段的依赖)。

- [ ] **Step 4: clippy**

Run: `cargo clippy -p serverbee-agent -- -D warnings`
Expected: 0 warning。

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "feat(agent): pinned-source perform_upgrade, ignore server download_url/sha256"
```

---

## Task 13: 协议字段 `#[serde(default)]` + 废弃 doc

**Files:**
- Modify: `crates/common/src/protocol.rs`

- [ ] **Step 1: 改协议 + 写测试**

在 `crates/common/src/protocol.rs` 的 `ServerMessage::Upgrade` 变体改为:

```rust
    /// Agent 自升级。`download_url`/`sha256` 自 pinned-source 版本起**废弃**:
    /// 新 Agent 忽略,仅 `version` 有效(来源由 Agent 本地配置决定)。
    Upgrade {
        version: String,
        #[serde(default)]
        download_url: String,
        #[serde(default)]
        sha256: String,
        #[serde(default)]
        job_id: Option<String>,
    },
```

在 `protocol.rs` 测试区追加:

```rust
    #[test]
    fn test_upgrade_deserializes_without_deprecated_fields() {
        let json = r#"{"type":"upgrade","version":"1.0.0"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Upgrade {
                version,
                download_url,
                sha256,
                job_id,
            } => {
                assert_eq!(version, "1.0.0");
                assert_eq!(download_url, "");
                assert_eq!(sha256, "");
                assert_eq!(job_id, None);
            }
            _ => panic!("expected Upgrade"),
        }
    }
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p serverbee-common protocol -- --nocolor`
Expected: 新测试 PASS;既有 `test_upgrade_messages_without_job_id_stay_backward_compatible` 等仍 PASS(带字段也能解析)。

- [ ] **Step 3: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "refactor(common): deprecate Upgrade.download_url/sha256 with serde default"
```

---

## Task 14: 简化 Server 端 trigger_upgrade,移除 resolve_asset

**Files:**
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/service/upgrade_release.rs`

- [ ] **Step 1: 简化 trigger_upgrade**

在 `crates/server/src/router/api/server.rs` 的 `trigger_upgrade` 中:

1. 删除从 `get_agent_platform` / `map_os` / `map_arch` / `asset_name` / `state.upgrade_release_service.resolve_asset(...)` 这一整段(平台与 asset 解析不再由 Server 负责)。
2. 构造消息改为:

```rust
    let msg = ServerMessage::Upgrade {
        version: version.to_string(),
        download_url: String::new(),
        sha256: String::new(),
        job_id: Some(job.job_id.clone()),
    };
```

3. 若 `map_os`/`map_arch` 仅被此处使用,删除其定义(`cargo build` 报未使用即删)。保留 `version` 格式校验与 `upgrade_tracker` 逻辑不变。

- [ ] **Step 2: 移除 resolve_asset / ReleaseAsset**

在 `crates/server/src/service/upgrade_release.rs` 删除 `pub struct ReleaseAsset` 与 `pub async fn resolve_asset(...)` 及其专属测试。保留 `latest()` / `fetch_latest()` / `LatestAgentVersionResponse` / 缓存逻辑不动。

- [ ] **Step 3: 编译 + 测试**

Run: `cargo build -p serverbee-server && cargo test -p serverbee-server upgrade -- --nocolor`
Expected: 编译通过;删除 `resolve_asset` 后无悬挂引用;`latest`/`upgrade_tracker` 相关测试仍 PASS。

- [ ] **Step 4: clippy**

Run: `cargo clippy -p serverbee-server -- -D warnings`
Expected: 0 warning(含无未使用导入/函数)。

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/server.rs crates/server/src/service/upgrade_release.rs
git commit -m "refactor(server): upgrade sends version only, drop server-side asset resolution"
```

---

## Task 15: 前端 latest 文案改 advisory

**Files:**
- Modify: `apps/web/src/components/server/agent-version-section.tsx`
- Test: `apps/web/src/components/server/agent-version-section.test.tsx`

- [ ] **Step 1: 定位现有文案**

Run: `grep -n "latest\|Latest\|最新\|version" apps/web/src/components/server/agent-version-section.tsx | head`
Expected: 找到展示 "最新版本 / latest" 的文本节点。

- [ ] **Step 2: 写/改失败测试**

在 `agent-version-section.test.tsx` 增加一条断言:渲染时存在 advisory 提示文本(下载源以 Agent 本地配置为准)。示例:

```tsx
  it("shows advisory that download source follows agent config", () => {
    render(<AgentVersionSection currentVersion="1.0.0" latestVersion="1.3.0" />);
    expect(
      screen.getByText(/Agent 实际下载源以其本地配置为准|download source follows agent/i)
    ).toBeInTheDocument();
  });
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cd apps/web && bun run test agent-version-section -- --run`
Expected: 新测试 FAIL(文案尚未加)。

- [ ] **Step 4: 加 advisory 文案**

在该组件展示 latest 版本附近,加一行说明文本(i18n 若存在则走现有 i18n key 机制;否则直接中文+英文短句),措辞:`最新发布版本(来自 Server 配置源,Agent 实际下载源以其本地配置为准)`。

- [ ] **Step 5: 运行测试 + lint**

Run: `cd apps/web && bun run test agent-version-section -- --run && bun x ultracite check`
Expected: 测试 PASS;lint 0 error。

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/server/agent-version-section.tsx apps/web/src/components/server/agent-version-section.test.tsx
git commit -m "fix(web): clarify latest version is advisory vs agent pinned source"
```

---

## Task 16: 文档(ENV / configuration / CHANGELOG 破坏性变更)

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`
- Modify: `apps/docs/content/docs/cn/configuration.mdx`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: ENV.md**

新增 `SERVERBEE_UPGRADE__RELEASE_REPO_URL` 与 `SERVERBEE_UPGRADE__RELEASE_CERT_SPKI_SHA256` 两个变量条目:用途、默认值(官方 releases base / 空)、SPKI 须 64 位小写 hex 且非法则启动 warn(权限)/升级时 fail。

- [ ] **Step 2: configuration.mdx(en + cn 同步)**

在两个语言文件新增 `[upgrade]` 段:`release_repo_url`(默认官方,任意 HTTPS 镜像须同构 GitHub releases 目录布局 `{base}/download/v{version}/{asset}`)、`release_cert_spki_sha256`(可选,获取命令):

```
openssl x509 -in cert.pem -pubkey -noout \
  | openssl pkey -pubin -outform der \
  | openssl dgst -sha256 -r | awk '{print $1}'
```

并写明:建议 Server 与 Agent 的 release 源配同一仓库,UI "最新版本" 为 advisory。

- [ ] **Step 3: CHANGELOG 破坏性变更**

在未发布段加显著 **BREAKING** 条目:升级到本版本 Server 后,所有低于本版本的存量 Agent 自动升级将失败,必须**人工重装一次**(重跑安装脚本/部署新二进制)后才进入 pinned-source 自升级体系。

- [ ] **Step 4: typecheck(docs 站)**

Run: `bun run typecheck`
Expected: 通过(无 MDX/TS 报错)。

- [ ] **Step 5: Commit**

```bash
git add ENV.md apps/docs/content/docs/en/configuration.mdx apps/docs/content/docs/cn/configuration.mdx CHANGELOG.md
git commit -m "docs: document [upgrade] config and pinned-source breaking change"
```

---

## Task 17: 全量回归 + 手动 E2E 清单

**Files:**
- Create: `tests/agent-upgrade-pinned-source.md`

- [ ] **Step 1: 全量自动化回归**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings && cd apps/web && bun run test && bun run typecheck`
Expected: 全绿,0 clippy warning。

- [ ] **Step 2: 写手动 E2E 清单**

创建 `tests/agent-upgrade-pinned-source.md`,覆盖(参照 `tests/README.md` 既有格式):
1. 默认源:UI 触发升级,Agent 从 `github.com/.../releases/download/vX/...` 拉取并成功重启升版。
2. 自定义源:`agent.toml` 配 `[upgrade] release_repo_url` 指向镜像,升级走镜像。
3. CLI 覆盖:`--release-repo` 优先级高于配置文件。
4. 防降级:UI 触发一个 <= 当前版本,Agent 拒绝并上报失败。
5. SPKI pin:配置正确 pin 成功;错误 pin 升级失败;非法格式启动日志 warn。
6. 恶意 Server 模拟:手动构造 `Upgrade` 带伪 `download_url`/`sha256`,确认 Agent 忽略、仍走 pinned 源。
7. group/world-writable `agent.toml` 启动出现 SECURITY warn 但不中断。

- [ ] **Step 3: Commit**

```bash
git add tests/agent-upgrade-pinned-source.md
git commit -m "test: add manual E2E checklist for pinned-source upgrade"
```

---

## Self-Review 结果

- **Spec coverage**:§2 信任模型(T12 忽略 url/sha256、T14 Server 只发 version)、§2.4 TLS(T8 webpki+pin)、§2.5 残留(T4 接受预发布,清单 #6)、§3 配置(T9/T10/T11)、§3.3 URL 布局(T3)、§4 数据流(T12)、§5/§5.1 破坏性变更(T13/T14/T16 CHANGELOG)、§5.2 advisory(T15)、§6 测试与文档(各 Task TDD + T16/T17)、§8.1 依赖(T1 + T6 选定 x509-parser)。均有对应任务。
- **Placeholder scan**:无 TBD/TODO;每个改码步骤含完整代码;rustls/x509-parser 的 API 名漂移以"编译/测试验证步骤"兜底并给出对照指引(非占位,是验证)。
- **Type consistency**:`UpgradeConfig`(T9)→ `perform_upgrade(.., &crate::config::UpgradeConfig, ..)`(T12)一致;`build_upgrade_client(Option<&str>)`(T8)与 `spki.as_deref()`(T12)一致;`current_asset_name`/`derive_urls`/`checksum_for`/`ensure_upgrade`/`normalize_spki_pin`/`redirect_decision` 命名跨任务一致。
