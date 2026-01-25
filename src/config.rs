use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub pool_addr: String,
    pub worker_name: String,
    pub worker_password: String,
    pub agent: String,
    pub use_tui: bool,

    /// Automatically reconnect when the pool disconnects.
    pub reconnect: bool,

    /// Maximum backoff delay between reconnect attempts.
    pub reconnect_max_delay_ms: u64,

    /// Number of parallel CPU mining threads.
    pub miner_threads: usize,

    /// Maximum time to wait for the Stratum subscribe response during handshake.
    pub handshake_timeout_ms: u64,

    /// If no message is received from the pool for this duration, the session is considered stuck
    /// and will be reconnected (when reconnect is enabled).
    pub idle_timeout_secs: u64,
}

impl Config {
    pub fn from_env() -> Self {
        let pool_addr = env::var("MINING_POOL").unwrap_or_else(|_| "solo.ckpool.org:3333".to_string());
        let worker_name = env::var("MINING_USER").unwrap_or_else(|_| "lightminer.1".to_string());
        let worker_password = env::var("MINING_PASS").unwrap_or_else(|_| "x".to_string());
        let agent_default = format!("LightMiner-Rust/{}", env!("CARGO_PKG_VERSION"));
        let agent = env::var("MINING_AGENT").unwrap_or(agent_default);
        let use_tui = env::var("NO_TUI").is_err();

        let reconnect = parse_bool_env("MINING_RECONNECT", true);
        let reconnect_max_delay_ms = env::var("MINING_RECONNECT_MAX_DELAY_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30_000);

        let miner_threads = env::var("MINING_THREADS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or_else(|| num_cpus::get().max(1));

        let handshake_timeout_ms = env::var("MINING_HANDSHAKE_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(10_000);

        let idle_timeout_secs = env::var("MINING_IDLE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(120);

        Self {
            pool_addr,
            worker_name,
            worker_password,
            agent,
            use_tui,

            reconnect,
            reconnect_max_delay_ms,
            miner_threads,

            handshake_timeout_ms,
            idle_timeout_secs,
        }
    }
}

fn parse_bool_env(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            match v.as_str() {
                "1" | "true" | "yes" | "y" | "on" => true,
                "0" | "false" | "no" | "n" | "off" => false,
                _ => default,
            }
        }
        Err(_) => default,
    }
}
