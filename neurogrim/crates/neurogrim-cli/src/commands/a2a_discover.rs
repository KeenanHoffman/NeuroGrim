//! `neurogrim a2a-discover` — fetch and pretty-print a peer Brain's Agent
//! Card.
//!
//! The minimal end-to-end view of another Brain: no invocation, just
//! discovery. Useful for verifying a peer's Agent Card is reachable and
//! shaped correctly before you try to talk to it.

use anyhow::{Context, Result};
use neurogrim_a2a::envelope::MessageType;
use neurogrim_a2a::TaskClient;
use url::Url;

/// Entry point for the `a2a-discover` subcommand.
pub async fn run(peer_url: String) -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let url = Url::parse(&peer_url).with_context(|| format!("invalid peer URL {peer_url:?}"))?;

    let client = TaskClient::new_http();
    let card = client
        .discover(&url)
        .await
        .context("failed to discover peer Agent Card")?;

    println!("Peer Agent Card @ {url}");
    println!("  id:                {}", card.id);
    println!("  name:              {}", card.name);
    println!("  version:           {}", card.version);
    println!("  interface_version: {}", card.interface_version);
    println!("  schema_version:    {}", card.schema_version);
    println!("  transport:");
    println!(
        "    protocol:        {}",
        match card.transport.protocol {
            neurogrim_a2a::agent_card::TransportProtocol::HttpSse => "http+sse",
            neurogrim_a2a::agent_card::TransportProtocol::JsonRpc => "json-rpc",
        }
    );
    println!("    endpoint:        {}", card.transport.endpoint);
    println!("    tasks_path:      {}", card.transport.tasks_path);
    println!(
        "  auth scheme:       {}",
        match card.authentication.scheme {
            neurogrim_a2a::agent_card::AuthScheme::None => "none",
            neurogrim_a2a::agent_card::AuthScheme::Bearer => "bearer",
        }
    );
    println!("  accepts:");
    for mt in &card.capabilities.accepts {
        println!("    - {}", wire_name(mt));
    }
    if !card.capabilities.emits.is_empty() {
        println!("  emits:");
        for mt in &card.capabilities.emits {
            println!("    - {}", wire_name(mt));
        }
    } else {
        println!("  emits:             (none)");
    }
    if card.capabilities.streaming {
        println!("  streaming:         yes");
    }
    if let Some(topo) = &card.topology {
        println!("  topology:");
        if let Some(role) = topo.role {
            println!("    role:            {role:?}");
        }
        if let Some(parent) = &topo.parent_id {
            println!("    parent_id:       {parent}");
        }
    }
    Ok(())
}

/// Keep the wire-format rendering local — duplicating the three-line helper
/// from `a2a_invoke` is cheaper than a public module dependency for one fn.
fn wire_name(mt: &MessageType) -> &'static str {
    match mt {
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
    }
}
