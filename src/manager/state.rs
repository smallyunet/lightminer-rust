use super::Metrics;
use crate::protocol::{Job, SubscriptionResult};
use std::collections::HashSet;
use std::sync::Arc;

pub struct ManagerState {
    pub subscription: Option<SubscriptionResult>,
    pub current_job: Option<Job>,
    pub difficulty: f64,
    pub metrics: Arc<Metrics>,
    pub(crate) pending_submit_ids: HashSet<u64>,
    pub(crate) authorize_request_id: Option<u64>,
}

impl Default for ManagerState {
    fn default() -> Self {
        Self {
            subscription: None,
            current_job: None,
            difficulty: 1.0,
            metrics: Arc::new(Metrics::new()),
            pending_submit_ids: HashSet::new(),
            authorize_request_id: None,
        }
    }
}
