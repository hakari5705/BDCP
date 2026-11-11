//! Real FHE backend using tfhe-rs (Zama TFHE).
//!
//! Wraps tfhe types into BDCP's portable `Ciphertext(Vec<u8>)`
//! via bincode serialization for network transport.
//!
//! Key insight: uses `CompressedServerKey` (~59 MB) for network transfer
//! instead of raw `ServerKey` (~176 MB).

use crate::crypto::fhe::{FheEngine, FheError, FheKeychain, FheResult};
use crate::types as bdcp_types;
use crate::types::*;
use tfhe::prelude::*;
use tfhe::{generate_keys, CompactPublicKey, CompressedServerKey, ConfigBuilder};

/// TFHE-rs backend engine (stateless — keys are passed per-call).
pub struct TfheEngine {
    config: tfhe::Config,
}

impl std::fmt::Debug for TfheEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TfheEngine")
            .field("config", &"tfhe-default-128bit")
            .finish()
    }
}

impl Default for TfheEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TfheEngine {
    pub fn new() -> Self {
        Self {
            config: ConfigBuilder::default().build(),
        }
    }

    /// Create an engine with a specific config (e.g. ZK-compatible params).
    pub fn with_config(config: tfhe::Config) -> Self {
        Self { config }
    }
}

// ── Serialization helpers ────────────────────────────────────────

fn to_ct<T: serde::Serialize>(val: &T) -> FheResult<Ciphertext> {
    bincode::serialize(val)
        .map(Ciphertext)
        .map_err(|e| FheError::SerializationFailed(e.to_string()))
}

fn from_ct<'de, T: serde::Deserialize<'de>>(ct: &'de Ciphertext) -> FheResult<T> {
    bincode::deserialize(&ct.0)
        .map_err(|e| FheError::SerializationFailed(e.to_string()))
}

// ── FheEngine implementation ─────────────────────────────────────

impl FheEngine for TfheEngine {
    fn name(&self) -> &str {
        "tfhe-rs-v1.6-integer"
    }

    fn generate_keychain(&self, _level: SecurityLevel) -> FheResult<FheKeychain> {
        let (client_key, _server_key) = generate_keys(self.config.clone());
        let compressed_server_key = CompressedServerKey::new(&client_key);
        let public_key = tfhe::PublicKey::new(&client_key);
        let compact_pk = CompactPublicKey::new(&client_key);

        Ok(FheKeychain {
            // ClientKey: stays local (~31 KB)
            secret_key: SecretKey(to_ct(&client_key)?.0),
            // PublicKey: derived from ClientKey, sent to Bob for encryption
            public_key: PublicKey(to_ct(&public_key)?.0),
            // CompactPublicKey: for third-party encryption (~few KB)
            compact_public_key: bdcp_types::CompactPublicKey(to_ct(&compact_pk)?.0),
            // EvaluationKey: CompressedServerKey for network transfer (~59 MB)
            evaluation_key: EvaluationKey(to_ct(&compressed_server_key)?.0),
        })
    }

    fn encrypt(&self, plain: &Plaintext, pk: &PublicKey) -> FheResult<Ciphertext> {
        let public_key: tfhe::PublicKey = from_ct(&Ciphertext(pk.0.clone()))?;
        let val = u32::from_le_bytes(
            plain.0[..4].try_into().map_err(|_| {
                FheError::EncryptionFailed("plaintext must be 4 bytes (u32)".into())
            })?,
        );
        let ct = tfhe::FheUint32::try_encrypt(val, &public_key)
            .map_err(|e| FheError::EncryptionFailed(e.to_string()))?;
        to_ct(&ct)
    }

    fn encrypt_compact(&self, _plain: &Plaintext, _cpk: &bdcp_types::CompactPublicKey) -> FheResult<Ciphertext> {
        Err(FheError::BackendUnavailable(
            "encrypt_compact requires u32 — use `encrypt_u32_with_compact` instead".into(),
        ))
    }

    fn decrypt(&self, _cipher: &Ciphertext, _sk: &SecretKey) -> FheResult<Plaintext> {
        Err(FheError::BackendUnavailable(
            "decrypt requires ClientKey — use `decrypt_u32` instead".into(),
        ))
    }

    fn add(&self, _a: &Ciphertext, _b: &Ciphertext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        Err(FheError::BackendUnavailable(
            "add requires ServerKey — call `with_server_key` first".into(),
        ))
    }

    fn mul(&self, _a: &Ciphertext, _b: &Ciphertext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        Err(FheError::BackendUnavailable(
            "mul requires ServerKey".into(),
        ))
    }

    fn mul_plain(&self, _ct: &Ciphertext, _pt: &Plaintext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        Err(FheError::BackendUnavailable(
            "mul_plain requires ServerKey".into(),
        ))
    }

    fn add_plain(&self, _ct: &Ciphertext, _pt: &Plaintext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        Err(FheError::BackendUnavailable(
            "add_plain requires ServerKey".into(),
        ))
    }

