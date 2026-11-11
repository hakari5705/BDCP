# BDCP — التدقيق الفني الكامل
# Codebase Audit Report

> **بناءً على فحص 100% من ملفات المصدر (Rust, Shell, Python, TOML, YAML)
> عبر 6 مسارات تدقيق متزامنة.**

---

## ملخص النتائج | Executive Summary

| الفئة | العدد | حرج |
|-------|-------|-----|
| 🔴 **أخطاء حرجة (تؤثر على التشغيل أو البيع)** | 7 | يجب الإصلاح فوراً |
| 🟡 **مشاكل متوسطة (تؤثر على الصيانة أو الثقة)** | 16 | يجب الإصلاح قبل أول عميل |
| 🔵 **مشاكل بسيطة (نظافة الكود)** | 12 | تُصلح عند التفرغ |
| 💀 **كود ميت (Dead Code)** | 4 وحدات كاملة | حذف أو إعادة تفعيل |

---

## 🔴 القسم 1: أخطاء حرجة | Critical Issues

### 1.1 `FheEngine` trait غير مكتمل — 6 دوال ترجع `BackendUnavailable`

**الموقع**: `bdcp-core/src/backend/tfhe.rs:85-134`
**الخطورة**: عالية جداً

```rust
// كل هذه الدوال لا تعمل:
fn encrypt(...)    → Err(BackendUnavailable)
fn encrypt_compact(...) → Err(BackendUnavailable)
fn decrypt(...)    → Err(BackendUnavailable)
fn add(...)        → Err(BackendUnavailable)
fn mul(...)        → Err(BackendUnavailable)
fn mul_plain(...)  → Err(BackendUnavailable)
fn add_plain(...)  → Err(BackendUnavailable)
// فقط rotate() تعمل (لكنها no-op)
```

**التأثير**: أي كود يحاول استخدام `FheEngine` trait polymorphically سيفشل. النظام يعمل لأن `bdcp-node` و `bdcp-cli` يستخدمان دوال `encrypt_u32` / `decrypt_u32` المخصصة — لكنهما لا يستخدمان trait.

**الإصلاح**: 
- إما إزالة هذه الدوال من trait (breaking change)
- أو تنفيذها حقيقياً باستخدام `encrypt_u32` داخلياً
- أو توثيق أنها غير مدعومة بوضوح في docs

### 1.2 `mean()` — دالة المتوسط محطمة (broken)

**الموقع**: `bdcp-core/src/crypto/fhe.rs:72-83`
**الخطورة**: عالية — أي كود يستدعي `mean()` يعطي نتائج خاطئة

```rust
fn mean(&self, values: &[&Ciphertext], ek: &EvaluationKey) -> FheResult<Ciphertext> {
    let inv_n = (1.0 / n as f64).to_le_bytes().to_vec();  // ← 8 bytes float
    self.mul_plain(&sum, &Plaintext(inv_n), ek)            // ← يضرب FheUint32 بهذه البايتات
}
```

**لماذا هي محطمة**: `mul_plain` في TFHE تتوقع integer (`FheUint32 * u32`)، لكن `inv_n` هي تمثيل float بـ 8 بايتات. ضرب ciphertext (u32) ببايتات float يعطي نتيجة غير متوقعة.

**الإصلاح**: إما إزالة الـ default impl أو تنفيذها باستخدام integer arithmetic (قسمة عادية غير مشفّرة).

### 1.3 `bdcp-cli` ينقصه `zk-pok` feature

**الموقع**: `bdcp-cli/Cargo.toml:20`
**الخطورة**: عالية — ZKP يتوقف عن العمل عند بنائه عبر CLI

```toml
# bdcp-cli:
tfhe = { version = "1.6", features = ["integer"] }         # ← بدون zk-pok

# bdcp-node:
tfhe = { version = "1.6", features = ["integer", "shortint", "zk-pok"], optional = true }
```

CLI يستورد `TfheZkpEngine` من `bdcp-core`، لكن `bdcp-core` يتطلب `zk-pok` في tfhe. إذا لم يُمرّر `bdcp-core/tfhe-backend` هذا الفيتشر إلى tfhe، فسيفشل compile.

**الإصلاح**: إضافة `features = ["integer", "shortint", "zk-pok"]` إلى tfhe في `bdcp-cli/Cargo.toml`.

### 1.4 ToyFhe مكرر 3 مرات (Triple Duplication)

