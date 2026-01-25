use std::env;
use std::str::FromStr;

use crate::presets::{find_preset, parse_preset_list, PoolPreset};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coin {
    Btc,
    Bch,
    Ltc,
    Doge,
    /// Unknown coin symbol. Kept for display/future expansion.
    Other(String),
}

impl Coin {
    pub fn symbol(&self) -> &str {
        match self {
            Coin::Btc => "BTC",
            Coin::Bch => "BCH",
            Coin::Ltc => "LTC",
            Coin::Doge => "DOGE",
            Coin::Other(s) => s.as_str(),
        }
    }
}

impl FromStr for Coin {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim();
        if v.is_empty() {
            return Err(());
        }
        let lower = v.to_ascii_lowercase();
        Ok(match lower.as_str() {
            "btc" | "bitcoin" => Coin::Btc,
            "bch" | "bitcoincash" | "bitcoin-cash" => Coin::Bch,
            "ltc" | "litecoin" => Coin::Ltc,
            "doge" | "dogecoin" => Coin::Doge,
            _ => Coin::Other(v.to_string()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiningAlgorithm {
    Sha256d,
    /// Litecoin/Dogecoin style PoW: scrypt_1024_1_1_256
    Scrypt,
    /// Unknown algorithm string. Kept for display/future expansion.
    Other(String),
}

impl MiningAlgorithm {
    pub fn name(&self) -> &str {
        match self {
            MiningAlgorithm::Sha256d => "sha256d",
            MiningAlgorithm::Scrypt => "scrypt",
            MiningAlgorithm::Other(s) => s.as_str(),
        }
    }

    pub fn infer_from_coin(coin: &Coin) -> Self {
        match coin {
            Coin::Btc | Coin::Bch => MiningAlgorithm::Sha256d,
            Coin::Ltc | Coin::Doge => MiningAlgorithm::Scrypt,
            Coin::Other(_) => MiningAlgorithm::Sha256d,
        }
    }

    pub fn from_env_default() -> Self {
        match env::var("MINING_ALGO") {
            Ok(v) => MiningAlgorithm::from_str(&v).unwrap_or(MiningAlgorithm::Sha256d),
            Err(_) => MiningAlgorithm::Sha256d,
        }
    }
}

impl FromStr for MiningAlgorithm {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim();
        if v.is_empty() {
            return Err(());
        }
        let lower = v.to_ascii_lowercase();
        Ok(match lower.as_str() {
            "sha256d" | "sha-256d" | "sha256" | "sha-256" => MiningAlgorithm::Sha256d,
            "scrypt" | "scrypt_1024_1_1_256" | "scrypt1024" => MiningAlgorithm::Scrypt,
            _ => MiningAlgorithm::Other(v.to_string()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolStrategy {
    /// Try pools in order; stick to the current pool until it fails.
    Failover,
    /// Rotate pool selection on each (re)connect attempt.
    RoundRobin,
    /// Weighted round-robin by `PoolConfig.weight`.
    Weighted,
}

impl PoolStrategy {
    pub fn from_env() -> Self {
        match env::var("MINING_POOL_STRATEGY") {
            Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
                "failover" | "priority" => PoolStrategy::Failover,
                "round_robin" | "roundrobin" | "rr" => PoolStrategy::RoundRobin,
                "weighted" | "weighted_rr" | "wrr" => PoolStrategy::Weighted,
                _ => PoolStrategy::Failover,
            },
            Err(_) => PoolStrategy::Failover,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub name: String,
    pub addr: String,
    pub user: String,
    pub pass: String,
    pub coin: Coin,
    pub algo: MiningAlgorithm,
    /// Relative weight for weighted strategies (>= 1).
    pub weight: u32,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Backwards-compatible primary pool fields.
    pub pool_addr: String,
    pub worker_name: String,
    pub worker_password: String,

    /// Multi-pool configuration (v0.0.6+).
    pub pools: Vec<PoolConfig>,
    pub pool_strategy: PoolStrategy,
    pub pool_failures_before_cooldown: u32,
    pub pool_failure_cooldown_secs: u64,

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

        // Priority order:
        // 1) MINING_POOLS (explicit)
        // 2) MINING_PRESET / MINING_PRESET_FILE (presets)
        // 3) MINING_POOL (+ MINING_ALGO) single-pool fallback
        let pools = parse_pools_env(&pool_addr, &worker_name, &worker_password);
        let (pool_addr, worker_name, worker_password) = pools
            .first()
            .map(|p| (p.addr.clone(), p.user.clone(), p.pass.clone()))
            .unwrap_or((pool_addr, worker_name, worker_password));

        let pool_strategy = PoolStrategy::from_env();
        let pool_failures_before_cooldown = env::var("MINING_POOL_FAILURES_BEFORE_COOLDOWN")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(3);
        let pool_failure_cooldown_secs = env::var("MINING_POOL_COOLDOWN_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(30);

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

            pools,
            pool_strategy,
            pool_failures_before_cooldown,
            pool_failure_cooldown_secs,

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

fn parse_pools_env(default_addr: &str, default_user: &str, default_pass: &str) -> Vec<PoolConfig> {
    // Format:
    //   MINING_POOLS="name@host:port|user|pass|coin|weight|algo;backup@host:port|user|pass|btc|1|sha256d"
    // All fields after addr are optional. Missing user/pass fallback to MINING_USER/MINING_PASS.
    // Missing coin defaults to BTC. Missing weight defaults to 1.
    let raw = match env::var("MINING_POOLS") {
        Ok(v) => v,
        Err(_) => String::new(),
    };

    let mut pools = Vec::new();

    for (idx, entry) in raw
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .enumerate()
    {
        let mut name = String::new();
        let mut rest = entry;

        if let Some((n, r)) = entry.split_once('@') {
            let n = n.trim();
            if !n.is_empty() {
                name = n.to_string();
                rest = r;
            }
        }

        let parts: Vec<&str> = rest.split('|').collect();
        let addr = parts.get(0).map(|v| v.trim()).unwrap_or("");
        if addr.is_empty() {
            continue;
        }

        let user = parts
            .get(1)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .unwrap_or(default_user)
            .to_string();
        let pass = parts
            .get(2)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .unwrap_or(default_pass)
            .to_string();

        let coin = parts
            .get(3)
            .and_then(|v| Coin::from_str(v).ok())
            .unwrap_or(Coin::Btc);

        let weight = parts
            .get(4)
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1);

        let algo = parts
            .get(5)
            .and_then(|v| MiningAlgorithm::from_str(v).ok())
            .unwrap_or_else(|| MiningAlgorithm::infer_from_coin(&coin));

        let name = if name.is_empty() {
            format!("pool{}", idx + 1)
        } else {
            name
        };

        pools.push(PoolConfig {
            name,
            addr: addr.to_string(),
            user,
            pass,
            coin,
            algo,
            weight,
        });
    }

    if pools.is_empty() {
        // Try presets if MINING_POOLS isn't specified.
        if let Some(preset_pools) = parse_presets_env(default_user, default_pass) {
            pools = preset_pools;
        } else {
            let algo = MiningAlgorithm::from_env_default();
            let coin = Coin::Btc;
            pools.push(PoolConfig {
                name: "pool1".to_string(),
                addr: default_addr.to_string(),
                user: default_user.to_string(),
                pass: default_pass.to_string(),
                coin,
                algo,
                weight: 1,
            });
        }
    }

    pools
}

fn parse_presets_env(default_user: &str, default_pass: &str) -> Option<Vec<PoolConfig>> {
    let preset_names = env::var("MINING_PRESET").ok();
    let preset_file = env::var("MINING_PRESET_FILE").ok();

    if preset_names.as_deref().unwrap_or("").trim().is_empty() && preset_file.is_none() {
        return None;
    }

    let mut presets: Vec<PoolPreset> = Vec::new();

    if let Some(path) = preset_file {
        if let Ok(text) = std::fs::read_to_string(path.trim()) {
            if let Ok(mut v) = serde_json::from_str::<Vec<PoolPreset>>(&text) {
                presets.append(&mut v);
            }
        }
    }

    if let Some(names_raw) = preset_names {
        for n in parse_preset_list(&names_raw) {
            if let Some(p) = find_preset(&n) {
                presets.push(p);
            }
        }
    }

    if presets.is_empty() {
        return None;
    }

    Some(
        presets
            .iter()
            .map(|p| p.to_pool_config(default_user, default_pass))
            .collect(),
    )
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
