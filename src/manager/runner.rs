use super::handshake::connect_and_handshake;
use super::session::{dispatch_job, handle_nonce_found, handle_pool_message};
use super::{ManagerCommand, ManagerEvent, Metrics, PoolStatus};
use crate::config::{Config, PoolStrategy};
use crate::miner::{MinerCommand, NonceFound};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

use super::events::emit;

struct PoolRuntime {
    pool: crate::config::PoolConfig,
    failures: u32,
    cooldown_until: Option<tokio::time::Instant>,
    disabled: bool,
}

impl PoolRuntime {
    fn is_available(&self) -> bool {
        if self.disabled {
            return false;
        }
        match self.cooldown_until {
            None => true,
            Some(t) => tokio::time::Instant::now() >= t,
        }
    }

    fn cooldown_remaining(&self) -> Option<std::time::Duration> {
        self.cooldown_until
            .and_then(|t| t.checked_duration_since(tokio::time::Instant::now()))
    }
}

struct PoolPicker {
    strategy: PoolStrategy,
    schedule: Vec<usize>,
    cursor: usize,
    active_idx: Option<usize>,
}

impl PoolPicker {
    fn new(strategy: PoolStrategy, pools: &[crate::config::PoolConfig]) -> Self {
        let mut schedule = Vec::new();
        match strategy {
            PoolStrategy::Weighted => {
                // Weighted round-robin schedule.
                // Cap total slots to avoid pathological large weights.
                let mut total_slots: usize = 0;
                for (i, p) in pools.iter().enumerate() {
                    let w = p.weight.max(1) as usize;
                    total_slots = total_slots.saturating_add(w);
                    if total_slots > 128 {
                        break;
                    }
                    for _ in 0..w {
                        schedule.push(i);
                    }
                }
                if schedule.is_empty() {
                    schedule.extend(0..pools.len());
                }
            }
            PoolStrategy::Failover | PoolStrategy::RoundRobin => {
                schedule.extend(0..pools.len());
            }
        }

        if schedule.is_empty() {
            schedule.push(0);
        }

        Self {
            strategy,
            schedule,
            cursor: 0,
            active_idx: None,
        }
    }

    fn pick_next(&mut self, pools: &[PoolRuntime]) -> Option<usize> {
        if pools.is_empty() {
            return None;
        }

        if self.strategy == PoolStrategy::Failover {
            if let Some(i) = self.active_idx {
                if pools.get(i).map(|p| p.is_available()).unwrap_or(false) {
                    return Some(i);
                }
            }
        }

        let tries = self.schedule.len().max(1);
        for _ in 0..tries {
            let idx = self.schedule[self.cursor % self.schedule.len()];
            self.cursor = self.cursor.wrapping_add(1);
            if pools.get(idx).map(|p| p.is_available()).unwrap_or(false) {
                return Some(idx);
            }
        }

        None
    }

    fn pick_relative(&mut self, pools: &[PoolRuntime], from: usize, step: isize) -> Option<usize> {
        if pools.is_empty() || self.schedule.is_empty() {
            return None;
        }

        // Find the first position in the schedule that maps to `from`.
        let Some(mut pos) = self.schedule.iter().position(|i| *i == from) else {
            return None;
        };

        let len = self.schedule.len() as isize;
        for _ in 0..self.schedule.len() {
            pos = (pos as isize + step).rem_euclid(len) as usize;
            let idx = self.schedule[pos];
            if pools.get(idx).map(|p| p.is_available()).unwrap_or(false) {
                // Move cursor to start after the chosen slot.
                self.cursor = (pos + 1) % self.schedule.len();
                return Some(idx);
            }
        }
        None
    }
}

fn snapshot_pools_status(pools: &[PoolRuntime]) -> Vec<PoolStatus> {
    pools
        .iter()
        .map(|p| PoolStatus {
            name: p.pool.name.clone(),
            addr: p.pool.addr.clone(),
            coin: p.pool.coin.symbol().to_string(),
            algo: p.pool.algo.name().to_string(),
            disabled: p.disabled,
            cooldown_secs_remaining: p
                .cooldown_remaining()
                .map(|d| d.as_secs().max(1)),
            failures: p.failures,
        })
        .collect()
}

