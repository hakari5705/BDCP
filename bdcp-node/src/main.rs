mod client;
mod config;
mod ipc;
mod relay;

use clap::Parser;

#[derive(Parser)]
#[command(name = "bdcp-node", about = "Blind Data Collaboration Protocol Node Daemon")]
struct Args {
    /// Path to configuration file (default: /etc/bdcp/bdcp.toml)
    #[arg(long, default_value = "/etc/bdcp/bdcp.toml")]
    config: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("rustls crypto provider");
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let cfg = config::DaemonConfig::load(&args.config)
        .map_err(|e| anyhow::anyhow!("failed to load config '{}': {e}", args.config.display()))?;

    tracing::info!(
        "Starting '{}' as {} on {} (tfhe={}, zkp={})",
        cfg.node.name,
        cfg.node.role,
        cfg.node.listen_addr,
        cfg.use_tfhe(),
        cfg.use_zkp(),
    );

    let listen: std::net::SocketAddr = cfg.node.listen_addr.parse()
        .map_err(|e| anyhow::anyhow!("invalid listen_addr '{}': {e}", cfg.node.listen_addr))?;

    match cfg.node.role.as_str() {
        "relay" => relay::run(&cfg.node.name, listen, cfg.use_tfhe(), cfg.use_zkp()).await,
        "alice" | "bob" => {
            let socket_path = match &cfg.ipc {
                Some(ipc_cfg) => &ipc_cfg.socket_path,
                None => {
                    tracing::warn!("No [ipc] section in config; daemon starts but no IPC available");
                    // Still run but warn
                    "/tmp/bdcp.sock"
                }
            };
            ipc::serve_ipc(&cfg, socket_path).await
        }
        other => anyhow::bail!("unknown role '{other}' — expected 'relay', 'alice', or 'bob'"),
    }
}
