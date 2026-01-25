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
use tracing::info;

pub mod config;
pub mod manager;
pub mod miner;
pub mod network;
pub mod protocol;
pub mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::from_env();

    if config.use_tui {
        // TUI mode - no logging to stdout
        run_with_tui(config).await
    } else {
        // Log mode - traditional logging
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
        info!("LightMiner-Rust initialized (log mode).");
        info!("Starting Manager...");

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            let ctrl_c_result = tokio::signal::ctrl_c().await;
            if ctrl_c_result.is_ok() {
                let _ = shutdown_tx.send(true);
            }
        });

        let metrics = std::sync::Arc::new(manager::Metrics::new());
        if let Err(e) = manager::run_with_config(config, metrics, None, None, shutdown_rx).await {
            tracing::error!("Manager error: {:?}", e);
        }
        Ok(())
    }
}

async fn run_with_tui(config: config::Config) -> Result<()> {
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Initialize terminal
    let terminal = ui::init_terminal()?;

    // Create shared app state
    let app_state = Arc::new(RwLock::new(ui::AppState::new(
        &config.pool_addr,
        config.miner_threads,
    )));
    {
        let mut state = app_state.write().await;
        state.add_log("Starting LightMiner-Rust...".to_string());
    }

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn({
        let shutdown_tx = shutdown_tx.clone();
        async move {
            let ctrl_c_result = tokio::signal::ctrl_c().await;
            if ctrl_c_result.is_ok() {
                let _ = shutdown_tx.send(true);
            }
        }
    });

    let metrics = Arc::new(manager::Metrics::new());
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<manager::ManagerEvent>(256);
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<manager::ManagerCommand>(32);

    let state_task = tokio::spawn(ui::run_state_updater(
        Arc::clone(&app_state),
        Arc::clone(&metrics),
        event_rx,
        shutdown_rx.clone(),
    ));

    let mut manager_task = tokio::spawn(manager::run_with_config(
        config,
        Arc::clone(&metrics),
        Some(event_tx),
        Some(cmd_rx),
        shutdown_rx.clone(),
    ));

    let mut ui_task = tokio::spawn(ui::run_ui(
        terminal,
        Arc::clone(&app_state),
        Some(cmd_tx),
        shutdown_rx,
    ));

    // IMPORTANT: `JoinHandle` panics if it is polled after completion.
    // We track which task finished in `select!` so we don't `.await` it again.
    let mut ui_finished = false;
    let mut manager_finished = false;

    tokio::select! {
        _ = &mut ui_task => {
            let _ = shutdown_tx.send(true);
            ui_finished = true;
        }
        _ = &mut manager_task => {
            let _ = shutdown_tx.send(true);
            manager_finished = true;
        }
    }

    if !manager_finished {
        let _ = manager_task.await;
    }
    let _ = state_task.await;
    if !ui_finished {
        let _ = ui_task.await;
    }

    Ok(())
}
