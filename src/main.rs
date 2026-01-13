use anyhow::Result;
use std::env;
use tracing::info;

mod manager;
mod miner;
mod network;
mod protocol;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    // Check if TUI mode is disabled
    let use_tui = env::var("NO_TUI").is_err();

    if use_tui {
        // TUI mode - no logging to stdout
        run_with_tui().await
    } else {
        // Log mode - traditional logging
        tracing_subscriber::fmt::init();
        info!("LightMiner-Rust initialized (log mode).");
        info!("Starting Manager...");
        if let Err(e) = manager::run().await {
            tracing::error!("Manager error: {:?}", e);
        }
        Ok(())
    }
}

async fn run_with_tui() -> Result<()> {
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Initialize terminal
    let mut terminal = ui::init_terminal()?;

    // Create shared app state
    let pool_addr =
        env::var("MINING_POOL").unwrap_or_else(|_| "solo.ckpool.org:3333".to_string());
    let app_state = Arc::new(RwLock::new(ui::AppState::new(&pool_addr)));

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Clone for UI task
    let ui_state = Arc::clone(&app_state);

    // Run UI in separate task
    let ui_handle = tokio::spawn(async move {
        if let Err(e) = ui::run_ui(terminal, ui_state, shutdown_rx).await {
            eprintln!("UI error: {:?}", e);
        }
    });

    // For now, just wait for UI to finish
    // In a full implementation, we'd run the manager here and update app_state
    let _ = ui_handle.await;

    Ok(())
}

