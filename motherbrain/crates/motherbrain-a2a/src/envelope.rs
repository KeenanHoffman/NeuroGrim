//! A2A message envelope types.
//!
//! Matches `schemas/a2a-envelope-v1.schema.json` in the spec repo. Every peer-Brain
//! message on the wire is wrapped in this envelope regardless of transport.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Canonical A2A envelope. Validates against `a2a-envelope-v1.schema.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct A2aEnvelope {
    /// Envelope schema version. Always "1" for this spec version.
    pub schema_version: String,

    /// Unique message identifier. Used as the idempotency key — duplicate receipt MUST
    /// be a no-op that returns the cached response.
    pub message_id: String,

    /// Correlation ID for multi-message tasks. Optional for one-shot messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,

    /// ISO 8601 UTC timestamp of message origination.
    pub timestamp: DateTime<Utc>,

    /// Originating Brain id. Matches the `id` in its Agent Card.
    pub brain_id: String,

    /// One of the 10 canonical message types.
    pub message_type: MessageType,

    /// For request/response patterns: the `message_id` being answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,

    /// Message-type-specific payload. See spec Appendix G.5 for shapes.
    pub payload: Value,

    /// Transport/routing metadata (trace IDs, delivery hints, etc.).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

/// The 10 canonical A2A message types defined in spec §10.4.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    #[serde(rename = "score.updated")]
    ScoreUpdated,
    #[serde(rename = "gate.changed")]
    GateChanged,
    #[serde(rename = "ecosystem.scored")]
    EcosystemScored,
    #[serde(rename = "incident.detected")]
    IncidentDetected,
    #[serde(rename = "incident.resolved")]
    IncidentResolved,
    #[serde(rename = "snapshot.requested")]
    SnapshotRequested,
    #[serde(rename = "snapshot.delivered")]
    SnapshotDelivered,
    #[serde(rename = "proposal.created")]
    ProposalCreated,
    #[serde(rename = "proposal.resolved")]
    ProposalResolved,
    #[serde(rename = "config.changed")]
    ConfigChanged,
}

impl A2aEnvelope {
    /// Build a new envelope with a fresh UUID v4 as `message_id`. Sets the schema
    /// version and current UTC timestamp automatically.
    pub fn new(brain_id: impl Into<String>, message_type: MessageType, payload: Value) -> Self {
        Self {
            schema_version: "1".into(),
            message_id: uuid::Uuid::new_v4().to_string(),
            task_id: None,
            timestamp: Utc::now(),
            brain_id: brain_id.into(),
            message_type,
            reply_to: None,
            payload,
            metadata: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_roundtrip_preserves_fields() {
        let env = A2aEnvelope::new(
            "test-brain",
            MessageType::ScoreUpdated,
            json!({"score": 72}),
        );
        let serialized = serde_json::to_string(&env).unwrap();
        let parsed: A2aEnvelope = serde_json::from_str(&serialized).unwrap();
        assert_eq!(env, parsed);
    }

    #[test]
    fn message_type_wire_format_uses_dots() {
        let serialized = serde_json::to_string(&MessageType::ScoreUpdated).unwrap();
        assert_eq!(serialized, r#""score.updated""#);
    }

    #[test]
    fn all_ten_message_types_roundtrip() {
        let types = [
            MessageType::ScoreUpdated,
            MessageType::GateChanged,
            MessageType::EcosystemScored,
            MessageType::IncidentDetected,
            MessageType::IncidentResolved,
            MessageType::SnapshotRequested,
            MessageType::SnapshotDelivered,
            MessageType::ProposalCreated,
            MessageType::ProposalResolved,
            MessageType::ConfigChanged,
        ];
        for mt in types {
            let s = serde_json::to_string(&mt).unwrap();
            let back: MessageType = serde_json::from_str(&s).unwrap();
            assert_eq!(mt, back);
        }
    }
}
