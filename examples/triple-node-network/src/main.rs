//! BDCP triple-node integration test.
//!
//! Runs Relay, Alice, and Bob as concurrent Tokio tasks communicating
//! over QUIC. Demonstrates the full protocol lifecycle:
//!
//! 1. Relay listens on 127.0.0.1:9447
//! 2. Alice connects, creates a job, sends compressed key + encrypted data
//! 3. Bob connects, accepts the job, sends his encrypted data
//! 4. Relay computes the sum homomorphically, sends result to Alice
//! 5. Alice decrypts and prints the result
//!
//! Uses a toy additive cipher (not real FHE) for fast iteration.
//! The QUIC transport, stream architecture, and protocol messages
//! are all real — ready for swap-in of TfheEngine.

use tracing::info;

use bdcp_core::crypto::fhe::{FheEngine, FheKeychain, FheResult};
use bdcp_core::protocol::messages::{ComputeResult, JobAccept, JobRequest, Message, MessageKind};
use bdcp_core::types::{Ciphertext, CompactPublicKey, EvaluationKey, NodeId, Plaintext, PublicKey, SecretKey, SecurityLevel};
use bdcp_net::{BdcpNode, NodeConfig, NodeRole};

// ═══════════════════════════════════════════════════════════════
//  Toy FHE (additive cipher — NOT secure, for integration only)
// ═══════════════════════════════════════════════════════════════

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
        Ok(Ciphertext(x.wrapping_add(y).wrapping_sub(k).to_le_bytes().to_vec()))
    }

    fn mul(&self, _a: &Ciphertext, _b: &Ciphertext, _ek: &EvaluationKey) -> FheResult<Ciphertext> {
        unimplemented!("mul not used in this test")
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

// ═══════════════════════════════════════════════════════════════
//  Relay task
// ═══════════════════════════════════════════════════════════════

async fn run_relay(
    alice_val: u64,
    bob_val: u64,
    alice_result: tokio::sync::oneshot::Sender<u64>,
    bob_release: tokio::sync::oneshot::Sender<()>,
    relay_release: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let engine = ToyFhe;
    let relay = BdcpNode::new(
        NodeRole::Relay,
        NodeConfig::new("relay", "127.0.0.1:9447"),
    )
    .await?;

    info!("Relay listening on 127.0.0.1:9447");

    // ── Accept Alice ──────────────────────────────────────
    let alice_conn = relay.accept().await?;
    info!("Accepted Alice connection");

    let mut alice_ctrl = relay.accept_control(&alice_conn).await?;
    let alice_frame = alice_ctrl.recv().await?;
    let job_req = match &alice_frame.message.kind {
        MessageKind::JobRequest(jr) => jr,
        _ => anyhow::bail!("expected JobRequest from Alice"),
    };
    info!("Received JobRequest: {}", job_req.description);

    // Read Alice's data streams: evaluation key, compact pk, ciphertext
    let ek_bytes = relay.receive_data(&alice_conn).await?;
    let _cpk_bytes = relay.receive_data(&alice_conn).await?;
    let alice_ct_bytes = relay.receive_data(&alice_conn).await?;
    let alice_ct = Ciphertext(alice_ct_bytes);

    info!("Received Alice's data (ek + cpk + ct)");

    // ── Accept Bob ────────────────────────────────────────
    let bob_conn = relay.accept().await?;
    info!("Accepted Bob connection");

    let mut bob_ctrl = relay.accept_control(&bob_conn).await?;
    let bob_frame = bob_ctrl.recv().await?;
    match &bob_frame.message.kind {
        MessageKind::JobAccept(ja) => {
            info!("Received JobAccept for job {}", ja.job_id.0);
        }
        _ => anyhow::bail!("expected JobAccept from Bob"),
    }

    // Read Bob's ciphertext
    let bob_ct_bytes = relay.receive_data(&bob_conn).await?;
    let bob_ct = Ciphertext(bob_ct_bytes);
    info!("Received Bob's ciphertext");

    // Bob can now disconnect safely
    let _ = bob_release.send(());

    // ── Compute ───────────────────────────────────────────
    info!("Computing encrypted addition...");
    let sum = engine.add(&alice_ct, &bob_ct, &EvaluationKey(ek_bytes))?;
    info!("Computed E(sum)");

    // ── Send result to Alice ──────────────────────────────
    relay
        .send_message(
            &alice_conn,
            &Message {
                kind: MessageKind::ComputeResult(ComputeResult {
                    data: sum.clone(),
                    proof: None,
                }),
                sender: NodeId("relay".into()),
                job_id: alice_frame.message.job_id.clone(),
                timestamp: 0,
            },
        )
        .await?;
    info!("Sent ComputeResult to Alice");

    // ── Verify locally (toy only — decrypt with shared key) ──
    let sum_pt = engine.decrypt(&sum, &SecretKey(KEY.to_le_bytes().to_vec()))?;
    let result = u64::from_le_bytes(sum_pt.0[..8].try_into().unwrap());
    info!(
        "Relay verification: {} + {} = {} ✓",
        alice_val, bob_val, result
    );
    assert_eq!(result, alice_val + bob_val);

    let _ = alice_result.send(result);

    // Wait for Alice to confirm receipt before dropping endpoint
    let _ = relay_release.await;
    info!("Relay shutting down");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  Alice task
// ═══════════════════════════════════════════════════════════════

async fn run_alice(
    val: u64,
    relay_release: tokio::sync::oneshot::Sender<()>,
) -> anyhow::Result<u64> {
    let engine = ToyFhe;
    let alice = BdcpNode::new(
        NodeRole::Client,
        NodeConfig::new("alice", "127.0.0.1:0"),
    )
    .await?;

    // Small delay to ensure relay is listening
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let conn = alice.connect("127.0.0.1:9447").await?;
    info!("Alice connected");

    let kc = engine.generate_keychain(SecurityLevel::Standard)?;
    let alice_ct = engine.encrypt(
        &Plaintext(val.to_le_bytes().to_vec()),
        &kc.public_key,
    )?;

    // Send job request over control stream
    alice
        .send_message(
            &conn,
            &Message {
                kind: MessageKind::JobRequest(JobRequest {
                    description: "encrypted addition".into(),
                    required_participants: 2,
                    fhe_scheme: "toy-additive".into(),
                    security_level: SecurityLevel::Standard,
                }),
                sender: NodeId("alice".into()),
                job_id: bdcp_core::types::JobId("triple-test".into()),
                timestamp: 0,
            },
        )
        .await?;
    info!("Alice sent JobRequest");

    // Send data streams: evaluation key, compact public key, ciphertext
    alice.stream_data(&conn, &kc.evaluation_key.0).await?;
    alice.stream_data(&conn, &kc.compact_public_key.0).await?;
    alice.stream_data(&conn, &alice_ct.0).await?;
    info!("Alice sent her data streams");

    // Wait for result via control stream
    let mut result_ctrl = alice.accept_control(&conn).await?;
    let result_frame = result_ctrl.recv().await?;
    let result_data = match &result_frame.message.kind {
        MessageKind::ComputeResult(cr) => cr.data.clone(),
        _ => anyhow::bail!("expected ComputeResult"),
    };
    info!("Alice received ComputeResult");

    let result_pt = engine.decrypt(&result_data, &kc.secret_key)?;
    let result = u64::from_le_bytes(result_pt.0[..8].try_into().unwrap());
    info!("Alice decrypted result: {}", result);

    // Signal relay that it can shut down safely
    let _ = relay_release.send(());
    info!("Alice disconnecting");

    Ok(result)
}

// ═══════════════════════════════════════════════════════════════
//  Bob task
// ═══════════════════════════════════════════════════════════════

async fn run_bob(val: u64, release_rx: tokio::sync::oneshot::Receiver<()>) -> anyhow::Result<()> {
    let engine = ToyFhe;
    let bob = BdcpNode::new(
        NodeRole::Client,
        NodeConfig::new("bob", "127.0.0.1:0"),
    )
    .await?;

    // Small delay to ensure relay is listening
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let conn = bob.connect("127.0.0.1:9447").await?;
    info!("Bob connected");

    // Bob uses Alice's compact public key to encrypt (in toy FHE, KEY is shared)
    let bob_ct = engine.encrypt_compact(
        &Plaintext(val.to_le_bytes().to_vec()),
        &CompactPublicKey(KEY.to_le_bytes().to_vec()),
    )?;

    // Send job accept over control stream
    bob.send_message(
        &conn,
        &Message {
            kind: MessageKind::JobAccept(JobAccept {
                job_id: bdcp_core::types::JobId("triple-test".into()),
                supported_ops: vec!["add".into()],
            }),
            sender: NodeId("bob".into()),
            job_id: bdcp_core::types::JobId("triple-test".into()),
            timestamp: 0,
        },
    )
    .await?;
    info!("Bob sent JobAccept");

    // Send ciphertext via data stream
    bob.stream_data(&conn, &bob_ct.0).await?;
    info!("Bob sent his ciphertext");

    // Wait for relay to read data before dropping connection
    let _ = release_rx.await;
    info!("Bob disconnecting");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  Main — orchestrate all three
// ═══════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Must install a rustls crypto provider before any QUIC usage.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("rustls crypto provider install failed");

    tracing_subscriber::fmt::init();

    let alice_val: u64 = 42;
    let bob_val: u64 = 58;
    let expected = alice_val + bob_val;

    let (alice_result_tx, alice_result_rx) = tokio::sync::oneshot::channel::<u64>();
    let (bob_release_tx, bob_release_rx) = tokio::sync::oneshot::channel::<()>();
    let (relay_release_tx, relay_release_rx) = tokio::sync::oneshot::channel::<()>();

    info!("═══ BDCP Triple-Node Network Test ═══");
    info!("Alice value: {}", alice_val);
    info!("Bob value:   {}", bob_val);
    info!("Expected:    {}", expected);

    // Spawn relay (must be first)
    let relay_handle = tokio::spawn(async move {
        if let Err(e) = run_relay(alice_val, bob_val, alice_result_tx, bob_release_tx, relay_release_rx).await {
            panic!("Relay failed: {e}");
        }
    });

    // Spawn Alice and Bob concurrently
    let alice_handle = tokio::spawn(async move { run_alice(alice_val, relay_release_tx).await });

    let bob_handle = tokio::spawn(async move { run_bob(bob_val, bob_release_rx).await });

    // Wait for Alice's result
    let alice_result = alice_handle.await??;
    bob_handle.await??;
    relay_handle.await?;

    // Also wait for relay channel
    let relay_result = alice_result_rx.await.unwrap();

    info!("═══ Result ═══");
    info!("Alice received: {} (expected: {})", alice_result, expected);
    info!("Relay verified: {} (expected: {})", relay_result, expected);

    assert_eq!(alice_result, expected);
    assert_eq!(relay_result, expected);

    println!(
        "\n  ✓ BDCP Triple-Node Test PASSED: {} + {} = {}\n",
        alice_val, bob_val, alice_result
    );

    Ok(())
}
