use super::handshake::connect_and_handshake;
use super::session::{dispatch_job, handle_nonce_found, handle_pool_message};
use super::{ManagerEvent, Metrics};
use crate::config::Config;
use crate::miner::{MinerCommand, NonceFound};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

use super::events::emit;

pub async fn run_with_config(
    config: Config,
    metrics: Arc<Metrics>,
    ui_events: Option<mpsc::Sender<ManagerEvent>>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    // Create channels for miner communication
    let (miner_tx, mut miner_rx) = mpsc::channel::<NonceFound>(32);
    let (job_tx, job_rx) = mpsc::channel::<MinerCommand>(32);

    // Start the miner worker once; job cancellation is epoch-based.
    let miner_metrics = Arc::clone(&metrics);
    let miner_threads = config.miner_threads;
    let miner_handle = tokio::spawn(async move {
        crate::miner::run_worker(job_rx, miner_tx, miner_metrics, miner_threads).await
    });

    let mut backoff_ms: u64 = 500;
    let max_backoff_ms = config.reconnect_max_delay_ms.max(500);

    loop {
        if *shutdown_rx.borrow() {
            emit(&ui_events, ManagerEvent::Log("Shutdown requested".to_string())).await;
            break;
        }

        let session = connect_and_handshake(&config, Arc::clone(&metrics), &ui_events).await;
        let (mut client, mut state, mut request_id, pending_job) = match session {
            Ok(v) => {
                backoff_ms = 500;
                v
            }
            Err(e) => {
                emit(&ui_events, ManagerEvent::Connected(false)).await;
                emit(&ui_events, ManagerEvent::ProxyInfo(None)).await;
                emit(&ui_events, ManagerEvent::CurrentJob(None)).await;
                emit(
                    &ui_events,
                    ManagerEvent::Log(format!("Connect/handshake failed: {e:?}")),
                )
                .await;

                if !config.reconnect {
                    break;
                }

                let wait_ms = backoff_ms;
                emit(
                    &ui_events,
                    ManagerEvent::Log(format!("Reconnecting in {wait_ms}ms...")),
                )
                .await;
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(wait_ms)) => {}
                    _ = shutdown_rx.changed() => {}
                }
                backoff_ms = (backoff_ms.saturating_mul(2)).min(max_backoff_ms);
                continue;
            }
        };

        // If we received a job notification during the handshake (before subscribe completed),
        // dispatch it now so the miner doesn't wait indefinitely.
        if let Some(job) = pending_job {
            dispatch_job(&job, &mut state, &job_tx, &ui_events).await;
        }

        info!("Entering session event loop...");
        let mut last_rx = Instant::now();
        let mut session_ended = false;
        while !session_ended {
            // If the connection is stuck (no inbound messages), force a reconnect.
            let idle_timeout = std::time::Duration::from_secs(config.idle_timeout_secs.max(1));
            let elapsed = last_rx.elapsed();
            let remaining = idle_timeout.saturating_sub(elapsed);
            let idle_sleep = tokio::time::sleep(remaining);
            tokio::pin!(idle_sleep);

            tokio::select! {
                shutdown_result = shutdown_rx.changed() => {
                    match shutdown_result {
                        Ok(()) => {
                            if *shutdown_rx.borrow() {
                                emit(&ui_events, ManagerEvent::Log("Shutdown requested".to_string())).await;
                                session_ended = true;
                            }
                        }
                        Err(_) => {
                            session_ended = true;
                        }
                    }
                }
                _ = &mut idle_sleep => {
                    warn!("Idle timeout reached ({}s) - reconnecting", config.idle_timeout_secs);
                    emit(&ui_events, ManagerEvent::Connected(false)).await;
                    emit(&ui_events, ManagerEvent::Log(format!("Idle timeout ({}s) - reconnecting", config.idle_timeout_secs))).await;
                    session_ended = true;
                }
                msg_result = client.next_message() => {
                    match msg_result {
                        Ok(Some(line)) => {
                            last_rx = Instant::now();
                            handle_pool_message(&line, &mut state, &job_tx, &ui_events).await?;
                        }
                        Ok(None) => {
                            warn!("Connection closed by server.");
                            emit(&ui_events, ManagerEvent::Connected(false)).await;
                            emit(&ui_events, ManagerEvent::Log("Disconnected".to_string())).await;
                            session_ended = true;
                        }
                        Err(e) => {
                            error!("Network error: {:?}", e);
                            emit(&ui_events, ManagerEvent::Connected(false)).await;
                            emit(&ui_events, ManagerEvent::Log(format!("Network error: {e:?}"))).await;
                            session_ended = true;
                        }
                    }
                }
                Some(nonce_found) = miner_rx.recv() => {
                    handle_nonce_found(&nonce_found, &mut client, &mut request_id, &mut state, &config, &ui_events).await?;
                }
            }
        }

        // Stop miner work for the old session.
        let _ = job_tx.send(MinerCommand::Stop).await;
        emit(&ui_events, ManagerEvent::CurrentJob(None)).await;

        if !config.reconnect || *shutdown_rx.borrow() {
            break;
        }

        let wait_ms = backoff_ms;
        emit(
            &ui_events,
            ManagerEvent::Log(format!("Reconnecting in {wait_ms}ms...")),
        )
        .await;
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(wait_ms)) => {}
            _ = shutdown_rx.changed() => {}
        }
        backoff_ms = (backoff_ms.saturating_mul(2)).min(max_backoff_ms);
    }

    // Cleanup
    let _ = job_tx.send(MinerCommand::Stop).await;
    drop(job_tx);
    let _ = miner_handle.await;

    Ok(())
}
