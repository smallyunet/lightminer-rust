use crate::config::{Coin, MiningAlgorithm, PoolConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolPreset {
    pub name: String,
    pub addr: String,
    pub coin: String,
    pub algo: String,
    #[serde(default)]
    pub weight: u32,
    #[serde(default)]
    pub note: Option<String>,
}

impl PoolPreset {
    pub fn to_pool_config(&self, default_user: &str, default_pass: &str) -> PoolConfig {
        let coin = self
            .coin
            .parse::<Coin>()
            .unwrap_or_else(|_| Coin::Other(self.coin.clone()));
        let algo = self
            .algo
            .parse::<MiningAlgorithm>()
            .unwrap_or_else(|_| MiningAlgorithm::infer_from_coin(&coin));

        PoolConfig {
            name: self.name.clone(),
            addr: self.addr.clone(),
            user: default_user.to_string(),
            pass: default_pass.to_string(),
            coin,
            algo,
            weight: self.weight.max(1),
        }
    }
}

pub fn embedded_presets() -> Vec<PoolPreset> {
    vec![
        PoolPreset {
            name: "btc.ckpool".to_string(),
            addr: "solo.ckpool.org:3333".to_string(),
            coin: "btc".to_string(),
            algo: "sha256d".to_string(),
            weight: 1,
            note: Some(
                "BTC solo (ckpool). Username usually requires a BTC address: <address>.worker"
                    .to_string(),
            ),
        },
        // NOTE: These are example presets; users should override to their preferred pool.
        PoolPreset {
            name: "ltc.example".to_string(),
            addr: "ltc-pool.example.com:3333".to_string(),
            coin: "ltc".to_string(),
            algo: "scrypt".to_string(),
            weight: 1,
            note: Some("Example LTC scrypt StratumV1 pool (replace with real pool)".to_string()),
        },
        PoolPreset {
            name: "doge.example".to_string(),
            addr: "doge-pool.example.com:3333".to_string(),
            coin: "doge".to_string(),
            algo: "scrypt".to_string(),
            weight: 1,
            note: Some("Example DOGE scrypt StratumV1 pool (replace with real pool)".to_string()),
        },
    ]
}

pub fn find_preset(name: &str) -> Option<PoolPreset> {
    let target = name.trim();
    if target.is_empty() {
        return None;
    }
    embedded_presets()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(target))
}

pub fn parse_preset_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_preset_list() {
        let v = parse_preset_list(" btc.ckpool, ltc.example ,,doge.example ");
        assert_eq!(v, vec!["btc.ckpool", "ltc.example", "doge.example"]);
    }

    #[test]
    fn test_find_preset_case_insensitive() {
        assert!(find_preset("BTC.CKPOOL").is_some());
    }
}
