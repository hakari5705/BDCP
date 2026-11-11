use crate::types::*;
use std::fmt::Debug;

/// Error type for ZKP operations.
#[derive(Debug, thiserror::Error)]
pub enum ZkpError {
    #[error("proof generation failed: {0}")]
    ProofGenerationFailed(String),
    #[error("proof verification failed: {0}")]
    ProofVerificationFailed(String),
    #[error("CRS generation failed: {0}")]
    CrsGenerationFailed(String),
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),
}

/// Result alias for ZKP operations.
pub type ZkpResult<T> = Result<T, ZkpError>;

/// Abstracts zero-knowledge proof operations for FHE.
///
/// In tfhe-rs, zk-pok proves that compact public key encryption is valid
/// (ciphertexts are well-formed) without revealing the plaintext.
/// Computation integrity proofs (add, mul) are not yet supported by the
/// tfhe-rs zk-pok feature — they are reserved for future extension.
pub trait ZkpEngine: Debug + Send + Sync {
    /// Name of the zero-knowledge backend.
    fn name(&self) -> &str;

    /// Generate a Common Reference String from the given config parameters.
    ///
    /// The CRS is an offline-phase artifact shared among all parties (Alice,
    /// Relay, Bob) before any encrypted data is exchanged.
    fn generate_crs(&self, log2_num_slots: usize) -> ZkpResult<Crs>;

    /// Create a proven compact ciphertext list.
    ///
    /// Encrypts `values` under `cpk`, attaching a ZK proof that the resulting
    /// ciphertexts are valid. Returns the serialised `ProvenCompactCiphertextList`.
    fn prove_encrypt_compact(
        &self,
        values: &[u32],
        cpk: &CompactPublicKey,
        crs: &Crs,
        metadata: &[u8],
        load: ZkProofLoad,
    ) -> ZkpResult<(Ciphertext, CompactProof)>;

    /// Verify a proven compact ciphertext list and expand to individual ciphertexts.
    ///
    /// Returns the serialised `FheUint32` ciphertexts ready for homomorphic computation.
    fn verify_and_expand(
        &self,
        proof: &CompactProof,
        cpk: &CompactPublicKey,
        crs: &Crs,
        metadata: &[u8],
        num_values: usize,
    ) -> ZkpResult<Vec<Ciphertext>>;

    /// Prove that `result_ct` is the correct homomorphic addition of `a_ct` and `b_ct`.
    ///
    /// NOTE: not yet supported by tfhe-rs zk-pok — returns BackendUnavailable.
    fn prove_add(
        &self,
        _result_ct: &Ciphertext,
        _a_ct: &Ciphertext,
        _b_ct: &Ciphertext,
        _ek: &EvaluationKey,
    ) -> ZkpResult<ComputationProof> {
        Err(ZkpError::BackendUnavailable(
            "computation proofs not yet supported".into(),
        ))
    }

    /// Verify an addition proof.
    fn verify_add(
        &self,
        _result_ct: &Ciphertext,
        _a_ct: &Ciphertext,
        _b_ct: &Ciphertext,
        _proof: &ComputationProof,
        _ek: &EvaluationKey,
    ) -> ZkpResult<bool> {
        Err(ZkpError::BackendUnavailable(
            "computation proofs not yet supported".into(),
        ))
    }

    /// Prove that `result_ct` is the correct homomorphic multiplication of `a_ct` and `b_ct`.
    fn prove_mul(
        &self,
        _result_ct: &Ciphertext,
        _a_ct: &Ciphertext,
        _b_ct: &Ciphertext,
        _ek: &EvaluationKey,
    ) -> ZkpResult<ComputationProof> {
        Err(ZkpError::BackendUnavailable(
            "computation proofs not yet supported".into(),
        ))
    }

    /// Verify a multiplication proof.
    fn verify_mul(
        &self,
        _result_ct: &Ciphertext,
        _a_ct: &Ciphertext,
        _b_ct: &Ciphertext,
        _proof: &ComputationProof,
        _ek: &EvaluationKey,
    ) -> ZkpResult<bool> {
        Err(ZkpError::BackendUnavailable(
            "computation proofs not yet supported".into(),
        ))
    }
}

/// How much computation the prover should invest vs. the verifier.
#[derive(Clone, Copy, Debug)]
pub enum ZkProofLoad {
    /// Faster proof generation, slower verification (good for weak clients).
    Verify,
    /// Slower proof generation, faster verification (good for strong servers).
    Proof,
}
