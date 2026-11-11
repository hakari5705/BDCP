pub mod messages;

use thiserror::Error;

/// Top-level protocol errors.
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("crypto error: {0}")]
    CryptoError(String),
}

impl From<crate::crypto::fhe::FheError> for ProtocolError {
    fn from(e: crate::crypto::fhe::FheError) -> Self {
        ProtocolError::CryptoError(e.to_string())
    }
}

impl From<crate::crypto::zkp::ZkpError> for ProtocolError {
    fn from(e: crate::crypto::zkp::ZkpError) -> Self {
        ProtocolError::CryptoError(e.to_string())
    }
}
