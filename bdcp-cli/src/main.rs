mod relay;
mod alice;
mod bob;

use clap::{Parser, Subcommand};
use std::net::SocketAddr;

#[derive(Parser)]
#[command(name = "bdcp", about = "Blind Data Collaboration Protocol CLI")]
struct Cli {
    /// Use real TFHE-rs backend (heavy) instead of toy cipher
    #[arg(long, global = true)]
    tfhe: bool,

    /// Enable zero-knowledge proofs for ciphertext validity (requires --tfhe)
    #[arg(long, global = true)]
    zkp: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a Relay node that orchestrates encrypted computation
    Relay {
        /// Address to listen on
        #[arg(long, default_value = "0.0.0.0:9447")]
        listen: SocketAddr,
    },
    /// Launch Alice (job creator & key generator)
    Alice {
        /// Relay address
        #[arg(long)]
        relay: SocketAddr,
        /// Single secret value (shorthand for --values)
        #[arg(long, conflicts_with = "values")]
        value: Option<u32>,
        /// Comma-separated secret values (e.g. --values 42,58,100)
        #[arg(long, conflicts_with = "value")]
        values: Option<String>,
    },
    /// Launch Bob (data contributor)
    Bob {
        /// Relay address
        #[arg(long)]
        relay: SocketAddr,
        /// Job ID to join
        #[arg(long)]
        job_id: String,
        /// Single secret value (shorthand for --values)
        #[arg(long, conflicts_with = "values")]
        value: Option<u32>,
        /// Comma-separated secret values (e.g. --values 42,58,100)
        #[arg(long, conflicts_with = "value")]
        values: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("rustls crypto provider");
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    if cli.zkp && !cli.tfhe {
        anyhow::bail!("--zkp requires --tfhe (ZKP proofs are only available with TFHE-rs backend)");
    }

    if cli.tfhe {
        tracing::info!("Using TFHE-rs backend (real FHE)");
    } else {
        tracing::info!("Using toy-FHE backend (fast, NOT secure)");
    }
    if cli.zkp {
        tracing::info!("Zero-knowledge proofs enabled (ciphertext integrity)");
    }

    match cli.command {
        Commands::Relay { listen } => relay::run_relay(listen, cli.tfhe, cli.zkp).await,
        Commands::Alice { relay, value, values } => {
            let vals = parse_values(value, values)?;
            alice::run_alice(relay, &vals, cli.tfhe, cli.zkp).await
        }
        Commands::Bob { relay, job_id, value, values } => {
            let vals = parse_values(value, values)?;
            bob::run_bob(relay, &job_id, &vals, cli.tfhe, cli.zkp).await
        }
    }
}

fn parse_values(single: Option<u32>, multi: Option<String>) -> anyhow::Result<Vec<u32>> {
    if let Some(vals) = multi {
        vals.split(',')
            .map(|s| {
                s.trim()
                    .parse::<u32>()
                    .map_err(|e| anyhow::anyhow!("invalid value '{s}': {e}"))
            })
            .collect()
    } else if let Some(v) = single {
        Ok(vec![v])
    } else {
        anyhow::bail!("either --value or --values is required");
    }
}
