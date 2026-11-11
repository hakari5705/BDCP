//! Toy additive cipher — for fast testing without real FHE.
//!
//! NOT SECURE. Uses a hardcoded key (`0xDEAD_BEEF_CAFE_4242`) with XOR-like
//! addition cipher. Only suitable for integration testing and CI.

use crate::crypto::fhe::{FheEngine, FheKeychain, FheResult};
use crate::types::{
    Ciphertext, CompactPublicKey, EvaluationKey, Plaintext, PublicKey, SecretKey, SecurityLevel,
};

const KEY: u64 = 0xDEAD_BEEF_CAFE_4242;

#[derive(Debug)]
pub struct ToyFhe;

impl FheEngine for ToyFhe {
    fn name(&self) -> &str {
        "toy-additive"
    }

    fn generate_keychain(&self, _level: SecurityLevel) -> FheResult<FheKeychain> {
        Ok(FheKeychain {
            secret_key: SecretKey(KEY.to_le_bytes().to_vec()),
            public_key: PublicKey(KEY.to_le_bytes().to_vec()),
            compact_public_key: CompactPublicKey(KEY.to_le_bytes().to_vec()),
            evaluation_key: EvaluationKey(KEY.to_le_bytes().to_vec()),
        })
    }

    fn encrypt(&self, plain: &Plaintext, _pk: &PublicKey) -> FheResult<Ciphertext> {
        let v = u64::from_le_bytes(plain.0[..8].try_into().unwrap());
        Ok(Ciphertext(v.wrapping_add(KEY).to_le_bytes().to_vec()))
    }

    fn encrypt_compact(&self, plain: &Plaintext, _cpk: &CompactPublicKey) -> FheResult<Ciphertext> {
        self.encrypt(plain, &PublicKey(vec![]))
    }

    fn decrypt(&self, cipher: &Ciphertext, _sk: &SecretKey) -> FheResult<Plaintext> {
        let v = u64::from_le_bytes(cipher.0[..8].try_into().unwrap());
        Ok(Plaintext(v.wrapping_sub(KEY).to_le_bytes().to_vec()))
    }

    fn add(&self, a: &Ciphertext, b: &Ciphertext, ek: &EvaluationKey) -> FheResult<Ciphertext> {
        let k = u64::from_le_bytes(ek.0[..8].try_into().unwrap());
        let x = u64::from_le_bytes(a.0[..8].try_into().unwrap());
        let y = u64::from_le_bytes(b.0[..8].try_into().unwrap());
        Ok(Ciphertext(
            x.wrapping_add(y)
                .wrapping_sub(k)
                .to_le_bytes()
                .to_vec(),
        ))
    }

    fn mul(&self, _a: &Ciphertext, _b: &Ciphertext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        unimplemented!("mul not used in testing")
    }

    fn mul_plain(&self, _ct: &Ciphertext, _pt: &Plaintext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        unimplemented!("mul_plain not used")
    }

    fn add_plain(&self, _ct: &Ciphertext, _pt: &Plaintext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        unimplemented!("add_plain not used")
    }

    fn rotate(&self, ct: &Ciphertext, _n: usize, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        Ok(ct.clone())
    }
}
