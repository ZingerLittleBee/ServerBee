use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tracing::info!("ServerBee starting...");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9527").await?;
    tracing::info!("Listening on {}", listener.local_addr()?);

    let app = axum::Router::new().route("/healthz", axum::routing::get(|| async { "ok" }));

    axum::serve(listener, app).await?;

    Ok(())
}
