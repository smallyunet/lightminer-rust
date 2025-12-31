use anyhow::Result;
use tracing::info;

mod manager;
mod miner;
mod network;
mod protocol;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the tracing subscriber for logging
    tracing_subscriber::fmt::init();

    info!("LightMiner-Rust initialized.");
    info!("Starting Manager...");
    if let Err(e) = manager::run().await {
        tracing::error!("Manager error: {:?}", e);
    }

    Ok(())
}
