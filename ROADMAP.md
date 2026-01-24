# LightMiner-Rust Development Roadmap

## Phase 1: The Communicator (Networking & Protocol) ✅
**Goal:** Establish a stable TCP connection to a mining pool and participate in the Stratum V1 handshake.
- [x] **Protocol Layer**: Define `StratumRequest` and `StratumResponse` structs (JSON-RPC 2.0).
- [x] **Protocol Layer**: Implement serialization/deserialization tests.
- [x] **Network Layer**: Create a TCP client using `tokio::net::TcpStream`.
- [x] **Network Layer**: Implement `LinesCodec` or similar for line-based JSON streaming.
- [x] **Manager Layer**: Implement the `mining.subscribe` flow.
- [x] **Manager Layer**: Implement the `mining.authorize` flow.
- [x] **Deliverable**: A CLI that connects to a pool, logs in, and prints received jobs to the console.

## Phase 2: The Solver (Mining Logic) ✅
**Goal:** Implement the CPU mining algorithm and the worker loop.
- [x] **Algorithm**: Implement SHA256d hashing function.
- [x] **Miner Layer**: Create a worker thread that accepts a `Job` template.
- [x] **Miner Layer**: Implement the nonce search loop.
- [x] **Manager Layer**: Dispatch jobs from Network to Miner.
- [x] **Deliverable**: The application prints "Nonce Found!" when a valid hash is discovered locally.

## Phase 3: The Submitter (Integration) ✅
**Goal:** Close the loop by submitting work to the pool and handling feedback.
- [x] **Protocol Layer**: Add `mining.submit` message support.
- [x] **Manager Layer**: Handle `NonceFound` events from Miner and send `mining.submit` to Network.
- [x] **Network Layer**: Handle pool responses (Accept/Reject shares).
- [x] **Metrics**: Track hashrate and accepted/rejected share counts.
- [x] **Deliverable**: A functional miner that can submit shares to pools.

## Phase 4: The Interface (User Experience) ✅
**Goal:** Replace raw logs with a professional TUI.
- [x] **UI**: Integrate `ratatui` for the interface.
- [x] **UI**: Design a dashboard with sections: Connection Status, Hashrate, Shares, Recent Logs.
- [x] **Manager**: Separate log output from UI rendering (TUI vs NO_TUI mode).
- [x] **Deliverable**: A polished TUI miner with dual-mode support.

## Release v0.0.2: Functional TUI + Config + Shutdown
**Goal:** Make the default TUI mode fully functional and configurable for everyday use.
- [x] **TUI Integration**: Run the Manager in TUI mode and stream status/logs into the dashboard.
- [x] **Config**: Support `MINING_USER`, `MINING_PASS`, and `MINING_AGENT` environment variables.
- [x] **Shutdown**: Graceful shutdown on `Q/Esc` and Ctrl-C.
- [x] **Correctness**: Track share accept/reject only for actual `mining.submit` responses.

## Release v0.0.3: Proxy Support + Better UX ✅
**Goal:** Make connectivity more robust behind proxies and improve user feedback.
- [x] **Proxy**: Support `MINING_PROXY` and standard env vars (`ALL_PROXY`/`HTTP_PROXY`/`SOCKS5_PROXY`).
- [x] **macOS**: Detect system proxy via `scutil --proxy`.
- [x] **UX**: Improve authorization failure messaging.

## Release v0.0.4: Reconnect + Parallel Miner + Status ✅
**Goal:** Improve long-running stability and make CPU mining scale with cores.
- [x] **Reconnect**: Auto-reconnect with exponential backoff (`MINING_RECONNECT*`).
- [x] **Miner**: True multi-thread mining via `MINING_THREADS`.
- [x] **Correctness**: Robust job cancellation (old jobs must never resume).
- [x] **TUI**: Show proxy / uptime / threads in header.
