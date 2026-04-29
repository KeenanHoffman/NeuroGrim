//! Supply-chain signal payload + sender + default receiver handler
//! per LSP-Brains v2.6 §16.6 + the
//! `a2a-supply-chain-signal-v1.schema.json` schema.
//!
//! # Bidirectional opt-in
//!
//! Both peers MUST declare `MessageType::SupplyChainSignal` in
//! their Agent Card before signals flow. This is a tighter posture
//! than the default one-direction-by-Agent-Card-declaration for
//! other A2A message types — supply-chain findings carry legal-
//! exposure (defamation) and FP-multiplication risk, so the spec
//! requires explicit consent in BOTH directions.
//!
//! # Sender flow
//!
//! 1. Local Brain produces a Layer 2 vigilance finding (or other
//!    relevant signal).
//! 2. Local Brain checks ITS OWN Agent Card declares `supply-chain-
//!    signal` in `emits[]`. If not, return early.
//! 3. For each configured peer:
//!    - Fetch peer's Agent Card.
//!    - Verify peer declares `supply-chain-signal` in `accepts[]`.
//!    - If yes, build envelope + send via `TaskClient`.
//!
//! # Receiver flow
//!
//! 1. `TaskServer` registers a default handler for
//!    `MessageType::SupplyChainSignal` (operators may override).
//! 2. Default handler validates the payload shape + appends to a
//!    `received-signals.jsonl` log + returns a `score.updated`-style
//!    no-op acknowledgement.
//!
//! # Cross-Brain aggregation
//!
//! v1 honesty: aggregation is implementation-defined per spec
//! §16.6. The default handler increments a per-(advisory_id, package)
//! counter in the local log; downstream consumers can read the log
//! and produce `cross_brain_count` per signal. Richer aggregation
//! (e.g., a persistent per-Brain reputation map) is v2 candidate.

use crate::envelope::{A2aEnvelope, MessageType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Payload shape for `supply-chain-signal` messages. Matches
/// `a2a-supply-chain-signal-v1.schema.json` (LSP-Brains v2.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SupplyChainSignalPayload {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory_id: Option<String>,
    pub package: PackageRef,
    pub severity_class: SeverityClass,
    pub discovery_source: DiscoverySource,
    pub peer_brain_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_brain_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovered_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisory_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_disclaimer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_action: Option<RecommendedAction>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageRef {
    pub name: String,
    pub ecosystem: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_range: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SeverityClass {
    Critical,
    High,
    Medium,
    Low,
    Informational,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoverySource {
    Osv,
    Rustsec,
    Pypa,
    Ghsa,
    Vigilance,
    AgentReview,
    Operator,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RecommendedAction {
    PinToLastGood,
    Remove,
    Downgrade,
    ReviewRequired,
    NoActionYet,
    Other,
}

impl SupplyChainSignalPayload {
    /// Build a payload from minimal required fields. Optional fields
    /// can be set fluently after construction.
    pub fn new(
        peer_brain_id: impl Into<String>,
        package_name: impl Into<String>,
        ecosystem: impl Into<String>,
        version: impl Into<String>,
        severity_class: SeverityClass,
        discovery_source: DiscoverySource,
    ) -> Self {
        Self {
            schema_version: "1".to_string(),
            advisory_id: None,
            package: PackageRef {
                name: package_name.into(),
                ecosystem: ecosystem.into(),
                version: version.into(),
                version_range: None,
            },
            severity_class,
            discovery_source,
            peer_brain_id: peer_brain_id.into(),
            cross_brain_count: Some(1),
            discovered_at: Some(chrono::Utc::now()),
            advisory_uri: None,
            summary: None,
            legal_disclaimer: None,
            recommended_action: None,
            metadata: serde_json::Map::new(),
        }
    }

    /// Validate required fields per the §16.7 schema. Returns an
    /// error string describing the first violation, if any.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != "1" {
            return Err(format!(
                "schema_version must be \"1\"; got {:?}",
                self.schema_version
            ));
        }
        if self.peer_brain_id.trim().is_empty() {
            return Err("peer_brain_id must be non-empty".into());
        }
        if self.package.name.trim().is_empty() {
            return Err("package.name must be non-empty".into());
        }
        if self.package.ecosystem.trim().is_empty() {
            return Err("package.ecosystem must be non-empty".into());
        }
        if self.package.version.trim().is_empty() {
            return Err("package.version must be non-empty".into());
        }
        Ok(())
    }

    /// Wrap the payload in an `A2aEnvelope` with `MessageType::SupplyChainSignal`.
    pub fn into_envelope(self, brain_id: impl Into<String>) -> Result<A2aEnvelope, String> {
        self.validate()?;
        let payload =
            serde_json::to_value(&self).map_err(|e| format!("serialize payload: {}", e))?;
        Ok(A2aEnvelope::new(
            brain_id,
            MessageType::SupplyChainSignal,
            payload,
        ))
    }
}

// =========================================================================
// Default receiver-side handler
// =========================================================================

/// A received signal log entry — what the default handler appends
/// to `received-signals.jsonl` when an inbound `supply-chain-signal`
/// arrives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedSignalEntry {
    pub received_at: chrono::DateTime<chrono::Utc>,
    pub from_brain_id: String,
    pub envelope_message_id: String,
    pub payload: SupplyChainSignalPayload,
}

/// Default path for the received-signals log.
pub fn default_received_signals_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("received-signals.jsonl")
}

