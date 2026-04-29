//! Agent Card — Brain self-description for A2A peer discovery.
//!
//! Matches `schemas/agent-card-v1.schema.json`. Published at `/.well-known/agent-card.json`
//! by a conformant Brain that serves as an A2A peer (spec §13.2, Appendix G.2).

use crate::envelope::MessageType;
use serde::{Deserialize, Serialize};

/// Agent Card. Validates against `agent-card-v1.schema.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCard {
    /// Schema version — always "1" for this spec version.
    pub schema_version: String,

    /// Unique identifier for this Brain instance. Stable across restarts.
    pub id: String,

    /// Human-readable name.
    pub name: String,

    /// Brain software version (informational; does NOT imply protocol compatibility).
    pub version: String,

    /// Interface Contract version this Brain produces (spec §6). Default "1".
    #[serde(default = "default_interface_version")]
    pub interface_version: String,

    pub capabilities: Capabilities,

    pub transport: Transport,

    #[serde(default)]
    pub authentication: Authentication,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub topology: Option<Topology>,

    /// v4.1 S13-B-9 — coordination-bus queue endpoints this Brain
    /// surfaces over A2A. Additive field; older peers (pre-v4.1)
    /// deserialize as `None` thanks to `#[serde(default)]`. Cross-
    /// Brain consumers (e.g., the ecosystem Brain subscribing to a
    /// child's `_neurogrim/notifications`) read this list to learn
    /// which topics they can subscribe to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_endpoints: Option<QueueEndpoints>,
}

fn default_interface_version() -> String {
    "1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capabilities {
    /// Message types this Brain will receive and act on.
    pub accepts: Vec<MessageType>,

    /// Message types this Brain produces.
    pub emits: Vec<MessageType>,

    /// Whether this Brain supports long-running tasks with SSE progress streams.
    #[serde(default)]
    pub streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transport {
    pub protocol: TransportProtocol,
    pub endpoint: String,
    #[serde(default = "default_tasks_path")]
    pub tasks_path: String,
}

fn default_tasks_path() -> String {
    "/tasks".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TransportProtocol {
    #[serde(rename = "http+sse")]
    HttpSse,
    #[serde(rename = "json-rpc")]
    JsonRpc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Authentication {
    #[serde(default = "default_auth_scheme")]
    pub scheme: AuthScheme,
}

fn default_auth_scheme() -> AuthScheme {
    AuthScheme::None
}

/// Authentication schemes declared on the Agent Card.
///
/// - `None`: no auth. Use only on trusted networks or behind a firewall.
///   The default for dev and single-host topologies.
/// - `Bearer`: the client MUST send `Authorization: Bearer <token>` on
///   every task request. Enables multi-tenant and remote-agent
///   deployments (METHODOLOGY-EVOLUTION §10, 2026-04-20).
///
/// mTLS remains a future addition.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthScheme {
    #[default]
    None,
    Bearer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Topology {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<TopologyRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TopologyRole {
    Project,
    Ecosystem,
    Local,
    External,
}

/// v4.1 S13-B-9 — coordination-bus endpoint metadata. Each entry
/// describes a single topic exposed for cross-Brain subscription.
/// Consumers connect to `<base_url>?topic=<topic>` for SSE pubsub.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueueEndpoints {
    /// Base URL for queue endpoints. Mirrors the dashboard's HTTP
    /// surface — peers append `<base>/<topic>` for read endpoints
    /// and `<base>/<topic>/events` for SSE subscription. Same
    /// transport scheme as the agent's primary `transport.endpoint`.
    pub base_url: String,
    /// List of topics the Brain advertises for cross-Brain
    /// subscription. Adopters typically expose system topics
    /// (`_neurogrim/notifications`) plus a handful of project
    /// topics (`pc-state/alerts`).
    pub advertised_topics: Vec<String>,
    /// Whether SSE pubsub is supported on these endpoints. v4.1
    /// always sets this to true; reserved for forward-compat with
    /// JSON-RPC-only future transports.
    #[serde(default = "default_true")]
    pub supports_sse: bool,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn minimal_agent_card_roundtrip() {
        let card = AgentCard {
            schema_version: "1".into(),
            id: "project-alpha".into(),
            name: "Project Alpha Brain".into(),
            version: "0.1.0".into(),
            interface_version: "1".into(),
            capabilities: Capabilities {
                accepts: vec![MessageType::SnapshotRequested],
                emits: vec![MessageType::ScoreUpdated, MessageType::GateChanged],
                streaming: false,
            },
            transport: Transport {
                protocol: TransportProtocol::HttpSse,
                endpoint: "https://alpha.internal/a2a/v1/".into(),
                tasks_path: "/tasks".into(),
            },
            authentication: Authentication::default(),
            topology: None,
            queue_endpoints: None,
        };
        let s = serde_json::to_string(&card).unwrap();
        let back: AgentCard = serde_json::from_str(&s).unwrap();
        assert_eq!(card, back);
    }

    /// v4.1 S13-B-9 — Agent Cards with the new optional
    /// `queue_endpoints` field round-trip through serde, and older
    /// peers without the field deserialize as `None`.
    #[test]
    fn queue_endpoints_field_is_additive() {
        let with_qe = AgentCard {
            schema_version: "1".into(),
            id: "project-alpha".into(),
            name: "Project Alpha Brain".into(),
            version: "0.1.0".into(),
            interface_version: "1".into(),
            capabilities: Capabilities {
                accepts: vec![],
                emits: vec![],
                streaming: false,
            },
            transport: Transport {
                protocol: TransportProtocol::HttpSse,
                endpoint: "https://alpha.internal/a2a/v1/".into(),
                tasks_path: "/tasks".into(),
            },
            authentication: Authentication::default(),
            topology: None,
            queue_endpoints: Some(QueueEndpoints {
                base_url: "https://alpha.internal/api/brains/alpha/queues".into(),
                advertised_topics: vec![
                    "_neurogrim/notifications".into(),
                    "pc-state/alerts".into(),
                ],
                supports_sse: true,
            }),
        };
        let s = serde_json::to_string(&with_qe).unwrap();
        let back: AgentCard = serde_json::from_str(&s).unwrap();
        assert_eq!(with_qe, back);
        assert!(s.contains("queue_endpoints"));
        assert!(s.contains("_neurogrim/notifications"));

        // Older peer / pre-v4.1 card → queue_endpoints absent →
        // deserializes as None.
        let pre_v41 = json!({
            "schema_version": "1",
            "id": "x",
            "name": "x",
            "version": "0",
            "capabilities": {"accepts": [], "emits": []},
            "transport": {"protocol": "http+sse", "endpoint": "http://x/"}
        });
        let card: AgentCard = serde_json::from_value(pre_v41).unwrap();
        assert_eq!(card.queue_endpoints, None);
    }

    #[test]
    fn auth_defaults_to_none() {
        let json = json!({
            "schema_version": "1",
            "id": "x",
            "name": "x",
            "version": "0",
            "capabilities": {"accepts": [], "emits": []},
            "transport": {"protocol": "http+sse", "endpoint": "http://x/"}
        });
        let card: AgentCard = serde_json::from_value(json).unwrap();
        assert_eq!(card.authentication.scheme, AuthScheme::None);
        assert_eq!(card.interface_version, "1");
    }
}
