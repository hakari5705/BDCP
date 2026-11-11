//! Wire message sent over QUIC control streams.

use bdcp_core::protocol::messages::Message;
use serde::{Deserialize, Serialize};

/// A frame sent on a control stream.
/// Wraps BDCP protocol messages with minimal framing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlFrame {
    /// The BDCP protocol message.
    pub message: Message,
}
