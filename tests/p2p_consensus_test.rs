use byz_time::p2p::{evt_loop, InternalEvent};
use libp2p::gossipsub::IdentTopic;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[tokio::test]
async fn test_p2p_time_consensus() {
    // Initialize tracing to see debug/trace logs
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .try_init();

    let topic = IdentTopic::new("consensus-test");

    // Spawn 3 nodes
    for i in 0..3 {
        let topic_clone = topic.clone();
        tokio::spawn(async move {
            let (send_tx, send_rx) = tokio::sync::mpsc::channel(100);
            let (recv_tx, _recv_rx) = tokio::sync::mpsc::channel(100);

            println!("Starting Node {}", i);
            if let Err(e) = evt_loop(send_rx, recv_tx, topic_clone).await {
                eprintln!("Node {} error: {:?}", i, e);
            }
        });
    }

    // Run the test for 60 seconds to observe consensus
    println!("Simulation running for 60 seconds. Check logs for TimeSync messages...");
    tokio::time::sleep(Duration::from_secs(60)).await;
    println!("Simulation finished.");
}
