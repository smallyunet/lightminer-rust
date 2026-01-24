mod metrics;

use crate::config::Config;
use crate::miner::{MinerCommand, NonceFound};
use crate::network::Client;
use crate::protocol::{parse_message, Job, MessageType, Request, SubscriptionResult};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

pub use metrics::Metrics;

#[derive(Debug, Clone)]
pub enum ManagerEvent {
    Log(String),
    Connected(bool),
    Difficulty(f64),
    CurrentJob(Option<String>),
    ShareAccepted,
    ShareRejected,
}

pub struct ManagerState {
    pub subscription: Option<SubscriptionResult>,
    pub current_job: Option<Job>,
    pub difficulty: f64,
    pub metrics: Arc<Metrics>,
    pending_submit_ids: HashSet<u64>,
}

impl Default for ManagerState {
    fn default() -> Self {
        Self {
            subscription: None,
            current_job: None,
            difficulty: 1.0,
            metrics: Arc::new(Metrics::new()),
            pending_submit_ids: HashSet::new(),
        }
    }
}

pub async fn run_with_config(
    config: Config,
    metrics: Arc<Metrics>,
    ui_events: Option<mpsc::Sender<ManagerEvent>>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    emit(&ui_events, ManagerEvent::Connected(false)).await;
    emit(
        &ui_events,
        ManagerEvent::Log(format!("Connecting to {}...", config.pool_addr)),
    )
    .await;

    // Create channels for miner communication
    let (miner_tx, mut miner_rx) = mpsc::channel::<NonceFound>(32);
    let (job_tx, job_rx) = mpsc::channel::<MinerCommand>(32);

    // Initialize manager state
    let mut state = ManagerState {
        metrics,
        ..ManagerState::default()
    };

    // 1. Connect
    let (mut client, proxy_info) = Client::connect_with_proxy_info(&config.pool_addr).await?;
    if let Some(p) = proxy_info {
        emit(&ui_events, ManagerEvent::Log(format!("Using proxy: {p}"))).await;
    }
    emit(&ui_events, ManagerEvent::Connected(true)).await;
    emit(
        &ui_events,
        ManagerEvent::Log(format!("Connected to {}", config.pool_addr)),
    )
    .await;

    // 2. Subscribe
    let subscribe_req = Request::subscribe(1, &config.agent);
    let req_json = serde_json::to_string(&subscribe_req).context("Failed to serialize request")?;

    info!("Sending Subscribe Request...");
    client.send(&req_json).await?;

    // Wait for subscription response
    let mut request_id: u64 = 2;

    // Read subscription response
    if let Some(line) = client.next_message().await? {
        info!("Received: {}", line);
        if let Ok(MessageType::Response(resp)) = parse_message(&line) {
            if resp.id == Some(1) && resp.is_success() {
                if let Some(sub) = resp.parse_subscription() {
                    info!(
                        "Subscribed! extranonce1={}, extranonce2_size={}",
                        sub.extranonce1, sub.extranonce2_size
                    );
                    state.subscription = Some(sub);
                    emit(&ui_events, ManagerEvent::Log("Subscribed successfully".to_string())).await;
                }
            }
        }
    }

    // 3. Authorize
    let auth_req = Request::authorize(request_id, &config.worker_name, &config.worker_password);
    let auth_json = serde_json::to_string(&auth_req).context("Failed to serialize auth request")?;
    request_id += 1;

    info!("Sending Authorize Request for {}...", config.worker_name);
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
            shutdown_result = shutdown_rx.changed() => {
                match shutdown_result {
                    Ok(()) => {
                        if *shutdown_rx.borrow() {
                            emit(&ui_events, ManagerEvent::Log("Shutdown requested".to_string())).await;
                            break;
                        } else {
                            continue;
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
            // Handle messages from pool
            msg_result = client.next_message() => {
                match msg_result {
                    Ok(Some(line)) => {
                        handle_pool_message(&line, &mut state, &job_tx, &ui_events).await?;
                    }
                    Ok(None) => {
                        warn!("Connection closed by server.");
                        emit(&ui_events, ManagerEvent::Connected(false)).await;
                        emit(&ui_events, ManagerEvent::Log("Disconnected".to_string())).await;
                        break;
                    }
                    Err(e) => {
                        error!("Network error: {:?}", e);
                        emit(&ui_events, ManagerEvent::Connected(false)).await;
                        emit(&ui_events, ManagerEvent::Log(format!("Network error: {e:?}"))).await;
                        break;
                    }
                }
            }
            // Handle nonce found from miner
            Some(nonce_found) = miner_rx.recv() => {
                handle_nonce_found(&nonce_found, &mut client, &mut request_id, &mut state, &config, &ui_events).await?;
            }
        }
    }

    // Cleanup
    let _ = job_tx.send(MinerCommand::Stop).await;
    drop(job_tx);
    let _ = miner_handle.await;

    Ok(())
}

async fn emit(ui_events: &Option<mpsc::Sender<ManagerEvent>>, event: ManagerEvent) {
    if let Some(tx) = ui_events {
        let _ = tx.send(event).await;
    } else {
        // No-op
    }
}

async fn handle_pool_message(
    line: &str,
    state: &mut ManagerState,
    job_tx: &mpsc::Sender<MinerCommand>,
    ui_events: &Option<mpsc::Sender<ManagerEvent>>,
) -> Result<()> {
    info!("Pool: {}", line);

    match parse_message(line) {
        Ok(MessageType::Response(resp)) => {
            if resp.is_authorized() {
                info!("Authorization successful!");
                emit(ui_events, ManagerEvent::Log("Authorization successful".to_string())).await;
            } else if resp.error.is_some() {
                warn!("Response error: {:?}", resp.error);
                emit(ui_events, ManagerEvent::Log(format!("Pool error: {:?}", resp.error))).await;
            } else {
                // No-op
            }

            let is_submit_response = resp
                .id
                .map(|id| state.pending_submit_ids.remove(&id))
                .unwrap_or(false);
            if is_submit_response {
                if resp.is_success() {
                    state.metrics.add_accepted();
                    emit(ui_events, ManagerEvent::ShareAccepted).await;
                    emit(ui_events, ManagerEvent::Log("Share accepted".to_string())).await;
                } else {
                    state.metrics.add_rejected();
                    emit(ui_events, ManagerEvent::ShareRejected).await;
                    emit(ui_events, ManagerEvent::Log("Share rejected".to_string())).await;
                }
            } else {
                // No-op
            }
        }
        Ok(MessageType::Notification(notif)) => {
            if let Some(diff) = notif.parse_difficulty() {
                info!("Difficulty set to: {}", diff);
                state.difficulty = diff;
                emit(ui_events, ManagerEvent::Difficulty(diff)).await;
            } else {
                // No-op
            }

            if let Some(job) = notif.parse_job() {
                info!(
                    "New job received: {} (clean={})",
                    job.job_id, job.clean_jobs
                );

                emit(ui_events, ManagerEvent::CurrentJob(Some(job.job_id.clone()))).await;
                emit(ui_events, ManagerEvent::Log(format!("New job: {}", job.job_id))).await;

                // Stop current mining if clean_jobs is true
                if job.clean_jobs {
                    let _ = job_tx.send(MinerCommand::Stop).await;
                    emit(ui_events, ManagerEvent::Log("Clean jobs requested; stopping current work".to_string())).await;
                } else {
                    // No-op
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
                } else {
                    emit(ui_events, ManagerEvent::Log("Received job before subscription; ignoring".to_string())).await;
                }

                state.current_job = Some(job);
            } else {
                // No-op
            }
        }
        Err(e) => {
            warn!("Failed to parse message: {:?}", e);
            emit(ui_events, ManagerEvent::Log(format!("Failed to parse message: {e:?}"))).await;
        }
    }

    Ok(())
}

async fn handle_nonce_found(
    nonce_found: &NonceFound,
    client: &mut Client,
    request_id: &mut u64,
    state: &mut ManagerState,
    config: &Config,
    ui_events: &Option<mpsc::Sender<ManagerEvent>>,
) -> Result<()> {
    info!(
        "Nonce found! job_id={}, nonce={}, extranonce2={}",
        nonce_found.job_id, nonce_found.nonce, nonce_found.extranonce2
    );

    emit(
        ui_events,
        ManagerEvent::Log(format!(
            "Nonce found: job_id={}, nonce={}",
            nonce_found.job_id, nonce_found.nonce
        )),
    )
    .await;

    let submit_req = Request::submit(
        *request_id,
        &config.worker_name,
        &nonce_found.job_id,
        &nonce_found.extranonce2,
        &nonce_found.ntime,
        &nonce_found.nonce,
    );

    state.pending_submit_ids.insert(*request_id);

    *request_id += 1;

    let submit_json = serde_json::to_string(&submit_req)?;
    info!("Submitting share: {}", submit_json);
    client.send(&submit_json).await?;

    emit(ui_events, ManagerEvent::Log("Submitted share".to_string())).await;

    Ok(())
}
