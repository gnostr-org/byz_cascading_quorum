#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    byz_time::p2p::start_p2p_node().await
}
