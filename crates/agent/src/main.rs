#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    tracing::info!("ServerBee Agent starting...");
    Ok(())
}
