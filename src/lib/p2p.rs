use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use libp2p::{
    Multiaddr, PeerId, StreamProtocol,
    futures::stream::StreamExt,
    gossipsub, identify, identity, kad, mdns, noise, ping,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use serde_json;
use tokio::{
    io::{self, AsyncBufReadExt},
    select,
    sync::Mutex,
};
use tokio_stream::wrappers::LinesStream;
use tracing::{debug, info, trace, warn};

// Use types from the main library
use crate::{EstimationUtc, SyncNodeUtc, estimate_offset_utc};

// Placeholder for application-specific types
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Msg {
    pub from: Option<String>,
    pub kind: MsgKind,
    pub commit_id: Option<String>,
    pub nostr_event: Option<String>,
    pub message_id: Option<String>,
    pub sequence_num: Option<usize>,
    pub total_chunks: Option<usize>,
    pub content: Vec<String>,
}

impl Msg {
    pub fn set_content(mut self, content: String, _index: usize) -> Self {
        self.content.push(content);
        self
    }
    pub fn set_kind(mut self, kind: MsgKind) -> Self {
        self.kind = kind;
        self
    }
}

impl std::fmt::Display for Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.content.is_empty() {
            write!(f, "")
        } else {
            write!(f, "{}", self.content[0])
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MsgKind {
    Chat,
    OneShot,
    GitDiff,
    System,
    NostrEvent,
    TimeSync,
}

impl Default for MsgKind {
    fn default() -> Self {
        MsgKind::Chat
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimeSyncMessage {
    pub peer_id: String,
    pub system_time: DateTime<Utc>,
    pub adjustment_ms: i64,
    pub listen_addrs: Vec<Multiaddr>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileRequest(String);
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileResponse(Vec<u8>);

#[derive(Debug, Clone)]
pub enum InternalEvent {
    ChatMessage(Msg),
    Dial(Multiaddr),
    ShowErrorMsg(String),
    ShowInfoMsg(String),
}

pub const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");

pub struct MessageReassembler {
    buffer: Mutex<HashMap<String, (usize, usize, Vec<Option<Msg>>)>>,
}

impl MessageReassembler {
    pub fn new() -> Self {
        MessageReassembler {
            buffer: Mutex::new(HashMap::new()),
        }
    }

    pub async fn add_chunk_and_reassemble(&self, msg_chunk: Msg) -> Option<Msg> {
        debug!(
            "Reassembler: Attempting to add chunk and reassemble for msg_chunk: {:?}",
            msg_chunk
        );
        if msg_chunk.message_id.is_none()
            || msg_chunk.sequence_num.is_none()
            || msg_chunk.total_chunks.is_none()
        {
            return None;
        }

        let message_id = msg_chunk.message_id.clone().unwrap();
        let sequence_num = msg_chunk.sequence_num.unwrap();
        let total_chunks = msg_chunk.total_chunks.unwrap();

        let mut buffer_guard = self.buffer.lock().await;

        let (buffered_total_chunks, received_count, chunks) = buffer_guard
            .entry(message_id.clone())
            .or_insert_with(|| (total_chunks, 0, vec![None; total_chunks]));

        if *buffered_total_chunks != total_chunks {
            buffer_guard.remove(&message_id);
            return None;
        }

        if sequence_num < total_chunks {
            if chunks[sequence_num].is_none() {
                chunks[sequence_num] = Some(msg_chunk.clone());
                *received_count += 1;
            }
        } else {
            return None;
        }

        if *received_count == total_chunks {
            let mut full_content = String::new();
            let mut reassembled_msg = Msg::default();

            for (i, chunk_option) in chunks.iter().enumerate() {
                if let Some(chunk) = chunk_option {
                    if i == 0 {
                        reassembled_msg.from = chunk.from.clone();
                        reassembled_msg.kind = chunk.kind;
                        reassembled_msg.commit_id = chunk.commit_id.clone();
                        reassembled_msg.nostr_event = chunk.nostr_event.clone();
                        reassembled_msg.message_id = None;
                        reassembled_msg.sequence_num = None;
                        reassembled_msg.total_chunks = None;
                    }
                    full_content.push_str(&chunk.content[0]);
                }
            }
            reassembled_msg.content = vec![full_content];
            buffer_guard.remove(&message_id);
            debug!(
                "Reassembler: Successfully reassembled message for message_id: {}",
                message_id
            );
            Some(reassembled_msg)
        } else {
            None
        }
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MyBehaviourEvent")]
pub struct MyBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub ping: ping::Behaviour,
    pub request_response: request_response::cbor::Behaviour<FileRequest, FileResponse>,
    pub dcutr: libp2p::dcutr::Behaviour,
    pub relay: libp2p::relay::client::Behaviour,
}

#[derive(Debug)]
pub enum MyBehaviourEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    Identify(identify::Event),
    Kademlia(kad::Event),
    Ping(ping::Event),
    RequestResponse(request_response::Event<FileRequest, FileResponse>),
    Dcutr(libp2p::dcutr::Event),
    Relay(libp2p::relay::client::Event),
}

impl From<gossipsub::Event> for MyBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        MyBehaviourEvent::Gossipsub(event)
    }
}

impl From<mdns::Event> for MyBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        MyBehaviourEvent::Mdns(event)
    }
}

impl From<identify::Event> for MyBehaviourEvent {
    fn from(event: identify::Event) -> Self {
        MyBehaviourEvent::Identify(event)
    }
}

impl From<kad::Event> for MyBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        MyBehaviourEvent::Kademlia(event)
    }
}

