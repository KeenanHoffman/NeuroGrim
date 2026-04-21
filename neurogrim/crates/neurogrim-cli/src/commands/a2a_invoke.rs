//! `neurogrim a2a-invoke` — send a single A2A envelope to a peer Brain.
//!
//! End-to-end exercise of the client path: discover the peer's Agent Card,
//! construct an envelope, send it, print the terminal reply. Useful for
//! poking a peer during development and for CI smoke tests against the
//! sibling `a2a-serve` command.

use anyhow::{anyhow, Context, Result};
use neurogrim_a2a::envelope::{A2aEnvelope, MessageType};
use neurogrim_a2a::TaskClient;
use url::Url;

/// Parse a CLI-friendly message-type string into the canonical enum. We
/// accept dotted wire form (`snapshot.requested`) and also kebab-case
/// (`snapshot-requested`) for humans who default to Unix conventions.
///
/// Returns an `anyhow::Error` with the list of supported names so the user
/// sees the fix without a round-trip to docs.
pub fn parse_message_type(s: &str) -> Result<MessageType> {
    let norm = s.trim().to_ascii_lowercase().replace('-', ".");
    match norm.as_str() {
        "score.updated" => Ok(MessageType::ScoreUpdated),
        "gate.changed" => Ok(MessageType::GateChanged),
        "ecosystem.scored" => Ok(MessageType::EcosystemScored),
        "incident.detected" => Ok(MessageType::IncidentDetected),
        "incident.resolved" => Ok(MessageType::IncidentResolved),
        "snapshot.requested" => Ok(MessageType::SnapshotRequested),
        "snapshot.delivered" => Ok(MessageType::SnapshotDelivered),
        "proposal.created" => Ok(MessageType::ProposalCreated),
        "proposal.resolved" => Ok(MessageType::ProposalResolved),
        "config.changed" => Ok(MessageType::ConfigChanged),
        other => Err(anyhow!(
            "unknown message_type {other:?}; valid values: \
             score.updated, gate.changed, ecosystem.scored, incident.detected, \
             incident.resolved, snapshot.requested, snapshot.delivered, \
             proposal.created, proposal.resolved, config.changed"
        )),
    }
}

/// Entry point for the `a2a-invoke` subcommand.
///
/// When `bearer` is `Some`, it is injected as `Authorization: Bearer <token>`
/// on every request (including task poll). Peers that advertise
/// `authentication.scheme: bearer` on their Agent Card require this; peers
/// that advertise `none` ignore the header.
pub async fn run(
    peer_url: String,
    message_type: String,
    payload: Option<String>,
    bearer: Option<String>,
) -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let url = Url::parse(&peer_url).with_context(|| format!("invalid peer URL {peer_url:?}"))?;

    eprintln!("✦ Communing with {peer_url}…");

    let mt = parse_message_type(&message_type)?;
    let payload_value: serde_json::Value = match payload {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s)
            .with_context(|| format!("--payload is not valid JSON: {s:?}"))?,
        _ => serde_json::json!({}),
    };

    let client = match bearer.as_deref() {
        Some(tok) if !tok.is_empty() => TaskClient::new_http_with_bearer(tok.to_string()),
        _ => TaskClient::new_http(),
    };

    // Step 1: discover — print a summary before blowing bytes at the peer.
    let card = client
        .discover(&url)
        .await
        .context("failed to discover peer Agent Card")?;
    eprintln!("Peer Agent Card:");
    eprintln!("  id:                {}", card.id);
    eprintln!("  name:              {}", card.name);
    eprintln!("  version:           {}", card.version);
    eprintln!("  interface_version: {}", card.interface_version);
    eprintln!(
        "  accepts:           {}",
        card.capabilities
            .accepts
            .iter()
            .map(message_type_wire_name)
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !card.capabilities.emits.is_empty() {
        eprintln!(
            "  emits:             {}",
            card.capabilities
                .emits
                .iter()
                .map(message_type_wire_name)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    eprintln!();

    // Refuse up front if the peer doesn't accept what we're sending —
    // spec §9.7 pre-flight.
    if !card.capabilities.accepts.contains(&mt) {
        anyhow::bail!(
            "peer {:?} does not accept {} (accepts: {:?})",
            card.id,
            message_type_wire_name(&mt),
            card.capabilities
                .accepts
                .iter()
                .map(message_type_wire_name)
                .collect::<Vec<_>>()
        );
    }

    // Step 2: build envelope and invoke.
    let envelope = A2aEnvelope::new("neurogrim-cli", mt, payload_value);
    let response = client
        .invoke(&url, envelope)
        .await
        .context("peer invocation failed")?;

    // Step 3: print the reply. We go pretty JSON so humans and pipes both
    // get something reasonable.
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

/// Convert a MessageType back to its wire-format string. We have it via
/// serde but going through to_string would pull in an extra allocation —
/// a static match is simpler and honest.
fn message_type_wire_name(mt: &MessageType) -> String {
    match mt {
        MessageType::ScoreUpdated => "score.updated".into(),
        MessageType::GateChanged => "gate.changed".into(),
        MessageType::EcosystemScored => "ecosystem.scored".into(),
        MessageType::IncidentDetected => "incident.detected".into(),
        MessageType::IncidentResolved => "incident.resolved".into(),
        MessageType::SnapshotRequested => "snapshot.requested".into(),
        MessageType::SnapshotDelivered => "snapshot.delivered".into(),
        MessageType::ProposalCreated => "proposal.created".into(),
        MessageType::ProposalResolved => "proposal.resolved".into(),
        MessageType::ConfigChanged => "config.changed".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_wire_format() {
        assert_eq!(
            parse_message_type("snapshot.requested").unwrap(),
            MessageType::SnapshotRequested
        );
        assert_eq!(
            parse_message_type("score.updated").unwrap(),
            MessageType::ScoreUpdated
        );
    }

    #[test]
    fn parse_accepts_kebab_case() {
        // Humans writing CLI flags lean kebab; we don't punish that.
        assert_eq!(
            parse_message_type("snapshot-requested").unwrap(),
            MessageType::SnapshotRequested
        );
    }

    #[test]
    fn parse_is_case_insensitive() {
        assert_eq!(
            parse_message_type("Snapshot.Requested").unwrap(),
            MessageType::SnapshotRequested
        );
    }

    #[test]
    fn parse_rejects_unknown_with_helpful_list() {
        // Critical-but-kind: the error must tell the user what IS valid,
        // not just that they were wrong.
        let err = parse_message_type("make.coffee").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("snapshot.requested"),
            "error must list valid values; got: {msg}"
        );
    }

    #[test]
    fn wire_name_roundtrips_through_parse() {
        // If we emit "snapshot.requested" in discovery output, parse must
        // accept the same string. This is a regression guard against drift
        // between the print helper and the parse helper.
        let all = [
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
        for mt in all {
            let wire = message_type_wire_name(&mt);
            let back = parse_message_type(&wire).unwrap();
            assert_eq!(back, mt, "wire name {wire} must roundtrip");
        }
    }
}
