mod metrics;

use crate::miner::{MinerCommand, NonceFound};
use crate::network::Client;
use crate::protocol::{parse_message, Job, MessageType, Request, SubscriptionResult};
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub use metrics::Metrics;

const WORKER_NAME: &str = "lightminer.1";
const WORKER_PASSWORD: &str = "x";

pub struct ManagerState {
    pub subscription: Option<SubscriptionResult>,
    pub current_job: Option<Job>,
    pub difficulty: f64,
    pub metrics: Arc<Metrics>,
}

impl Default for ManagerState {
    fn default() -> Self {
        Self {
            subscription: None,
            current_job: None,
            difficulty: 1.0,
            metrics: Arc::new(Metrics::new()),
        }
    }
}

pub async fn run() -> Result<()> {
    let pool_addr =
        std::env::var("MINING_POOL").unwrap_or_else(|_| "solo.ckpool.org:3333".to_string());

    // Create channels for miner communication
    let (miner_tx, mut miner_rx) = mpsc::channel::<NonceFound>(32);
    let (job_tx, job_rx) = mpsc::channel::<MinerCommand>(32);

    // Initialize manager state
    let mut state = ManagerState::default();

    // 1. Connect
    let mut client = Client::connect(&pool_addr).await?;

    // 2. Subscribe
    let subscribe_req = Request::subscribe(1, "LightMiner/0.1");
    let req_json = serde_json::to_string(&subscribe_req).context("Failed to serialize request")?;

    info!("Sending Subscribe Request...");
    client.send(&req_json).await?;

    // Wait for subscription response
    let mut request_id: u64 = 2;

    // Read subscription response
    if let Some(line) = client.next_message().await? {
        info!("Received: {}", line);
        if let Ok(MessageType::Response(resp)) = parse_message(&line) {
            if resp.id == 1 && resp.is_success() {
                if let Some(sub) = resp.parse_subscription() {
                    info!(
                        "Subscribed! extranonce1={}, extranonce2_size={}",
                        sub.extranonce1, sub.extranonce2_size
                    );
                    state.subscription = Some(sub);
                }
            }
        }
    }

    // 3. Authorize
    let auth_req = Request::authorize(request_id, WORKER_NAME, WORKER_PASSWORD);
    let auth_json = serde_json::to_string(&auth_req).context("Failed to serialize auth request")?;
    request_id += 1;

    info!("Sending Authorize Request for {}...", WORKER_NAME);
    client.send(&auth_json).await?;

    // Start the miner worker
    let miner_metrics = Arc::clone(&state.metrics);
    let miner_handle = tokio::spawn(async move {
        crate::miner::run_worker(job_rx, miner_tx, miner_metrics).await
    });

    // Main event loop
    info!("Entering main event loop...");
    loop {
        tokio::select! {
            // Handle messages from pool
            msg_result = client.next_message() => {
                match msg_result {
                    Ok(Some(line)) => {
                        handle_pool_message(&line, &mut state, &job_tx, &mut client, &mut request_id).await?;
                    }
                    Ok(None) => {
                        warn!("Connection closed by server.");
                        break;
                    }
                    Err(e) => {
                        error!("Network error: {:?}", e);
                        break;
                    }
                }
            }
            // Handle nonce found from miner
            Some(nonce_found) = miner_rx.recv() => {
                handle_nonce_found(&nonce_found, &mut client, &mut request_id, &state).await?;
            }
        }
    }

    // Cleanup
    drop(job_tx);
    let _ = miner_handle.await;

    Ok(())
}

async fn handle_pool_message(
    line: &str,
    state: &mut ManagerState,
    job_tx: &mpsc::Sender<MinerCommand>,
    client: &mut Client,
    request_id: &mut u64,
) -> Result<()> {
    info!("Pool: {}", line);

    match parse_message(line) {
        Ok(MessageType::Response(resp)) => {
            if resp.is_authorized() {
                info!("Authorization successful!");
            } else if resp.error.is_some() {
                warn!("Response error: {:?}", resp.error);
            }
            // Track submit responses
            if resp.id >= 3 {
                if resp.is_success() {
                    state.metrics.add_accepted();
                    info!(
                        "Share accepted! Total: {} accepted, {} rejected",
                        state.metrics.accepted(),
                        state.metrics.rejected()
                    );
                } else {
                    state.metrics.add_rejected();
                    warn!(
                        "Share rejected! Total: {} accepted, {} rejected",
                        state.metrics.accepted(),
                        state.metrics.rejected()
                    );
                }
            }
        }
        Ok(MessageType::Notification(notif)) => {
            if let Some(diff) = notif.parse_difficulty() {
                info!("Difficulty set to: {}", diff);
                state.difficulty = diff;
            }

            if let Some(job) = notif.parse_job() {
                info!(
                    "New job received: {} (clean={})",
                    job.job_id, job.clean_jobs
                );

                // Stop current mining if clean_jobs is true
                if job.clean_jobs {
                    let _ = job_tx.send(MinerCommand::Stop).await;
                }

                // Dispatch job to miner
                if let Some(ref sub) = state.subscription {
                    let _ = job_tx
                        .send(MinerCommand::NewJob {
                            job: job.clone(),
                            extranonce1: sub.extranonce1.clone(),
                            extranonce2_size: sub.extranonce2_size,
                            difficulty: state.difficulty,
                        })
                        .await;
                }

                state.current_job = Some(job);
            }
        }
        Err(e) => {
            warn!("Failed to parse message: {:?}", e);
        }
    }

    Ok(())
}

async fn handle_nonce_found(
    nonce_found: &NonceFound,
    client: &mut Client,
    request_id: &mut u64,
    state: &ManagerState,
) -> Result<()> {
    info!(
        "Nonce found! job_id={}, nonce={}, extranonce2={}",
        nonce_found.job_id, nonce_found.nonce, nonce_found.extranonce2
    );

    let submit_req = Request::submit(
        *request_id,
        WORKER_NAME,
        &nonce_found.job_id,
        &nonce_found.extranonce2,
        &nonce_found.ntime,
        &nonce_found.nonce,
    );

    *request_id += 1;

    let submit_json = serde_json::to_string(&submit_req)?;
    info!("Submitting share: {}", submit_json);
    client.send(&submit_json).await?;

    Ok(())
}