    fn rotate(&self, ct: &Ciphertext, _n: usize, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        Ok(ct.clone())
    }
}

// ── Extended API (beyond the core trait) ─────────────────────────

impl TfheEngine {
    /// Encrypt a u32 using a deserialised ClientKey.
    pub fn encrypt_u32(&self, val: u32, ck: &tfhe::ClientKey) -> FheResult<Ciphertext> {
        let ct = tfhe::FheUint32::try_encrypt(val, ck)
            .map_err(|e| FheError::EncryptionFailed(e.to_string()))?;
        to_ct(&ct)
    }

    /// Encrypt a u32 using a CompactPublicKey (for third parties like Bob).
    ///
    /// This is the key method that enables multi-party encrypted computation:
    /// Bob receives Alice's CompactPublicKey, calls this, and the output
    /// ciphertext is compatible with Alice's ServerKey on the Relay.
    ///
    /// Uses `CompactCiphertextList::builder` + `expand` under the hood
    /// because tfhe's `CompactPublicKey` requires the compact list API.
    pub fn encrypt_u32_with_compact(
        &self,
        val: u32,
        cpk: &tfhe::CompactPublicKey,
    ) -> FheResult<Ciphertext> {
        use tfhe::CompactCiphertextList;
        let mut builder = CompactCiphertextList::builder(cpk);
        builder.push(val);
        let list = builder.build();
        let expander = list
            .expand()
            .map_err(|e| FheError::EncryptionFailed(e.to_string()))?;
        let fhe: tfhe::FheUint32 = expander
            .get::<tfhe::FheUint32>(0)
            .map_err(|e| FheError::EncryptionFailed(e.to_string()))?
            .ok_or_else(|| FheError::EncryptionFailed("empty expansion".into()))?;
        to_ct(&fhe)
    }

    /// Encrypt multiple u32 values using a deserialised ClientKey.
    pub fn encrypt_u32_vec(&self, vals: &[u32], ck: &tfhe::ClientKey) -> FheResult<Vec<Ciphertext>> {
        vals.iter().map(|v| self.encrypt_u32(*v, ck)).collect()
    }

    /// Encrypt multiple u32 values using a CompactPublicKey in a single batch.
    ///
    /// Uses `CompactCiphertextList::builder` to pack all values into one compact list,
    /// then expands each slot individually. More efficient than calling
    /// `encrypt_u32_with_compact` in a loop.
    pub fn encrypt_u32_with_compact_vec(
        &self,
        vals: &[u32],
        cpk: &tfhe::CompactPublicKey,
    ) -> FheResult<Vec<Ciphertext>> {
        use tfhe::CompactCiphertextList;
        let mut builder = CompactCiphertextList::builder(cpk);
        for &v in vals {
            builder.push(v);
        }
        let list = builder.build();
        let expander = list
            .expand()
            .map_err(|e| FheError::EncryptionFailed(e.to_string()))?;
        let mut result = Vec::with_capacity(vals.len());
        for i in 0..vals.len() {
            let fhe: tfhe::FheUint32 = expander
                .get(i)
                .map_err(|e| FheError::EncryptionFailed(e.to_string()))?
                .ok_or_else(|| FheError::EncryptionFailed(format!("slot {i} empty")))?;
            result.push(to_ct(&fhe)?);
        }
        Ok(result)
    }

    /// Decrypt to u32 using a deserialised ClientKey.
    pub fn decrypt_u32(&self, ct: &Ciphertext, ck: &tfhe::ClientKey) -> FheResult<u32> {
        let fhe: tfhe::FheUint32 = from_ct(ct)?;
        Ok(fhe.decrypt(ck))
    }

    /// Deserialize a CompactPublicKey from BDCP's opaque wrapper.
    ///
    /// Called by Bob after receiving Alice's CompactPublicKey over the network.
    pub fn deserialize_compact_pk(&self, cpk: &bdcp_types::CompactPublicKey) -> FheResult<tfhe::CompactPublicKey> {
        from_ct(&Ciphertext(cpk.0.clone()))
    }

    /// Decompress a CompressedServerKey (from EvaluationKey) into a usable ServerKey.
    ///
    /// Called on the relay after receiving the compressed key over the network.
    /// The resulting ServerKey (~176 MB) stays in memory for the session.
    pub fn decompress_server_key(&self, ek: &EvaluationKey) -> FheResult<tfhe::ServerKey> {
        let compressed: CompressedServerKey = from_ct(&Ciphertext(ek.0.clone()))?;
        Ok(compressed.decompress())
    }

    /// Execute an arbitrary closure with the ServerKey set.
    ///
    /// `ek` should be a serialized `tfhe::ServerKey` (decompressed).
    /// For compressed keys, call `decompress_server_key` first.
    pub fn with_server_key<T>(&self, ek: &EvaluationKey, f: impl FnOnce() -> T) -> FheResult<T> {
        let ct = Ciphertext(ek.0.clone());
        let sk: tfhe::ServerKey = from_ct(&ct)?;
        tfhe::set_server_key(sk);
        Ok(f())
    }
}
