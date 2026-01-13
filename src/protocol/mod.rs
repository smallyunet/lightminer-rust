//! # Protocol Module
//!
//! Stratum V1 protocol implementation for mining pool communication.
//!
//! ## Overview
//!
//! This module implements the JSON-RPC 2.0 based Stratum V1 protocol used for
//! communication between miners and mining pools.
//!
//! ## Message Types
//!
//! - [`Request`] - Client-to-server requests (subscribe, authorize, submit)
//! - [`Response`] - Server-to-client responses
//! - [`Notification`] - Server-to-client notifications (set_difficulty, notify)
//!
//! ## Example
//!
//! ```rust
//! use lightminer_rust::protocol::Request;
//!
//! let subscribe = Request::subscribe(1, "LightMiner/0.1");
//! let authorize = Request::authorize(2, "worker.1", "password");
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stratum V1 Request
/// Example: {"id": 1, "method": "mining.subscribe", "params": []}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub params: Vec<Value>,
}

/// Stratum V1 Response
/// Example: {"id": 1, "result": [["mining.notify", "ae6812eb4cd7735a302a8a9dd95cf71f"], "f0", 4], "error": null}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

/// Notification (Server to Client)
/// Example: {"method": "mining.notify", "params": [...]}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub method: String,
    pub params: Vec<Value>,
}

/// Parsed mining job from mining.notify
#[derive(Debug, Clone)]
pub struct Job {
    pub job_id: String,
    pub prev_hash: String,
    pub coinbase1: String,
    pub coinbase2: String,
    pub merkle_branches: Vec<String>,
    pub version: String,
    pub nbits: String,
    pub ntime: String,
    pub clean_jobs: bool,
}

/// Subscription result from mining.subscribe response
#[derive(Debug, Clone)]
pub struct SubscriptionResult {
    pub subscription_id: String,
    pub extranonce1: String,
    pub extranonce2_size: usize,
}

impl Request {
    pub fn new(id: u64, method: &str, params: Vec<Value>) -> Self {
        Self {
            id,
            method: method.to_string(),
            params,
        }
    }

    /// Create a mining.subscribe request
    pub fn subscribe(id: u64, agent: &str) -> Self {
        Self::new(
            id,
            "mining.subscribe",
            vec![serde_json::json!(agent)],
        )
    }

    /// Create a mining.authorize request
    pub fn authorize(id: u64, worker_name: &str, password: &str) -> Self {
        Self::new(
            id,
            "mining.authorize",
            vec![serde_json::json!(worker_name), serde_json::json!(password)],
        )
    }

    /// Create a mining.submit request
    /// params: [worker_name, job_id, extranonce2, ntime, nonce]
    pub fn submit(
        id: u64,
        worker_name: &str,
        job_id: &str,
        extranonce2: &str,
        ntime: &str,
        nonce: &str,
    ) -> Self {
        Self::new(
            id,
            "mining.submit",
            vec![
                serde_json::json!(worker_name),
                serde_json::json!(job_id),
                serde_json::json!(extranonce2),
                serde_json::json!(ntime),
                serde_json::json!(nonce),
            ],
        )
    }
}

impl Response {
    /// Check if the response indicates success
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }

    /// Check if authorize was successful (result should be true)
    pub fn is_authorized(&self) -> bool {
        self.result.as_ref().map(|r| r.as_bool().unwrap_or(false)).unwrap_or(false)
    }

    /// Parse subscription result from mining.subscribe response
    pub fn parse_subscription(&self) -> Option<SubscriptionResult> {
        let result = self.result.as_ref()?;
        let arr = result.as_array()?;
        if arr.len() < 3 {
            return None;
        }

        // First element is array of subscriptions, e.g. [["mining.notify", "subscription_id"]]
        let subscriptions = arr[0].as_array()?;
        let subscription_id = subscriptions
            .first()
            .and_then(|s| s.as_array())
            .and_then(|a| a.get(1))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let extranonce1 = arr[1].as_str()?.to_string();
        let extranonce2_size = arr[2].as_u64()? as usize;

        Some(SubscriptionResult {
            subscription_id,
            extranonce1,
            extranonce2_size,
        })
    }
}

