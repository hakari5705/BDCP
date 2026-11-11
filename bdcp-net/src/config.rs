//! TLS and transport configuration for QUIC connections.

use quinn::VarInt;
use std::time::Duration;

/// TLS configuration for QUIC connections.
pub struct TlsConfig {
    pub server_cert: Vec<u8>,
    pub server_key: Vec<u8>,
}

/// Node configuration.
pub struct NodeConfig {
    /// Human-readable node identifier.
    pub id: String,
    /// Listen address (e.g. "127.0.0.1:9447").
    pub listen: String,
    /// Optional TLS config. If None, a self-signed cert is generated.
    pub tls: Option<TlsConfig>,
}

impl NodeConfig {
    pub fn new(id: impl Into<String>, listen: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            listen: listen.into(),
            tls: None,
        }
    }

    pub fn with_tls(mut self, tls: TlsConfig) -> Self {
        self.tls = Some(tls);
        self
    }
}

/// Create a transport config tuned for FHE payloads (up to 60 MB).
///
/// Default quinn limits are far too small for `CompressedServerKey` (~59 MB).
/// This raises stream/connection windows and idle timeout,
/// and enables keep-alive to prevent timeouts during ZKP computation.
pub fn fhe_transport_config() -> quinn::TransportConfig {
    let mut tc = quinn::TransportConfig::default();
    tc.max_idle_timeout(Some(Duration::from_secs(120).try_into().unwrap()));
    tc.keep_alive_interval(Some(Duration::from_secs(10)));
    tc.stream_receive_window(VarInt::from_u32(64 * 1024 * 1024));
    tc.receive_window(VarInt::from_u32(96 * 1024 * 1024));
    tc
}
