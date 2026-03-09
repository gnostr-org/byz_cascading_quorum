pub mod p2p {
    use libp2p::{futures::StreamExt, identity, PeerId, swarm::{NetworkBehaviour, SwarmEvent}};
    use libp2p::gossipsub::{self, MessageAuthenticity, MessageId, GossipsubEvent, GossipsubMessage, /*RawGossipsubMessage*/};
    use libp2p::kad::{self, KademliaEvent};
    use libp2p::mdns::{self, MdnsEvent};
    use libp2p::autonat::{self, NatStatus};
    use std::collections::hash_map::{DefaultHasher, HashMap};
    use std::hash::{Hash, Hasher};
    use std::time::Duration;
    use tokio::{io::{self, AsyncBufReadExt}, select};

    // We create a custom network behaviour that combines Gossipsub and Kademlia. // GEMINI: Add autonat, identify and mdns
    #[derive(NetworkBehaviour)]
    #[behaviour(out_event = "MyBehaviourEvent")]
    pub struct MyBehaviour {
        pub gossipsub: gossipsub::Gossipsub,
        pub kademlia: kad::Kademlia<libp2p::kad::store::MemoryStore>,
        pub mdns: mdns::async_mdns::Behaviour,
        pub identify: libp2p::identify::Behaviour,
        pub autonat: autonat::Behaviour,
    }

    #[derive(Debug)]
    pub enum MyBehaviourEvent {
        Gossipsub(GossipsubEvent),
        Kademlia(KademliaEvent),
        Mdns(MdnsEvent),
        Identify(libp2p::identify::Event),
        Autonat(autonat::Event),
    }

    impl From<GossipsubEvent> for MyBehaviourEvent {
        fn from(event: GossipsubEvent) {
            MyBehaviourEvent::Gossipsub(event)
        }
    }

    impl From<KademliaEvent> for MyBehaviourEvent {
        fn from(event: KademliaEvent) {
            MyBehaviourEvent::Kademlia(event)
        }
    }

    impl From<MdnsEvent> for MyBehaviourEvent {
        fn from(event: MdnsEvent) {
            MyBehaviourEvent::Mdns(event)
        }
    }

    impl From<libp2p::identify::Event> for MyBehaviourEvent {
        fn from(event: libp2p::identify::Event) {
            MyBehaviourEvent::Identify(event)
        }
    }

    impl From<autonat::Event> for MyBehaviourEvent {
        fn from(event: autonat::Event) {
            MyBehaviourEvent::Autonat(event)
        }
    }

    pub async fn start_p2p_node() -> Result<(), Box<dyn std::error::Error>> {
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local peer id: {:?}", local_peer_id);

        let transport = libp2p::development_transport(local_key.clone()).await?;

        // Create a custom gossipsub configuration
        let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by sending heartbeats more often.
            .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation or checks performed by the network.
            .build()?;

        // Create a gossipsub network behaviour
        let mut gossipsub = gossipsub::Gossipsub::new(MessageAuthenticity::Signed(local_key.clone()), gossipsub_config)?;

        // Create a Kademlia network behaviour
        let mut /*kademlia*/ kademlia_config = kad::KademliaConfig::default();
        kademlia_config.set_query_timeout(Duration::from_secs(5 * 60));
        let mut kademlia = kad::Kademlia::new(local_peer_id, kad::store::MemoryStore::new(local_peer_id), kademlia_config);
        
        // Create mDNS, identify, and autonat behaviours
        let mdns = mdns::async_mdns::Behaviour::new(mdns::Config::default(), local_peer_id)?;
        let identify = libp2p::identify::Behaviour::new(libp2p::identify::Config::new("/ipfs/0.1.0".to_string(), local_key.public()));
        let autonat = autonat::Behaviour::new(local_peer_id, autonat::Config::default());


        // Create a Swarm to manage peers and events
        let mut swarm = libp2p::SwarmBuilder::with_tokio_transport(transport, MyBehaviour { gossipsub, kademlia, mdns, identify, autonat }, local_peer_id).build();

        // Listen on all interfaces and whatever port the OS assigns
        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
        swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

        // We enter a loop to receive events from the Swarm.
        let mut stdin = io::BufReader::new(io::stdin()).lines();

        loop {
            select! {
                line = stdin.next().fuse() => {
                    if let Some(Ok(line)) = line {
                        let mut parts = line.splitn(2, ' ');
                        let command = parts.next().unwrap_or("");
                        let arg = parts.next().unwrap_or("");

                        match command {
                            "SUB" => {
                                let topic = gossipsub::Topic::new(arg);
                                if let Err(e) = swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                                    eprintln!("Error subscribing to topic {}: {}", arg, e);
                                } else {
                                    println!("Subscribed to topic: {}", arg);
                                }
                            },
                            "PUB" => {
                                let mut parts = arg.splitn(2, ' ');
                                let topic_str = parts.next().unwrap_or("");
                                let message = parts.next().unwrap_or("");
                                let topic = gossipsub::Topic::new(topic_str);
                                if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic, message.as_bytes()) {
                                    eprintln!("Error publishing to topic {}: {}", topic_str, e);
                                } else {
                                    println!("Published to topic {} message: {}", topic_str, message);
                                }
                            },
                            "DIAL" => {
                                match arg.parse() {
                                    Ok(to_dial) => {
                                        if let Err(e) = swarm.dial(to_dial) {
                                            eprintln!("Error dialing: {}", e);
                                        }
                                    }
                                    Err(err) => eprintln!("Failed to parse address to dial: {}", err),
                                }
                            }
                            _ => { /* Ignore */ }
                        }
                    }
                },
                event = swarm.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("Listening on {:?}", address);
                    },
                    SwarmEvent::Behaviour(event) => match event {
                        MyBehaviourEvent::Gossipsub(gossipsub::GossipsubEvent::Message { propagation_source: peer_id, message_id: id, message }) => {
                            println!(
                                "Got message: 
                                    topic: {},
                                    message ID: {},
                                    message from: {},
                                    message: {}",
                                    message.topic,
                                    id,
                                    peer_id,
                                    String::from_utf8_lossy(&message.data),
                            );
                        },
                        MyBehaviourEvent::Kademlia(KademliaEvent::OutboundQueryCompleted { result, .. }) => {
                            if let kad::QueryResult::GetProviders(ok) = result {
                                for peer in ok.providers {
                                    println!("Peer {:?} provides service.", peer);
                                }
                            }
                        },
                        MyBehaviourEvent::Mdns(mdns::MdnsEvent::Discovered(list)) => {
                            for (peer_id, multiaddr) in list {
                                println!("mDNS discovered a new peer: {} with address {}", peer_id, multiaddr);
                                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                swarm.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
                            }
                        },
                        MyBehaviourEvent::Mdns(mdns::MdnsEvent::Expired(list)) => {
                            for (peer_id, multiaddr) in list {
                                println!("mDNS discovered peer expired: {} with address {}", peer_id, multiaddr);
                                swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                swarm.behaviour_mut().kademlia.remove_address(&peer_id, multiaddr);
                            }
                        },
                        MyBehaviourEvent::Identify(identify_event) => {
                            // println!("Identify event: {:?}", identify_event);
                            if let libp2p::identify::Event::Received { peer_id, info, .. } = identify_event {
                                for addr in info.listen_addrs {
                                    swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                                }
                            }
                        },
                        MyBehaviourEvent::Autonat(autonat_event) => {
                            if let autonat::Event::StatusChanged { new_status, .. } = autonat_event {
                                match new_status {
                                    NatStatus::Public(addr) => println!("Autonat: Public address identified: {}", addr),
                                    NatStatus::Private => println!("Autonat: Private network detected."),
                                    NatStatus::Unknown => println!("Autonat: NAT status unknown."),
                                }
                            }
                        }
                        _ => { /* Ignore other events */ }
                    }
                    _ => { /* Ignore other swarm events */ }
                }
            }
        }
    }
}
