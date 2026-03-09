use byz_time::p2p::{evt_loop, InternalEvent};
use libp2p::gossipsub::IdentTopic;
use std::time::Duration;
use tracing::{info, debug};
use tracing_subscriber::EnvFilter;

#[tokio::test]
async fn test_p2p_time_consensus() {
    // Initialize tracing with a restrictive filter
    let filter = EnvFilter::from_default_env()
        .add_directive(tracing::Level::WARN.into())
        .add_directive("byz_time=info".parse().unwrap());

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init();

    let topic = IdentTopic::new("consensus-test");
    let mut addrs = Vec::new();
    let mut senders = Vec::new();

    // Spawn 3 nodes with different initial offsets
    // Node 0: 0s offset
    // Node 1: +10s offset
    // Node 2: -15s offset
    let offsets = vec![0, 10, -15];

    for i in 0..3 {
        let topic_clone = topic.clone();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
        let (send_tx, send_rx) = tokio::sync::mpsc::channel(100);
        let (recv_tx, _recv_rx) = tokio::sync::mpsc::channel(100);
        let initial_offset = offsets[i];
        
        senders.push(send_tx);

        tokio::spawn(async move {
            info!("Starting Node {} with initial offset {}s", i, initial_offset);
            if let Err(e) = evt_loop(send_rx, recv_tx, topic_clone, Some(addr_tx), initial_offset).await {
                eprintln!("Node {} error: {:?}", i, e);
            }
        });

        // Wait for the node to report its address
        if let Ok(addr) = addr_rx.await {
            info!("Node {} reporting address: {}", i, addr);
            addrs.push(addr);
        }
    }

    // Connect nodes: Node 0 -> Node 1, Node 1 -> Node 2
    if addrs.len() == 3 {
        info!("Manual dialing: Node 0 -> Node 1 ({})", addrs[1]);
        let _ = senders[0].send(InternalEvent::Dial(addrs[1].clone())).await;
        
        info!("Manual dialing: Node 1 -> Node 2 ({})", addrs[2]);
        let _ = senders[1].send(InternalEvent::Dial(addrs[2].clone())).await;
    }

    info!("Simulation running indefinitely. Observe Quorum Consensus Reports in logs. Press Ctrl+C to stop.");
    std::future::pending::<()>().await;
}
