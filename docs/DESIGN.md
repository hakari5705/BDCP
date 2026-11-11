# BDCP — Blind Data Collaboration Protocol

## Design Document v0.1

> **Vision**: Enable institutions to jointly train AI models and run analytics
> on their combined sensitive data **without any party ever seeing another's raw data**.

---

## 1. Problem Statement

Hospitals, banks, and government agencies hold vast amounts of sensitive data.
Combined, this data could power life-saving medical discoveries and financial
crisis prediction — but privacy regulations (GDPR, HIPAA, CCPA) and security
concerns prevent sharing.

Existing approaches (anonymisation, differential privacy) have proven vulnerable
to re-identification attacks. The only provably secure solution is to **compute
on encrypted data without ever decrypting it**.

---

## 2. Solution Overview

BDCP is a _protocol_ and _reference implementation_ for privacy-preserving
collaborative computation. It combines:

| Technology | Role |
|-----------|------|
| **Fully Homomorphic Encryption (FHE)** | Compute on encrypted ciphertexts |
| **Zero-Knowledge Proofs (ZKPs)** | Prove computation correctness without revealing data |
| **Secure Multi-Party Computation (SMPC)** | Distribute trust across participants |

### Core Principle

> **Data never leaves its owner's trust boundary.**
> Only encrypted ciphertexts and ZK proofs cross the network.
> The relay sees only opaque random-looking blobs.

---

## 3. Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Alice       │     │  BDCP Relay  │     │  Bob         │
│  (Hospital)  │     │  (Cloud)     │     │  (Research)  │
├──────────────┤     ├──────────────┤     ├──────────────┤
│ encrypt(x)───┼────>│              │<────┼──encrypt(y)  │
│              │     │ E(x)+E(y)    │     │              │
│              │<────┼──E(result)───┼────>│              │
│ decrypt(res) │     │              │     │ decrypt(res) │
└──────────────┘     └──────────────┘     └──────────────┘
     ▲                     ▲                      ▲
     │                     │                      │
  Secret Key           Never sees              Secret Key
  (local only)        plaintext!              (local only)
```

### 3.1 Workspace Layout

```
bdcp/                     # Workspace root
├── bdcp-core/            # Core traits, types, protocol messages
│   └── src/
│       ├── crypto/       # FHE + ZKP abstractions
│       │   ├── fhe.rs    # FheEngine trait
│       │   └── zkp.rs    # ZkpEngine trait
│       ├── protocol/     # Wire protocol
│       │   ├── messages.rs  # Message types
│       │   └── handshake.rs # State machine
│       └── types.rs      # Common types (Ciphertext, Key, etc.)
├── bdcp-node/            # Node daemon — relay + peer networking
│   └── src/
│       ├── main.rs       # Entry point
│       └── relay.rs      # Computation relay
├── bdcp-cli/             # CLI interface
│   └── src/main.rs
├── examples/
│   └── simple-poc/       # Two-party encrypted sum demo
└── docs/
    └── DESIGN.md         # This document
```

### 3.2 Key Abstractions

#### `FheEngine` trait (`bdcp-core/src/crypto/fhe.rs`)

The central abstraction. Any FHE backend (TFHE, BFV, CKKS) can implement it:

```rust
pub trait FheEngine: Debug + Send + Sync {
    fn name(&self) -> &str;
    fn generate_keychain(&self, level: SecurityLevel) -> FheResult<FheKeychain>;
    fn encrypt(&self, plain: &Plaintext, pk: &PublicKey) -> FheResult<Ciphertext>;
    fn decrypt(&self, cipher: &Ciphertext, sk: &SecretKey) -> FheResult<Plaintext>;
    fn add(&self, a: &Ciphertext, b: &Ciphertext, ek: &EvaluationKey) -> FheResult<Ciphertext>;
    fn mul(&self, a: &Ciphertext, b: &Ciphertext, ek: &EvaluationKey) -> FheResult<Ciphertext>;
    fn mul_plain(&self, ct: &Ciphertext, pt: &Plaintext, ek: &EvaluationKey) -> FheResult<Ciphertext>;
    fn add_plain(&self, ct: &Ciphertext, pt: &Plaintext, ek: &EvaluationKey) -> FheResult<Ciphertext>;
    fn rotate(&self, ct: &Ciphertext, n: usize, ek: &EvaluationKey) -> FheResult<Ciphertext>;
}
```

#### `ZkpEngine` trait (`bdcp-core/src/crypto/zkp.rs`)

Proof-of-correctness abstraction:

```rust
pub trait ZkpEngine: Debug + Send + Sync {
    fn prove_encryption(&self, ct, plain, pk, sk) -> ZkpResult<ComputationProof>;
    fn verify_encryption(&self, ct, proof, pk) -> ZkpResult<bool>;
    fn prove_add(&self, result_ct, a_ct, b_ct, ek) -> ZkpResult<ComputationProof>;
    fn verify_add(&self, result_ct, a_ct, b_ct, proof, ek) -> ZkpResult<bool>;
    // ... mul, mul_plain, etc.
}
```

---

## 4. Protocol Flow (Detailed)

### 4.1 Job Lifecycle

```
┌─────────┐          ┌──────────┐          ┌─────────┐
│ Alice   │          │  Relay   │          │  Bob    │
└────┬────┘          └────┬─────┘          └────┬────┘
     │                    │                     │
     │  JobRequest        │                     │
     │───────────────────>│                     │
     │                    │  JobAccept          │
     │                    │<────────────────────│
     │  KeyExchange(pk)   │                     │
     │───────────────────>│                     │
     │                    │  KeyExchange(ek)    │
     │                    │<────────────────────│
     │                    │    ** Handshake     │
     │                    │       Complete **   │
     │  DataSubmission   │                     │
     │  (E(x))           │                     │
     │───────────────────>│                     │
     │                    │  DataSubmission     │
     │                    │  (E(y))             │
     │                    │<────────────────────│
     │                    │                     │
     │                    │  ComputeRequest     │
     │                    │  ("add", [E(x),E(y)])│
     │                    │<────────────────────│
     │                    │                     │
     │                    │  ComputeResult      │
     │                    │  (E(x+y))           │
     │                    │────────────────────>│
     │                    │                     │
     │  ComputeResult     │                     │
     │  (E(x+y))         │                     │
     │<───────────────────│                     │
     │                    │                     │
     │  Decrypt: x+y     │                     │
     │  ✓ Done            │                     │
