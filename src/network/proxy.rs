use anyhow::{Context, Result};
use std::env;
use std::process::Command;
use url::Url;

#[derive(Debug, Clone)]
pub enum ProxyKind {
    Socks5,
    Http,
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub source: &'static str,
}

impl ProxyConfig {
    pub fn display(&self) -> String {
        let scheme = match self.kind {
            ProxyKind::Socks5 => "socks5",
            ProxyKind::Http => "http",
        };
        format!("{scheme}://{}:{} ({})", self.host, self.port, self.source)
    }
}

/// Resolve the proxy for a specific pool address.
///
/// `MINING_PROXY=off` (also `none`, `direct`, `-`) forces a direct connection.
/// Loopback targets skip auto-detected proxies so a system proxy cannot
/// intercept connections to a local pool.
pub fn detect_proxy_for(target_addr: &str) -> Option<ProxyConfig> {
    if let Some(p) = env_first(&["MINING_PROXY"]) {
        if proxy_directive_disables(&p) {
            return None;
        }
        if let Ok(cfg) = parse_proxy_url(&p, "MINING_PROXY") {
            return Some(cfg);
        }
    }

    if is_loopback_target(target_addr) {
        return None;
    }

    // Standard proxy env vars
    for (keys, source) in [
        (&["ALL_PROXY", "all_proxy"][..], "ALL_PROXY"),
        (
            &["SOCKS5_PROXY", "socks5_proxy", "SOCKS_PROXY", "socks_proxy"][..],
            "SOCKS_PROXY",
        ),
        (
            &["HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy"][..],
            "HTTP_PROXY",
        ),
    ] {
        if let Some(p) = env_first(keys) {
            if let Ok(mut cfg) = parse_proxy_url(&p, source) {
                cfg.source = source;
                return Some(cfg);
            }
        }
    }

    // macOS system proxy
    #[cfg(target_os = "macos")]
    {
        if let Ok(Some(cfg)) = detect_macos_scutil_proxy() {
            return Some(cfg);
        }
    }

    None
}

fn proxy_directive_disables(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "off" | "none" | "direct" | "0" | "-"
    )
}

fn is_loopback_target(addr: &str) -> bool {
    let host = if let Some(rest) = addr.strip_prefix('[') {
        rest.split_once(']').map(|(host, _)| host).unwrap_or(rest)
    } else {
        addr.rsplit_once(':').map(|(host, _)| host).unwrap_or(addr)
    };
    let host = host.trim();
    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.")
}

fn env_first(keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Ok(v) = env::var(k) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn parse_proxy_url(input: &str, source: &'static str) -> Result<ProxyConfig> {
    // Allow shorthand like "127.0.0.1:7890" by assuming http.
    let url = if input.contains("://") {
        Url::parse(input).context("Invalid proxy URL")?
    } else {
        Url::parse(&format!("http://{input}")).context("Invalid proxy URL")?
    };

    let scheme = url.scheme().to_ascii_lowercase();
    let kind = match scheme.as_str() {
        "socks5" | "socks5h" | "socks" => ProxyKind::Socks5,
        "http" | "https" => ProxyKind::Http,
        other => anyhow::bail!("Unsupported proxy scheme: {other}"),
    };

    let host = url.host_str().context("Proxy host missing")?.to_string();
    let port = url.port().unwrap_or_else(|| match kind {
        ProxyKind::Socks5 => 1080,
        ProxyKind::Http => 8080,
    });

    let username = if url.username().is_empty() {
        None
    } else {
        Some(url.username().to_string())
    };
    let password = url.password().map(|p| p.to_string());

    Ok(ProxyConfig {
        kind,
        host,
        port: port as u16,
        username,
        password,
        source,
    })
}

#[cfg(target_os = "macos")]
fn detect_macos_scutil_proxy() -> Result<Option<ProxyConfig>> {
    let output = Command::new("scutil")
        .arg("--proxy")
        .output()
        .context("Failed to execute scutil --proxy")?;

    if !output.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut http_enable = false;
    let mut http_host: Option<String> = None;
    let mut http_port: Option<u16> = None;

    let mut socks_enable = false;
    let mut socks_host: Option<String> = None;
    let mut socks_port: Option<u16> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Format: Key : Value
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();

        match key {
            "HTTPEnable" => http_enable = val == "1",
            "HTTPProxy" => {
                if !val.is_empty() {
                    http_host = Some(val.to_string())
                }
            }
            "HTTPPort" => http_port = val.parse::<u16>().ok(),

            "SOCKSEnable" => socks_enable = val == "1",
            "SOCKSProxy" => {
                if !val.is_empty() {
                    socks_host = Some(val.to_string())
                }
            }
            "SOCKSPort" => socks_port = val.parse::<u16>().ok(),
            _ => {}
        }
    }

    // Prefer SOCKS for raw TCP (Stratum).
    if socks_enable {
        if let (Some(host), Some(port)) = (socks_host, socks_port) {
            return Ok(Some(ProxyConfig {
                kind: ProxyKind::Socks5,
                host,
                port,
                username: None,
                password: None,
                source: "macOS-system",
            }));
        }
    }

    if http_enable {
        if let (Some(host), Some(port)) = (http_host, http_port) {
            return Ok(Some(ProxyConfig {
                kind: ProxyKind::Http,
                host,
                port,
                username: None,
                password: None,
                source: "macOS-system",
            }));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_proxy_directives() {
        assert!(proxy_directive_disables("off"));
        assert!(proxy_directive_disables(" NONE "));
        assert!(proxy_directive_disables("direct"));
        assert!(proxy_directive_disables("0"));
        assert!(proxy_directive_disables("-"));
        assert!(!proxy_directive_disables("socks5://127.0.0.1:7890"));
    }

    #[test]
    fn loopback_targets_skip_auto_proxy() {
        assert!(is_loopback_target("127.0.0.1:3333"));
        assert!(is_loopback_target("localhost:3333"));
        assert!(is_loopback_target("[::1]:3333"));
        assert!(!is_loopback_target("solo.ckpool.org:3333"));
        assert!(!is_loopback_target(""));
    }
}
