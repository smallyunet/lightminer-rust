# LightMiner-Rust Development Roadmap

## Phase 1: The Communicator (Networking & Protocol)
**Goal:** Establish a stable TCP connection to a mining pool and participate in the Stratum V1 handshake.
- [x] **Protocol Layer**: Define `StratumRequest` and `StratumResponse` structs (JSON-RPC 2.0).
- [ ] **Protocol Layer**: Implement serialization/deserialization tests.
- [x] **Network Layer**: Create a TCP client using `tokio::net::TcpStream`.
- [x] **Network Layer**: Implement `LinesCodec` or similar for line-based JSON streaming.
- [x] **Manager Layer**: Implement the `mining.subscribe` flow.
- [ ] **Manager Layer**: Implement the `mining.authorize` flow.
- [ ] **Deliverable**: A CLI that connects to a pool, logs in, and prints received jobs to the console.

## Phase 2: The Solver (Mining Logic)
**Goal:** Implement the CPU mining algorithm and the worker loop.
- [ ] **Algorithm**: Implement `Scrypt` (or standard SHA256d) hashing function.
- [ ] **Miner Layer**: Create a worker thread that accepts a `Job` template.
- [ ] **Miner Layer**: Implement the nonce search loop.
- [ ] **Manager Layer**: Dispatch jobs from Network to Miner.
- [ ] **Deliverable**: The application prints "Nonce Found!" when a valid hash is discovered locally (mock difficulty).

## Phase 3: The Submitter (Integration)
**Goal:** Close the loop by submitting work to the pool and handling feedback.
- [ ] **Protocol Layer**: Add `mining.submit` message support.
- [ ] **Manager Layer**: Handle `NonceFound` events from Miner and send `mining.submit` to Network.
- [ ] **Network Layer**: Handle pool responses (Accept/Reject shares).
- [ ] **Metrics**: Track hashrate and accepted/rejected share counts.
- [ ] **Deliverable**: A functional miner that earns valid shares on a testnet/public pool.

## Phase 4: The Interface (User Experience)
**Goal:** Replace raw logs with a professional TUI.
- [ ] **UI**: Integrate `ratatui` for the interface.
- [ ] **UI**: Design a dashboard with sections: Connection Status, Hashrate Graph, Recent Logs.
- [ ] **Manager**: separate log output from UI rendering.
- [ ] **Deliverable**: A polished, standalone binary release.
