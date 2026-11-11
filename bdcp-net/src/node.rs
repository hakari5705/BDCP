//! BDCP node — QUIC-based peer.

use crate::config::NodeConfig;
use crate::message::ControlFrame;
use crate::stream::{ControlStream, DataStream};
use anyhow::Result;
use bdcp_core::protocol::messages::Message;
use bdcp_core::types::NodeId;
use quinn::{Connection, Endpoint};
use rcgen::{CertificateParams, KeyPair};
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tracing::info;

/// Dev-mode verifier that accepts any server certificate.
#[derive(Debug)]
struct DevVerifier;

impl rustls::client::danger::ServerCertVerifier for DevVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

/// Whether this node acts as a client or relay.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeRole {
    /// Initiates connections, submits encrypted data.
    Client,
    /// Accepts connections, orchestrates encrypted computations.
    Relay,
}

/// A BDCP network node.
///
/// Wraps a QUIC endpoint with BDCP protocol logic.
pub struct BdcpNode {
    pub role: NodeRole,
    pub id: NodeId,
    pub endpoint: Endpoint,
}

impl BdcpNode {
    /// Create and bind a new BDCP node.
    pub async fn new(role: NodeRole, config: NodeConfig) -> Result<Self> {
        let (mut server_config, mut client_config) = if let Some(tls) = &config.tls {
            Self::tls_configs(tls)?
        } else {
            Self::dev_tls_configs()?
        };

        // Apply FHE-tuned transport config to both sides
        let tc = Arc::new(crate::config::fhe_transport_config());
        server_config.transport_config(Arc::clone(&tc));
        client_config.transport_config(tc);

        let addr = config
            .listen
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| anyhow::anyhow!("invalid listen address"))?;

        let mut endpoint = if role == NodeRole::Relay {
            Endpoint::server(server_config, addr)?
        } else {
            Endpoint::client(addr)?
        };
        endpoint.set_default_client_config(client_config);

        info!(
            "BDCP node '{}' ({:?}) listening on {}",
            config.id, role, addr
        );

        Ok(Self {
            role,
            id: NodeId(config.id),
            endpoint,
        })
    }

    /// Connect to a remote peer (client side).
    pub async fn connect(&self, addr: &str) -> Result<Connection> {
        let addr = addr.to_socket_addrs()?.next().unwrap();
        let conn = self.endpoint.connect(addr, "localhost")?.await?;
        info!("Connected to {}", addr);
        Ok(conn)
    }

    /// Accept an incoming connection (relay side).
    pub async fn accept(&self) -> Result<Connection> {
        let incoming = self.endpoint.accept().await.unwrap();
        let conn = incoming.await?;
        info!("Accepted connection from {}", conn.remote_address());
        Ok(conn)
    }

    // ═══════════════════════════════════════════════════════════
    //  Stream helpers
    // ═══════════════════════════════════════════════════════════

    /// Open a bi-directional control stream to send a BDCP message.
    pub async fn open_control(&self, conn: &Connection) -> Result<ControlStream> {
        let (send, recv) = conn.open_bi().await?;
        Ok(ControlStream::new(send, recv))
    }

    /// Accept a bi-directional control stream from an incoming connection.
    pub async fn accept_control(&self, conn: &Connection) -> Result<ControlStream> {
        let (send, recv) = conn.accept_bi().await?;
        Ok(ControlStream::new(send, recv))
    }

    // ═══════════════════════════════════════════════════════════
    //  High-level protocol operations
    // ═══════════════════════════════════════════════════════════

    /// Send a BDCP protocol message over a new control stream.
    pub async fn send_message(&self, conn: &Connection, msg: &Message) -> Result<()> {
        let mut stream = self.open_control(conn).await?;
        let frame = ControlFrame {
            message: msg.clone(),
        };
        stream.send(&frame).await?;
        Ok(())
    }

    /// Stream a large payload (e.g. CompressedServerKey) over a data stream.
    pub async fn stream_data(&self, conn: &Connection, payload: &[u8]) -> Result<()> {
        let send = conn.open_uni().await?;
        let mut stream = DataStream::new_sender(send);
        stream.send(payload).await?;
        info!("Streamed {} bytes", payload.len());
        Ok(())
    }

    /// Receive a large payload from a data stream.
    pub async fn receive_data(&self, conn: &Connection) -> Result<Vec<u8>> {
        let recv = conn.accept_uni().await?;
        let mut stream = DataStream::new_receiver(recv);
        let payload = stream.recv().await?;
        info!("Received {} bytes", payload.len());
        Ok(payload)
    }

    // ═══════════════════════════════════════════════════════════
    //  TLS setup
    // ═══════════════════════════════════════════════════════════

    fn dev_tls_configs() -> Result<(quinn::ServerConfig, quinn::ClientConfig)> {
        let key = KeyPair::generate()?;
        let cert = CertificateParams::new(vec!["localhost".into()])?
            .self_signed(&key)?;

        let server_config = quinn::ServerConfig::with_single_cert(
            vec![cert.der().clone().into()],
            rustls::pki_types::PrivateKeyDer::Pkcs8(key.serialize_der().into()),
        )?;

        // Dev mode: skip certificate verification on the client side
        let tls_client = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(DevVerifier))
            .with_no_client_auth();

        let quic_config = quinn::crypto::rustls::QuicClientConfig::try_from(tls_client)?;
        let client_config = quinn::ClientConfig::new(Arc::new(quic_config));

        Ok((server_config, client_config))
    }

    fn tls_configs(tls: &crate::TlsConfig) -> Result<(quinn::ServerConfig, quinn::ClientConfig)> {
        let server_config = quinn::ServerConfig::with_single_cert(
            vec![tls.server_cert.clone().into()],
            rustls::pki_types::PrivateKeyDer::try_from(tls.server_key.clone())
                .map_err(|_| anyhow::anyhow!("invalid TLS private key"))?,
        )?;

        let mut roots = rustls::RootCertStore::empty();
        roots.add(tls.server_cert.clone().into())?;

        let tls_client = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        let quic_config = quinn::crypto::rustls::QuicClientConfig::try_from(tls_client)?;
        let client_config = quinn::ClientConfig::new(Arc::new(quic_config));

        Ok((server_config, client_config))
    }
}
