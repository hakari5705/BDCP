//! High-level QUIC stream wrappers.

use crate::message::ControlFrame;
use anyhow::Result;
use quinn::{RecvStream, SendStream};

/// Maximum frame size for control messages (64 MB).
const MAX_CONTROL_FRAME: u64 = 67_108_864;

/// Maximum frame size for data messages (256 MB).
const MAX_DATA_FRAME: u64 = 268_435_456;

/// A bi-directional control stream.
///
/// Used for handshake messages and small protocol traffic.
/// Messages are length-delimited: u64 (big-endian) length prefix + bincode payload.
pub struct ControlStream {
    send: SendStream,
    recv: RecvStream,
}

impl ControlStream {
    pub fn new(send: SendStream, recv: RecvStream) -> Self {
        Self { send, recv }
    }

    pub async fn send(&mut self, frame: &ControlFrame) -> Result<()> {
        let bytes = bincode::serialize(frame)?;
        let len = bytes.len() as u64;
        self.send.write_all(&len.to_be_bytes()).await?;
        self.send.write_all(&bytes).await?;
        self.send.finish()?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<ControlFrame> {
        let len_bytes = {
            let mut buf = [0u8; 8];
            self.recv.read_exact(&mut buf).await?;
            u64::from_be_bytes(buf)
        };

        if len_bytes > MAX_CONTROL_FRAME {
            anyhow::bail!("control frame too large: {} bytes (max {})", len_bytes, MAX_CONTROL_FRAME);
        }

        let mut payload = vec![0u8; len_bytes as usize];
        self.recv.read_exact(&mut payload).await?;

        Ok(bincode::deserialize(&payload)?)
    }
}

/// A uni-directional data stream for large payloads.
///
/// Used for CompressedServerKey (~59 MB) and ciphertext batches.
/// Format: u64 (big-endian) payload length + payload bytes.
pub struct DataStream {
    send: Option<SendStream>,
    recv: Option<RecvStream>,
}

impl DataStream {
    pub fn new_sender(send: SendStream) -> Self {
        Self {
            send: Some(send),
            recv: None,
        }
    }

    pub fn new_receiver(recv: RecvStream) -> Self {
        Self {
            send: None,
            recv: Some(recv),
        }
    }

    pub async fn send(&mut self, payload: &[u8]) -> Result<()> {
        let send = self.send.as_mut().unwrap();
        let len = payload.len() as u64;
        send.write_all(&len.to_be_bytes()).await?;
        send.write_all(payload).await?;
        send.finish()?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>> {
        let recv = self.recv.as_mut().unwrap();

        let len_bytes = {
            let mut buf = [0u8; 8];
            recv.read_exact(&mut buf).await?;
            u64::from_be_bytes(buf)
        };

        if len_bytes > MAX_DATA_FRAME {
            anyhow::bail!("data frame too large: {} bytes (max {})", len_bytes, MAX_DATA_FRAME);
        }

        let mut payload = vec![0u8; len_bytes as usize];
        recv.read_exact(&mut payload).await?;

        Ok(payload)
    }
}
