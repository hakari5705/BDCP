use bdcp_core::backend::toy::ToyFhe;
use bdcp_core::crypto::fhe::FheEngine;
use bdcp_core::protocol::messages::{
    ComputeResult, Message, MessageKind, PublicKeyResponse as PubKeyResp,
};
use bdcp_core::types::{Ciphertext, CompactPublicKey, Crs, EvaluationKey, NodeId, SecretKey};
use bdcp_net::{BdcpNode, NodeConfig, NodeRole};
use std::net::SocketAddr;
use tracing::info;

/// Run the relay daemon forever.
pub async fn run(
    name: &str,
    listen: SocketAddr,
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<()> {
    let relay = BdcpNode::new(NodeRole::Relay, NodeConfig::new(name, listen.to_string())).await?;

    info!("Relay daemon '{name}' listening on {listen} (tfhe={use_tfhe}, zkp={use_zkp})");

    loop {
        match process_job(&relay, use_tfhe, use_zkp).await {
            Ok(()) => info!("Job completed, waiting for next pair..."),
            Err(e) => tracing::warn!("Job failed: {e}"),
        }
    }
}

async fn process_job(
    relay: &BdcpNode,
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<()> {
    // ── Accept connections ─────────────────────────────────
    let conn_1 = relay.accept().await?;
    let mut ctrl_1 = relay.accept_control(&conn_1).await?;
    let frame_1 = ctrl_1.recv().await?;

    let conn_2 = relay.accept().await?;
    let mut ctrl_2 = relay.accept_control(&conn_2).await?;
    let frame_2 = ctrl_2.recv().await?;

    let (alice_conn, bob_conn, alice_frame, bob_frame) =
        match (&frame_1.message.kind, &frame_2.message.kind) {
            (MessageKind::JobRequest(_), MessageKind::GetPublicKey(_)) => {
                info!("Peer 1 = Alice, Peer 2 = Bob");
                (conn_1, conn_2, frame_1, frame_2)
            }
            (MessageKind::GetPublicKey(_), MessageKind::JobRequest(_)) => {
                info!("Peer 1 = Bob, Peer 2 = Alice");
                (conn_2, conn_1, frame_2, frame_1)
            }
            (k1, k2) => {
                anyhow::bail!("expected JobRequest + GetPublicKey, got {k1:?} + {k2:?}");
            }
        };

    if let MessageKind::JobRequest(jr) = &alice_frame.message.kind {
        info!("Alice job: {} participants", jr.required_participants);
    }
    let job_id = alice_frame.message.job_id.clone();

    // ── Receive Alice's data streams ───────────────────────
    let crs = if use_zkp {
        let crs_bytes = relay.receive_data(&alice_conn).await?;
        info!("Received CRS ({} bytes)", crs_bytes.len());
        Crs(crs_bytes)
    } else {
        Crs(Vec::new())
    };

    let ek_bytes = relay.receive_data(&alice_conn).await?;
    let cpk_bytes = relay.receive_data(&alice_conn).await?;
    let alice_batch_raw = relay.receive_data(&alice_conn).await?;
    let alice_batch: Vec<Ciphertext> = bincode::deserialize(&alice_batch_raw)
        .map_err(|e| anyhow::anyhow!("deserialize Alice batch: {e}"))?;
    let cpk = CompactPublicKey(cpk_bytes);
    info!(
        "Received Alice's data (ek + cpk + {} ciphertexts)",
        alice_batch.len()
    );

    // ── Handle Bob's GetPublicKey + stream CRS ─────────────
    if let MessageKind::GetPublicKey(req) = &bob_frame.message.kind {
        info!("Bob requested pubkey for job {}", req.job_id.0);
        relay
            .send_message(
                &bob_conn,
                &Message {
                    kind: MessageKind::PublicKeyResponse(PubKeyResp {
                        compact_public_key: cpk.clone(),
                    }),
                    sender: NodeId("relay".into()),
                    job_id: job_id.clone(),
                    timestamp: 0,
                },
            )
            .await?;
    }

    if use_zkp {
        relay.stream_data(&bob_conn, &crs.0).await?;
        info!("Streamed CRS to Bob ({} bytes)", crs.0.len());
    }

    let bob_raw = relay.receive_data(&bob_conn).await?;
    info!("Received Bob's data ({} bytes)", bob_raw.len());

    // ── Compute encrypted addition ─────────────────────────
    info!("Computing encrypted addition ({} values)...", alice_batch.len());
    let sum_batch: Vec<Ciphertext> = tfhe_compute(
        use_tfhe,
        use_zkp,
        &alice_batch,
        &bob_raw,
        &cpk,
        &crs,
        &ek_bytes,
    )?;
    info!("Computed E(sum) batch ({} values)", sum_batch.len());

    let sum_bytes = bincode::serialize(&sum_batch)
        .map_err(|e| anyhow::anyhow!("serialize result batch: {e}"))?;

    // ── Send result to Alice ───────────────────────────────
    relay
        .send_message(
            &alice_conn,
            &Message {
                kind: MessageKind::ComputeResult(ComputeResult {
                    data: Ciphertext(sum_bytes.clone()),
                    proof: None,
                }),
                sender: NodeId("relay".into()),
                job_id: alice_frame.message.job_id,
                timestamp: 0,
            },
        )
        .await?;
    info!("Sent result to Alice");

    Ok(())
}

/// Dispatch to TFHE or toy compute.
#[cfg(feature = "tfhe-backend")]
fn tfhe_compute(
    use_tfhe: bool,
    use_zkp: bool,
    alice_batch: &[Ciphertext],
    bob_raw: &[u8],
    cpk: &CompactPublicKey,
    crs: &Crs,
    ek_bytes: &[u8],
) -> anyhow::Result<Vec<Ciphertext>> {
    if !use_tfhe {
        return toy_compute(alice_batch, bob_raw, ek_bytes);
    }

    use bdcp_core::backend::tfhe::TfheEngine;
    use bdcp_core::backend::zkp::TfheZkpEngine;
    use bdcp_core::crypto::zkp::ZkpEngine as _;

    let tfhe_engine = TfheEngine::new();
    let compressed = EvaluationKey(ek_bytes.to_vec());
    let server_key = tfhe_engine
        .decompress_server_key(&compressed)
        .map_err(|e| anyhow::anyhow!("decompress ServerKey: {e}"))?;
    tfhe::set_server_key(server_key);

    let mut fa_vals: Vec<tfhe::FheUint32> = Vec::with_capacity(alice_batch.len());
    for ct in alice_batch {
        fa_vals.push(
            bincode::deserialize(&ct.0)
                .map_err(|e| anyhow::anyhow!("deserialize Alice ct: {e}"))?,
        );
    }

    let fb_vals: Vec<tfhe::FheUint32> = if use_zkp {
        let zkp = TfheZkpEngine::new();
        let proof = bdcp_core::types::CompactProof(bob_raw.to_vec());
        let cts = zkp
            .verify_and_expand(&proof, cpk, crs, b"bdcp-zkp", alice_batch.len())?;
        info!("Bob's ZK proof verified ({} values)", cts.len());
        cts.iter()
            .map(|ct| {
                bincode::deserialize(&ct.0)
                    .map_err(|e| anyhow::anyhow!("deserialize Bob expanded ct: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let bob_batch: Vec<Ciphertext> = bincode::deserialize(bob_raw)
            .map_err(|e| anyhow::anyhow!("deserialize Bob batch: {e}"))?;
        if bob_batch.len() != alice_batch.len() {
            anyhow::bail!(
                "value count mismatch: Alice sent {}, Bob sent {}",
                alice_batch.len(),
                bob_batch.len()
            );
        }
        bob_batch
            .iter()
            .map(|ct| {
                bincode::deserialize(&ct.0)
                    .map_err(|e| anyhow::anyhow!("deserialize Bob ct: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    let sums: Vec<tfhe::FheUint32> = fa_vals
        .into_iter()
        .zip(fb_vals.into_iter())
        .map(|(fa, fb)| fa + fb)
        .collect();

    sums.into_iter()
        .map(|s| {
            bincode::serialize(&s)
                .map(Ciphertext)
                .map_err(|e| anyhow::anyhow!("serialize sum ct: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()
}

/// Fallback when tfhe-backend is disabled.
#[cfg(not(feature = "tfhe-backend"))]
fn tfhe_compute(
    _use_tfhe: bool,
    _use_zkp: bool,
    alice_batch: &[Ciphertext],
    bob_raw: &[u8],
    _cpk: &CompactPublicKey,
    _crs: &Crs,
    ek_bytes: &[u8],
) -> anyhow::Result<Vec<Ciphertext>> {
    if !_use_tfhe {
        return toy_compute(alice_batch, bob_raw, ek_bytes);
    }
    anyhow::bail!("TFHE backend not available — recompile with --features tfhe-backend");
}

/// Toy additive cipher (single value only, for testing).
fn toy_compute(
    alice_batch: &[Ciphertext],
    bob_raw: &[u8],
    ek_bytes: &[u8],
) -> anyhow::Result<Vec<Ciphertext>> {
    let bob_ct = Ciphertext(bob_raw.to_vec());
    let engine = ToyFhe;
    let sum = engine.add(&alice_batch[0], &bob_ct, &EvaluationKey(ek_bytes.to_vec()))?;

    let engine = ToyFhe;
    let sk = SecretKey(0xDEAD_BEEF_CAFE_4242u64.to_le_bytes().to_vec());
    let pt = engine.decrypt(&sum, &sk)?;
    let v = u64::from_le_bytes(pt.0[..8].try_into().unwrap());
    println!("  ✓ Computed: {v}");

    Ok(vec![sum])
}