/// Append a received signal entry to the log. Atomic single-line
/// JSONL append.
///
/// # Error semantics
///
/// Returns `Err` for any of:
/// - parent-directory creation failed (I/O)
/// - log file open-for-append failed (I/O)
/// - entry serialized to invalid JSON (`InvalidData`)
/// - serialized line contained an embedded newline
///   (`InvalidData`; defensive — `serde_json::to_string` escapes
///   `\n` by default so this only fires if the serializer config
///   ever changes to `to_string_pretty` or similar; 2026-04-26
///   A16 fix: error message names the specific failure class
///   rather than the generic "must serialize to a single line").
/// - write or flush failed (I/O)
pub fn append_received_signal(
    log_path: &Path,
    entry: &ReceivedSignalEntry,
) -> Result<(), std::io::Error> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let line = serde_json::to_string(entry).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("supply-chain-signal entry serialize failed: {}", e),
        )
    })?;
    if line.contains('\n') {
        // Defensive guard: with `serde_json::to_string` (compact)
        // this is unreachable. If a future change switches to
        // `to_string_pretty` or similar, the JSONL format breaks
        // silently — surface a clear actionable error instead.
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "supply-chain-signal serialized line contains an embedded newline \
                 (received from {}, message_id {}). JSONL log requires \
                 single-line entries; do not switch the serializer to \
                 to_string_pretty without updating the log format.",
                entry.from_brain_id, entry.envelope_message_id,
            ),
        ));
    }
    writeln!(f, "{}", line)?;
    f.flush()?;
    Ok(())
}

/// Default handler logic for an incoming `supply-chain-signal`
/// envelope: validate the payload + append to the received-signals
/// log + return a no-op acknowledgement envelope.
///
/// Operators who want richer handling (e.g., immediate ticket
/// auto-creation from received signals) override this by registering
/// their own handler with `TaskServer::register_handler`.
pub fn default_handle_received(
    inbound: &A2aEnvelope,
    log_path: &Path,
    receiver_brain_id: &str,
) -> Result<A2aEnvelope, String> {
    // Validate + parse payload.
    let payload: SupplyChainSignalPayload = serde_json::from_value(inbound.payload.clone())
        .map_err(|e| format!("parse supply-chain-signal payload: {}", e))?;
    payload.validate()?;

    // Append to received-signals log.
    let entry = ReceivedSignalEntry {
        received_at: chrono::Utc::now(),
        from_brain_id: inbound.brain_id.clone(),
        envelope_message_id: inbound.message_id.clone(),
        payload,
    };
    append_received_signal(log_path, &entry).map_err(|e| format!("append signal log: {}", e))?;

    // Return a minimal acknowledgement envelope. We use ScoreUpdated
    // shape because there's no first-class "ack" message type; the
    // payload is just a status indicator.
    let ack = A2aEnvelope::new(
        receiver_brain_id,
        MessageType::ScoreUpdated,
        serde_json::json!({
            "ack": "supply-chain-signal-received",
            "envelope_message_id": inbound.message_id,
        }),
    );
    Ok(ack)
}

// =========================================================================
// Bidirectional opt-in helpers
// =========================================================================

