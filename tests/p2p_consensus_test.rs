use std::time::Duration;

use byz_time::p2p::{InternalEvent, evt_loop};
use libp2p::gossipsub::IdentTopic;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::test]
async fn test_p2p_time_consensus() {
    // Initialize tracing with a restrictive filter
    let filter = EnvFilter::from_default_env()
        .add_directive(tracing::Level::WARN.into())
        .add_directive("byz_time=info".parse().unwrap())
        .add_directive("libp2p_mdns=off".parse().unwrap());

    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    let topic = IdentTopic::new("consensus-test");
    let mut addrs = Vec::new();
    let mut senders = Vec::new();

    let offsets = vec![0, 1, -1, 2, -2];

    // Spawn 3 nodes and collect their addresses
    for i in 0..5 {
        let topic_clone = topic.clone();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
        let (send_tx, send_rx) = tokio::sync::mpsc::channel(100);
        let (recv_tx, _recv_rx) = tokio::sync::mpsc::channel(100);
        let initial_offset = offsets[i];

        senders.push(send_tx);

        tokio::spawn(async move {
            info!(
                "Starting Node {} with initial offset {}s",
                i, initial_offset
            );
            if let Err(e) = evt_loop(
                send_rx,
                recv_tx,
                topic_clone,
                Some(addr_tx),
                initial_offset,
                vec![],
                6,
            )
            .await
            {
                eprintln!("Node {} error: {:?}", i, e);
            }
        });

        if let Ok(addr) = addr_rx.await {
            info!("Node {} reporting address: {}", i, addr);
            addrs.push(addr);
        }
    }

    // Connect nodes in a ring topology
    if addrs.len() == 5 {
        info!("Connecting nodes in a ring...");
        for i in 0..addrs.len() {
            let next_node_idx = (i + 1) % addrs.len();
            let _ = senders[i]
                .send(InternalEvent::Dial(addrs[next_node_idx].clone()))
                .await;
            info!("Node {} dialing Node {}", i, next_node_idx);
        }
    }

    info!("Quorum running. Waiting 30 seconds for stabilization before late joiner...");
    tokio::time::sleep(Duration::from_secs(30)).await;

    // Spawn a 4th node (Late Joiner) using Node 0 as bootstrap
    let bootstrap_addr = addrs[0].clone();
    let topic_clone = topic.clone();
    tokio::spawn(async move {
        let (_addr_tx, _addr_rx) = tokio::sync::oneshot::channel::<libp2p::Multiaddr>();
        let (_send_tx, send_rx) = tokio::sync::mpsc::channel(100);
        let (recv_tx, _recv_rx) = tokio::sync::mpsc::channel(100);
        let initial_offset = 60; // Huge 1-minute offset

        info!(
            "Starting Late Joiner Node 3 with initial offset {}s using bootstrap {}",
            initial_offset, bootstrap_addr
        );
        if let Err(e) = evt_loop(
            send_rx,
            recv_tx,
            topic_clone,
            None,
            initial_offset,
            vec![bootstrap_addr],
            6,
        )
        .await
        {
            eprintln!("Late Joiner Node 3 error: {:?}", e);
        }
    });

    info!(
        "Late joiner started. Observe Quorum Consensus Reports for Node 3 to join and sync. Running indefinitely."
    );
    std::future::pending::<()>().await;
}
