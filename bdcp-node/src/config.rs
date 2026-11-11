use serde::Deserialize;
use std::path::Path;

/// Top-level daemon configuration from `bdcp.toml`.
#[derive(Clone, Debug, Deserialize)]
pub struct DaemonConfig {
    pub node: NodeConfig,
    pub network: NetworkConfig,
    pub crypto: CryptoConfig,

    /// Optional path to a Unix domain socket for local IPC
    /// (used by Alice and Bob daemons; relay ignores this).
    #[serde(default)]
    pub ipc: Option<IpcConfig>,
}

/// Node identity.
#[derive(Clone, Debug, Deserialize)]
pub struct NodeConfig {
    /// Human-readable node name.
    pub name: String,
    /// Role: "relay", "alice", or "bob".
    pub role: String,
    /// QUIC listen address (e.g. "0.0.0.0:9447").
    pub listen_addr: String,
}

/// Network tuning.
#[derive(Clone, Debug, Deserialize)]
pub struct NetworkConfig {
    /// Relay address to connect to (for Alice and Bob).
    #[serde(default)]
    pub relay_addr: Option<String>,
    /// QUIC keep-alive interval in seconds.
    #[serde(default = "default_keep_alive")]
    #[allow(dead_code)]
    pub keep_alive_secs: u64,
    /// QUIC idle timeout in seconds.
    #[serde(default = "default_idle_timeout")]
    #[allow(dead_code)]
    pub idle_timeout_secs: u64,
}

fn default_keep_alive() -> u64 { 10 }
fn default_idle_timeout() -> u64 { 120 }

/// Crypto backend selection.
#[derive(Clone, Debug, Deserialize)]
pub struct CryptoConfig {
    /// Backend: "tfhe" or "toy".
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Enable zero-knowledge proofs (requires `backend = "tfhe"`).
    #[serde(default)]
    pub enable_zkp: bool,
}

fn default_backend() -> String { "toy".into() }

/// Local IPC endpoint (Unix domain socket).
#[derive(Clone, Debug, Deserialize)]
pub struct IpcConfig {
    /// Path to the Unix domain socket (e.g. "/var/run/bdcp.sock").
    pub socket_path: String,
}

impl DaemonConfig {
    /// Load and parse config from a TOML file path.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let data = std::fs::read_to_string(path.as_ref())
            .map_err(|e| anyhow::anyhow!("failed to read config '{}': {e}", path.as_ref().display()))?;
        toml::from_str(&data)
            .map_err(|e| anyhow::anyhow!("failed to parse config: {e}"))
    }

    /// Whether the daemon should use TFHE-rs.
    pub fn use_tfhe(&self) -> bool {
        self.crypto.backend == "tfhe"
    }

    /// Whether ZKP proofs are enabled.
    pub fn use_zkp(&self) -> bool {
        self.crypto.enable_zkp
    }
}
