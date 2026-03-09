use byz_time::p2p::{evt_loop, InternalEvent, MsgKind, Msg, IPFS_PROTO_NAME};
use libp2p::gossipsub::IdentTopic;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Dummy channels for compilation
    let (send_tx, send_rx) = tokio::sync::mpsc::channel(100);
    let (recv_tx, recv_rx) = tokio::sync::mpsc::channel(100);

    // Dummy topic for compilation
    let topic = IdentTopic::new("dummy-topic");

    evt_loop(send_rx, recv_tx, topic).await.map_err(anyhow::Error::into)
}