```

### 4.2 Message Types

Defined in `bdcp-core/src/protocol/messages.rs`:

| Variant | Direction | Payload |
|---------|-----------|---------|
| `JobRequest` | → Relay | description, participants, scheme |
| `JobAccept` | ← Relay | job_id, supported_ops |
| `JobReject` | ← Relay | reason |
| `KeyExchange` | ↔ | PublicKey or EvaluationKey |
| `DataSubmission` | → Relay | Ciphertext + optional ZK proof |
| `ComputeRequest` | → Relay | operation string, data references |
| `ComputeResult` | ← Relay | Ciphertext + optional proof |
| `Attestation` | ↔ | ComputationProof |

---

## 5. Backend Comparison & Selection

| Criterion | Microsoft SEAL | Zama Concrete | OpenFHE |
|-----------|---------------|---------------|---------|
| Language | C++ | Rust (core) + Python | C++ |
| Schemes | BFV, CKKS | TFHE | BFV, CKKS, FHEW, TFHE |
| GPU Support | No | Yes (concrete-cuda) | Experimental |
| Rust Bindings | seal-rs (3rd party) | Native | openfhe-rs (3rd party) |
| ML Focus | Generic | Concrete-ML | Generic |
| Maturity | ★★★★★ | ★★★★ | ★★★★ |
| Ease of Use | ★★ | ★★★ | ★★ |
| Performance | ★★★★ | ★★★ (TFHE) / ★★★★ (CKKS) | ★★★★ |

**Initial choice: Zama Concrete** — Rust-native, has Concrete-ML for AI
workloads, active development, GPU support via concrete-cuda.

---

## 6. Security Model

### 6.1 Threat Model

| Threat | Mitigation |
|--------|-----------|
| Malicious relay | ZK proofs on every computation step |
| Malicious participant | ZK proof of correct encryption |
| Eavesdropper | All traffic is encrypted ciphertexts |
| Side-channel | Constant-time operations (FHE backend ensures this) |
| Replay attack | Nonces + timestamps in every message |

### 6.2 Trust Assumptions

- Each participant keeps their **secret key** local
- The **relay** is honest-but-curious (can try to infer but follows protocol)
- At least **one participant** is honest per computation
- The FHE backend's security parameters are correctly configured

---

## 7. Performance Targets

| Metric | Current FHE Baseline | BDCP Target |
|--------|---------------------|-------------|
| Ciphertext add | ~1ms | <1ms |
| Ciphertext mul | ~10ms | <5ms |
| Bootstrap (TFHE) | ~50ms | <20ms |
| Neural net inference | ~1 hour | <10 min |
| Key generation | ~5s | <1s |

Primary optimisation strategies:
1. **GPU acceleration** (CUDA/ROCm for ciphertext operations)
2. **Batched processing** (SIMD-style packing of plaintext slots)
3. **Lazy re-linearisation** (defer noise reduction where possible)
4. **Pruned circuits** (ML-specific optimisations via Concrete-ML)

---

## 8. Roadmap

### Phase 1 — Foundation (Current)
- [x] Workspace structure & core traits
- [x] Protocol message definitions
- [x] Simple PoC (toy FHE)
- [ ] Integration with Concrete-core v2
- [ ] Node-to-node networking (QUIC)

### Phase 2 — MVP
- [ ] Working multi-party addition
- [ ] ZK proof verification
- [ ] CLI tool for job submission
- [ ] Docker-based local testnet

### Phase 3 — ML Support
- [ ] Concrete-ML integration
- [ ] Encrypted inference pipeline
- [ ] Model training (SGD over encrypted data)
- [ ] GPU acceleration

### Phase 4 — Production
- [ ] Benchmark suite
- [ ] Security audit
- [ ] Kubernetes deployment
- [ ] Public testnet

---

## 9. Related Work

| Project | Approach | Limitation |
|---------|----------|------------|
| Duality Technologies | FHE-based analytics | Proprietary, slow |
| Zama Concrete-ML | TFHE for ML | Single-party only |
| OpenMined (PySyft) | SMPC + DP | High network overhead |
| Google Private Join | Custom MPC | Limited to intersection |
| Apple Differential Privacy | DP on-device | Reduces accuracy |

**BDCP differentiator**: Open-source, Rust-native, multi-party FHE-first design
with integrated ZK attestation and an API designed for developers.

---

## 10. Contributing

See `CONTRIBUTING.md` (TODO).

## 11. License

MIT — see `LICENSE`.
