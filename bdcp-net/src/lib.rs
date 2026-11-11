//! bdcp-net: network layer for the Blind Data Collaboration Protocol.
//!
//! Built on QUIC (via quinn) for mandatory encryption, multiplexing,
//! and stream concurrency.
//!
//! ## Stream architecture
//!
//! Each QUIC connection carries two kinds of streams:
//!
//! - **Control stream** (bi-directional, short-lived):
//!   Handles protocol handshake messages (`JobRequest`, `KeyExchange`, etc.)
//!
//! - **Data stream** (uni-directional, one-shot):
//!   Transfers large payloads:
//!   • `CompressedServerKey` (~59 MB) — once per session
//!   • Batch of `Ciphertext`s (~257 KB × N) — per data submission

pub mod config;
pub mod message;
pub mod node;
pub mod stream;

pub use config::{NodeConfig, TlsConfig};
pub use node::{BdcpNode, NodeRole};
pub use stream::{ControlStream, DataStream};