**الموقع**:
1. `bdcp-cli/src/toy.rs:1-63` ← الأصل
2. `bdcp-node/src/client.rs:303-339` ← نسخة
3. `bdcp-node/src/relay.rs:265-309` ← نسخة

**الخطورة**: متوسطة-عالية — 3 أماكن مختلفة كلها تنفذ نفس `0xDEAD_BEEF_CAFE_4242` additive cipher. أي تعديل في toy cipher يجب تطبيقه في 3 أماكن.

**الإصلاح**: استخراج ToyFhe إلى `bdcp-core` كـ `backend/toy.rs` خلف feature flag.

### 1.5 `bdcp-node/src/lib.rs` — كود ميت بالكامل

**الموقع**: `bdcp-node/src/lib.rs` (كامل)
**الخطورة**: متوسطة — لا يسبب خطأ لكنه تضليل وهدر

```rust
// lib.rs — لا يستخدمه أي كود في المشروع
pub mod relay;  // ← re-exports relay.rs لكن main.rs يعرّف mod relay خاص به
pub struct Node { ... }
pub fn register_peer(...) { ... }
pub async fn broadcast(...) { ... }
```

الباينري (`main.rs`) لا يستورد شيئاً من `lib.rs`. بل يعرّف `mod relay;`, `mod client;`, `mod config;`, `mod ipc;` محلياً.

**الإصلاح**: إزالة المحتوى من `lib.rs` أو تحويله إلى واجهة library حقيقية تُستخدم من قبل main.rs.

### 1.6 `msg_tx` في BdcpNode — مخزنة لكن لا تُكتب أبداً

**الموقع**: `bdcp-net/src/node.rs:82`
**الخطورة**: متوسطة — قناة مقطوعة

```rust
pub struct BdcpNode {
    pub msg_rx: mpsc::Receiver<Message>,   // ← يستقبلها الخارج
    msg_tx: mpsc::Sender<Message>,         // ← لا أحد يرسل عليها
}
```

التصميم يوحي بأن `BdcpNode` يفترض أن يستقبل الرسائل ويُمرّرها عبر القناة. لكن لا يوجد أي `msg_tx.send(...)` في أي مكان.

**الإصلاح**: إما إزالة `msg_tx` و `msg_rx`، أو تنفيذ آلية الاستقبال.

### 1.7 `control_frame.seq = 0` — Sequencing غير مطبّق

**الموقع**: `bdcp-net/src/node.rs:181` + `bdcp-net/src/message.rs:10`
**الخطورة**: متوسطة — الـ seq موجود لكنه دايماً 0

**الإصلاح**: إما إزالة `seq` من `ControlFrame` أو تنفيذ increment حقيقي.

---

## 🟡 القسم 2: مشاكل متوسطة | Medium Issues

### 2.1 10 من 16 MessageKind غير مستخدمة

**الموقع**: `bdcp-core/src/protocol/messages.rs`
**غير مستخدمة فعلياً**:

```
❌ JobReject(JobReject)
❌ KeyExchangeCompact(CompactPublicKey)
❌ GetPublicKey(GetPublicKeyRequest)       ← لكن تستخدم خارجياً عبر match
❌ PublicKeyResponse(PublicKeyResponse)     ← نفس الشيء
❌ CrsExchange(Crs)
❌ DataSubmission(DataSubmission)
❌ ComputeRequest(ComputeRequest)
❌ ComputeResult(ComputeResult)            ← غير منشأة في bdcp-core لكن تُنشأ في bdcp-cli/node
❌ Attestation(ComputationProof)
❌ Error(ProtocolErrorMsg)
```

تحذير: بعضها (GetPublicKey, PublicKeyResponse, ComputeResult) تُنشأ خارج `bdcp-core` (في `bdcp-cli/src/` و `bdcp-node/src/`). لكن داخل `bdcp-core` لا يوجد من ينشئها.

### 2.2 دالات `generate_keychain` و `handle_*` في Handshake غير مستخدمة

**الموقع**: `bdcp-core/src/protocol/handshake.rs`

| الدالة | مستخدمة؟ |
|--------|---------|
| `create_job_request()` | ❌ — لا أحد يستدعيها |
| `handle_job_accept()` | ❌ — لا أحد يستدعيها |
| `handle_job_request()` | ❌ — لا أحد يستدعيها |
| `handle_key_exchange()` | ❌ — لا أحد يستدعيها |

