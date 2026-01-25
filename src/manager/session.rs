use super::{ManagerEvent, ManagerState};
use crate::config::Config;
use crate::miner::{MinerCommand, NonceFound};
use crate::network::Client;
use crate::protocol::{parse_message, Job, MessageType, Request};
use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::events::emit;

pub(crate) async fn handle_pool_message(
    line: &str,
    state: &mut ManagerState,
    job_tx: &mpsc::Sender<MinerCommand>,
    ui_events: &Option<mpsc::Sender<ManagerEvent>>,
) -> Result<()> {
    info!("Pool: {}", line);

    match parse_message(line) {
        Ok(MessageType::Response(resp)) => {
            let is_auth_response = state.authorize_request_id.is_some()
                && resp.id == state.authorize_request_id;
            if is_auth_response {
                if resp.is_authorized() {
                    info!("Authorization successful!");
                    emit(ui_events, ManagerEvent::Log("Authorization successful".to_string()))
                        .await;
                    emit(ui_events, ManagerEvent::Authorized(true)).await;
                } else {
                    // Many pools (including ckpool) respond with: {"result": false, "error": null}
                    // when the username/address is invalid. Without this log it looks like "connected but no job".
                    warn!("Authorization failed (result=false). worker_name={}", "<redacted>");
                    emit(
                        ui_events,
                        ManagerEvent::Log(
                            "Authorization failed (pool returned result=false). Check MINING_USER (ckpool usually requires a BTC address, e.g. <address>.worker)."
                                .to_string(),
                        ),
                    )
                    .await;
                    emit(ui_events, ManagerEvent::Authorized(false)).await;
                }

                // Only treat the first authorize response as authoritative.
                state.authorize_request_id = None;
            }

            if resp.error.is_some() {
                warn!("Response error: {:?}", resp.error);
                emit(ui_events, ManagerEvent::Log(format!("Pool error: {:?}", resp.error))).await;
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
            }
        }
        Ok(MessageType::Notification(notif)) => {
            if let Some(diff) = notif.parse_difficulty() {
                info!("Difficulty set to: {}", diff);
                state.difficulty = diff;
                emit(ui_events, ManagerEvent::Difficulty(diff)).await;
            }

            if let Some(job) = notif.parse_job() {
                info!(
                    "New job received: {} (clean={})",
                    job.job_id, job.clean_jobs
                );

                dispatch_job(&job, state, job_tx, ui_events).await;
            }
        }
        Err(e) => {
            warn!("Failed to parse message: {:?}", e);
            emit(ui_events, ManagerEvent::Log(format!("Failed to parse message: {e:?}"))).await;
        }
    }

    Ok(())
}

pub(crate) async fn dispatch_job(
    job: &Job,
    state: &mut ManagerState,
    job_tx: &mpsc::Sender<MinerCommand>,
    ui_events: &Option<mpsc::Sender<ManagerEvent>>,
) {
    emit(ui_events, ManagerEvent::CurrentJob(Some(job.job_id.clone()))).await;
    emit(ui_events, ManagerEvent::Log(format!("New job: {}", job.job_id))).await;

    if job.clean_jobs {
        let _ = job_tx.send(MinerCommand::Stop).await;
        emit(
            ui_events,
            ManagerEvent::Log("Clean jobs requested; stopping current work".to_string()),
        )
        .await;
    }

    if let Some(ref sub) = state.subscription {
        let _ = job_tx
            .send(MinerCommand::NewJob {
                job: job.clone(),
                extranonce1: sub.extranonce1.clone(),
                extranonce2_size: sub.extranonce2_size,
                difficulty: state.difficulty,
            })
            .await;
        state.current_job = Some(job.clone());
    } else {
        emit(
            ui_events,
            ManagerEvent::Log("Received job before subscription; ignoring".to_string()),
        )
        .await;
    }
}

pub(crate) async fn handle_nonce_found(
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