impl Notification {
    /// Parse mining.notify parameters into a Job struct
    pub fn parse_job(&self) -> Option<Job> {
        if self.method != "mining.notify" {
            return None;
        }

        let params = &self.params;
        if params.len() < 9 {
            return None;
        }

        let merkle_branches: Vec<String> = params[4]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Some(Job {
            job_id: params[0].as_str()?.to_string(),
            prev_hash: params[1].as_str()?.to_string(),
            coinbase1: params[2].as_str()?.to_string(),
            coinbase2: params[3].as_str()?.to_string(),
            merkle_branches,
            version: params[5].as_str()?.to_string(),
            nbits: params[6].as_str()?.to_string(),
            ntime: params[7].as_str()?.to_string(),
            clean_jobs: params[8].as_bool().unwrap_or(false),
        })
    }

    /// Parse mining.set_difficulty to get the new difficulty
    pub fn parse_difficulty(&self) -> Option<f64> {
        if self.method != "mining.set_difficulty" {
            return None;
        }
        self.params.first()?.as_f64()
    }
}

/// Try to parse a raw JSON line as either Response or Notification
pub fn parse_message(json_line: &str) -> Result<MessageType, serde_json::Error> {
    // If it has an "id" field, it's a Response; otherwise it's a Notification
    let value: Value = serde_json::from_str(json_line)?;
    
    if value.get("id").is_some() {
        let response: Response = serde_json::from_value(value)?;
        Ok(MessageType::Response(response))
    } else {
        let notification: Notification = serde_json::from_value(value)?;
        Ok(MessageType::Notification(notification))
    }
}

/// Enum representing different message types from the pool
#[derive(Debug)]
pub enum MessageType {
    Response(Response),
    Notification(Notification),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_subscribe() {
        let req = Request::subscribe(1, "LightMiner/0.1");
        let json = serde_json::to_string(&req).unwrap();
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

    #[test]
    fn test_serialize_submit() {
        let req = Request::submit(3, "worker.1", "job123", "00000001", "5f5e1000", "12345678");
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"method\":\"mining.submit\""));
        assert!(json.contains("job123"));
        assert!(json.contains("12345678"));
    }

    #[test]
    fn test_deserialize_response_success() {
        let json = r#"{"id":1,"result":[["mining.notify","ae6812eb"],"f0002000",4],"error":null}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.is_success());
        
        let sub = resp.parse_subscription().unwrap();
        assert_eq!(sub.extranonce1, "f0002000");
        assert_eq!(sub.extranonce2_size, 4);
    }

    #[test]
    fn test_deserialize_response_error() {
        let json = r#"{"id":2,"result":null,"error":["Unauthorized","",null]}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert!(!resp.is_success());
    }

    #[test]
    fn test_deserialize_authorize_response() {
        let json = r#"{"id":2,"result":true,"error":null}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert!(resp.is_authorized());
    }

    #[test]
    fn test_deserialize_notification_mining_notify() {
        let json = r#"{"method":"mining.notify","params":["job123","0000000000000000000abc","01000000010000","ffffffff","[]","20000000","1d00ffff","5f5e1000",true]}"#;
        let notif: Notification = serde_json::from_str(json).unwrap();
        assert_eq!(notif.method, "mining.notify");
        
        let job = notif.parse_job().unwrap();
        assert_eq!(job.job_id, "job123");
        assert!(job.clean_jobs);
    }

    #[test]
    fn test_deserialize_notification_set_difficulty() {
        let json = r#"{"method":"mining.set_difficulty","params":[1024.0]}"#;
        let notif: Notification = serde_json::from_str(json).unwrap();
        let diff = notif.parse_difficulty().unwrap();
        assert_eq!(diff, 1024.0);
    }

    #[test]
    fn test_parse_message_response() {
        let json = r#"{"id":1,"result":true,"error":null}"#;
        match parse_message(json).unwrap() {
            MessageType::Response(r) => assert_eq!(r.id, 1),
            _ => panic!("Expected Response"),
        }
    }

    #[test]
    fn test_parse_message_notification() {
        let json = r#"{"method":"mining.set_difficulty","params":[512]}"#;
        match parse_message(json).unwrap() {
            MessageType::Notification(n) => assert_eq!(n.method, "mining.set_difficulty"),
            _ => panic!("Expected Notification"),
        }
    }
}
