pub mod p2p {
    use libp2p::futures::FutureExt;
    use tokio_stream::StreamExt;
    use libp2p::{
        autonat::{self, NatStatus},
        gossipsub::{self, MessageAuthenticity, Topic},
        identify,
        identity,
        kad::{self, store::MemoryStore},
        mdns,
        swarm::{NetworkBehaviour, SwarmEvent},
        PeerId,
        Transport,
    };
    use std::time::Duration;
    use tokio::{io::{self, AsyncBufReadExt}, select};
    use tokio_stream::wrappers::LinesStream;
    use libp2p_noise::Config as NoiseConfig;
    use libp2p::core::upgrade;
    use libp2p::core::transport::OrTransport;
    use libp2p::tcp;

    #[derive(NetworkBehaviour)]
    #[behaviour(out_event = "MyBehaviourEvent")]
    pub struct MyBehaviour {
        pub gossipsub: gossipsub::Behaviour,
        pub kademlia: kad::Behaviour<MemoryStore>,
        pub mdns: mdns::tokio::Behaviour,
        pub identify: identify::Behaviour,
        pub autonat: autonat::Behaviour,
    }

    #[derive(Debug)]
    pub enum MyBehaviourEvent {
        Gossipsub(gossipsub::Event),
        Kademlia(kad::Event),
        Mdns(mdns::Event),
        Identify(identify::Event),
        Autonat(autonat::Event),
    }

    impl From<gossipsub::Event> for MyBehaviourEvent {
        fn from(event: gossipsub::Event) -> Self {
            MyBehaviourEvent::Gossipsub(event)
        }
    }

    impl From<kad::Event> for MyBehaviourEvent {
        fn from(event: kad::Event) -> Self {
            MyBehaviourEvent::Kademlia(event)
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

    impl From<autonat::Event> for MyBehaviourEvent {
        fn from(event: autonat::Event) -> Self {
            MyBehaviourEvent::Autonat(event)
        }
    }

    pub async fn start_p2p_node() -> Result<(), Box<dyn std::error::Error>> {
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local peer id: {:?}", local_peer_id);

        let transport = {
            let tcp_config = tcp::Config::new().nodelay(true);

            // TCP/WebSocket transport with upgrades
            let tcp_transport = libp2p::dns::tokio::Transport::system(libp2p::tcp::tokio::Transport::new(tcp_config.clone()))?
                .upgrade(upgrade::Version::V1Lazy)
                .authenticate(NoiseConfig::new(&local_key).expect("Failed to create noise config."))
                .multiplex(libp2p::yamux::Config::default())
                .boxed();

            // QUIC transport
            let quic_transport = libp2p::quic::tokio::Transport::new(libp2p::quic::Config::new(&local_key))
                .boxed();

            // Combine transports
            OrTransport::new(tcp_transport, quic_transport)
                .timeout(Duration::from_secs(30))
                .boxed()
        };
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by sending heartbeats more often.
            .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation or checks performed by the network.
            .build()?;
        let mut gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )?;

        let mut kademlia_config = kad::Config::new();
        kademlia_config.set_query_timeout(Duration::from_secs(5 * 60));
        let mut kademlia = kad::Behaviour::with_config(
            local_peer_id,
            MemoryStore::new(local_peer_id),
            kademlia_config,
        );

        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?;
        let identify = identify::Behaviour::new(identify::Config::new(
            "/ipfs/0.1.0".to_string(),
            local_key.public(),
        ));
        let autonat = autonat::Behaviour::new(local_peer_id, autonat::Config::default());

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key)
            .with_tokio()
            .with_other_transport(|_| transport)?
            .with_behaviour(MyBehaviour { gossipsub, kademlia, mdns, identify, autonat })?
            .build();

        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
        swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;

        let mut stdin = LinesStream::new(io::BufReader::new(io::stdin()).lines());

        loop {
            select! {
                line = stdin.next().fuse() => {
                    if let Some(Ok(line)) = line {
                        let mut parts = line.splitn(2, ' ');
                        let command = parts.next().unwrap_or("");
                        let arg = parts.next().unwrap_or("");

                        match command {
                            "SUB" => {
                                let topic = Topic::new(arg);
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
                                let topic = Topic::new(topic_str);
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
                        MyBehaviourEvent::Gossipsub(gossipsub::Event::Message { propagation_source: peer_id, message_id: id, message }) => {
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
                        MyBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })), .. }) => {
                            for peer in providers {
                                println!("Peer {:?} provides service.", peer);
                            }
                        },
                        MyBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. })), .. }) => {
                            println!("Kademlia: No additional providers found.");
                        },
                        MyBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetProviders(Err(err)), .. }) => {
                            eprintln!("Error getting providers: {:?}", err);
                        },
                        MyBehaviourEvent::Mdns(mdns::Event::Discovered(list)) => {
                            for (peer_id, multiaddr) in list {
                                println!("mDNS discovered a new peer: {} with address {}", peer_id, multiaddr);
                                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                swarm.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
                            }
                        },
                        MyBehaviourEvent::Mdns(mdns::Event::Expired(list)) => {
                            for (peer_id, multiaddr) in list {
                                println!("mDNS discovered peer expired: {} with address {}", peer_id, multiaddr);
                                swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                swarm.behaviour_mut().kademlia.remove_address(&peer_id, multiaddr);
                            }
                        },
                        MyBehaviourEvent::Identify(identify_event) => {
                            if let identify::Event::Received { peer_id, info, .. } = identify_event {
                                for addr in info.listen_addrs {
                                    swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                                }
                            }
                        },
                        MyBehaviourEvent::Autonat(autonat_event) => {
                            if let autonat::Event::StatusChanged { new, .. } = autonat_event {
                                match new {
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
