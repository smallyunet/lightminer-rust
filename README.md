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
| `NO_TUI` | Disable TUI, use log mode | (unset) |

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
