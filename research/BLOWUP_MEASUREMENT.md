# Ciphertext Blowup Measurement — Findings

## tfhe-rs v1.6.2, Default Config (128-bit security)

```
┌──────────────┬──────────┬────────────┬───────────┬──────────┐
│ Type         │ Plain    │ Ciphertext │ Size      │ Blowup   │
├──────────────┼──────────┼────────────┼───────────┼──────────┤
│ FheUint8     │     1 B  │   65,880 B │   64.34 KB │ 65,880× │
│ FheUint16    │     2 B  │  131,736 B │  128.65 KB │ 65,868× │
│ FheUint32    │     4 B  │  263,448 B │  257.27 KB │ 65,862× │
│ FheUint64    │     8 B  │  526,872 B │  514.52 KB │ 65,859× │
│ FheUint32+   │     8 B  │  263,448 B │  257.27 KB │ 32,931× │
│ FheUint32*   │     8 B  │  263,448 B │  257.27 KB │ 32,931× │
│ ServerKey    │     -    │ 180,610,691│~176,378 KB │    -    │
│ ClientKey    │     -    │      31,267│   30.53 KB │    -    │
└──────────────┴──────────┴────────────┴───────────┴──────────┘
```

## Key Findings

### 1. Blowup Factor
- **~66,000×** for any FheUint type — consistent across bit widths
- 4 bytes of plaintext → **257 KB** of ciphertext
- This is inherent to TFHE's security guarantees (polynomial ring elements)

### 2. Operations Don't Grow Ciphertexts
- `add` and `mul` produce same-sized ciphertexts as inputs
- This means compound computations (e.g., neural network layers) stay at the same size
- No "ciphertext expansion" during computation — only at encryption

### 3. ServerKey is Massive
- **176 MB** — must be sent once per computation session
- **Critical for QUIC design**: needs compression and/or chunked transfer
- Compression with Zstandard could reduce this significantly (polynomial coefficients have structure)

### 4. ClientKey is Small
- **31 KB** — easily stored and backed up

## Implications for BDCP Architecture

| Concern | Impact | Mitigation |
|---------|--------|------------|
| Per-ciphertext size | 257 KB for u32 | Use batching (pack multiple values into one ciphertext) |
| ServerKey transfer | 176 MB per session | Compress with zstd; stream over QUIC in chunks |
| RAM usage | 10 ciphertexts = 2.5 MB | Manageable for moderate workloads |
| Network throughput | ~2.5 MB per 10 values | Compress; consider packed encoding |

## Compression Test

```rust
// Using tfhe's built-in compression (zstd via serialization):
let ct = FheUint32::encrypt(42, &client_key);
let compressed = zstd::encode(&bincode::serialize(&ct).unwrap(), 3);
// Expected: ~257 KB → ~10-50 KB (estimated)
```

## Recommendation for Network Layer

1. **ServerKey transfer**: Dedicated QUIC stream with compression
2. **Ciphertext transfer**: Batch multiple values into one message; compress payload
3. **Memory pool**: Pre-allocate ciphertext buffers to avoid per-allocation overhead
4. **GPU consideration**: With concrete-cuda, ciphertexts stay on GPU — no host-device transfer
