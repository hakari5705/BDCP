//! BDCP Proof-of-Concept: encrypted addition between two parties.
//!
//! Toy FHE: E(x) = x + KEY  (mod 2^64)
//!          D(y) = y - KEY  (mod 2^64)
//!   add(E(x), E(y)) = E(x) + E(y) - KEY = E(x + y)
//!
//! ⚠ This toy cipher is NOT secure — it only demonstrates the BDCP workflow.
//!    A production system would use concrete-core (TFHE) or Microsoft SEAL.

use bdcp_core::crypto::fhe::{FheEngine, FheKeychain, FheResult};
use bdcp_core::types::{Ciphertext, CompactPublicKey, EvaluationKey, Plaintext, PublicKey, SecretKey, SecurityLevel};

/// The secret key is shared for this toy; in production each party has its own.
const KEY: u64 = 0xDEAD_BEEF_CAFE_4242;

#[derive(Debug)]
struct ToyFhe;

impl FheEngine for ToyFhe {
    fn name(&self) -> &str {
        "toy-additive"
    }

    fn generate_keychain(&self, _level: SecurityLevel) -> FheResult<FheKeychain> {
        Ok(FheKeychain {
            secret_key: SecretKey(KEY.to_le_bytes().to_vec()),
            public_key: PublicKey(vec![0u8; 8]),
            compact_public_key: CompactPublicKey(vec![0u8; 8]),
            evaluation_key: EvaluationKey(KEY.to_le_bytes().to_vec()),
        })
    }

    fn encrypt(&self, plain: &Plaintext, _pk: &PublicKey) -> FheResult<Ciphertext> {
        let v = u64::from_le_bytes(plain.0[..8].try_into().unwrap());
        Ok(Ciphertext(v.wrapping_add(KEY).to_le_bytes().to_vec()))
    }

    fn encrypt_compact(&self, plain: &Plaintext, _cpk: &CompactPublicKey) -> FheResult<Ciphertext> {
        let v = u64::from_le_bytes(plain.0[..8].try_into().unwrap());
        Ok(Ciphertext(v.wrapping_add(KEY).to_le_bytes().to_vec()))
    }

    fn decrypt(&self, cipher: &Ciphertext, _sk: &SecretKey) -> FheResult<Plaintext> {
        let v = u64::from_le_bytes(cipher.0[..8].try_into().unwrap());
        Ok(Plaintext(v.wrapping_sub(KEY).to_le_bytes().to_vec()))
    }

    fn add(&self, a: &Ciphertext, b: &Ciphertext, ek: &EvaluationKey) -> FheResult<Ciphertext> {
        let k = u64::from_le_bytes(ek.0[..8].try_into().unwrap());
        let x = u64::from_le_bytes(a.0[..8].try_into().unwrap());
        let y = u64::from_le_bytes(b.0[..8].try_into().unwrap());
        // (a+KEY) + (b+KEY) - KEY = a+b+KEY = E(a+b)
        Ok(Ciphertext(x.wrapping_add(y).wrapping_sub(k).to_le_bytes().to_vec()))
    }

    fn mul(&self, a: &Ciphertext, b: &Ciphertext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        let x = u64::from_le_bytes(a.0[..8].try_into().unwrap());
        let y = u64::from_le_bytes(b.0[..8].try_into().unwrap());
        Ok(Ciphertext(x.wrapping_mul(y).to_le_bytes().to_vec()))
    }

    fn mul_plain(&self, ct: &Ciphertext, pt: &Plaintext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        let c = u64::from_le_bytes(ct.0[..8].try_into().unwrap());
        let p = u64::from_le_bytes(pt.0[..8].try_into().unwrap());
        Ok(Ciphertext(c.wrapping_mul(p).to_le_bytes().to_vec()))
    }

    fn add_plain(&self, ct: &Ciphertext, pt: &Plaintext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        let c = u64::from_le_bytes(ct.0[..8].try_into().unwrap());
        let p = u64::from_le_bytes(pt.0[..8].try_into().unwrap());
        // E(x) + p = (x+KEY) + p = E(x+p)
        Ok(Ciphertext(c.wrapping_add(p).to_le_bytes().to_vec()))
    }

    fn rotate(&self, ct: &Ciphertext, _n: usize, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        Ok(ct.clone())
    }
}

fn load_u64(pt: &Plaintext) -> u64 {
    u64::from_le_bytes(pt.0[..8].try_into().unwrap())
}

fn main() {
    let engine = ToyFhe;
    let kc = engine.generate_keychain(SecurityLevel::Standard).unwrap();

    // ── Phase 1: Each party encrypts locally ────────────
    let alice_val: u64 = 42;
    let bob_val: u64 = 58;

    let alice_ct = engine.encrypt(&Plaintext(alice_val.to_le_bytes().to_vec()), &kc.public_key).unwrap();
    let bob_ct = engine.encrypt(&Plaintext(bob_val.to_le_bytes().to_vec()), &kc.public_key).unwrap();

    println!("═══ Phase 1: Encryption =══");
    println!("Alice plain:  {}", alice_val);
    println!("Bob plain:    {}", bob_val);
    println!("Alice cipher: {:02x?}", &alice_ct.0[..8]);
    println!("Bob cipher:   {:02x?}", &bob_ct.0[..8]);

    // ── Phase 2: Relay computes on ciphertexts only ────
    println!("\n═══ Phase 2: Encrypted computation =══");
    let sum_ct = engine.add(&alice_ct, &bob_ct, &kc.evaluation_key).unwrap();
    println!("E(sum)  = {:02x?}", &sum_ct.0[..8]);

    // Encrypted mean = E(sum) * inv(2) via add_plain trick:
    // Since we want the mean, and adding a number to itself is same as *2,
    // we can't easily divide. But we CAN compute E(sum+42) to show add_plain:
    let bonus = engine.add_plain(&sum_ct, &Plaintext(100u64.to_le_bytes().to_vec()), &kc.evaluation_key).unwrap();
    println!("E(sum+100) = {:02x?}", &bonus.0[..8]);

    // ── Phase 3: Party decrypts result ─────────────────
    println!("\n═══ Phase 3: Decryption =══");
    let sum_pt = engine.decrypt(&sum_ct, &kc.secret_key).unwrap();
    let sum_val = load_u64(&sum_pt);
    println!("Decrypted sum:  {}  (expected: {})", sum_val, alice_val + bob_val);

    let bonus_pt = engine.decrypt(&bonus, &kc.secret_key).unwrap();
    let bonus_val = load_u64(&bonus_pt);
    println!("Decrypted +100: {}  (expected: {})", bonus_val, alice_val + bob_val + 100);

    assert_eq!(sum_val, alice_val + bob_val);
    assert_eq!(bonus_val, alice_val + bob_val + 100);

    println!("\n═══════════════════════════════════");
    println!("  ✓ BDCP PoC: Encrypted computation");
    println!("    completed successfully!");
    println!("═══════════════════════════════════");
}