impl From<ping::Event> for MyBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        MyBehaviourEvent::Ping(event)
    }
}

impl From<request_response::Event<FileRequest, FileResponse>> for MyBehaviourEvent {
    fn from(event: request_response::Event<FileRequest, FileResponse>) -> Self {
        MyBehaviourEvent::RequestResponse(event)
    }
}

impl From<libp2p::dcutr::Event> for MyBehaviourEvent {
    fn from(event: libp2p::dcutr::Event) -> Self {
        MyBehaviourEvent::Dcutr(event)
    }
}

impl From<libp2p::relay::client::Event> for MyBehaviourEvent {
    fn from(event: libp2p::relay::client::Event) -> Self {
        MyBehaviourEvent::Relay(event)
    }
}

fn print_quorum_status(
    local_peer_id: &PeerId,
    local_node: &SyncNodeUtc,
    local_addrs: &[Multiaddr],
    peer_reports: &HashMap<String, (DateTime<Utc>, DateTime<Utc>, i64, String, Vec<Multiaddr>)>,
) {
    let now = Utc::now();
    println!(
        "\n======================================================================================================="
    );
    println!(
        "[QUORUM CONSENSUS REPORT] - {}",
        now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    println!(
        "======================================================================================================="
    );
    println!(
        "{:<18} | {:<12} | {:<12} | {:<10} | {:<6} | {:<20}",
        "PEER ID", "SYSTEM UTC", "LOGICAL UTC", "DRIFT", "STATE", "MULTIADDRESS"
    );
    println!("{:-<103}", "");

    // Print Local Node (Self)
    let local_addr_str = local_addrs
        .first()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    let local_peer_id_str = local_peer_id.to_string();
    let short_id = format!(
        "{:.9}/{:.8}",
        &local_peer_id_str[..9],
        &local_peer_id_str[local_peer_id_str.len() - 8..]
    );

    println!(
        "{:<12} | {:<12} | {:<12} | {:<10} | {:<12} | {:<20}",
        format!("{}", short_id),
        now.format("%H:%M:%S%.3f"),
        local_node.get_logical_utc().format("%H:%M:%S%.3f"),
        format!("{}ms", local_node.adjustment.num_milliseconds()),
        local_node.state,
        local_addr_str
    );

    // Print Peers
    for (peer_id, (system_time, logical_time, adj, state, addrs)) in peer_reports {
        let addr_str = addrs
            .first()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "N/A".to_string());
        let peer_id_str = peer_id.to_string();
        let short_peer_id = format!(
            "{:.9}/{:.8}",
            &peer_id_str[..9],
            &peer_id_str[peer_id_str.len() - 8..]
        );

        if adj.clone() >= 0 {
            println!(
                "{:<12} | {:<12} | {:<12} |  { :<9} | { :<4} | {:<20}",
                short_peer_id,
                system_time.format("%H:%M:%S%.3f"),
                logical_time.format("%H:%M:%S%.3f"),
                format!("{}ms", adj), // DRIFT
                state, // STATE 
                addr_str
            );
        } else {
            println!(
                "{:<12} | {:<12} | {:<12} | { :<10} | { :<4} | {:<20}",
                short_peer_id,
                system_time.format("%H:%M:%S%.3f"),
                logical_time.format("%H:%M:%S%.3f"),
                format!("{}ms", adj), // DRIFT
                state, // STATE
                addr_str
            );
        };
    }
    println!("{:-<103}\n", "");
}
pub async fn evt_loop(
    mut send: tokio::sync::mpsc::Receiver<InternalEvent>,
    recv: tokio::sync::mpsc::Sender<InternalEvent>,
    topic: gossipsub::IdentTopic,
    mut addr_sender: Option<tokio::sync::oneshot::Sender<Multiaddr>>,
    initial_offset_sec: i64,
    bootstrap_nodes: Vec<Multiaddr>,
    num_nodes: usize,
) -> Result<()> {
    debug!("evt_loop: Starting event loop with topic: {}", topic);
    let reassembler = Arc::new(MessageReassembler::new());

    // Time Consensus State
    let mut local_node = SyncNodeUtc::new(0, num_nodes, 0, 30.0, initial_offset_sec);
    let mut peer_estimates: HashMap<String, EstimationUtc> = HashMap::new();
    let mut peer_reports: HashMap<
        String,
        (DateTime<Utc>, DateTime<Utc>, i64, String, Vec<Multiaddr>),
    > = HashMap::new();
    let mut local_listen_addrs: Vec<Multiaddr> = Vec::new();
    let mut time_sync_interval = tokio::time::interval(Duration::from_secs(5));
    let mut discovery_interval = tokio::time::interval(Duration::from_secs(30));

    let keypair = identity::Keypair::generate_ed25519();
    let public_key = keypair.public();
    let local_peer_id = public_key.to_peer_id();
    warn!("Local PeerId: {}", local_peer_id);

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .build()
        .map_err(|msg| anyhow!(msg))?;

    let (_relay_transport, relay_client) = libp2p::relay::client::new(local_peer_id);

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_dns()?
        .with_behaviour(|key| {
            let mut kad_config = kad::Config::new(IPFS_PROTO_NAME);
            kad_config.set_query_timeout(Duration::from_secs(120));
            let kad_store = kad::store::MemoryStore::new(key.public().to_peer_id());

            Ok(MyBehaviour {
                gossipsub: gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )
                .expect("Failed to create gossipsub behaviour"),
                mdns: libp2p::mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )?,
                identify: identify::Behaviour::new(identify::Config::new(
                    "/ipfs/id/1.0.0".to_string(),
                    key.public(),
                )),
                kademlia: kad::Behaviour::with_config(
                    key.public().to_peer_id(),
                    kad_store,
                    kad_config,
                ),
                ping: ping::Behaviour::new(ping::Config::new()),
                request_response: request_response::cbor::Behaviour::new(
                    [(
                        StreamProtocol::new("/file-exchange/1"),
                        ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                ),
                dcutr: libp2p::dcutr::Behaviour::new(local_peer_id),
                relay: relay_client,
            })
        })?
        .with_swarm_config(|c: libp2p::swarm::Config| {
            c.with_idle_connection_timeout(Duration::from_secs(60))
        })
        .build();

    debug!(
        "evt_loop: Swarm built with local PeerId: {}",
        swarm.local_peer_id()
    );
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    // Dial bootstrap nodes
    for addr in bootstrap_nodes {
        info!("evt_loop: Dialing bootstrap node: {}", addr);
        if let Err(e) = swarm.dial(addr) {
            warn!("evt_loop: Failed to dial bootstrap node: {:?}", e);
        }
    }

    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let mut stdin = LinesStream::new(io::BufReader::new(io::stdin()).lines());

    loop {
        let connected_peers_count = swarm.connected_peers().count();
        local_node.n = connected_peers_count + 1;
        local_node.f = connected_peers_count / 3;

        select! {
            line = stdin.next() => {
                if let Some(Ok(line)) = line {
                    let mut parts = line.splitn(2, ' ');
                    let command = parts.next().unwrap_or("");
                    let arg = parts.next().unwrap_or("");

                    match command {
                        "DIAL" => {
                            if let Ok(addr) = arg.parse::<libp2p::Multiaddr>() {
                                info!("Manual dial requested: {}", addr);
                                if let Err(e) = swarm.dial(addr) {
                                    warn!("Failed to dial {}: {:?}", arg, e);
                                }
                            }
                        },
                        "BOOTSTRAP" => {
                            info!("Kademlia: Manual bootstrap triggered.");
                            if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
                                warn!("Kademlia: Bootstrap error: {:?}", e);
                            }
                        },
                        "TIME" => {
                            print_quorum_status(&local_peer_id, &local_node, &local_listen_addrs, &peer_reports);
                        },
                        "PUB" => {
                            let mut msg = Msg::default();
                            msg.kind = MsgKind::Chat;
                            msg.content = vec![arg.to_string()];
                            if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), serde_json::to_vec(&msg)?) {
                                warn!("Failed to publish message: {:?}", e);
                            }
                        },
                        _ => debug!("Unknown command: {}", command),
                    }
                }
            }
            _ = discovery_interval.tick() => {
                debug!("Kademlia: Periodic discovery tick. Current peers: {}", connected_peers_count);
                if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
                    debug!("Kademlia: Bootstrap error during discovery: {:?}", e);
                }
                swarm.behaviour_mut().kademlia.get_closest_peers(local_peer_id);
            }
            _ = time_sync_interval.tick() => {
                debug!("TimeSync: Interval tick. Estimates count: {}. Connected peers: {}", peer_estimates.len(), connected_peers_count);

                let /*mut*/ estimates: Vec<EstimationUtc> = peer_estimates.values().cloned().collect();

                if estimates.len() >= local_node.n - local_node.f {
                    local_node.run_sync_cycle(estimates);
                    debug!("TimeSync: Node {} updated adjustment to: {}ms (State: {})",
                        local_peer_id,
                        local_node.adjustment.num_milliseconds(),
                        local_node.state
                    );
                    print_quorum_status(&local_peer_id, &local_node, &local_listen_addrs, &peer_reports);
                    peer_estimates.clear();
                }

                if connected_peers_count > 0 {
                    let sync_msg = TimeSyncMessage {
                        peer_id: local_peer_id.to_string(),
                        system_time: Utc::now(),
                        adjustment_ms: local_node.adjustment.num_milliseconds(),
                        listen_addrs: local_listen_addrs.clone(),
                    };

                    trace!("TimeSync: Broadcasting local time: {:?}", sync_msg);
                    let mut msg = Msg::default();
                    msg.kind = MsgKind::TimeSync;
                    msg.content = vec![serde_json::to_string(&sync_msg)?, local_node.state.clone()];

                    if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), serde_json::to_vec(&msg)?) {
                        warn!("TimeSync: Failed to publish time sync message: {:?}", e);
                    }
                }
            }
            Some(event) = send.recv() => {
                match event {
                    InternalEvent::ChatMessage(m) => {
                        debug!("evt_loop: Sending outgoing ChatMessage.");
                        if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), serde_json::to_vec(&m)?) {
                            warn!("evt_loop: Publish error: {:?}", e);
                        }
                    },
                    InternalEvent::Dial(addr) => {
                        info!("Internal dial requested: {}", addr);
                        if let Err(e) = swarm.dial(addr) {
                            warn!("evt_loop: Internal dial error: {:?}", e);
                        }
                    },
                    _ => {}
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("Local node is listening on {address}");
                    local_listen_addrs.push(address.clone());
                    if !address.to_string().contains("127.0.0.1") {
                        if let Some(sender) = addr_sender.take() {
                            let _ = sender.send(address);
                        }
                    }
                },
                SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                    debug!("Connection established with {} at {:?}", peer_id, endpoint.get_remote_address());
                    // Proactively add remote address to Kademlia and trigger bootstrap
                    swarm.behaviour_mut().kademlia.add_address(&peer_id, endpoint.get_remote_address().clone());
                    if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
                        debug!("Kademlia: Bootstrap error on new connection: {:?}", e);
                    }
                },
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    debug!("Connection closed with {}", peer_id);
                    peer_reports.remove(&peer_id.to_string());
                    peer_estimates.remove(&peer_id.to_string());
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, multiaddr) in list {
                        debug!("mDNS discovered peer: {}", peer_id);
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        debug!("mDNS peer expired: {}", peer_id);
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
                    debug!("Identify: Received info from {}: {:?}", peer_id, info);
                    for addr in info.listen_addrs {
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message, })) => {
                    trace!("Received Gossipsub message from {} with id {}", peer_id, id);
                    match serde_json::from_slice::<Msg>(&message.data) {
                        Ok(msg) => {
                            if msg.kind == MsgKind::TimeSync {
                                if let Ok(sync_msg) = serde_json::from_str::<TimeSyncMessage>(&msg.content[0]) {
                                    let peer_state = msg.content.get(1).cloned().unwrap_or_else(|| "Unknown".to_string());
                                    debug!("TimeSync: Received from peer {}: {:?} (State: {})", peer_id, sync_msg, peer_state);

                                    let s = local_node.get_logical_utc();
                                    let r = s;
                                    let c = sync_msg.system_time.checked_add_signed(chrono::Duration::milliseconds(sync_msg.adjustment_ms))
                                        .unwrap_or(sync_msg.system_time);
                                    let mut estimate = estimate_offset_utc(s, r, c);
                                    // For one-way gossip, we don't know the round-trip delay,
                                    // so we set a default uncertainty (e.g., 100ms).
                                    estimate.a = 0.1;

                                    peer_estimates.insert(sync_msg.peer_id.clone(), estimate);
                                    peer_reports.insert(sync_msg.peer_id.clone(), (sync_msg.system_time, c, sync_msg.adjustment_ms, peer_state, sync_msg.listen_addrs));
                                }
                            } else if msg.message_id.is_some() && msg.sequence_num.is_some() && msg.total_chunks.is_some() {
                                if let Some(reassembled_msg) = reassembler.add_chunk_and_reassemble(msg).await {
                                    let _ = recv.send(InternalEvent::ChatMessage(reassembled_msg)).await;
                                }
                            } else {
                                let _ = recv.send(InternalEvent::ChatMessage(msg)).await;
                            }
                        },
                        Err(e) => warn!("evt_loop: Error deserializing message: {:?}", e),
                    }
                },
                _ => {}
            }
        }
    }
}
