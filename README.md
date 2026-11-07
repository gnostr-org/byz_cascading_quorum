# Byzantine Cascading Quorum

Rust experiments in cascading quorum-based time synchronization. The crate provides simulations of staged clock synchronization and a separate libp2p peer-to-peer node that exchanges time reports over gossipsub.

> These binaries are experimental examples, not production time services. The peer-to-peer clock reports are not a substitute for a trusted time source.

## Requirements

- Rust and Cargo with support for the Rust 2024 edition.

## Run

Run a binary from the repository root with `cargo run --bin <name>`:

| Binary | Purpose |
| --- | --- |
| `byz_cascading_quorum` | Runs the original cascading quorum simulation. |
| `byz_cascading_quorum_v2` | Runs the second version of the cascading quorum simulation. |
| `byz_time` | Runs the time-synchronization simulation. |
| `utc_consensus` | Runs the UTC consensus simulation. |
| `p2p_node` | Starts a networked node that exchanges time-sync messages with peers. |

For example:

```sh
cargo run --bin byz_cascading_quorum
cargo run --bin byz_cascading_quorum_v2
cargo run --bin byz_time
cargo run --bin utc_consensus
```

### Peer-to-peer node

`p2p_node` requires the expected quorum size. Bootstrap addresses are optional; nodes also use local peer discovery.

```sh
cargo run --bin p2p_node -- --num-nodes 2
```

Available options:

```text
--num-nodes <NUM_NODES>    Total number of nodes expected in the quorum [required]
--bootstrap <MULTIADDR>    Bootstrap node multiaddress to dial
--offset <SECONDS>         Initial clock offset in seconds [default: 0]
--topic <TOPIC>            Gossipsub topic to join [default: gnostr]
```

Pass `--help` to see the CLI help. The node listens on dynamically assigned TCP and QUIC addresses and prints peer and quorum reports as it runs. Its standard input accepts these commands:

| Command | Action |
| --- | --- |
| `PUB <message>` | Publish a chat message to the topic. |
| `TIME` | Print the current local and peer time report. |
| `DIAL <multiaddr>` | Dial a peer by multiaddress. |
| `BOOTSTRAP` | Trigger a Kademlia bootstrap lookup. |

Use the same topic for nodes that should exchange messages. To connect across hosts, provide a reachable peer multiaddress with `--bootstrap`; addresses and connectivity depend on the network and firewall configuration.

## Library

The `byz_time` library exposes the simulation entry points, synchronization node types, UTC offset estimation, and the peer-to-peer event loop. The networking implementation uses libp2p for transport, discovery, gossipsub messaging, and time-report exchange.

## Tests

Run the test suite with:

```sh
cargo test --all-targets --all-features
```

The GitHub Actions workflow builds and tests all targets and features on Linux, macOS, and Windows.

## Browser WASM lab

The GitHub Pages site runs the four simulation binaries in separate Web Workers compiled to WebAssembly. Multiple examples can run at once; each has its own output log. The `p2p_node` menu entry starts the browser networking client, which uses WebSockets to reach relay candidates, WebRTC for browser-compatible peer connections, circuit relay reservations, DCUtR hole punching, and the same `gnostr` gossipsub topic as the native node.

The editable candidate list includes the libp2p auto-relay seed and the public IPFS bootstrap addresses used by [RandyMcMillan/ipfs-gh-pages](https://github.com/RandyMcMillan/ipfs-gh-pages). These community-operated endpoints are best-effort, and not every bootstrap peer necessarily provides circuit relay service. For direct browser-to-browser connectivity, share and dial a peer's relayed multiaddress; DCUtR/WebRTC with public STUN servers can then attempt a direct path. STUN providers can observe public network addresses. GitHub Pages only hosts the static app, so remote peer connectivity depends on those external endpoints. To build the site locally, install the `wasm32-unknown-unknown` Rust target and `wasm-pack`, then run:

```sh
wasm-pack build --no-default-features --target web --out-dir web/src/pkg --out-name byz_time
cd web && npm ci && npm run build
```

The generated site is written to `dist/`. The `WASM demo and GitHub Pages` workflow builds and deploys it from `main`.
