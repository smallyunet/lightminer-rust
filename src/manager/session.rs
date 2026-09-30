use super::submit::{decide_share_submit, difficulties_close};
use super::{ManagerEvent, ManagerState};
use crate::miner::{MinerCommand, NonceFound};
use crate::network::Client;
use crate::protocol::{parse_message, Job, MessageType, Request, SubscriptionResult};
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
            let is_auth_response =
                state.authorize_request_id.is_some() && resp.id == state.authorize_request_id;
            if is_auth_response {
                if resp.is_authorized() {
                    info!("Authorization successful!");
                    emit(
                        ui_events,
                        ManagerEvent::Log("Authorization successful".to_string()),
                    )
                    .await;
                    emit(ui_events, ManagerEvent::Authorized(true)).await;
                } else {
                    // Many pools (including ckpool) respond with: {"result": false, "error": null}
                    // when the username/address is invalid. Without this log it looks like "connected but no job".
                    warn!(
                        "Authorization failed (result=false). worker_name={}",
                        "<redacted>"
                    );
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
                emit(
                    ui_events,
                    ManagerEvent::Log(format!("Pool error: {:?}", resp.error)),
                )
                .await;
            }

            let is_submit_response = resp
                .id
                .map(|id| state.pending_submit_ids.remove(&id))
                .unwrap_or(false);
            if is_submit_response {
                if resp.share_accepted() {
                    state.metrics.add_accepted();
                    emit(ui_events, ManagerEvent::ShareAccepted).await;
                    emit(ui_events, ManagerEvent::Log("Share accepted".to_string())).await;
                } else {
                    let reason = resp.share_reject_reason();
                    state.metrics.add_rejected();
                    emit(
                        ui_events,
                        ManagerEvent::ShareRejected {
                            reason: reason.clone(),
                        },
                    )
                    .await;
                    emit(
                        ui_events,
                        ManagerEvent::Log(format!("Share rejected: {reason}")),
                    )
                    .await;
                }
            }
        }
        Ok(MessageType::Notification(notif)) => {
            if let Some(diff) = notif.parse_difficulty() {
                apply_difficulty(diff, state, job_tx, ui_events).await;
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
            emit(
                ui_events,
                ManagerEvent::Log(format!("Failed to parse message: {e:?}")),
            )
            .await;
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
    emit(
        ui_events,
        ManagerEvent::CurrentJob(Some(job.job_id.clone())),
    )
    .await;
    emit(
        ui_events,
        ManagerEvent::Log(format!("New job: {}", job.job_id)),
    )
    .await;

    if job.clean_jobs {
        let _ = job_tx.send(MinerCommand::Stop).await;
        emit(
            ui_events,
            ManagerEvent::Log("Clean jobs requested; stopping current work".to_string()),
        )
        .await;
    }

    if let Some(ref sub) = state.subscription {
        let difficulty = state.difficulty;
        state.current_job = Some(job.clone());
        send_job(job.clone(), sub, difficulty, state, job_tx).await;
    } else {
        emit(
            ui_events,
            ManagerEvent::Log("Received job before subscription; ignoring".to_string()),
        )
        .await;
    }
}

pub(crate) async fn apply_difficulty(
    diff: f64,
    state: &mut ManagerState,
    job_tx: &mpsc::Sender<MinerCommand>,
    ui_events: &Option<mpsc::Sender<ManagerEvent>>,
) {
    let changed = !difficulties_close(state.difficulty, diff);
    info!("Difficulty set to: {}", diff);
    state.difficulty = diff;
    emit(ui_events, ManagerEvent::Difficulty(diff)).await;
    if !changed {
        return;
    }

    let Some(job) = state.current_job.clone() else {
        emit(
            ui_events,
            ManagerEvent::Log(format!("Difficulty set to {diff}")),
        )
        .await;
        return;
    };
    let Some(sub) = state.subscription.clone() else {
        emit(
            ui_events,
            ManagerEvent::Log(format!("Difficulty set to {diff}")),
        )
        .await;
        return;
    };

    emit(
        ui_events,
        ManagerEvent::Log(format!(
            "Difficulty updated to {diff}; restarting job {}",
            job.job_id
        )),
    )
    .await;
    send_job(job, &sub, diff, state, job_tx).await;
}

async fn send_job(
    job: Job,
    sub: &SubscriptionResult,
    difficulty: f64,
    state: &ManagerState,
    job_tx: &mpsc::Sender<MinerCommand>,
) {
    let _ = job_tx
        .send(MinerCommand::NewJob {
            job,
            extranonce1: sub.extranonce1.clone(),
            extranonce2_size: sub.extranonce2_size,
            difficulty,
            algorithm: state.algorithm.clone(),
        })
        .await;
}

pub(crate) async fn handle_nonce_found(
    nonce_found: &NonceFound,
    client: &mut Client,
    request_id: &mut u64,
    state: &mut ManagerState,
    worker_name: &str,
    ui_events: &Option<mpsc::Sender<ManagerEvent>>,
) -> Result<()> {
    info!(
        "Nonce found! job_id={}, nonce={}, extranonce2={}, difficulty={}",
        nonce_found.job_id, nonce_found.nonce, nonce_found.extranonce2, nonce_found.difficulty
    );

    if let Err(reason) = decide_share_submit(
        &nonce_found.job_id,
        nonce_found.difficulty,
        state.current_job.as_ref().map(|job| job.job_id.as_str()),
        state.difficulty,
    ) {
        info!("Dropped share: {reason}");
        emit(
            ui_events,
            ManagerEvent::Log(format!("Dropped share: {reason}")),
        )
        .await;
        return Ok(());
    }

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
        worker_name,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MiningAlgorithm;
    use crate::miner::{MinerCommand, NonceFound};
    use crate::network::Client;
    use crate::protocol::{Job, SubscriptionResult};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    fn sample_job(id: &str) -> Job {
        Job {
            job_id: id.to_string(),
            prev_hash: "00".repeat(32),
            coinbase1: "aa".to_string(),
            coinbase2: "bb".to_string(),
            merkle_branches: Vec::new(),
            version: "20000000".to_string(),
            nbits: "1d00ffff".to_string(),
            ntime: "5f5e1000".to_string(),
            clean_jobs: false,
        }
    }

    fn subscribed_state() -> ManagerState {
        let mut state = ManagerState::default();
        state.subscription = Some(SubscriptionResult {
            subscription_id: "sub".to_string(),
            extranonce1: "00000001".to_string(),
            extranonce2_size: 4,
        });
        state.algorithm = MiningAlgorithm::Sha256d;
        state.metrics = Arc::new(crate::manager::Metrics::new());
        state
    }

    fn nonce(job_id: &str, difficulty: f64) -> NonceFound {
        NonceFound {
            job_id: job_id.to_string(),
            extranonce2: "00000001".to_string(),
            ntime: "5f5e1000".to_string(),
            nonce: "0000002a".to_string(),
            difficulty,
        }
    }

    #[tokio::test]
    async fn set_difficulty_restarts_the_active_job() {
        let mut state = subscribed_state();
        state.current_job = Some(sample_job("job-1"));
        state.difficulty = 1.0;
        let (tx, mut rx) = mpsc::channel(4);

        handle_pool_message(
            r#"{"method":"mining.set_difficulty","params":[8]}"#,
            &mut state,
            &tx,
            &None,
        )
        .await
        .unwrap();

        assert_eq!(state.difficulty, 8.0);
        match rx.try_recv().unwrap() {
            MinerCommand::NewJob {
                job,
                difficulty,
                extranonce1,
                ..
            } => {
                assert_eq!(job.job_id, "job-1");
                assert_eq!(difficulty, 8.0);
                assert_eq!(extranonce1, "00000001");
            }
            other => panic!("expected NewJob, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn same_difficulty_does_not_restart_the_job() {
        let mut state = subscribed_state();
        state.current_job = Some(sample_job("job-1"));
        state.difficulty = 8.0;
        let (tx, mut rx) = mpsc::channel(4);

        handle_pool_message(
            r#"{"method":"mining.set_difficulty","params":[8]}"#,
            &mut state,
            &tx,
            &None,
        )
        .await
        .unwrap();

        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn difficulty_before_a_job_is_stored_without_a_restart() {
        let mut state = subscribed_state();
        state.difficulty = 1.0;
        let (tx, mut rx) = mpsc::channel(4);
        let (ui_tx, mut ui_rx) = mpsc::channel(8);

        handle_pool_message(
            r#"{"method":"mining.set_difficulty","params":[32]}"#,
            &mut state,
            &tx,
            &Some(ui_tx),
        )
        .await
        .unwrap();

        assert_eq!(state.difficulty, 32.0);
        assert!(rx.try_recv().is_err());
        let mut saw = false;
        while let Ok(event) = ui_rx.try_recv() {
            if let ManagerEvent::Log(message) = event {
                if message.contains("Difficulty set to 32") {
                    saw = true;
                }
                assert!(!message.contains("restarting"));
            }
        }
        assert!(saw);
    }

    #[tokio::test]
    async fn submit_rejection_keeps_the_pool_reason() {
        let metrics = Arc::new(crate::manager::Metrics::new());
        let mut state = subscribed_state();
        state.metrics = Arc::clone(&metrics);
        state.pending_submit_ids.insert(7);
        let (tx, _rx) = mpsc::channel(2);
        let (ui_tx, mut ui_rx) = mpsc::channel(8);

        handle_pool_message(
            r#"{"id":7,"result":null,"error":[23,"Low difficulty",null]}"#,
            &mut state,
            &tx,
            &Some(ui_tx),
        )
        .await
        .unwrap();

        assert_eq!(metrics.rejected(), 1);
        assert_eq!(metrics.accepted(), 0);
        let mut reason = None;
        while let Ok(event) = ui_rx.try_recv() {
            if let ManagerEvent::ShareRejected { reason: got } = event {
                reason = Some(got);
            }
        }
        assert_eq!(reason.as_deref(), Some("Low difficulty (23)"));
    }

    #[tokio::test]
    async fn submit_result_false_is_a_rejection() {
        let metrics = Arc::new(crate::manager::Metrics::new());
        let mut state = subscribed_state();
        state.metrics = Arc::clone(&metrics);
        state.pending_submit_ids.insert(7);
        let (tx, _rx) = mpsc::channel(2);

        handle_pool_message(
            r#"{"id":7,"result":false,"error":null}"#,
            &mut state,
            &tx,
            &None,
        )
        .await
        .unwrap();

        assert_eq!(metrics.rejected(), 1);
        assert_eq!(metrics.accepted(), 0);
    }

    #[tokio::test]
    async fn submit_result_true_is_accepted() {
        let metrics = Arc::new(crate::manager::Metrics::new());
        let mut state = subscribed_state();
        state.metrics = Arc::clone(&metrics);
        state.pending_submit_ids.insert(7);
        let (tx, _rx) = mpsc::channel(2);

        handle_pool_message(
            r#"{"id":7,"result":true,"error":null}"#,
            &mut state,
            &tx,
            &None,
        )
        .await
        .unwrap();

        assert_eq!(metrics.accepted(), 1);
        assert_eq!(metrics.rejected(), 0);
    }

    #[tokio::test]
    async fn share_submit_policy_is_enforced_on_the_wire() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            socket
        });

        let mut client = Client::connect(&addr.to_string()).await.unwrap();
        let socket = server.await.unwrap();
        let (reader, _writer) = tokio::io::split(socket);
        let mut lines = BufReader::new(reader).lines();

        let mut state = subscribed_state();
        state.current_job = Some(sample_job("job-b"));
        state.difficulty = 1024.0;
        let mut request_id = 3u64;
        let (ui_tx, mut ui_rx) = mpsc::channel(8);

        handle_nonce_found(
            &nonce("job-a", 1024.0),
            &mut client,
            &mut request_id,
            &mut state,
            "worker.1",
            &Some(ui_tx.clone()),
        )
        .await
        .unwrap();
        assert_eq!(request_id, 3, "stale share must not consume a request id");
        assert!(
            timeout(Duration::from_millis(200), lines.next_line())
                .await
                .is_err(),
            "stale share was written to the pool"
        );

        handle_nonce_found(
            &nonce("job-b", 1.0),
            &mut client,
            &mut request_id,
            &mut state,
            "worker.1",
            &Some(ui_tx.clone()),
        )
        .await
        .unwrap();
        assert_eq!(
            request_id, 3,
            "too-easy share must not consume a request id"
        );
        assert!(
            timeout(Duration::from_millis(200), lines.next_line())
                .await
                .is_err(),
            "below-difficulty share was written to the pool"
        );

        handle_nonce_found(
            &nonce("job-b", 1024.0),
            &mut client,
            &mut request_id,
            &mut state,
            "worker.1",
            &Some(ui_tx),
        )
        .await
        .unwrap();
        assert_eq!(request_id, 4);

        let line = timeout(Duration::from_secs(2), lines.next_line())
            .await
            .expect("timed out waiting for submit")
            .unwrap()
            .expect("pool closed");
        let submitted: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(submitted["method"], "mining.submit");
        assert_eq!(submitted["params"][1], "job-b");
        assert_eq!(submitted["id"], 3);

        let mut dropped = Vec::new();
        while let Ok(event) = ui_rx.try_recv() {
            if let ManagerEvent::Log(message) = event {
                if message.starts_with("Dropped share:") {
                    dropped.push(message);
                }
            }
        }
        assert!(
            dropped
                .iter()
                .any(|message| message.contains("stale job job-a")),
            "{dropped:?}"
        );
        assert!(
            dropped
                .iter()
                .any(|message| message.contains("below pool difficulty")),
            "{dropped:?}"
        );
    }
}
