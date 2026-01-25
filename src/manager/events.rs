use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ManagerEvent {
    Log(String),
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
