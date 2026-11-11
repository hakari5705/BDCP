//! Real ZKP backend using tfhe-rs `zk-pok` feature.
//!
//! Proves that compact public key encryption produces valid ciphertexts
//! without revealing the plaintext. Uses `ProvenCompactCiphertextList`
//! with a shared `CompactPkeCrs` (Common Reference String).

use crate::crypto::zkp::{ZkpEngine, ZkpError, ZkProofLoad, ZkpResult};
use crate::types::*;
use tfhe::prelude::*;
use tfhe::zk::{CompactPkeCrs, ZkComputeLoad};

/// TFHE-rs zero-knowledge proof engine.
#[derive(Debug)]
pub struct TfheZkpEngine {
    /// The base config from which the CRS is derived.
    config: tfhe::Config,
}

impl Default for TfheZkpEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TfheZkpEngine {
    /// Create an engine with ZK-compatible parameters.
    pub fn new() -> Self {
        Self {
            config: Self::zkp_config(),
        }
    }

    /// Create an engine with explicit parameters (advanced use).
    pub fn with_config(config: tfhe::Config) -> Self {
        Self { config }
    }

    /// Return the ZK-compatible Config for use by other engines (e.g. TfheEngine).
    pub fn zkp_config() -> tfhe::Config {
        use tfhe::shortint::parameters::{
            PARAM_MESSAGE_2_CARRY_2_KS_PBS_TUNIFORM_2M128,
            PARAM_PKE_MESSAGE_2_CARRY_2_KS_PBS_TUNIFORM_2M128,
            PARAM_KEYSWITCH_MESSAGE_2_CARRY_2_KS_PBS_TUNIFORM_2M128,
        };
        tfhe::ConfigBuilder::with_custom_parameters(
            PARAM_MESSAGE_2_CARRY_2_KS_PBS_TUNIFORM_2M128,
        )
        .use_dedicated_compact_public_key_parameters((
            PARAM_PKE_MESSAGE_2_CARRY_2_KS_PBS_TUNIFORM_2M128,
            PARAM_KEYSWITCH_MESSAGE_2_CARRY_2_KS_PBS_TUNIFORM_2M128,
        ))
        .build()
    }
}

impl ZkpEngine for TfheZkpEngine {
    fn name(&self) -> &str {
        "tfhe-rs-v1.6-zk-pok"
    }

    fn generate_crs(&self, log2_num_slots: usize) -> ZkpResult<Crs> {
        let num_slots = 1 << log2_num_slots;
        let crs = CompactPkeCrs::from_config(self.config.clone(), num_slots)
            .map_err(|e| ZkpError::CrsGenerationFailed(e.to_string()))?;
        let bytes = bincode::serialize(&crs)
            .map_err(|e| ZkpError::CrsGenerationFailed(e.to_string()))?;
        Ok(Crs(bytes))
    }

    fn prove_encrypt_compact(
        &self,
        values: &[u32],
        cpk: &CompactPublicKey,
        crs: &Crs,
        metadata: &[u8],
        load: ZkProofLoad,
    ) -> ZkpResult<(Ciphertext, CompactProof)> {
        let cpk: tfhe::CompactPublicKey = bincode::deserialize(&cpk.0)
            .map_err(|e| ZkpError::ProofGenerationFailed(format!("deserialize cpk: {e}")))?;
        let crs: CompactPkeCrs = bincode::deserialize(&crs.0)
            .map_err(|e| ZkpError::ProofGenerationFailed(format!("deserialize crs: {e}")))?;

        let zk_load = match load {
            ZkProofLoad::Verify => ZkComputeLoad::Verify,
            ZkProofLoad::Proof => ZkComputeLoad::Proof,
        };

        let mut builder = tfhe::ProvenCompactCiphertextList::builder(&cpk);
        for &v in values {
            builder.push(v);
        }
        let proven = builder
            .build_with_proof_packed(&crs, metadata, zk_load)
            .map_err(|e| ZkpError::ProofGenerationFailed(e.to_string()))?;

        let bytes = bincode::serialize(&proven)
            .map_err(|e| ZkpError::ProofGenerationFailed(e.to_string()))?;
        // The proven list contains both ciphertexts and proof in one blob.
        // The CompactProof slot is unused by callers (relay re-wraps raw bytes).
        Ok((Ciphertext(bytes), CompactProof(Vec::new())))
    }

    fn verify_and_expand(
        &self,
        proof: &CompactProof,
        cpk: &CompactPublicKey,
        crs: &Crs,
        metadata: &[u8],
        num_values: usize,
    ) -> ZkpResult<Vec<Ciphertext>> {
        let proven: tfhe::ProvenCompactCiphertextList = bincode::deserialize(&proof.0)
            .map_err(|e| ZkpError::ProofVerificationFailed(format!("deserialize proof: {e}")))?;
        let cpk: tfhe::CompactPublicKey = bincode::deserialize(&cpk.0)
            .map_err(|e| ZkpError::ProofVerificationFailed(format!("deserialize cpk: {e}")))?;
        let crs: CompactPkeCrs = bincode::deserialize(&crs.0)
            .map_err(|e| ZkpError::ProofVerificationFailed(format!("deserialize crs: {e}")))?;

        let expander = proven
            .verify_and_expand(&crs, &cpk, metadata)
            .map_err(|e| ZkpError::ProofVerificationFailed(e.to_string()))?;

        let mut results = Vec::with_capacity(num_values);
        for i in 0..num_values {
            let fhe: tfhe::FheUint32 = expander
                .get::<tfhe::FheUint32>(i)
                .map_err(|e| ZkpError::ProofVerificationFailed(e.to_string()))?
                .ok_or_else(|| ZkpError::ProofVerificationFailed(format!("slot {i} is empty")))?;
            let bytes = bincode::serialize(&fhe)
                .map_err(|e| ZkpError::ProofVerificationFailed(e.to_string()))?;
            results.push(Ciphertext(bytes));
        }

        Ok(results)
    }
}
