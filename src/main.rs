//! # LightMiner-Rust
//!
//! A lightweight CPU miner written in Rust with Stratum V1 protocol support.
//!
//! ## Features
//!
//! - **Stratum V1 Protocol** - Full support for mining pool communication
//! - **SHA256d Mining** - Standard double-SHA256 hashing algorithm
//! - **Real-time Metrics** - Track hashrate and share statistics
//! - **Professional TUI** - Beautiful terminal interface with ratatui
//!
//! ## Modules
//!
//! - [`protocol`] - Stratum V1 protocol implementation (JSON-RPC 2.0)
//! - [`network`] - Async TCP client using tokio
//! - [`manager`] - Main orchestration and event loop
//! - [`miner`] - Mining logic with SHA256d and nonce search
//! - [`ui`] - Terminal user interface
//!
//! ## Usage
//!
//! ```bash
//! # TUI mode (default)
//! cargo run
//!
//! # Log mode
//! NO_TUI=1 cargo run
//!
//! # Custom pool
//! MINING_POOL="stratum.pool.com:3333" cargo run
//! ```

use anyhow::Result;
use std::env;
use tracing::info;

pub mod manager;
pub mod miner;
pub mod network;
pub mod protocol;
pub mod ui;

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
    let terminal = ui::init_terminal()?;

    // Create shared app state
    let pool_addr =
        env::var("MINING_POOL").unwrap_or_else(|_| "solo.ckpool.org:3333".to_string());
    let app_state = Arc::new(RwLock::new(ui::AppState::new(&pool_addr)));

    // Create shutdown channel
    let (_shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

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
