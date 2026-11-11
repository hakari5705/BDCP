//! Alice and Bob daemon flows — connect to relay, submit values, collect result.

use bdcp_core::backend::toy::ToyFhe;
use bdcp_core::crypto::fhe::{FheEngine, FheKeychain};
use bdcp_core::protocol::messages::{
    GetPublicKeyRequest, JobRequest, Message, MessageKind,
};
use bdcp_core::types::{Ciphertext, Crs, NodeId, Plaintext, SecurityLevel};
#[cfg(feature = "tfhe-backend")]
use bdcp_core::types::CompactPublicKey;
use bdcp_net::{BdcpNode, NodeConfig, NodeRole};
use std::net::SocketAddr;

/// Run the Alice flow: connect to relay, send encrypted values, get decrypted result.
pub async fn alice_submit(
    relay_addr: SocketAddr,
    values: &[u32],
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<Vec<u32>> {
    // Generate keys BEFORE connecting — CPU-bound work must not block QUIC.
    let (kc, cts, crs) = tokio::task::spawn_blocking({
        let values = values.to_vec();
        move || generate_keys_and_encrypt(&values, use_tfhe, use_zkp)
    })
    .await
    .map_err(|e| anyhow::anyhow!("keygen task panicked: {e}"))??;

    let node = BdcpNode::new(NodeRole::Client, NodeConfig::new("alice-daemon", "127.0.0.1:0"))
        .await?;
    let conn = node.connect(&relay_addr.to_string()).await?;

    // Send JobRequest
    node.send_message(
        &conn,
        &Message {
            kind: MessageKind::JobRequest(JobRequest {
                description: "encrypted addition (daemon)".into(),
                required_participants: 2,
                fhe_scheme: if use_tfhe {
                    "tfhe-rs-v1.6-integer".into()
                } else {
                    "toy-additive".into()
                },
                security_level: SecurityLevel::Standard,
            }),
            sender: NodeId("alice".into()),
            job_id: bdcp_core::types::JobId("daemon-job".into()),
            timestamp: 0,
        },
    )
    .await?;

    // Stream CRS (if ZKP), evaluation key, compact public key, ciphertexts
    if use_zkp {
        node.stream_data(&conn, &crs.0).await?;
    }
    node.stream_data(&conn, &kc.evaluation_key.0).await?;
    node.stream_data(&conn, &kc.compact_public_key.0).await?;
    let batch_bytes = bincode::serialize(&cts)
        .map_err(|e| anyhow::anyhow!("serialize batch: {e}"))?;
    node.stream_data(&conn, &batch_bytes).await?;

    // Wait for result
    let mut result_ctrl = node.accept_control(&conn).await?;
    let result_frame = result_ctrl.recv().await?;
    let result_data = match &result_frame.message.kind {
        MessageKind::ComputeResult(cr) => &cr.data,
        other => anyhow::bail!("expected ComputeResult, got {other:?}"),
    };
    drop(conn);

    decrypt_result(result_data, &kc, use_tfhe)
}

/// Run the Bob flow: connect to relay, encrypt and submit values.
pub async fn bob_submit(
    relay_addr: SocketAddr,
    values: &[u32],
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<()> {
    let node = BdcpNode::new(NodeRole::Client, NodeConfig::new("bob-daemon", "127.0.0.1:0"))
        .await?;
    let conn = node.connect(&relay_addr.to_string()).await?;

    // Request CompactPublicKey
    node.send_message(
        &conn,
        &Message {
            kind: MessageKind::GetPublicKey(GetPublicKeyRequest {
                job_id: bdcp_core::types::JobId("daemon-job".into()),
            }),
            sender: NodeId("bob".into()),
            job_id: bdcp_core::types::JobId("daemon-job".into()),
            timestamp: 0,
        },
    )
    .await?;

    // Receive CPK
    let mut resp_ctrl = node.accept_control(&conn).await?;
    let resp_frame = resp_ctrl.recv().await?;
    let cpk = match &resp_frame.message.kind {
        MessageKind::PublicKeyResponse(resp) => resp.compact_public_key.clone(),
        other => anyhow::bail!("expected PublicKeyResponse, got {other:?}"),
    };

    // Receive CRS if ZKP
    let crs = if use_zkp {
        let crs_bytes = node.receive_data(&conn).await?;
        Crs(crs_bytes)
    } else {
        Crs(Vec::new())
    };

    // Encrypt and submit
    // CPU-bound work (key gen, ZK proof) runs on blocking pool to keep QUIC alive.
    if use_tfhe && use_zkp {
        let val_owned = values.to_vec();
        let cpk_owned = cpk.clone();
        let crs_owned = crs.0.clone();
        let proof_bytes = tokio::task::spawn_blocking(move || {
            bob_prove_raw(&val_owned, &cpk_owned.0, &crs_owned)
        })
        .await
        .map_err(|e| anyhow::anyhow!("ZK proof task panicked: {e}"))??;
        node.stream_data(&conn, &proof_bytes).await?;
    } else if use_tfhe {
        let val_owned = values.to_vec();
        let cpk_owned = cpk.clone();
        let batch_bytes = tokio::task::spawn_blocking(move || {
            bob_tfhe_raw(&val_owned, &cpk_owned.0)
        })
        .await
        .map_err(|e| anyhow::anyhow!("TFHE encrypt task panicked: {e}"))??;
        node.stream_data(&conn, &batch_bytes).await?;
    } else {
        let engine = ToyFhe;
        let ct = engine
            .encrypt_compact(
                &Plaintext((values[0] as u64).to_le_bytes().to_vec()),
                &cpk,
            )?;
        node.stream_data(&conn, &ct.0).await?;
    }

    // Keep connection alive for relay to read
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    drop(conn);
    Ok(())
}

// ═══════════════════════════════════════════════════════════
// Internal helpers (mirrored from bdcp-cli)
// ═══════════════════════════════════════════════════════════

#[cfg(feature = "tfhe-backend")]
fn generate_keys_and_encrypt(
    values: &[u32],
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<(FheKeychain, Vec<Ciphertext>, Crs)> {
    if !use_tfhe {
        return Ok(toy_keychain_and_encrypt(values));
    }
    use bdcp_core::backend::tfhe::TfheEngine;
    use bdcp_core::backend::zkp::TfheZkpEngine;
    use bdcp_core::crypto::zkp::ZkpEngine as _;

    let (engine, crs) = if use_zkp {
        let zkp_config = TfheZkpEngine::zkp_config();
        let engine = TfheEngine::with_config(zkp_config.clone());
        let zkp = TfheZkpEngine::with_config(zkp_config);
        let num_slots = values.len().next_power_of_two();
        let log2_slots = num_slots.ilog2() as usize;
        let crs = zkp.generate_crs(log2_slots)?;
        (engine, crs)
    } else {
        (TfheEngine::new(), Crs(Vec::new()))
    };

    let kc = engine.generate_keychain(SecurityLevel::Standard)?;
    let ck: tfhe::ClientKey =
        bincode::deserialize(&kc.secret_key.0).map_err(|e| anyhow::anyhow!("deserialize ClientKey: {e}"))?;
    let cts = engine.encrypt_u32_vec(values, &ck)?;
    Ok((kc, cts, crs))
}

#[cfg(not(feature = "tfhe-backend"))]
fn generate_keys_and_encrypt(
    _values: &[u32],
    use_tfhe: bool,
    _use_zkp: bool,
) -> anyhow::Result<(FheKeychain, Vec<Ciphertext>, Crs)> {
    if use_tfhe {
        anyhow::bail!("TFHE backend not available — recompile with --features tfhe-backend");
    }
    Ok(toy_keychain_and_encrypt(_values))
}

fn toy_keychain_and_encrypt(values: &[u32]) -> (FheKeychain, Vec<Ciphertext>, Crs) {
    let engine = ToyFhe;
    let kc = engine
        .generate_keychain(SecurityLevel::Standard)
        .expect("toy keychain");
    let ct = engine
        .encrypt(
            &Plaintext((values[0] as u64).to_le_bytes().to_vec()),
            &kc.public_key,
        )
        .expect("toy encrypt");
    (kc, vec![ct], Crs(Vec::new()))
}

#[cfg(feature = "tfhe-backend")]
fn decrypt_result(
    result_data: &Ciphertext,
    kc: &FheKeychain,
    use_tfhe: bool,
) -> anyhow::Result<Vec<u32>> {
    if !use_tfhe {
        return toy_decrypt(result_data, kc);
    }
    use bdcp_core::backend::tfhe::TfheEngine;
    let engine = TfheEngine::new();
    let ck: tfhe::ClientKey =
        bincode::deserialize(&kc.secret_key.0).map_err(|e| anyhow::anyhow!("deserialize ClientKey: {e}"))?;
    let result_batch: Vec<Ciphertext> = bincode::deserialize(&result_data.0)
        .map_err(|e| anyhow::anyhow!("deserialize result batch: {e}"))?;
    let mut results = Vec::with_capacity(result_batch.len());
    for ct in &result_batch {
        results.push(engine.decrypt_u32(ct, &ck)?);
    }
    Ok(results)
}

#[cfg(not(feature = "tfhe-backend"))]
fn decrypt_result(
    result_data: &Ciphertext,
    kc: &FheKeychain,
    use_tfhe: bool,
) -> anyhow::Result<Vec<u32>> {
    if use_tfhe {
        anyhow::bail!("TFHE backend not available");
    }
    toy_decrypt(result_data, kc)
}

fn toy_decrypt(result_data: &Ciphertext, kc: &FheKeychain) -> anyhow::Result<Vec<u32>> {
    let result_batch: Vec<Ciphertext> = bincode::deserialize(&result_data.0)
        .map_err(|e| anyhow::anyhow!("deserialize result batch: {e}"))?;
    let engine = ToyFhe;
    let mut results = Vec::with_capacity(result_batch.len());
    for ct in &result_batch {
        let pt = engine.decrypt(ct, &kc.secret_key)?;
        let v = u64::from_le_bytes(pt.0[..8].try_into().unwrap());
        results.push(v as u32);
    }
    Ok(results)
}

// Bob-specific helpers — sync, CPU-bound, run on blocking pool.

/// Generate ZK proof and return serialized proven ciphertext bytes.
#[cfg(feature = "tfhe-backend")]
fn bob_prove_raw(values: &[u32], cpk_bytes: &[u8], crs_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use bdcp_core::backend::zkp::TfheZkpEngine;
    use bdcp_core::crypto::zkp::{ZkpEngine as _, ZkProofLoad};
    let zkp = TfheZkpEngine::new();
    let cpk = CompactPublicKey(cpk_bytes.to_vec());
    let crs = Crs(crs_bytes.to_vec());
    let (ct, _proof) = zkp.prove_encrypt_compact(values, &cpk, &crs, b"bdcp-zkp", ZkProofLoad::Verify)?;
    Ok(ct.0)
}

#[cfg(not(feature = "tfhe-backend"))]
fn bob_prove_raw(_values: &[u32], _cpk_bytes: &[u8], _crs_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    anyhow::bail!("TFHE backend not available");
}

/// Encrypt values with compact public key, return bincode-serialised batch.
#[cfg(feature = "tfhe-backend")]
fn bob_tfhe_raw(values: &[u32], cpk_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use bdcp_core::backend::tfhe::TfheEngine;
    let engine = TfheEngine::new();
    let tfhe_cpk: tfhe::CompactPublicKey = bincode::deserialize(cpk_bytes)
        .map_err(|e| anyhow::anyhow!("deserialize CompactPublicKey: {e}"))?;
    let cts = engine
        .encrypt_u32_with_compact_vec(values, &tfhe_cpk)
        .map_err(|e| anyhow::anyhow!("tfhe compact encrypt: {e}"))?;
    bincode::serialize(&cts).map_err(|e| anyhow::anyhow!("serialize batch: {e}"))
}

#[cfg(not(feature = "tfhe-backend"))]
fn bob_tfhe_raw(_values: &[u32], _cpk_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    anyhow::bail!("TFHE backend not available");
}
