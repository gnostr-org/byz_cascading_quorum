use byz_time::p2p::{evt_loop, InternalEvent};
use libp2p::gossipsub::IdentTopic;
use std::time::Duration;
use tracing::{info, debug};
use tracing_subscriber::EnvFilter;

#[tokio::test]
async fn test_p2p_time_consensus() {
    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .try_init();

    let topic = IdentTopic::new("consensus-test");
    let mut addrs = Vec::new();
    let mut senders = Vec::new();

    // Spawn 2 nodes and collect their addresses
    for i in 0..2 {
        let topic_clone = topic.clone();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
        let (send_tx, send_rx) = tokio::sync::mpsc::channel(100);
        let (recv_tx, _recv_rx) = tokio::sync::mpsc::channel(100);
        
        senders.push(send_tx);

        tokio::spawn(async move {
            info!("Starting Node {}", i);
            if let Err(e) = evt_loop(send_rx, recv_tx, topic_clone, Some(addr_tx)).await {
                eprintln!("Node {} error: {:?}", i, e);
            }
        });

        // Wait for the node to report its address
        if let Ok(addr) = addr_rx.await {
            info!("Node {} reporting address: {}", i, addr);
            addrs.push(addr);
        }
    }

    // Have the first node dial the second node
    if addrs.len() == 2 {
        info!("Manual dialing: Node 0 -> Node 1 ({})", addrs[1]);
        let _ = senders[0].send(InternalEvent::Dial(addrs[1].clone())).await;
    }

    info!("Simulation running for 60 seconds. Check logs for TimeSync messages...");
    tokio::time::sleep(Duration::from_secs(60)).await;
    info!("Simulation finished.");
}
