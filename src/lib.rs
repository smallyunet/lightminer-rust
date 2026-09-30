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

pub mod config;
pub mod manager;
pub mod miner;
pub mod network;
pub mod presets;
pub mod protocol;
pub mod ui;
