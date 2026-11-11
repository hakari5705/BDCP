//! Bob subcommand — joins a job, fetches CompactPublicKey + CRS, submits encrypted data.

use bdcp_core::backend::toy::ToyFhe;
use bdcp_core::crypto::fhe::FheEngine;
use bdcp_core::protocol::messages::{GetPublicKeyRequest, Message, MessageKind};
use bdcp_core::types::{CompactPublicKey, Crs, NodeId, Plaintext};
use bdcp_net::{BdcpNode, NodeConfig, NodeRole};
use std::net::SocketAddr;
use tracing::info;

pub async fn run_bob(
    relay_addr: SocketAddr,
    job_id: &str,
    values: &[u32],
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<()> {
    let bob = BdcpNode::new(
        NodeRole::Client,
        NodeConfig::new("bob", "127.0.0.1:0"),
    )
    .await?;

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let conn = bob.connect(&relay_addr.to_string()).await?;
    info!("Bob connected to relay");

    // Step 1: Request the CompactPublicKey for this job
    bob.send_message(
        &conn,
        &Message {
            kind: MessageKind::GetPublicKey(GetPublicKeyRequest {
                job_id: bdcp_core::types::JobId(job_id.into()),
            }),
            sender: NodeId("bob".into()),
            job_id: bdcp_core::types::JobId(job_id.into()),
            timestamp: 0,
        },
    )
    .await?;
    info!("Requested public key");

    // Step 2: Receive the CompactPublicKey response
    let mut resp_ctrl = bob.accept_control(&conn).await?;
    let resp_frame = resp_ctrl.recv().await?;
    let cpk = match &resp_frame.message.kind {
        MessageKind::PublicKeyResponse(resp) => resp.compact_public_key.clone(),
        other => anyhow::bail!("expected PublicKeyResponse, got {other:?}"),
    };
    info!("Received CompactPublicKey ({} bytes)", cpk.0.len());

    // If ZKP enabled, also receive the CRS as a data stream
    let crs = if use_zkp {
        let crs_bytes = bob.receive_data(&conn).await?;
        info!("Received CRS ({} bytes)", crs_bytes.len());
        Crs(crs_bytes)
    } else {
        Crs(Vec::new())
    };

    // Step 3: Encrypt values and submit (with optional ZK proof)
    if use_tfhe && use_zkp {
        bob_prove_and_send(values, &cpk, &crs, &conn, &bob).await?;
    } else if use_tfhe {
        let cts = bob_tfhe(values, &cpk)?;
        let batch_bytes = bincode::serialize(&cts)
            .map_err(|e| anyhow::anyhow!("serialize ciphertext batch: {e}"))?;
        bob.stream_data(&conn, &batch_bytes).await?;
        info!("Sent {} ciphertexts", cts.len());
    } else {
        let engine = ToyFhe;
        let ct = engine.encrypt_compact(&Plaintext((values[0] as u64).to_le_bytes().to_vec()), &cpk)?;
        bob.stream_data(&conn, &ct.0).await?;
        info!("Sent ciphertext");
    }

    // Keep connection alive long enough for relay to read
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    info!("Bob disconnecting");
    Ok(())
}

async fn bob_prove_and_send(
    values: &[u32],
    cpk: &CompactPublicKey,
    crs: &Crs,
    conn: &quinn::Connection,
    bob: &BdcpNode,
) -> anyhow::Result<()> {
    use bdcp_core::backend::zkp::TfheZkpEngine;
    use bdcp_core::crypto::zkp::{ZkpEngine as _, ZkProofLoad};

    let zkp = TfheZkpEngine::new();
    let metadata = b"bdcp-zkp";

    info!("Generating ZK proofs for {} values...", values.len());
    let (ct, _proof) = zkp.prove_encrypt_compact(
        values,
        cpk,
        crs,
        metadata,
        ZkProofLoad::Verify,
    )?;
    info!("Generated proven ciphertext ({} bytes)", ct.0.len());

    // Send the ProvenCompactCiphertextList — it contains both data and proof
    bob.stream_data(conn, &ct.0).await?;
    info!("Sent proven ciphertext with ZK proof");
    Ok(())
}

fn bob_tfhe(values: &[u32], cpk: &CompactPublicKey) -> anyhow::Result<Vec<bdcp_core::types::Ciphertext>> {
    use bdcp_core::backend::tfhe::TfheEngine;
    let engine = TfheEngine::new();
    let tfhe_cpk: tfhe::CompactPublicKey = bincode::deserialize(&cpk.0)
        .map_err(|e| anyhow::anyhow!("deserialize CompactPublicKey: {e}"))?;
    engine
        .encrypt_u32_with_compact_vec(values, &tfhe_cpk)
        .map_err(|e| anyhow::anyhow!("tfhe compact encrypt batch: {e}"))
}
