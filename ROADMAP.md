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

## Release v0.0.5: Robust Handshake + Idle Timeout + Auth Status ✅
**Goal:** Reduce “connected but idle” failure modes and make session state clearer.
- [x] **Handshake**: Reliably wait for `mining.subscribe` response even if notifications arrive first.
- [x] **Reconnect**: Idle timeout reconnect when the TCP session stalls (`MINING_IDLE_TIMEOUT_SECS`).
- [x] **Config**: Add handshake timeout (`MINING_HANDSHAKE_TIMEOUT_MS`).
- [x] **TUI**: Show authorization status (Auth: OK/FAIL/-).

## Release v0.0.6: Multi-pool ✅
**Goal:** Keep mining when one pool fails, and choose pools by policy.
- [x] **Config**: `MINING_POOLS` with per-pool coin, weight, and algorithm.
- [x] **Strategy**: Failover, round-robin, and weighted selection, plus failure cooldown.

## Release v0.0.7: Pool controls in the TUI ✅
**Goal:** Switch or disable pools without restarting.
- [x] **TUI**: Next/previous pool and disable/enable the active pool.

## Release v0.0.8: Presets ✅
**Goal:** Start from a named pool instead of a raw address string.
- [x] **Presets**: Embedded presets plus a local JSON preset file.

## Release v0.0.9: Share correctness ✅
**Goal:** Submit only work that still matches what the pool is asking for, and show why a share was rejected.
- [x] **Difficulty**: Apply `mining.set_difficulty` to the in-flight job immediately.
- [x] **Stale work**: Drop nonces whose job id is no longer current, or whose difficulty is below the pool's current difficulty.
- [x] **Reject reason**: Record the pool's Stratum error (`result: false` included) in the log and the shares panel.
- [x] **Algorithms**: Refuse unknown algorithms instead of hashing them as SHA256d.
- [x] **Session loop**: Log mode waits for pool traffic instead of spinning when no UI command channel is attached.
- [x] **Tests**: Unit coverage for the submit policy, plus an integration test against a local Stratum pool.
