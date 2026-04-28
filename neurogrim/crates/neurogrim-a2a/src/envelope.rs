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

/// Canonical A2A message types defined in spec §10.4 + §16.6.
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
    /// Supply-chain finding shared across peer Brains under
    /// bidirectional opt-in consent (spec §16.6, v2.6).
    /// Payload conforms to `a2a-supply-chain-signal-v1.schema.json`.
    ///
    /// **Bidirectional opt-in (spec §16.6 normative):** before
    /// signals flow A↔B, BOTH peers MUST declare
    /// `supply-chain-signal` — sender in `capabilities.emits[]`
    /// and receiver in `capabilities.accepts[]`. The check is
    /// implemented in
    /// [`crate::supply_chain_signal::bidirectional_opt_in_satisfied`].
    /// This is a tighter posture than the one-direction-by-Agent-
    /// Card-declaration default for other A2A message types; the
    /// rationale is legal exposure + false-positive multiplication
    /// (see §16.6 prose).
    ///
    /// **Default receive handler:** see
    /// [`crate::supply_chain_signal::default_handle_received`]
    /// — operators wanting custom handling (e.g., immediate
    /// ticket auto-creation) override via
    /// `TaskServer::register_handler`.
    #[serde(rename = "supply-chain-signal")]
    SupplyChainSignal,
    /// Cross-Brain federated-pattern message under bidirectional
    /// opt-in consent (spec §16.6.1, v2.12). Payload conforms to
    /// `a2a-federated-pattern-v1.schema.json`.
    ///
    /// **Bidirectional opt-in (spec §16.6.1 normative):** mirrors
    /// the §16.6 supply-chain-signal precedent. Both peers MUST
    /// declare `federated-pattern` in their Agent Card capabilities
    /// (sender in `emits[]`, receiver in `accepts[]`) before
    /// federation flows. The check is implemented in
    /// [`crate::federated_pattern::bidirectional_opt_in_satisfied`].
    ///
    /// **Receiver semantics:** see
    /// [`crate::federated_pattern::handle_received_federated_pattern`]
    /// — validates the payload, runs the wire-level recursion guard
    /// (origin_set self-check), enforces the receiver-side
    /// rate-limit, and returns a `ReceiveOutcome` that the
    /// transport handler persists to the
    /// `pattern-aggregation-ledger.jsonl`.
    #[serde(rename = "federated-pattern")]
    FederatedPattern,
}

impl MessageType {
    /// Wire-format name for this message type, matching the `serde(rename = ...)`
    /// attribute on each variant.
    ///
    /// 2026-04-26 PRE-RELEASE Round 2 R2-1 fix (D2-D2): consolidated from
    /// previously-duplicated 12-arm matches in
    /// `neurogrim-cli/src/commands/a2a_discover.rs` and
    /// `neurogrim-cli/src/commands/a2a_invoke.rs`. Drift-prevention: a new
    /// `MessageType` variant now requires updating exactly ONE place
    /// (this match) instead of three (this + the two CLI helpers).
    pub fn wire_name(&self) -> &'static str {
        match self {
            MessageType::ScoreUpdated => "score.updated",
            MessageType::GateChanged => "gate.changed",
            MessageType::EcosystemScored => "ecosystem.scored",
            MessageType::IncidentDetected => "incident.detected",
            MessageType::IncidentResolved => "incident.resolved",
            MessageType::SnapshotRequested => "snapshot.requested",
            MessageType::SnapshotDelivered => "snapshot.delivered",
            MessageType::ProposalCreated => "proposal.created",
            MessageType::ProposalResolved => "proposal.resolved",
            MessageType::ConfigChanged => "config.changed",
            MessageType::SupplyChainSignal => "supply-chain-signal",
            MessageType::FederatedPattern => "federated-pattern",
        }
    }
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
    fn all_message_types_roundtrip() {
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
            MessageType::SupplyChainSignal,
            MessageType::FederatedPattern,
        ];
        for mt in types {
            let s = serde_json::to_string(&mt).unwrap();
            let back: MessageType = serde_json::from_str(&s).unwrap();
            assert_eq!(mt, back);
        }
    }

    #[test]
    fn supply_chain_signal_wire_format() {
        // Per LSP-Brains v2.6 §16.6 + spec a2a-envelope-v1 enum
        // extension (E-SC-7).
        let s = serde_json::to_string(&MessageType::SupplyChainSignal).unwrap();
        assert_eq!(s, r#""supply-chain-signal""#);
        let back: MessageType = serde_json::from_str(&s).unwrap();
        assert_eq!(back, MessageType::SupplyChainSignal);
    }
}
