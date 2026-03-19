use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::net::TcpStream;

use super::CheckResult;

/// Check TCP connectivity to `target` (expected format: "host:port").
///
/// Config options:
/// - `timeout`: connection timeout in seconds (default 10)
pub async fn check(target: &str, config: &Value) -> CheckResult {
    let timeout_secs = config
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(10);
    let timeout = Duration::from_secs(timeout_secs);

    let start = Instant::now();

    match tokio::time::timeout(timeout, TcpStream::connect(target)).await {
        Ok(Ok(_stream)) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            CheckResult {
                success: true,
                latency: Some(latency),
                detail: json!({ "connected": true }),
                error: None,
            }
        }
        Ok(Err(e)) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            CheckResult {
                success: false,
                latency: Some(latency),
                detail: json!({ "connected": false }),
                error: Some(format!("TCP connection failed: {e}")),
            }
        }
        Err(_) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            CheckResult {
                success: false,
                latency: Some(latency),
                detail: json!({ "connected": false }),
                error: Some(format!("TCP connection timed out after {timeout_secs}s")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_tcp_connect_success() {
        // Bind a listener on a random port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let config = json!({ "timeout": 5 });
        let result = check(&addr.to_string(), &config).await;

        assert!(result.success);
        assert!(result.latency.is_some());
        assert!(result.error.is_none());
        assert_eq!(result.detail["connected"], true);
    }

    #[tokio::test]
    async fn test_tcp_connect_refused() {
        // Use a port that is almost certainly not listening
        let config = json!({ "timeout": 2 });
        let result = check("127.0.0.1:1", &config).await;

        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(result.detail["connected"], false);
    }

    #[tokio::test]
    async fn test_tcp_default_timeout() {
        let config = json!({});
        // Just verify it doesn't panic with default config
        let result = check("127.0.0.1:1", &config).await;
        assert!(!result.success);
    }
}
