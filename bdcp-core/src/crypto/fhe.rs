use crate::types::*;
use std::fmt::Debug;

/// Error type for FHE operations.
#[derive(Debug, thiserror::Error)]
pub enum FheError {
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
    #[error("backend not available: {0}")]
    BackendUnavailable(String),
}

/// Result alias for FHE operations.
pub type FheResult<T> = Result<T, FheError>;

/// Abstract FHE keychain owned by a single party.
#[derive(Clone, Debug)]
pub struct FheKeychain {
    pub secret_key: SecretKey,
    pub public_key: PublicKey,
    pub compact_public_key: CompactPublicKey,
    pub evaluation_key: EvaluationKey,
}

/// A fully-homomorphic encryption engine.
///
/// Each implementation (TFHE, BFV, CKKS, etc.) provides its own backend.
///
/// # Backend Availability
///
/// Several trait methods return `Err(BackendUnavailable(…))` in the TFHE
/// backend because they require a `ServerKey` to be set via `with_server_key`.
/// Use the concrete extension methods on `TfheEngine` instead:
///
/// | Trait method             | Use instead                  |
/// |--------------------------|------------------------------|
/// | `encrypt`                | `TfheEngine::encrypt_u32`    |
/// | `encrypt_compact`        | `TfheEngine::encrypt_u32_with_compact` |
/// | `decrypt`                | `TfheEngine::decrypt_u32`    |
/// | `add`                    | `+` operator after `tfhe::set_server_key` |
/// | `mul`                    | `*` operator after `tfhe::set_server_key` |
/// | `mul_plain` / `add_plain`| `+`/`*` with literal after `set_server_key` |
pub trait FheEngine: Debug + Send + Sync {
    /// Friendly name of the backend (e.g. "tfhe-rs-v1.6-integer").
    fn name(&self) -> &str;

    /// Generate a fresh keychain at the given security level.
    fn generate_keychain(&self, level: SecurityLevel) -> FheResult<FheKeychain>;

    /// Encrypt a plaintext under the given public key.
    ///
    /// NOTE: TFHE backend returns `BackendUnavailable` — use
    /// `TfheEngine::encrypt_u32` instead.
    fn encrypt(&self, plain: &Plaintext, pk: &PublicKey) -> FheResult<Ciphertext>;

    /// Encrypt a plaintext under a compact public key.
    ///
    /// Allows third parties (e.g. Bob) to encrypt data for a job
    /// without access to the secret key.
    ///
    /// NOTE: TFHE backend returns `BackendUnavailable` — use
    /// `TfheEngine::encrypt_u32_with_compact` instead.
    fn encrypt_compact(&self, plain: &Plaintext, cpk: &CompactPublicKey) -> FheResult<Ciphertext>;

    /// Decrypt a ciphertext with the secret key.
    ///
    /// NOTE: TFHE backend returns `BackendUnavailable` — use
    /// `TfheEngine::decrypt_u32` instead.
    fn decrypt(&self, cipher: &Ciphertext, sk: &SecretKey) -> FheResult<Plaintext>;

    /// Add two ciphertexts homomorphically (element-wise).
    ///
    /// NOTE: TFHE backend returns `BackendUnavailable` — requires
    /// `tfhe::set_server_key` first, then use `+` operator.
    fn add(&self, a: &Ciphertext, b: &Ciphertext, ek: &EvaluationKey) -> FheResult<Ciphertext>;

    /// Multiply two ciphertexts homomorphically.
    ///
    /// NOTE: TFHE backend returns `BackendUnavailable`.
    fn mul(&self, a: &Ciphertext, b: &Ciphertext, ek: &EvaluationKey) -> FheResult<Ciphertext>;

    /// Multiply a ciphertext by a known scalar (plaintext).
    ///
    /// NOTE: TFHE backend returns `BackendUnavailable`.
    fn mul_plain(&self, ct: &Ciphertext, pt: &Plaintext, ek: &EvaluationKey) -> FheResult<Ciphertext>;

    /// Add a known scalar to a ciphertext.
    ///
    /// NOTE: TFHE backend returns `BackendUnavailable`.
    fn add_plain(&self, ct: &Ciphertext, pt: &Plaintext, ek: &EvaluationKey) -> FheResult<Ciphertext>;

    /// Rotate ciphertext elements by `n` positions (for vector encodings).
    fn rotate(&self, ct: &Ciphertext, n: usize, ek: &EvaluationKey) -> FheResult<Ciphertext>;
}
