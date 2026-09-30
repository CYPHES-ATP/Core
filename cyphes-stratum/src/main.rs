//! `cyphes-stratum`: serve a CYPHES node's block templates to BeamHash III
//! GPU miners over Beam's stratum protocol.
//!
//! ```text
//! cyphes-stratum --node http://127.0.0.1:2975 --rpc-cookie <cache dir>/.cookie
//! lolMiner --algo BEAM-III --pool 127.0.0.1:2977 --user rig1 --tls off
//! ```

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use clap::Parser;
use cyphes_stratum::{
    node::Node,
    server::{Bridge, Config},
};

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// The node's JSON-RPC address.
    #[arg(long, default_value = "http://127.0.0.1:2975")]
    node: String,
    /// The node's RPC cookie file (`<state cache dir>/.cookie`), if the node
    /// uses cookie authentication.
    #[arg(long)]
    rpc_cookie: Option<PathBuf>,
    /// Where miners connect. Use 0.0.0.0:2977 to accept rigs on the network.
    #[arg(long, default_value = "127.0.0.1:2977")]
    listen: SocketAddr,
    /// Beam share difficulty to ask miners for, below the block difficulty,
    /// so rigs report work between blocks. Blocks only, by default.
    #[arg(long)]
    share_difficulty: Option<u64>,
    /// Hex digits of nonce prefix to assign each connection (0 to 6).
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=6))]
    nonce_prefix_digits: u8,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();

    let node = Node::new(&args.node, args.rpc_cookie.as_deref())?;
    let bridge = Bridge::new(
        node,
        Config {
            share_difficulty: args.share_difficulty,
            nonce_prefix_digits: args.nonce_prefix_digits.into(),
        },
    );
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!("stratum listening on {}, node {}", args.listen, args.node);

    tokio::spawn(bridge.clone().run_templates());
    tokio::spawn(bridge.clone().serve(listener));
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = ticker.tick() => tracing::info!(stats = ?bridge.stats, "totals"),
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}
