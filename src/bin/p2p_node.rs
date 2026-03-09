use anyhow::Result;
use byz_time::p2p::evt_loop;
use clap::Parser;
use libp2p::{Multiaddr, gossipsub::IdentTopic};
use tracing::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Bootstrap node multiaddress to dial
    #[arg(short, long)]
    bootstrap: Option<Multiaddr>,

    /// Initial clock offset in seconds
    #[arg(short, long, default_value_t = 0)]
    offset: i64,

    /// Gossipsub topic to join
    #[arg(short, long, default_value = "gnostr")]
    topic: String,

    /// Total number of nodes expected in the quorum
    #[arg(short, long)]
    num_nodes: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize tracing
    let _ = tracing_subscriber::fmt::init();

    info!("p2p_node: Starting node with offset {}s", args.offset);

    // Dummy channels for internal communication (TUI would use these)
    let (_send_tx, send_rx) = tokio::sync::mpsc::channel(100);
    let (recv_tx, _recv_rx) = tokio::sync::mpsc::channel(100);

    let topic = IdentTopic::new(args.topic);
    let bootstrap_nodes = args.bootstrap.map(|a| vec![a]).unwrap_or_default();

    evt_loop(
        send_rx,
        recv_tx,
        topic,
        None,
        args.offset,
        bootstrap_nodes,
        args.num_nodes,
    )
    .await
    .map_err(anyhow::Error::into)
}