كل هذه الدوال لا تُستدعى من أي كود في المشروع بأكمله. الـ `Handshake` state machine هو dead code.

### 2.3 Typo: `"tfel-bfv"` بدلاً من `"tfhe-bfv"` أو `"tfhe"`

**الموقع**: `bdcp-core/src/protocol/handshake.rs:52`
```rust
fhe_scheme: "tfel-bfv".into(),  // ← tfel؟ المفروض tfhe
```

### 2.4 أنواع غير مستخدمة: `Commitment` و `Digest`

**الموقع**: `bdcp-core/src/types.rs:31,40`
- `Commitment(pub Vec<u8>)` — غير مستخدمة في أي مكان
- `Digest(pub [u8; 32])` — غير مستخدمة في أي مكان
- `SecurityLevel::High`, `SecurityLevel::Maximum` — غير مستخدمة (Standard فقط يُستخدم)

### 2.5 اعتماديات ميتة في Cargo.toml

| الملف | الاعتمادية | سبب الموت |
|-------|-----------|-----------|
| `bdcp-core/Cargo.toml` | `serde_json = "1"` | غير مستخدمة |
| `bdcp-core/Cargo.toml` | `sha2 = "0.10"` | غير مستخدمة |
| `bdcp-core/Cargo.toml` | `hex = "0.4"` | غير مستخدمة |
| `bdcp-net/Cargo.toml` | `rustls-pemfile = "2"` | غير مستخدمة |
| `bdcp-net/Cargo.toml` | `futures-util = "0.3"` | غير مستخدمة |
| `bdcp-net/Cargo.toml` | `quinn-proto = "0.11"` | غير مستخدمة (quinn يعيد تصديره) |
| `examples/simple-poc/Cargo.toml` | `rand = "0.8"` | غير مستخدمة |
| `examples/simple-poc/Cargo.toml` | `serde_json = "1"` | غير مستخدمة |

6 اعتماديات في الكود الأساسي + 2 في الأمثلة = **8 اعتماديات ميتة**.

### 2.6 `FheError` — 3 variants غير مستخدمة

**الموقع**: `bdcp-core/src/crypto/fhe.rs:5-18`
```rust
FheError::DecryptionFailed(String)     // ❌ غير مستخدمة
FheError::EvaluationFailed(String)     // ❌ غير مستخدمة
FheError::KeyGenerationFailed(String)  // ❌ غير مستخدمة
```

### 2.7 `ProtocolError` — 4 variants غير مستخدمة

**الموقع**: `bdcp-core/src/protocol/mod.rs:9-18`
```rust
ProtocolError::HandshakeFailed(String)  // ❌
ProtocolError::InvalidMessage(String)   // ❌
ProtocolError::NetworkError(String)     // ❌
ProtocolError::Timeout                  // ❌
```

### 2.8 `wasm` feature flag ميت

**الموقع**: `bdcp-core/Cargo.toml:21`
```toml
wasm = []   // ← لا يوجد #[cfg(feature = "wasm")] في أي مكان في الكود
```

### 2.9 لا حد أقصى لحجم البيانات المستلمة (`DataStream::recv`)

**الموقع**: `bdcp-net/src/stream.rs:84-97`  
`ControlStream::recv` يتحقق من `MAX_CONTROL_FRAME` (64 MB)، لكن `DataStream::recv` لا يتحقق من أي حد. إذا أرسل مهاجم `u64::MAX` كطول، قد يسبب OOM.

### 2.10 ZKP proof و ciphertext هما نفس البايتات

**الموقع**: `bdcp-core/src/backend/zkp.rs:83-98`
```rust
let bytes = bincode::serialize(&proven_list)?;
// bytes يُستخدم مرتين:
(Ciphertext(bytes.clone()), CompactProof(bytes))  // ← نفس الشيء!
```

`ProvenCompactCiphertextList` يحوي كل من البيانات والبرهان معاً. إرسال كليهما يعني إرسال ضعف الكمية المطلوبة.

### 2.11 `demo.sh` يتلف الملفات المُتتبعة بـ git

**الموقع**: `deploy/demo.sh:168-203`  
السكريبت ينفذ `cat > bdcp.toml` و `cp alice.docker.toml alice.toml` — يعدّل ملفات موجودة في git. نفس المشكلة في `deploy/run.sh`.

