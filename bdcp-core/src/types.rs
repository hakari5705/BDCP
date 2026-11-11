use serde::{Deserialize, Serialize};

/// Opaque handle to encrypted data (ciphertext).
/// The actual bytes are backend-specific.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ciphertext(pub Vec<u8>);

/// Plaintext data before encryption.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plaintext(pub Vec<u8>);

/// Evaluation key needed to compute on ciphertexts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvaluationKey(pub Vec<u8>);

/// Public key for encryption.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicKey(pub Vec<u8>);

/// Compact public key — allows third parties (e.g. Bob) to encrypt
/// data without knowing the secret key.
/// Significantly smaller than PublicKey and serialization-friendly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactPublicKey(pub Vec<u8>);

/// Secret key for decryption (never leaves the owner's domain).
///
/// Memory is zeroed on drop to prevent key leakage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretKey(pub Vec<u8>);

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.iter_mut().for_each(|b| *b = 0);
    }
}

/// A signed proof that a computation was performed correctly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComputationProof(pub Vec<u8>);

/// A zero-knowledge proof for compact public key encryption correctness.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactProof(pub Vec<u8>);

/// Common Reference String for ZK proof generation/verification.
/// Generated offline by the key owner (Alice), shared with Relay and Bob.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Crs(pub Vec<u8>);

/// Unique identifier for a node in the network.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

/// Unique identifier for a computation job.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct JobId(pub String);

/// Parameter set / security level.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SecurityLevel {
    Standard,  // 128-bit
}

impl Default for SecurityLevel {
    fn default() -> Self {
        Self::Standard
    }
}
