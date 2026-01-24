//! # Network Module
//!
//! Async TCP client for Stratum pool communication using tokio.
//!
//! Provides a simple line-based JSON streaming interface for the Stratum protocol.

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines};
use tokio::net::TcpStream;
use tracing::{debug, info};

mod proxy;

trait AsyncReadWrite: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite + ?Sized> AsyncReadWrite for T {}

type DynStream = Box<dyn AsyncReadWrite + Unpin + Send>;

struct PrebufferedStream {
    prefix: std::io::Cursor<Vec<u8>>,
    inner: TcpStream,
}

impl PrebufferedStream {
    fn new(prefix: Vec<u8>, inner: TcpStream) -> Self {
        Self {
            prefix: std::io::Cursor::new(prefix),
            inner,
        }
    }
}

impl AsyncRead for PrebufferedStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if (self.prefix.position() as usize) < self.prefix.get_ref().len() {
            let remaining = self.prefix.get_ref().len() - (self.prefix.position() as usize);
            let to_copy = remaining.min(buf.remaining());
            let start = self.prefix.position() as usize;
            let end = start + to_copy;
            buf.put_slice(&self.prefix.get_ref()[start..end]);
            self.prefix.set_position(end as u64);
            return std::task::Poll::Ready(Ok(()));
        }

        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrebufferedStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        data: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, data)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub struct Client {
    reader: Lines<BufReader<tokio::io::ReadHalf<DynStream>>>,
    writer: tokio::io::WriteHalf<DynStream>,
}

impl Client {
    /// Connect to a Stratum server (e.g., "stratum.aikapool.com:7915")
    pub async fn connect(addr: &str) -> Result<Self> {
        let (client, _proxy) = Self::connect_with_proxy_info(addr).await?;
        Ok(client)
    }

    /// Connect and also return proxy info (for UI/logging).
    pub async fn connect_with_proxy_info(addr: &str) -> Result<(Self, Option<String>)> {
        info!("Connecting to {}...", addr);

        let proxy_cfg = proxy::detect_proxy();
        let (stream, proxy_display) = match proxy_cfg {
            Some(cfg) => {
                let proxy_str = cfg.display();
                info!("Using proxy: {}", proxy_str);
                (connect_via_proxy(&cfg, addr).await?, Some(proxy_str))
            }
            None => {
                let tcp = TcpStream::connect(addr)
                    .await
                    .context("Failed to connect to pool")?;
                (Box::new(tcp) as DynStream, None)
            }
        };

        let (read_half, write_half) = tokio::io::split(stream);
        let reader = BufReader::new(read_half).lines();

        info!("Connected to {}", addr);
        Ok((
            Self {
                reader,
                writer: write_half,
            },
            proxy_display,
        ))
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

async fn connect_via_proxy(cfg: &proxy::ProxyConfig, addr: &str) -> Result<DynStream> {
    match cfg.kind {
        proxy::ProxyKind::Socks5 => connect_via_socks5(cfg, addr).await,
        proxy::ProxyKind::Http => connect_via_http_connect(cfg, addr).await,
    }
}

async fn connect_via_socks5(cfg: &proxy::ProxyConfig, addr: &str) -> Result<DynStream> {
    use tokio_socks::tcp::Socks5Stream;

    let proxy_addr = format!("{}:{}", cfg.host, cfg.port);

    let stream = match (&cfg.username, &cfg.password) {
        (Some(user), Some(pass)) => {
            Socks5Stream::connect_with_password(proxy_addr.as_str(), addr, user, pass)
                .await
                .context("Failed to connect via SOCKS5 proxy")?
                .into_inner()
        }
        _ => {
            Socks5Stream::connect(proxy_addr.as_str(), addr)
                .await
                .context("Failed to connect via SOCKS5 proxy")?
                .into_inner()
        }
    };

    Ok(Box::new(stream) as DynStream)
}

async fn connect_via_http_connect(cfg: &proxy::ProxyConfig, addr: &str) -> Result<DynStream> {
    use tokio::io::AsyncReadExt;

    let proxy_addr = format!("{}:{}", cfg.host, cfg.port);
    let mut tcp = TcpStream::connect(&proxy_addr)
        .await
        .with_context(|| format!("Failed to connect to HTTP proxy {}", proxy_addr))?;

    // Minimal HTTP CONNECT tunnel.
    let mut req = format!(
        "CONNECT {addr} HTTP/1.1\r\nHost: {addr}\r\nProxy-Connection: Keep-Alive\r\n"
    );

    // Basic auth if provided.
    if let (Some(user), Some(pass)) = (&cfg.username, &cfg.password) {
        let token = base64_basic(user, pass);
        req.push_str(&format!("Proxy-Authorization: Basic {token}\r\n"));
    }

    req.push_str("\r\n");
    tcp.write_all(req.as_bytes())
        .await
        .context("Failed to write CONNECT request")?;

    // Read response headers without losing any extra bytes.
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let header_end = loop {
        let mut chunk = [0u8; 512];
        let n = tcp.read(&mut chunk).await.context("Failed to read CONNECT response")?;
        if n == 0 {
            anyhow::bail!("HTTP proxy closed connection during CONNECT");
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_double_crlf(&buf) {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            anyhow::bail!("HTTP proxy CONNECT response too large");
        }
    };

    let status_line = first_status_line(&buf[..header_end])
        .unwrap_or_else(|| "".to_string());
    let status_code = parse_http_status_code(&status_line).unwrap_or(0);
    if status_code != 200 {
        anyhow::bail!("HTTP CONNECT failed: {status_line}");
    }

    let remainder = buf[(header_end + 4)..].to_vec();
    let stream = PrebufferedStream::new(remainder, tcp);
    Ok(Box::new(stream) as DynStream)
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
}

fn first_status_line(header: &[u8]) -> Option<String> {
    let mut i = 0;
    while i + 1 < header.len() {
        if header[i] == b'\r' && header[i + 1] == b'\n' {
            let line = &header[..i];
            return Some(String::from_utf8_lossy(line).to_string());
        }
        i += 1;
    }
    None
}

fn parse_http_status_code(status_line: &str) -> Option<u16> {
    let mut parts = status_line.split_whitespace();
    let _proto = parts.next()?;
    let code = parts.next()?;
    code.parse::<u16>().ok()
}

fn base64_basic(user: &str, pass: &str) -> String {
    // Minimal base64 for ASCII credentials.
    // If non-ASCII is needed later, we can switch to a base64 crate.
    base64_encode(format!("{user}:{pass}").as_bytes())
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        let n0 = b0 >> 2;
        let n1 = ((b0 & 0b0000_0011) << 4) | (b1 >> 4);
        let n2 = ((b1 & 0b0000_1111) << 2) | (b2 >> 6);
        let n3 = b2 & 0b0011_1111;

        out.push(TABLE[n0 as usize] as char);
        out.push(TABLE[n1 as usize] as char);

        if i + 1 < data.len() {
            out.push(TABLE[n2 as usize] as char);
        } else {
            out.push('=');
        }

        if i + 2 < data.len() {
            out.push(TABLE[n3 as usize] as char);
        } else {
            out.push('=');
        }

        i += 3;
    }
    out
}
