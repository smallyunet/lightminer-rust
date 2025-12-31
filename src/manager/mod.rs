use crate::network::Client;
use crate::protocol::Request;
use anyhow::{Context, Result};
use tracing::{info, warn};

// Public Dogecoin Testnet Pool (or similar)
// For now we use a common one.
const DEFAULT_POOL: &str = "stratum-test.dogepool.org:3333"; 
// Note: If this fails, we might need a different one. 
// A lot of pools require a valid wallet address to authorize, but subscribe should work.

pub async fn run() -> Result<()> {
    let pool_addr = std::env::var("MINING_POOL").unwrap_or_else(|_| "solo.ckpool.org:3333".to_string());
    
    // 1. Connect
    let mut client = Client::connect(&pool_addr).await?;

    // 2. Subscribe
    // id: 1, agent: "LightMiner/0.1"
    let subscribe_req = Request::subscribe(1, "LightMiner/0.1");
    let req_json = serde_json::to_string(&subscribe_req).context("Failed to serialize request")?;
    
    info!("Sending Subscribe Request...");
    client.send(&req_json).await?;

    // 3. Read Response (Loop a few times to see what we get)
    // Expecting: {"id":1, "result": [ ... ], "error": null}
    // Also likely to see mining.set_difficulty or mining.notify soon after.
    for _ in 0..5 {
        if let Some(line) = client.next_message().await? {
            info!("Received: {}", line);
            // TODO: Parse this into protocol::Response or Notification
        } else {
            warn!("Connection closed by server.");
            break;
        }
    }

    Ok(())
}
