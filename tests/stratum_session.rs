//! End-to-end Stratum session against a local fake pool.
//!
//! These tests drive `run_with_config` the same way the binary does: TCP
//! handshake, difficulty updates, share submission, and pool rejections.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lightminer_rust::config::{Coin, Config, MiningAlgorithm, PoolConfig, PoolStrategy};
use lightminer_rust::manager::{run_with_config, ManagerEvent, Metrics};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::timeout;

fn config_for(addr: &str, algo: MiningAlgorithm) -> Config {
    Config {
        pool_addr: addr.to_string(),
        worker_name: "worker.1".to_string(),
        worker_password: "x".to_string(),
        pools: vec![PoolConfig {
            name: "local".to_string(),
            addr: addr.to_string(),
            user: "worker.1".to_string(),
            pass: "x".to_string(),
            coin: Coin::Btc,
            algo,
            weight: 1,
        }],
        pool_strategy: PoolStrategy::Failover,
        pool_failures_before_cooldown: 3,
        pool_failure_cooldown_secs: 30,
        agent: "LightMiner-Rust/test".to_string(),
        use_tui: false,
        reconnect: false,
        reconnect_max_delay_ms: 1_000,
        miner_threads: 1,
        handshake_timeout_ms: 3_000,
        idle_timeout_secs: 30,
    }
}

fn notify(job_id: &str) -> String {
    json!({
        "method": "mining.notify",
        "params": [
            job_id,
            "0000000000000000000000000000000000000000000000000000000000000000",
            "aa",
            "bb",
            [],
            "20000000",
            "1d00ffff",
            "5f5e1000",
            true
        ]
    })
    .to_string()
}

async fn write_line(writer: &mut (impl AsyncWriteExt + Unpin), line: &str) {
    writer.write_all(line.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
}

async fn read_line(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<TcpStream>>>,
) -> String {
    timeout(Duration::from_secs(5), lines.next_line())
        .await
        .expect("timed out waiting for a miner message")
        .expect("read failed")
        .expect("miner closed the connection")
}

async fn handshake(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<TcpStream>>>,
    writer: &mut tokio::io::WriteHalf<TcpStream>,
) {
    let subscribe = read_line(lines).await;
    assert!(
        subscribe.contains("mining.subscribe"),
        "expected subscribe, got {subscribe}"
    );
    write_line(
        writer,
        r#"{"id":1,"result":[["mining.notify","ae6812eb"],"00000001",4],"error":null}"#,
    )
    .await;

    let authorize = read_line(lines).await;
    assert!(
        authorize.contains("mining.authorize"),
        "expected authorize, got {authorize}"
    );
    write_line(writer, r#"{"id":2,"result":true,"error":null}"#).await;
}

async fn drain_until_closed(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<TcpStream>>>,
) {
    loop {
        match lines.next_line().await {
            Ok(Some(_)) => {}
            _ => break,
        }
    }
}

#[tokio::test]
async fn unsupported_algorithm_refuses_to_start() {
    let metrics = Arc::new(Metrics::new());
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let err = run_with_config(
        config_for(
            "127.0.0.1:1",
            MiningAlgorithm::Other("equihash".to_string()),
        ),
        metrics,
        None,
        None,
        shutdown_rx,
    )
    .await
    .expect_err("unknown algorithms must not open a session");

    let message = err.to_string();
    assert!(message.contains("equihash"), "{message}");
    assert!(message.contains("sha256d"), "{message}");
    assert!(message.contains("scrypt"), "{message}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn difficulty_change_restarts_the_active_job() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (ready_tx, ready_rx) = oneshot::channel::<()>();

    let pool = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = tokio::io::split(socket);
        let mut lines = BufReader::new(reader).lines();
        handshake(&mut lines, &mut writer).await;
        write_line(
            &mut writer,
            r#"{"method":"mining.set_difficulty","params":[1000]}"#,
        )
        .await;
        write_line(&mut writer, &notify("job-1")).await;

        let _ = timeout(Duration::from_secs(5), ready_rx).await;
        write_line(
            &mut writer,
            r#"{"method":"mining.set_difficulty","params":[2000]}"#,
        )
        .await;
        drain_until_closed(&mut lines).await;
    });

    let (event_tx, mut event_rx) = mpsc::channel(256);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let manager = tokio::spawn(run_with_config(
        config_for(&addr, MiningAlgorithm::Sha256d),
        Arc::new(Metrics::new()),
        Some(event_tx),
        None,
        shutdown_rx,
    ));

    let mut ready_tx = Some(ready_tx);
    let mut logs = Vec::new();
    let mut restarted = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline && !restarted {
        match timeout(Duration::from_millis(200), event_rx.recv()).await {
            Ok(Some(ManagerEvent::Log(message))) => {
                if message.contains("New job: job-1") {
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(());
                    }
                }
                if message.contains("restarting job job-1") {
                    restarted = true;
                }
                logs.push(message);
            }
            Ok(Some(_)) => {}
            _ => {}
        }
    }

    drop(event_rx);
    let _ = shutdown_tx.send(true);
    let manager_result = timeout(Duration::from_secs(3), manager)
        .await
        .expect("manager did not exit after shutdown")
        .expect("manager task panicked");
    assert!(manager_result.is_ok(), "{manager_result:?}");
    let _ = timeout(Duration::from_secs(3), pool).await;
    assert!(
        restarted,
        "pool difficulty change did not restart the active job; logs={logs:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pool_reject_reason_is_reported_for_a_submitted_share() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let pool = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = tokio::io::split(socket);
        let mut lines = BufReader::new(reader).lines();
        handshake(&mut lines, &mut writer).await;
        write_line(
            &mut writer,
            r#"{"method":"mining.set_difficulty","params":[0.00000001]}"#,
        )
        .await;
        write_line(&mut writer, &notify("job-a")).await;

        let mut rejected = false;
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if rejected || !line.contains("mining.submit") {
                        continue;
                    }
                    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                    assert_eq!(
                        value["params"][1], "job-a",
                        "submitted the wrong job: {line}"
                    );
                    let body = json!({
                        "id": value["id"],
                        "result": false,
                        "error": [23, "Low difficulty", null]
                    });
                    write_line(&mut writer, &body.to_string()).await;
                    rejected = true;
                }
                _ => break,
            }
        }
    });

    let (event_tx, mut event_rx) = mpsc::channel(256);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let metrics = Arc::new(Metrics::new());
    let manager = tokio::spawn(run_with_config(
        config_for(&addr, MiningAlgorithm::Sha256d),
        Arc::clone(&metrics),
        Some(event_tx),
        None,
        shutdown_rx,
    ));

    let mut logs = Vec::new();
    let mut reason = None;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline && reason.is_none() {
        match timeout(Duration::from_millis(200), event_rx.recv()).await {
            Ok(Some(ManagerEvent::ShareRejected { reason: got })) => {
                reason = Some(got);
            }
            Ok(Some(ManagerEvent::Log(message))) => logs.push(message),
            Ok(Some(_)) => {}
            _ => {}
        }
    }

    drop(event_rx);
    let _ = shutdown_tx.send(true);
    let manager_result = timeout(Duration::from_secs(3), manager)
        .await
        .expect("manager did not exit after shutdown")
        .expect("manager task panicked");
    assert!(manager_result.is_ok(), "{manager_result:?}");
    let _ = timeout(Duration::from_secs(3), pool).await;

    assert_eq!(
        reason.as_deref(),
        Some("Low difficulty (23)"),
        "logs={logs:?}"
    );
    assert_eq!(metrics.rejected(), 1);
    assert_eq!(metrics.accepted(), 0);
}
