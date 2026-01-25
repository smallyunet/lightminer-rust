use super::{ManagerEvent, ManagerState, Metrics};
use crate::config::Config;
use crate::network::Client;
use crate::protocol::{parse_message, Job, MessageType, Request};
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::events::emit;

pub(crate) async fn connect_and_handshake(
    config: &Config,
    metrics: Arc<Metrics>,
    ui_events: &Option<mpsc::Sender<ManagerEvent>>,
) -> Result<(Client, ManagerState, u64, Option<Job>)> {
    emit(ui_events, ManagerEvent::Connected(false)).await;
    emit(
        ui_events,
        ManagerEvent::Log(format!("Connecting to {}...", config.pool_addr)),
    )
    .await;

    // 1. Connect
    let (mut client, proxy_info) = Client::connect_with_proxy_info(&config.pool_addr).await?;
    emit(ui_events, ManagerEvent::ProxyInfo(proxy_info.clone())).await;
    if let Some(p) = proxy_info {
        emit(ui_events, ManagerEvent::Log(format!("Using proxy: {p}"))).await;
    }
    emit(ui_events, ManagerEvent::Connected(true)).await;
    emit(
        ui_events,
        ManagerEvent::Log(format!("Connected to {}", config.pool_addr)),
    )
    .await;

    // Initialize session state
    let mut state = ManagerState {
        metrics,
        ..ManagerState::default()
    };
    state.pending_submit_ids.clear();
    state.authorize_request_id = None;

    // 2. Subscribe
    let subscribe_req = Request::subscribe(1, &config.agent);
    let req_json = serde_json::to_string(&subscribe_req).context("Failed to serialize request")?;
    info!("Sending Subscribe Request...");
    client.send(&req_json).await?;

    // Wait for subscription response (some pools may send difficulty/job notifications before the response).
    let mut pending_job: Option<Job> = None;
    let handshake_timeout = std::time::Duration::from_millis(config.handshake_timeout_ms.max(1));
    let subscribe_deadline = tokio::time::Instant::now() + handshake_timeout;

    while state.subscription.is_none() {
        let now = tokio::time::Instant::now();
        if now >= subscribe_deadline {
            anyhow::bail!("Handshake timeout waiting for mining.subscribe response");
        }
        let remaining = subscribe_deadline.saturating_duration_since(now);

        let line_opt = tokio::time::timeout(remaining, client.next_message()).await;
        let line_opt = match line_opt {
            Ok(v) => v?,
            Err(_) => anyhow::bail!("Handshake timeout waiting for mining.subscribe response"),
        };

        let Some(line) = line_opt else {
            anyhow::bail!("Connection closed while waiting for mining.subscribe response");
        };

        match parse_message(&line) {
            Ok(MessageType::Response(resp)) => {
                if resp.id == Some(1) {
                    if resp.is_success() {
                        let Some(sub) = resp.parse_subscription() else {
                            anyhow::bail!(
                                "Invalid subscribe response: missing subscription fields"
                            );
                        };
                        info!(
                            "Subscribed! extranonce1={}, extranonce2_size={}",
                            sub.extranonce1, sub.extranonce2_size
                        );
                        state.subscription = Some(sub);
                        emit(ui_events, ManagerEvent::Log("Subscribed successfully".to_string()))
                            .await;
                    } else {
                        anyhow::bail!("Subscribe failed: {:?}", resp.error);
                    }
                }
            }
            Ok(MessageType::Notification(notif)) => {
                if let Some(diff) = notif.parse_difficulty() {
                    state.difficulty = diff;
                    emit(ui_events, ManagerEvent::Difficulty(diff)).await;
                }
                if let Some(job) = notif.parse_job() {
                    pending_job = Some(job);
                }
            }
            Err(e) => {
                warn!("Failed to parse handshake message: {:?}", e);
            }
        }
    }

    // Next request id after subscribe.
    let mut request_id: u64 = 2;

    // 3. Authorize
    let auth_req = Request::authorize(request_id, &config.worker_name, &config.worker_password);
    let auth_json = serde_json::to_string(&auth_req).context("Failed to serialize auth request")?;
    state.authorize_request_id = Some(auth_req.id);
    request_id += 1;

    info!("Sending Authorize Request for {}...", config.worker_name);
    client.send(&auth_json).await?;

    Ok((client, state, request_id, pending_job))
}
