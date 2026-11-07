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
