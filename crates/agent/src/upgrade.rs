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
}