pub async fn run_with_config(
    config: Config,
    metrics: Arc<Metrics>,
    ui_events: Option<mpsc::Sender<ManagerEvent>>,
    mut command_rx: Option<mpsc::Receiver<ManagerCommand>>,
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

    let total_pools = config.pools.len().max(1);
    let mut pool_runtimes: Vec<PoolRuntime> = config
        .pools
        .iter()
        .cloned()
        .map(|p| PoolRuntime {
            pool: p,
            failures: 0,
            cooldown_until: None,
            disabled: false,
        })
        .collect();
    let mut picker = PoolPicker::new(config.pool_strategy, &config.pools);

    emit(
        &ui_events,
        ManagerEvent::PoolsStatus(snapshot_pools_status(&pool_runtimes)),
    )
    .await;

    // Manual override index set by UI commands.
    let mut manual_override: Option<usize> = None;

    loop {
        if *shutdown_rx.borrow() {
            emit(&ui_events, ManagerEvent::Log("Shutdown requested".to_string())).await;
            break;
        }

        let Some(pool_idx) = (if let Some(i) = manual_override.take() {
            if pool_runtimes.get(i).map(|p| p.is_available()).unwrap_or(false) {
                Some(i)
            } else {
                None
            }
        } else {
            picker.pick_next(&pool_runtimes)
        }) else {
            // All pools are on cooldown. Sleep until the earliest cooldown expires.
            let min_remaining = pool_runtimes
                .iter()
                .filter_map(|p| p.cooldown_remaining())
                .min();

            let sleep_for = min_remaining.unwrap_or_else(|| std::time::Duration::from_millis(500));
            emit(
                &ui_events,
                ManagerEvent::Log(format!(
                    "All pools are cooling down; retrying in {}s...",
                    sleep_for.as_secs_f64()
                )),
            )
            .await;
            tokio::select! {
                _ = tokio::time::sleep(sleep_for) => {}
                _ = shutdown_rx.changed() => {}
            }
            continue;
        };

        let active_pool = pool_runtimes[pool_idx].pool.clone();
        emit(
            &ui_events,
            ManagerEvent::ActivePool {
                name: active_pool.name.clone(),
                addr: active_pool.addr.clone(),
                coin: active_pool.coin.symbol().to_string(),
                algo: active_pool.algo.name().to_string(),
                index: pool_idx + 1,
                total: total_pools,
            },
        )
        .await;

        emit(
            &ui_events,
            ManagerEvent::PoolsStatus(snapshot_pools_status(&pool_runtimes)),
        )
        .await;

        let session =
            connect_and_handshake(&config, &active_pool, Arc::clone(&metrics), &ui_events).await;
        let (mut client, mut state, mut request_id, pending_job) = match session {
            Ok(v) => {
                backoff_ms = 500;
                pool_runtimes[pool_idx].failures = 0;
                pool_runtimes[pool_idx].cooldown_until = None;
                picker.active_idx = Some(pool_idx);
                emit(
                    &ui_events,
                    ManagerEvent::PoolsStatus(snapshot_pools_status(&pool_runtimes)),
                )
                .await;
                v
            }
            Err(e) => {
                // Track failures and cooldown per pool.
                pool_runtimes[pool_idx].failures = pool_runtimes[pool_idx].failures.saturating_add(1);
                if pool_runtimes[pool_idx].failures >= config.pool_failures_before_cooldown {
                    pool_runtimes[pool_idx].failures = 0;
                    pool_runtimes[pool_idx].cooldown_until = Some(
                        tokio::time::Instant::now()
                            + std::time::Duration::from_secs(config.pool_failure_cooldown_secs.max(1)),
                    );
                    emit(
                        &ui_events,
                        ManagerEvent::Log(format!(
                            "Pool {} entered cooldown for {}s",
                            active_pool.name,
                            config.pool_failure_cooldown_secs.max(1)
                        )),
                    )
                    .await;
                }

                emit(
                    &ui_events,
                    ManagerEvent::PoolsStatus(snapshot_pools_status(&pool_runtimes)),
                )
                .await;

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
                maybe_cmd = async {
                    match command_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => None,
                    }
                } => {
                    if let Some(cmd) = maybe_cmd {
                        match cmd {
                            ManagerCommand::SwitchPoolNext => {
                                if let Some(active) = picker.active_idx {
                                    if let Some(next) = picker.pick_relative(&pool_runtimes, active, 1) {
                                        manual_override = Some(next);
                                        emit(&ui_events, ManagerEvent::Log("Switching to next pool...".to_string())).await;
                                        session_ended = true;
                                    }
                                }
                            }
                            ManagerCommand::SwitchPoolPrev => {
                                if let Some(active) = picker.active_idx {
                                    if let Some(prev) = picker.pick_relative(&pool_runtimes, active, -1) {
                                        manual_override = Some(prev);
                                        emit(&ui_events, ManagerEvent::Log("Switching to previous pool...".to_string())).await;
                                        session_ended = true;
                                    }
                                }
                            }
                            ManagerCommand::ToggleDisableActivePool => {
                                if let Some(active) = picker.active_idx {
                                    let new_disabled = !pool_runtimes[active].disabled;
                                    pool_runtimes[active].disabled = new_disabled;
                                    let msg = if new_disabled { "Disabled current pool" } else { "Enabled current pool" };
                                    emit(&ui_events, ManagerEvent::Log(msg.to_string())).await;
                                    emit(
                                        &ui_events,
                                        ManagerEvent::PoolsStatus(snapshot_pools_status(&pool_runtimes)),
                                    )
                                    .await;

                                    if new_disabled {
                                        // Force reconnect away from disabled pool.
                                        picker.active_idx = None;
                                        session_ended = true;
                                    }
                                }
                            }
                        }
                    }
                }
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
                    handle_nonce_found(&nonce_found, &mut client, &mut request_id, &mut state, &active_pool.user, &ui_events).await?;
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
