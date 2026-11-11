//! Unix Domain Socket IPC for Alice and Bob daemons.
//!
//! Listens on a local socket for commands like:
//!   `submit 10,20,30`  — submit values for encrypted computation
//!   `shutdown`         — stop the daemon

use crate::client;
use crate::config::DaemonConfig;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

/// Listen on a Unix domain socket, handling each connection in a spawned task.
///
/// For Alice daemon: returns decrypted results via the socket.
/// For Bob daemon: confirms submission.
pub async fn serve_ipc(
    config: &DaemonConfig,
    socket_path: &str,
) -> anyhow::Result<()> {
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;

    let relay_addr: SocketAddr = config
        .network
        .relay_addr
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("[network.relay_addr] is required for Alice/Bob daemon"))?
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid relay_addr: {e}"))?;
    let use_tfhe = config.use_tfhe();
    let use_zkp = config.use_zkp();
    let role = config.node.role.clone();
    let name = config.node.name.clone();

    tracing::info!(
        "IPC socket listening at {socket_path} — '{name}' ({role}), relay at {relay_addr}"
    );

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let role = role.clone();
                let name = name.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        handle_connection(stream, &role, relay_addr, use_tfhe, use_zkp).await
                    {
                        tracing::warn!("IPC connection error ({name}): {e}");
                    }
                });
            }
            Err(e) => {
                tracing::error!("IPC accept error: {e}");
                break;
            }
        }
    }

    Ok(())
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    role: &str,
    relay_addr: SocketAddr,
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        match parts[0] {
            "submit" => {
                let val_str = parts.get(1).ok_or_else(|| anyhow::anyhow!("missing values"))?;
                let values: Vec<u32> = val_str
                    .split(',')
                    .map(|s| {
                        s.trim()
                            .parse::<u32>()
                            .map_err(|e| anyhow::anyhow!("invalid value '{s}': {e}"))
                    })
                    .collect::<Result<_, _>>()?;

                handle_submit(&mut writer, role, relay_addr, &values, use_tfhe, use_zkp)
                    .await?;
            }
            "shutdown" => {
                writer.write_all(b"shutting down\n").await?;
                std::process::exit(0);
            }
            other => {
                let msg = format!("ERR unknown command '{other}'\n");
                writer.write_all(msg.as_bytes()).await?;
            }
        }
    }

    Ok(())
}

async fn handle_submit(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    role: &str,
    relay_addr: SocketAddr,
    values: &[u32],
    use_tfhe: bool,
    use_zkp: bool,
) -> anyhow::Result<()> {
    match role {
        "alice" => {
            writer
                .write_all(format!("processing {} values via relay...\n", values.len()).as_bytes())
                .await?;
            let results = client::alice_submit(relay_addr, values, use_tfhe, use_zkp).await?;
            let resp = format!(
                "OK results: [{}]\n",
                results
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            writer.write_all(resp.as_bytes()).await?;
        }
        "bob" => {
            writer
                .write_all(format!("submitting {} values...\n", values.len()).as_bytes())
                .await?;
            client::bob_submit(relay_addr, values, use_tfhe, use_zkp).await?;
            writer.write_all(b"OK submitted\n").await?;
        }
        other => {
            let msg = format!("ERR role '{other}' does not support submit\n");
            writer.write_all(msg.as_bytes()).await?;
        }
    }

    Ok(())
}
