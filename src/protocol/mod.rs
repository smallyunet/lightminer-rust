use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stratum V1 Request
/// Example: {"id": 1, "method": "mining.subscribe", "params": []}
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub params: Vec<Value>,
}

/// Stratum V1 Response
/// Example: {"id": 1, "result": [["mining.notify", "ae6812eb4cd7735a302a8a9dd95cf71f"], "f0", 4], "error": null}
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

/// Notification (Server to Client)
/// Example: {"method": "mining.notify", "params": [...]}
#[derive(Debug, Serialize, Deserialize)]
pub struct Notification {
    pub method: String,
    pub params: Vec<Value>,
}

impl Request {
    pub fn new(id: u64, method: &str, params: Vec<Value>) -> Self {
        Self {
            id,
            method: method.to_string(),
            params,
        }
    }

    pub fn subscribe(id: u64, agent: &str) -> Self {
        Self::new(
            id,
            "mining.subscribe",
            vec![serde_json::json!(agent)],
        )
    }

    pub fn authorize(id: u64, worker_name: &str, password: &str) -> Self {
        Self::new(
            id,
            "mining.authorize",
            vec![serde_json::json!(worker_name), serde_json::json!(password)],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_subscribe() {
        let req = Request::subscribe(1, "LightMiner/0.1");
        let json = serde_json::to_string(&req).unwrap();
        // Note: field order in JSON is not guaranteed, but usually usually id comes first in serde if defined first.
        // We verify content.
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"mining.subscribe\""));
        assert!(json.contains("LightMiner/0.1"));
    }

    #[test]
    fn test_serialize_authorize() {
        let req = Request::authorize(2, "user.worker", "password");
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"method\":\"mining.authorize\""));
        assert!(json.contains("user.worker"));
    }
}
