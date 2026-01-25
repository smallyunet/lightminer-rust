# ⛏️ LightMiner-Rust

A lightweight CPU miner written in Rust with Stratum V1 protocol support and a professional TUI interface.

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

![TUI Screenshot](docs/assets/screenshot.png)

## Features

- 🔗 **Stratum V1 Protocol** - Full support for `mining.subscribe`, `mining.authorize`, and `mining.submit`
- ⚡ **SHA256d Mining** - Standard double-SHA256 hashing algorithm
- 📊 **Real-time Metrics** - Track hashrate, accepted/rejected shares
- 🖥️ **Professional TUI** - Beautiful terminal interface built with ratatui
- 📝 **Dual Mode** - TUI or traditional log output

## Quick Start

```bash
# Clone the repository
git clone https://github.com/yourusername/lightminer-rust.git
cd lightminer-rust

# Build and run
cargo run
```

## Usage

### TUI Mode (Default)

```bash
cargo run
```

**Controls:**
- `Q` or `Esc` - Quit
- `C` - Clear logs

### Log Mode

```bash
NO_TUI=1 cargo run
```

### Custom Mining Pool

```bash
MINING_POOL="stratum.pool.com:3333" cargo run
```

### Multiple Pools (Failover / Rotation)

Configure multiple pools with per-pool credentials/coin:

```bash
# Format: name@host:port|user|pass|coin|weight|algo;...
MINING_POOLS="primary@solo.ckpool.org:3333|<btc-address>.worker|x|btc|1|sha256d;backup@stratum.example.com:3333|user.worker|x|btc|1|sha256d" \
MINING_POOL_STRATEGY="failover" \
cargo run
```

### Select Mining Algorithm

Single-pool (legacy env vars) can set the algorithm explicitly:

```bash
MINING_POOL="ltc-pool.example.com:3333" \
MINING_USER="user.worker" MINING_PASS="x" \
MINING_ALGO="scrypt" \
cargo run
```

Or per-pool in `MINING_POOLS` (recommended):

```bash
MINING_POOLS="ltc@ltc-pool.example.com:3333|user.worker|x|ltc|1|scrypt" cargo run
```

Strategies:

```bash
MINING_POOL_STRATEGY="failover"   # default, stick to current pool until it fails
MINING_POOL_STRATEGY="round_robin" # rotate pools on each reconnect
MINING_POOL_STRATEGY="weighted"    # weighted round-robin via the per-pool weight
```

Cooldown control (avoid hammering a dead pool):

```bash
MINING_POOL_FAILURES_BEFORE_COOLDOWN=3
MINING_POOL_COOLDOWN_SECS=30
```

### Custom Worker Credentials

```bash
MINING_USER="user.worker" MINING_PASS="x" cargo run
```

### Logging Verbosity (Log Mode)

```bash
NO_TUI=1 RUST_LOG=info cargo run
```

## Proxy

LightMiner-Rust will automatically detect and use a proxy when available:

- Environment variables: `ALL_PROXY` / `SOCKS5_PROXY` / `HTTP_PROXY` (and lowercase variants)
- On macOS: system proxy settings via `scutil --proxy`

You can also override explicitly:

```bash
# SOCKS5 (recommended for Stratum/TCP)
MINING_PROXY="socks5://127.0.0.1:7890" cargo run

# HTTP proxy (uses HTTP CONNECT tunnel)
MINING_PROXY="http://127.0.0.1:7890" cargo run
```

## Architecture

```
src/
├── main.rs           # Entry point, mode selection
├── protocol/         # Stratum V1 protocol (JSON-RPC)
│   └── mod.rs        # Request, Response, Notification, Job
├── network/          # TCP client
│   └── mod.rs        # Async TCP with tokio
├── manager/          # Main orchestration
│   ├── mod.rs        # Event loop, job dispatch
│   └── metrics.rs    # Hashrate & share tracking
├── miner/            # Mining logic
│   ├── mod.rs        # SHA256d, worker thread
│   └── job.rs        # Block header construction
└── ui/               # Terminal UI
    ├── mod.rs        # ratatui drawing
    ├── state.rs      # Shared app state
    └── widgets.rs    # Custom widgets
```

## Configuration

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `MINING_POOL` | Pool address (host:port) | `solo.ckpool.org:3333` |
| `MINING_ALGO` | Single-pool algorithm: `sha256d`/`scrypt` | `sha256d` |
| `MINING_POOLS` | Multi-pool list: `name@addr|user|pass|coin|weight|algo;...` | (unset → uses `MINING_POOL`) |
| `MINING_POOL_STRATEGY` | Pool selection: `failover`/`round_robin`/`weighted` | `failover` |
| `MINING_POOL_FAILURES_BEFORE_COOLDOWN` | Failures before putting a pool on cooldown | `3` |
| `MINING_POOL_COOLDOWN_SECS` | Cooldown seconds for unhealthy pools | `30` |
| `MINING_USER` | Worker name / username | `lightminer.1` |
| `MINING_PASS` | Worker password | `x` |
| `MINING_AGENT` | Stratum `mining.subscribe` agent string | `LightMiner-Rust/<crate version>` |
| `MINING_PROXY` | Proxy URL override (`socks5://` or `http://`) | (auto-detect) |
| `MINING_RECONNECT` | Auto-reconnect on disconnect (`true/false`) | `true` |
| `MINING_RECONNECT_MAX_DELAY_MS` | Max reconnect backoff delay (ms) | `30000` |
| `MINING_HANDSHAKE_TIMEOUT_MS` | Handshake timeout waiting for `mining.subscribe` response (ms) | `10000` |
| `MINING_IDLE_TIMEOUT_SECS` | Reconnect if no message received for N seconds | `120` |
| `MINING_THREADS` | CPU mining threads | (CPU cores) |
| `NO_TUI` | Disable TUI, use log mode | (unset) |
| `RUST_LOG` | Log filter (only in log mode) | (unset) |

## Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test
```

## Documentation

API documentation is available at: https://yourusername.github.io/lightminer-rust/

Generate locally:
```bash
cargo doc --open
```

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the complete development roadmap.

**Completed:**
- ✅ Phase 1: Networking & Protocol
- ✅ Phase 2: Mining Logic
- ✅ Phase 3: Share Submission
- ✅ Phase 4: TUI Interface

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
