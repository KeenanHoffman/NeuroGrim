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

/// v2.1 supports only `none`. Bearer and mTLS are deferred to a future spec version.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthScheme {
    #[default]
    None,
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
        };
        let s = serde_json::to_string(&card).unwrap();
        let back: AgentCard = serde_json::from_str(&s).unwrap();
        assert_eq!(card, back);
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