/// Returns true if the local Brain's Agent Card emits + the remote
/// peer's Agent Card accepts `supply-chain-signal`. Both must be
/// true for signals to flow per spec §16.6 bidirectional opt-in.
pub fn bidirectional_opt_in_satisfied(
    local: &crate::agent_card::AgentCard,
    peer: &crate::agent_card::AgentCard,
) -> bool {
    let local_emits = local
        .capabilities
        .emits
        .iter()
        .any(|m| matches!(m, MessageType::SupplyChainSignal));
    let peer_accepts = peer
        .capabilities
        .accepts
        .iter()
        .any(|m| matches!(m, MessageType::SupplyChainSignal));
    local_emits && peer_accepts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> SupplyChainSignalPayload {
        SupplyChainSignalPayload::new(
            "test-brain",
            "litellm",
            "PyPI",
            "1.82.7",
            SeverityClass::High,
            DiscoverySource::Vigilance,
        )
    }

    #[test]
    fn payload_serialize_deserialize_roundtrip() {
        let p = sample_payload();
        let json = serde_json::to_string(&p).unwrap();
        let back: SupplyChainSignalPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn payload_validate_requires_schema_version_1() {
        let mut p = sample_payload();
        p.schema_version = "2".to_string();
        assert!(p.validate().is_err());
    }

    #[test]
    fn payload_validate_requires_non_empty_package_fields() {
        let mut p = sample_payload();
        p.package.name = "".to_string();
        assert!(p.validate().is_err());

        let mut p = sample_payload();
        p.package.ecosystem = "".to_string();
        assert!(p.validate().is_err());

        let mut p = sample_payload();
        p.package.version = "".to_string();
        assert!(p.validate().is_err());
    }

    #[test]
    fn into_envelope_uses_supply_chain_signal_message_type() {
        let p = sample_payload();
        let env = p.into_envelope("local-brain").unwrap();
        assert!(matches!(env.message_type, MessageType::SupplyChainSignal));
    }

    #[test]
    fn append_received_signal_creates_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("received-signals.jsonl");
        let entry = ReceivedSignalEntry {
            received_at: chrono::Utc::now(),
            from_brain_id: "peer-brain".to_string(),
            envelope_message_id: "msg-001".to_string(),
            payload: sample_payload(),
        };
        append_received_signal(&log, &entry).unwrap();
        let content = std::fs::read_to_string(&log).unwrap();
        let line_count = content.lines().count();
        assert_eq!(line_count, 1);
        // Re-parse:
        let line = content.lines().next().unwrap();
        let parsed: ReceivedSignalEntry = serde_json::from_str(line).unwrap();
        assert_eq!(parsed.from_brain_id, "peer-brain");
    }

    #[test]
    fn default_handle_received_writes_to_log_and_returns_ack() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("received-signals.jsonl");
        let inbound = sample_payload().into_envelope("peer-brain").unwrap();
        let ack = default_handle_received(&inbound, &log, "local-brain").unwrap();
        assert!(matches!(ack.message_type, MessageType::ScoreUpdated));
        assert!(log.exists());
    }

    #[test]
    fn bidirectional_opt_in_requires_both_sides() {
        use crate::agent_card::*;
        let local_only_emits = AgentCard {
            schema_version: "1".to_string(),
            id: "local".to_string(),
            name: "Local".to_string(),
            version: "0.1.0".to_string(),
            interface_version: "1".to_string(),
            capabilities: Capabilities {
                accepts: vec![],
                emits: vec![MessageType::SupplyChainSignal],
                streaming: false,
            },
            transport: Transport {
                protocol: TransportProtocol::HttpSse,
                endpoint: "http://127.0.0.1:8421/a2a/v1/".to_string(),
                tasks_path: "/tasks".to_string(),
            },
            authentication: Authentication {
                scheme: AuthScheme::None,
            },
            topology: None,
            queue_endpoints: None,
        };
        let peer_only_accepts = AgentCard {
            capabilities: Capabilities {
                accepts: vec![MessageType::SupplyChainSignal],
                emits: vec![],
                streaming: false,
            },
            ..local_only_emits.clone()
        };
        let no_optin = AgentCard {
            capabilities: Capabilities {
                accepts: vec![],
                emits: vec![],
                streaming: false,
            },
            ..local_only_emits.clone()
        };

        assert!(bidirectional_opt_in_satisfied(
            &local_only_emits,
            &peer_only_accepts
        ));
        assert!(!bidirectional_opt_in_satisfied(
            &local_only_emits,
            &no_optin
        ));
        assert!(!bidirectional_opt_in_satisfied(
            &no_optin,
            &peer_only_accepts
        ));
    }
}
