use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Metrics tracker for mining statistics
pub struct Metrics {
    accepted: AtomicU64,
    rejected: AtomicU64,
    hashes: AtomicU64,
    start_time: Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            accepted: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            hashes: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn add_accepted(&self) {
        self.accepted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_rejected(&self) {
        self.rejected.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_hashes(&self, count: u64) {
        self.hashes.fetch_add(count, Ordering::Relaxed);
    }

    pub fn accepted(&self) -> u64 {
        self.accepted.load(Ordering::Relaxed)
    }

    pub fn rejected(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }

    pub fn hashes(&self) -> u64 {
        self.hashes.load(Ordering::Relaxed)
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Calculate hashrate in H/s
    pub fn hashrate(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.hashes() as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Format hashrate with appropriate unit
    pub fn hashrate_formatted(&self) -> String {
        let hr = self.hashrate();
        if hr >= 1_000_000_000.0 {
            format!("{:.2} GH/s", hr / 1_000_000_000.0)
        } else if hr >= 1_000_000.0 {
            format!("{:.2} MH/s", hr / 1_000_000.0)
        } else if hr >= 1_000.0 {
            format!("{:.2} KH/s", hr / 1_000.0)
        } else {
            format!("{:.2} H/s", hr)
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
