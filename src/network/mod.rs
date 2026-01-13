//! # Network Module
//!
//! Async TCP client for Stratum pool communication using tokio.
//!
//! Provides a simple line-based JSON streaming interface for the Stratum protocol.

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::{tcp::{OwnedReadHalf, OwnedWriteHalf}, TcpStream};
use tracing::{debug, info};

pub struct Client {
    reader: Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
}

impl Client {
    /// Connect to a Stratum server (e.g., "stratum.aikapool.com:7915")
    pub async fn connect(addr: &str) -> Result<Self> {
        info!("Connecting to {}...", addr);
        let stream = TcpStream::connect(addr)
            .await
            .context("Failed to connect to pool")?;

        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half).lines();

        info!("Connected to {}", addr);
        Ok(Self {
            reader,
            writer: write_half,
        })
    }

    /// Send a raw JSON string (appends newline automatically)
    pub async fn send(&mut self, json_payload: &str) -> Result<()> {
        debug!("Sending: {}", json_payload);
        self.writer
            .write_all(json_payload.as_bytes())
            .await
            .context("Failed to write to socket")?;
        
        // Stratum requires newline delimiter
        self.writer
            .write_u8(b'\n')
            .await
            .context("Failed to write newline")?;
            
        Ok(())
    }

    /// Wait for the next message (line) from the server
    pub async fn next_message(&mut self) -> Result<Option<String>> {
        let line = self.reader.next_line().await.context("Failed to read line")?;
        if let Some(ref l) = line {
            debug!("Received: {}", l);
        }
        Ok(line)
    }
}
