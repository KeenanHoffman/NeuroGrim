//! A2A error types. Matches the error conditions in spec Appendix G.8.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum A2aError {
    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),

    #[error("unknown message type: {0}")]
    UnknownMessageType(String),

    #[error("message type {0:?} not in peer's accepts list")]
    MessageTypeNotAccepted(String),

    #[error("agent card unreachable at {0}")]
    AgentCardUnreachable(String),

    #[error("agent card failed validation: {0}")]
    AgentCardInvalid(String),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("peer returned {status}: {body}")]
    PeerError { status: u16, body: String },
}
