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
