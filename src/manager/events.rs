use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub name: String,
    pub addr: String,
    pub coin: String,
    pub algo: String,
    pub disabled: bool,
    pub cooldown_secs_remaining: Option<u64>,
    pub failures: u32,
}

#[derive(Debug, Clone)]
pub enum ManagerEvent {
    Log(String),
    PoolsStatus(Vec<PoolStatus>),
    ActivePool {
        name: String,
        addr: String,
        coin: String,
        algo: String,
        index: usize,
        total: usize,
    },
    Connected(bool),
    Authorized(bool),
    ProxyInfo(Option<String>),
    Difficulty(f64),
    CurrentJob(Option<String>),
    ShareAccepted,
    ShareRejected,
}

pub(crate) async fn emit(ui_events: &Option<mpsc::Sender<ManagerEvent>>, event: ManagerEvent) {
    if let Some(tx) = ui_events {
        let _ = tx.send(event).await;
    }
}