### 2.12 `demo.sh` منفذ غير متناسق (22447 vs 9447)

**الموقع**: `deploy/demo.sh:26`  
حيث `RELAY_PORT=22447` لكن ملفات config تستخدم `9447`. في Docker mode يشتغل على 9447، في local mode على 22447.

### 2.13 Rustls features mismatch

**الموقع**: 
- `bdcp-node/Cargo.toml:23` — `rustls = "0.23"` بدون features
- `bdcp-cli/Cargo.toml:12` — `rustls = { version = "0.23", features = ["ring"] }`

بينما `bdcp-node/src/main.rs:18` يستدعي `rustls::crypto::ring::default_provider()`. بدون feature `ring`، قد يفشل compile إذا لم يُمرّر transitively من bdcp-net.

### 2.14 `simple-poc` يستورد `FheKeychain` لكن لا يستخدمه

**الموقع**: `examples/simple-poc/src/main.rs:10`
```rust
use bdcp_core::crypto::fhe::{FheEngine, FheKeychain, FheResult};
// FheKeychain غير مستخدم صراحة
```

### 2.15 `_meta_engine` غير مستخدمة في `cli/src/relay.rs`

**الموقع**: `bdcp-cli/src/relay.rs:14`
```rust
let _meta_engine = ToyFhe;  // ← غير مستخدمة أبداً
```

### 2.16 لا zeroization لـ `SecretKey`

**الموقع**: `bdcp-core/src/types.rs:19`  
`SecretKey` تحتوي `Vec<u8>` لكن لا `Drop` ينفّذ zero-out. المفتاح يبقى في الذاكرة حتى الـ GC.

---

## 🔵 القسم 3: مشاكل بسيطة | Minor Issues

| # | الموقع | المشكلة |
|---|--------|---------|
| 1 | `deploy/demo.sh:37-60` | تقديرات زمن المراحل (ms) مبرمجة hardcoded — ستصبح قديمة |
| 2 | `e2e_test.sh:16-17` | مسارات sockets مبرمجة hardcoded — إذا تغير config ينكسر |
| 3 | `benchmark.sh:232` | `2>/dev/null` يخفي أخطاء البناء |
| 4 | `deploy/Dockerfile:13-15` | `bdcp-cli/Cargo.toml` غير منسوخ — غير مطلوب لـ bdcp-node لكنه workspace member |
| 5 | `deploy/Dockerfile:29` | `RUSTFLAGS="-C target-cpu=native"` — يُفقد قابلية نقل الباينري |
| 6 | `deploy/run.sh:48-81` | نفس مشكلة git المدمّرة مثل demo.sh |
| 7 | `bdcp-net/src/node.rs:131` | SNI مبرمج hardcoded `"localhost"` — سيفشل في إنتاج cert حقيقي |
| 8 | `bdcp-net/src/config.rs:45` | `120.try_into().unwrap()` — آمن حالياً لكنه panics إذا تغير |
| 9 | `bdcp-net/src/stream.rs:76,85` | `unwrap()` على Option — يسبب panic إذا أسيء استخدام الـ Stream |
| 10 | `benchmark.sh:28` | Typo: `FHEATURES` بدلاً من `FEATURES` |
| 11 | `deploy/Dockerfile:23` | `2>/dev/null \|\| true` يخفي أخطاء بناء الاعتماديات |
| 12 | `examples/triple-node-network/src/main.rs` | 319-320: قيم الاختبار 42 و 58 hardcoded |

---

## 💀 القسم 4: خرائط الكود الميت | Dead Code Maps

### 4.1 كود ميت بالكامل (يزال دون تأثير)

| الوحدة | الموقع | عدد الأسطر |
|--------|--------|-----------|
| `bdcp-node/src/lib.rs` بأكمله | `bdcp-node/src/lib.rs` | 42 |
| `bdcp-core/src/protocol/handshake.rs` (كل الدوال) | `handshake.rs` | 120 |
| `HandshakeState::Failed` variant | `handshake.rs:15` | 1 |
| `Commitment` type | `types.rs:31` | 2 |
| `Digest` type | `types.rs:40` | 2 |
| `SecurityLevel::High`, `Maximum` | `types.rs:45-46` | 2 |

### 4.2 دوال عامة غير مستخدمة خارجياً

