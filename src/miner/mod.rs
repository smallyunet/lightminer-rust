//! # Miner Module
//!
//! CPU mining implementation with SHA256d hashing and nonce search.
//!
//! ## Features
//!
//! - SHA256d (double SHA256) hashing
//! - Difficulty target calculation
//! - Multi-threaded worker with job dispatch
//! - Nonce iteration and valid share detection

mod job;

use crate::manager::Metrics;
use crate::protocol::Job;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info};

pub use job::BlockHeader;

/// Commands sent from manager to miner
#[derive(Debug)]
pub enum MinerCommand {
    NewJob {
        job: Job,
        extranonce1: String,
        extranonce2_size: usize,
        difficulty: f64,
    },
    Stop,
}

/// Result sent from miner to manager when nonce is found
#[derive(Debug, Clone)]
pub struct NonceFound {
    pub job_id: String,
    pub extranonce2: String,
    pub ntime: String,
    pub nonce: String,
}

/// Perform SHA256d (double SHA256) on input bytes
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(&first);
    let mut result = [0u8; 32];
    result.copy_from_slice(&second);
    result
}

/// Convert difficulty to target bytes (256-bit big-endian)
/// For pool mining, we typically use a simplified calculation
pub fn difficulty_to_target(difficulty: f64) -> [u8; 32] {
    // Bitcoin's difficulty 1 target
    // 0x00000000FFFF0000000000000000000000000000000000000000000000000000
    let diff1_target: [u8; 32] = [
        0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];

    if difficulty <= 0.0 {
        return [0xFF; 32]; // Maximum target
    }

    // target = diff1_target / difficulty
    // For simplicity, we use floating point approximation
    let mut target = [0u8; 32];
    let scale = 1.0 / difficulty;

    // Convert diff1 target to a big integer concept and scale
    // This is a simplified version - real implementation would use big integers
    for i in 0..32 {
        let scaled = (diff1_target[i] as f64) * scale;
        target[i] = scaled.min(255.0) as u8;
    }

    target
}

/// Check if hash meets target (hash <= target)
pub fn meets_target(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    // Compare big-endian: hash should be <= target
    for i in 0..32 {
        if hash[i] < target[i] {
            return true;
        } else if hash[i] > target[i] {
            return false;
        }
    }
    true // Equal
}

/// Run the miner worker loop
pub async fn run_worker(
    mut job_rx: mpsc::Receiver<MinerCommand>,
    result_tx: mpsc::Sender<NonceFound>,
    metrics: Arc<Metrics>,
    threads: usize,
) {
    let threads = threads.max(1);

    // Epoch-based cancellation: each NewJob/Stop bumps the epoch.
    // A mining task only runs while its captured epoch matches.
    let epoch = Arc::new(AtomicU64::new(0));

    loop {
        match job_rx.recv().await {
            Some(MinerCommand::NewJob {
                job,
                extranonce1,
                extranonce2_size,
                difficulty,
            }) => {
                // Stop any previous mining and start a new epoch.
                let my_epoch = epoch.fetch_add(1, Ordering::SeqCst).wrapping_add(1);

                info!(
                    "Starting mining on job {} with difficulty {}",
                    job.job_id, difficulty
                );

                let result_tx_clone = result_tx.clone();
                let metrics_clone = Arc::clone(&metrics);

                for thread_idx in 0..threads {
                    let job = job.clone();
                    let extranonce1 = extranonce1.clone();
                    let result_tx = result_tx_clone.clone();
                    let metrics = Arc::clone(&metrics_clone);
                    let epoch = Arc::clone(&epoch);

                    tokio::task::spawn_blocking(move || {
                        mine_job(
                            &job,
                            &extranonce1,
                            extranonce2_size,
                            difficulty,
                            epoch,
                            my_epoch,
                            result_tx,
                            metrics,
                            thread_idx as u32,
                            threads as u32,
                        );
                    });
                }
            }
            Some(MinerCommand::Stop) => {
                let _ = epoch.fetch_add(1, Ordering::SeqCst);
                debug!("Mining stopped");
            }
            None => {
                info!("Miner channel closed, exiting worker");
                break;
            }
        }
    }
}

fn mine_job(
    job: &Job,
    extranonce1: &str,
    extranonce2_size: usize,
    difficulty: f64,
    epoch: Arc<AtomicU64>,
    my_epoch: u64,
    result_tx: mpsc::Sender<NonceFound>,
    metrics: Arc<Metrics>,
    nonce_start: u32,
    nonce_step: u32,
) {
    let target = difficulty_to_target(difficulty);

    // Generate extranonce2 (incrementing counter)
    let mut extranonce2_counter: u64 = 0;

    // Build block header template
    let header_template = match job::build_header_template(job) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("Failed to build header template: {}", e);
            return;
        }
    };

    // Compute merkle root once per extranonce2 (it does NOT depend on nonce).
    let mut extranonce2 = format!(
        "{:0width$x}",
        extranonce2_counter,
        width = extranonce2_size * 2
    );
    let mut merkle_root = match job::compute_merkle_root_for(job, extranonce1, &extranonce2) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to compute merkle root: {}", e);
            return;
        }
    };

    let mut nonce: u32 = nonce_start;
    let mut pending_hashes: u64 = 0;
    let mut last_flush = Instant::now();

    while epoch.load(Ordering::Relaxed) == my_epoch {
        // Assemble header (fast path). Merkle root is constant until extranonce2 changes.
        let header = job::assemble_header(&header_template, &merkle_root, nonce);

        // Compute SHA256d
        let hash = sha256d(&header);
        pending_hashes += 1;

        // Check if hash meets target
        if meets_target(&hash, &target) {
            info!("🎉 Nonce Found! nonce={:08x}", nonce);

            let result = NonceFound {
                job_id: job.job_id.clone(),
                extranonce2: extranonce2.clone(),
                ntime: job.ntime.clone(),
                nonce: format!("{:08x}", nonce),
            };

            // Send result to manager (blocking in sync context)
            let _ = result_tx.blocking_send(result);
        }

        // Flush metrics on a short time interval so UI/log mode doesn't stay at 0 for a long time.
        if last_flush.elapsed() >= Duration::from_millis(250) {
            if pending_hashes > 0 {
                metrics.add_hashes(pending_hashes);
                pending_hashes = 0;
            }
            last_flush = Instant::now();
        }

        // Increment nonce across threads.
        let next = nonce.wrapping_add(nonce_step);
        let wrapped = next < nonce;
        nonce = next;

        // If nonce wraps around (for this stride), increment extranonce2 and recompute merkle root.
        if wrapped {
            extranonce2_counter += 1;
            extranonce2 = format!(
                "{:0width$x}",
                extranonce2_counter,
                width = extranonce2_size * 2
            );
            if let Ok(m) = job::compute_merkle_root_for(job, extranonce1, &extranonce2) {
                merkle_root = m;
            }
        }
    }

    // Final metrics update
    if pending_hashes > 0 {
        metrics.add_hashes(pending_hashes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256d() {
        // Test vector: SHA256d of empty string
        let result = sha256d(b"");
        // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        // SHA256(above) = 5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456
        assert_eq!(
            hex::encode(result),
            "5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456"
        );
    }

    #[test]
    fn test_meets_target() {
        let hash = [0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let target = [0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(meets_target(&hash, &target));

        let hash_higher = [0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                          0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                          0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(!meets_target(&hash_higher, &target));
    }
}
