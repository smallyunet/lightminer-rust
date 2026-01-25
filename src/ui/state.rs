//! Application state shared between manager and UI

use std::collections::VecDeque;

/// Maximum number of log entries to keep
const MAX_LOGS: usize = 100;

/// Shared application state for the TUI
#[derive(Debug, Clone)]
pub struct AppState {
    pub pool_address: String,
    pub connected: bool,
    pub authorized: Option<bool>,
    pub proxy: Option<String>,
    pub difficulty: f64,
    pub hashrate: f64,
    pub total_hashes: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub current_job: Option<String>,
    pub uptime_secs: u64,
    pub miner_threads: usize,
    pub logs: VecDeque<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            pool_address: String::new(),
            connected: false,
            authorized: None,
            proxy: None,
            difficulty: 1.0,
            hashrate: 0.0,
            total_hashes: 0,
            accepted: 0,
            rejected: 0,
            current_job: None,
            uptime_secs: 0,
            miner_threads: 1,
            logs: VecDeque::with_capacity(MAX_LOGS),
        }
    }
}

impl AppState {
    pub fn new(pool_address: &str, miner_threads: usize) -> Self {
        Self {
            pool_address: pool_address.to_string(),
            miner_threads: miner_threads.max(1),
            ..Default::default()
        }
    }

    pub fn add_log(&mut self, message: String) {
        if self.logs.len() >= MAX_LOGS {
            self.logs.pop_front();
        }
        self.logs.push_back(message);
    }

    pub fn hashrate_formatted(&self) -> String {
        let hr = self.hashrate;
        if hr >= 1_000_000_000.0 {
            format!("{:.2} GH/s", hr / 1_000_000_000.0)
        } else if hr >= 1_000_000.0 {
            format!("{:.2} MH/s", hr / 1_000_000.0)
        } else if hr >= 1_000.0 {
            format!("{:.2} KH/s", hr / 1_000.0)
        } else {
            format!("{:.2} H/s", hr)
        }
    }

    pub fn update_from_metrics(&mut self, metrics: &crate::manager::Metrics) {
        self.hashrate = metrics.hashrate();
        self.total_hashes = metrics.hashes();
        self.accepted = metrics.accepted();
        self.rejected = metrics.rejected();
        self.uptime_secs = metrics.uptime_secs();
    }
}