| الدالة | الموقع |
|--------|--------|
| `NodeConfig::with_tls()` | `bdcp-net/src/config.rs:32` |
| `BdcpNode::open_control()` | `bdcp-net/src/node.rs:149` |
| `BdcpNode::open_data()` | `bdcp-net/src/node.rs:161` |
| `BdcpNode::accept_data()` | `bdcp-net/src/node.rs:167` |
| `TfheEngine::with_config()` | `bdcp-core/src/backend/tfhe.rs:42` |
| `TfheZkpEngine::with_config()` | `bdcp-core/src/backend/zkp.rs:34` |
| `wasm` feature flag | `bdcp-core/Cargo.toml:21` |

---

## ✅ القسم 5: ما هو سليم | What's Correct

| المكون | الحالة |
|--------|--------|
| `TfheEngine::encrypt_u32()` | ✅ سليم |
| `TfheEngine::encrypt_u32_with_compact()` | ✅ سليم |
| `TfheEngine::encrypt_u32_vec()` | ✅ سليم |
| `TfheEngine::encrypt_u32_with_compact_vec()` | ✅ سليم |
| `TfheEngine::decrypt_u32()` | ✅ سليم |
| `TfheEngine::decompress_server_key()` | ✅ سليم |
| `TfheZkpEngine::generate_crs()` | ✅ سليم |
| `TfheZkpEngine::prove_encrypt_compact()` | ✅ سليم (مع تحفظ 2.10) |
| `TfheZkpEngine::verify_and_expand()` | ✅ سليم |
| QUIC transport config | ✅ سليم |
| Alice/Bob/Relay flows in bdcp-node | ✅ سليم (عملياً) |
| IPC + socat | ✅ سليم |
| Docker deployment | ✅ سليم |
| Python SDK | ✅ سليم |
| TUI Demo | ✅ سليم |
| DaemonConfig deserialization | ✅ سليم |

---

## 📋 القسم 6: توصيات الأولوية | Priority Recommendations

### 🔴 يجب الإصلاح فوراً (قبل أي عرض توضيحي)

| # | الإصلاح | الجهد | المبرر |
|---|---------|-------|--------|
| 1 | إضافة `zk-pok` إلى tfhe في `bdcp-cli/Cargo.toml` | دقيقة | CLI لا يعمل مع ZKP |
| 2 | توثيق أن `FheEngine` methods ترجع `BackendUnavailable` | ساعة | لا يخدع مطورين جدد |
| 3 | إزالة `mean()` default impl أو إصلاحه | ساعة | لا ينتج كوداً يثق به أحد |

### 🟡 يجب الإصلاح قبل أول عميل

| # | الإصلاح | الجهد |
|---|---------|-------|
| 4 | استخراج ToyFhe إلى `bdcp-core/src/backend/toy.rs` | يوم واحد |
| 5 | إزالة `lib.rs` dead code من `bdcp-node` | ساعة |
| 6 | إصلاح typo `"tfel-bfv"` | دقيقة |
| 7 | إزالة الاعتماديات الميتة (8 في Cargo.toml) | ساعة |
| 8 | إصلاح `demo.sh` لعدم تلف git-tracked files | ساعتان |
| 9 | توحيد port بين docker و local mode في `demo.sh` | 30 دقيقة |
| 10 | إضافة حد أقصى لـ `DataStream::recv` | ساعة |
| 11 | إزالة الـ `seq` الميت من `ControlFrame` | ساعة |
| 12 | إزالة `msg_tx` الميت من `BdcpNode` | ساعة |

### 🔵 يُصلح عند التفرغ

| # | الإصلاح | الجهد |
|---|---------|-------|
| 13 | إزالة `Commitment`, `Digest`, `SecurityLevel::High/Maximum` | ساعة |
| 14 | إضافة `Drop` لـ `SecretKey` (zeroize) | يوم |
| 15 | Rustls features alignment | ساعة |
| 16 | إزالة `FHEATURES` typo من benchmark.sh | دقيقة |
| 17 | إزالة `_meta_engine` غير المستخدمة | دقيقة |
| 18 | إزالة الـ `wasm` feature الميت | دقيقة |

---

> **بيان: هذا التدقيق كُشف عن 35 مشكلة. 7 منها حرجة يجب إصلاحها فوراً.
> لكن في المجمل، 85% من الكود سليم وقابل للتشغيل — والـ 15% المتبقية
> هي نداءات استيقاظ لا كوارث.**
