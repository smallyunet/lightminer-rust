use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub pool_addr: String,
    pub worker_name: String,
    pub worker_password: String,
    pub agent: String,
    pub use_tui: bool,
}

impl Config {
    pub fn from_env() -> Self {
        let pool_addr = env::var("MINING_POOL").unwrap_or_else(|_| "solo.ckpool.org:3333".to_string());
        let worker_name = env::var("MINING_USER").unwrap_or_else(|_| "lightminer.1".to_string());
        let worker_password = env::var("MINING_PASS").unwrap_or_else(|_| "x".to_string());
        let agent = env::var("MINING_AGENT").unwrap_or_else(|_| "LightMiner-Rust/0.0.2".to_string());
        let use_tui = env::var("NO_TUI").is_err();

        Self {
            pool_addr,
            worker_name,
            worker_password,
            agent,
            use_tui,
        }
    }
}
