use anyhow::Result;
use tracing::{debug, trace};
use byz_time::p2p::evt_loop;
use libp2p::gossipsub::IdentTopic;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    debug!("p2p_node: Starting p2p node.");
    // Dummy channels for compilation
    let (_send_tx, send_rx) = tokio::sync::mpsc::channel(100);
    let (recv_tx, _recv_rx) = tokio::sync::mpsc::channel(100);

    // Dummy topic for compilation
    let topic = IdentTopic::new("dummy-topic");

    evt_loop(send_rx, recv_tx, topic)
        .await
        .map_err(anyhow::Error::into)
}
