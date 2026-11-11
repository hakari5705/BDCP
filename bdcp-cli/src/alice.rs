//! Alice subcommand — creates a job, submits encrypted data, reads result.

use bdcp_core::backend::toy::ToyFhe;
use bdcp_core::crypto::fhe::{FheEngine, FheKeychain};
use bdcp_core::protocol::messages::{JobRequest, Message, MessageKind};
use bdcp_core::types::{Crs, NodeId, Plaintext, SecurityLevel};
use bdcp_net::{BdcpNode, NodeConfig, NodeRole};
use std::net::SocketAddr;
use tracing::info;

pub async fn run_alice(
    relay_addr: SocketAddr,
    values: &[u32],
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<()> {
    let alice = BdcpNode::new(
        NodeRole::Client,
        NodeConfig::new("alice", "127.0.0.1:0"),
    )
    .await?;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let conn = alice.connect(&relay_addr.to_string()).await?;
    info!("Alice connected to relay");

    let (kc, cts, crs) = if use_tfhe {
        alice_tfhe(values, use_zkp)?
    } else {
        let engine = ToyFhe;
        let kc = engine.generate_keychain(SecurityLevel::Standard)?;
        let ct = engine.encrypt(&Plaintext((values[0] as u64).to_le_bytes().to_vec()), &kc.public_key)?;
        (kc, vec![ct], Crs(Vec::new()))
    };

    // Send job request
    alice
        .send_message(
            &conn,
            &Message {
                kind: MessageKind::JobRequest(JobRequest {
                    description: "encrypted addition".into(),
                    required_participants: 2,
                    fhe_scheme: if use_tfhe { "tfhe-rs-v1.6-integer".into() } else { "toy-additive".into() },
                    security_level: SecurityLevel::Standard,
                }),
                sender: NodeId("alice".into()),
                job_id: bdcp_core::types::JobId("cli-test".into()),
                timestamp: 0,
            },
        )
        .await?;
    info!("Sent JobRequest");

    // Send data streams: CRS (if ZKP), evaluation key, compact public key, ciphertexts
    if use_zkp {
        alice.stream_data(&conn, &crs.0).await?;
        info!("Sent CRS ({} bytes)", crs.0.len());
    }
    alice.stream_data(&conn, &kc.evaluation_key.0).await?;
    alice.stream_data(&conn, &kc.compact_public_key.0).await?;
    let batch_bytes = bincode::serialize(&cts)
        .map_err(|e| anyhow::anyhow!("serialize ciphertext batch: {e}"))?;
    alice.stream_data(&conn, &batch_bytes).await?;
    info!("Sent data streams (ek + cpk + {} ciphertexts)", cts.len());

    // Wait for result
    let mut result_ctrl = alice.accept_control(&conn).await?;
    let result_frame = result_ctrl.recv().await?;
    let result_data = match &result_frame.message.kind {
        MessageKind::ComputeResult(cr) => &cr.data,
        other => anyhow::bail!("expected ComputeResult, got {other:?}"),
    };

    if use_tfhe {
        use bdcp_core::backend::tfhe::TfheEngine;
        let engine = TfheEngine::new();
        let ck: tfhe::ClientKey = bincode::deserialize(&kc.secret_key.0)
            .map_err(|e| anyhow::anyhow!("deserialize ClientKey: {e}"))?;
        let result_batch: Vec<bdcp_core::types::Ciphertext> = bincode::deserialize(&result_data.0)
            .map_err(|e| anyhow::anyhow!("deserialize result batch: {e}"))?;
        println!("  ✓ Alice decrypted results:");
        for (i, ct) in result_batch.iter().enumerate() {
            let pt = engine.decrypt_u32(ct, &ck)?;
            println!("      [{i}] = {pt}");
        }
    } else {
        let engine = ToyFhe;
        let pt = engine.decrypt(result_data, &kc.secret_key)?;
        let v = u64::from_le_bytes(pt.0[..8].try_into().unwrap());
        println!("  ✓ Alice decrypted result: {v}");
    }

    info!("Alice disconnecting");
    Ok(())
}

fn alice_tfhe(
    values: &[u32],
    use_zkp: bool,
) -> anyhow::Result<(FheKeychain, Vec<bdcp_core::types::Ciphertext>, Crs)> {
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

    let kc = engine.generate_keychain(bdcp_core::types::SecurityLevel::Standard)?;
    let ck: tfhe::ClientKey = bincode::deserialize(&kc.secret_key.0)
        .map_err(|e| anyhow::anyhow!("deserialize ClientKey: {e}"))?;
    let cts = engine.encrypt_u32_vec(values, &ck)?;

    Ok((kc, cts, crs))
}
