use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use libp2p::futures::stream::StreamExt;
use libp2p::{
    StreamProtocol, gossipsub, identify, kad, mdns, noise, ping,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, identity,
};
use tokio::sync::Mutex;
use terminal_size::{Width, terminal_size};
use textwrap::{self, Options};
use tokio::select;
use tracing::{debug, trace, warn};
use serde_json;
use chrono::{DateTime, Utc};

// Use types from the main library
use crate::{SyncNodeUtc, EstimationUtc, estimate_offset_utc};

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
    pub timestamp: DateTime<Utc>,
    pub adjustment_ms: i64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileRequest(String);
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileResponse(Vec<u8>);

#[derive(Debug)]
pub enum InternalEvent {
    ChatMessage(Msg),
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
        debug!("Reassembler: Attempting to add chunk and reassemble for msg_chunk: {:?}", msg_chunk);
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

        let (buffered_total_chunks, received_count, chunks) =
            buffer_guard.entry(message_id.clone()).or_insert_with(|| {
                (total_chunks, 0, vec![None; total_chunks])
            });

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
            debug!("Reassembler: Successfully reassembled message for message_id: {}", message_id);
            Some(reassembled_msg)
        } else {
            None
        }
    }
}

#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub ping: ping::Behaviour,
    pub request_response: request_response::cbor::Behaviour<FileRequest, FileResponse>,
}

pub async fn evt_loop(
    mut send: tokio::sync::mpsc::Receiver<InternalEvent>,
    recv: tokio::sync::mpsc::Sender<InternalEvent>,
    topic: gossipsub::IdentTopic,
) -> Result<()> {
    debug!("evt_loop: Starting event loop with topic: {}", topic);
    let reassembler = Arc::new(MessageReassembler::new());
    
    // Time Consensus State
    let mut local_node = SyncNodeUtc::new(0, 10, 3, 20.0, 0); // Local node ID 0
    let mut peer_estimates: HashMap<String, EstimationUtc> = HashMap::new();
    let mut time_sync_interval = tokio::time::interval(Duration::from_secs(10));

    let keypair = identity::Keypair::generate_ed25519();
    let public_key = keypair.public();
    let local_peer_id = public_key.to_peer_id();
    warn!("Local PeerId: {}", local_peer_id);

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .build()
        .map_err(|msg| anyhow!(msg))?;

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
                kademlia: kad::Behaviour::with_config(key.public().to_peer_id(), kad_store, kad_config),
                ping: ping::Behaviour::new(ping::Config::new()),
                request_response: request_response::cbor::Behaviour::new(
                    [(
                        StreamProtocol::new("/file-exchange/1"),
                        ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                ),
            })
        })?
        .with_swarm_config(|c: libp2p::swarm::Config| {
            c.with_idle_connection_timeout(Duration::from_secs(60))
        })
        .build();

    debug!("evt_loop: Swarm built with local PeerId: {}", swarm.local_peer_id());
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    loop {
        select! {
            _ = time_sync_interval.tick() => {
                debug!("TimeSync: Interval tick. Estimates count: {}", peer_estimates.len());
                if !peer_estimates.is_empty() {
                    let estimates: Vec<EstimationUtc> = peer_estimates.values().cloned().collect();
                    local_node.run_sync_cycle(estimates);
                    debug!("TimeSync: Local adjustment updated to: {}ms", local_node.adjustment.num_milliseconds());
                    peer_estimates.clear();
                }

                // Broadcast local time
                let sync_msg = TimeSyncMessage {
                    peer_id: local_peer_id.to_string(),
                    timestamp: Utc::now(),
                    adjustment_ms: local_node.adjustment.num_milliseconds(),
                };
                
                trace!("TimeSync: Broadcasting local time: {:?}", sync_msg);
                let mut msg = Msg::default();
                msg.kind = MsgKind::TimeSync;
                msg.content = vec![serde_json::to_string(&sync_msg)?];
                
                if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), serde_json::to_vec(&msg)?) {
                    warn!("TimeSync: Failed to publish time sync message: {:?}", e);
                }
            }
            Some(event) = send.recv() => {
                if let InternalEvent::ChatMessage(m) = event {
                    debug!("evt_loop: Sending outgoing ChatMessage.");
                    if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), serde_json::to_vec(&m)?) {
                        warn!("evt_loop: Publish error: {:?}", e);
                    }
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        debug!("evt_loop: mDNS discovered peer: {}", peer_id);
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        debug!("evt_loop: mDNS peer expired: {}", peer_id);
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message, })) => {
                    trace!("evt_loop: Received Gossipsub message from {} with id {}", peer_id, id);
                    match serde_json::from_slice::<Msg>(&message.data) {
                        Ok(msg) => {
                            if msg.kind == MsgKind::TimeSync {
                                if let Ok(sync_msg) = serde_json::from_str::<TimeSyncMessage>(&msg.content[0]) {
                                    debug!("TimeSync: Received from peer {}: {:?}", peer_id, sync_msg);
                                    let s = sync_msg.timestamp;
                                    let r = Utc::now();
                                    let c = sync_msg.timestamp + chrono::Duration::milliseconds(sync_msg.adjustment_ms);
                                    let estimate = estimate_offset_utc(s, r, c);
                                    trace!("TimeSync: Calculated estimate for peer {}: {:?}", peer_id, estimate);
                                    peer_estimates.insert(sync_msg.peer_id, estimate);
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
                SwarmEvent::NewListenAddr { address, .. } => {
                    debug!("evt_loop: Local node is listening on {address}");
                }
                _ => {}
            }
        }
    }
}
