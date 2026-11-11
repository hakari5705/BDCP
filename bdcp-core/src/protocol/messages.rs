use crate::types::*;
use serde::{Deserialize, Serialize};

/// Every wire message in the BDCP protocol.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub kind: MessageKind,
    pub sender: NodeId,
    pub job_id: JobId,
    pub timestamp: u64,
}

/// Variants of all protocol messages.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MessageKind {
    /// Start a new collaborative computation.
    JobRequest(JobRequest),
    /// Accept a job and advertise capabilities.
    JobAccept(JobAccept),

    /// Request the compact public key for a job (Bob → Relay).
    GetPublicKey(GetPublicKeyRequest),
    /// Response with the compact public key (Relay → Bob).
    PublicKeyResponse(PublicKeyResponse),

    /// Final result.
    ComputeResult(ComputeResult),

    /// Error.
    Error(ProtocolErrorMsg),
}

/// Request to start a new job.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobRequest {
    pub description: String,
    pub required_participants: u32,
    pub fhe_scheme: String,
    pub security_level: SecurityLevel,
}

/// Acceptance of a job.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobAccept {
    pub job_id: JobId,
    pub supported_ops: Vec<String>,
}

/// A computation result, possibly intermediate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComputeResult {
    pub data: Ciphertext,
    pub proof: Option<ComputationProof>,
}

/// Error payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProtocolErrorMsg {
    pub code: u32,
    pub detail: String,
}

/// Request to fetch the compact public key for a job.
/// Sent by Bob when he wants to encrypt his data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetPublicKeyRequest {
    pub job_id: JobId,
}

/// Response to a GetPublicKey request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicKeyResponse {
    pub compact_public_key: CompactPublicKey,
}
