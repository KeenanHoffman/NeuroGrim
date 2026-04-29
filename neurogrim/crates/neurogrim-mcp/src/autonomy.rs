//! Autonomy gate for MCP tool dispatch (S13-B-5, v4.1).
//!
//! Wires the existing `neurogrim_core::governance::resolve_autonomy()`
//! algorithm into the MCP server's tool dispatch path. The resolver
//! has been documented and tested since Stage 4 but never actually
//! called from dispatch — hard gates default-on in v4.1 closes that
//! gap.
//!
//! ## v1 scope
//!
//! - **Mutation tools wrapped**: `refresh_sensory`, `domain_new`,
//!   `record_subagent_outcome`, `queue_publish`. Read-only tools
//!   (`get_health_score`, `validate_registry`, `doctor`, etc.) are
//!   exempt in v1 — their action-type defaults to `read-only` which
//!   resolves to `Auto` regardless of the autonomy block; wrapping
//!   them adds boilerplate without behavior change. A future story
//!   can wrap them once a use-case for `Notify` on reads exists.
//!
//! - **Notify level**: collapsed into Allow in v1 (no separate
//!   post-execution publish to `_neurogrim/notifications`). The
//!   resolver still distinguishes the level; consumers can wire
//!   the publish later. Documented as a v1 carve-out.
//!
//! - **Approve flow**: publishes to `_neurogrim/approvals` with the
//!   action_id + action_type + tool_name. Returns
//!   `{"status":"pending_approval", "action_id": "..."}` to the
//!   agent, which polls via the new `await_approval` MCP tool until
//!   a resolution lands on `_neurogrim/approval-resolutions`.
//!
//! - **Blocked**: rejects with `{"error":"blocked","reason":"..."}`.
//!   Never executes. Surfaces the action_type that triggered the
//!   block so the operator can debug.
//!
//! ## Action-type taxonomy (v1)
//!
//! Two-bucket model: `read-only` (default Auto) vs `mutate-state`
//! (default Approve). Adopters can override per-tool via
//! `config.autonomy.action_types` in `brain-registry.json`. Future
//! schema versions can add finer-grained types
//! (`read-only-cheap`, `mutate-local`, `mutate-network`, `destroy`
//! per spec §5.4) — v1 keeps it minimal.

use crate::server::BrainServer;
use neurogrim_core::governance::{parse_autonomy_config, resolve_autonomy, ProposalConfidence};
use neurogrim_core::queue::{append, QueueMessage, Topic};
use neurogrim_core::types::AutonomyLevel;
use serde_json::json;
use std::path::Path;
use uuid::Uuid;

/// Reserved system topics consumed by this module.
pub const APPROVALS_TOPIC: &str = "_neurogrim/approvals";
pub const APPROVAL_RESOLUTIONS_TOPIC: &str = "_neurogrim/approval-resolutions";
pub const NOTIFICATIONS_TOPIC: &str = "_neurogrim/notifications";

/// Decision returned by [`check_autonomy`]. Tools branch on this
/// before executing their body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomyOutcome {
    /// Proceed with execution. Resolver returned `Auto` or `Notify`
    /// (Notify is collapsed into Allow in v1; see module docs).
    Allow,
    /// Operator approval required. The resolver wrote an entry to
    /// `_neurogrim/approvals` carrying this `action_id`; tools
    /// short-circuit by returning a `pending_approval` response.
    Approve { action_id: String, action_type: &'static str },
    /// Hard-blocked by safety invariants or policy. Tools
    /// short-circuit by returning a `blocked` error.
    Block { action_type: &'static str, reason: String },
}

/// v1 action-type catalog. Returned by [`tool_action_type`].
/// Adopters can override per-tool via `config.autonomy.action_types`
/// in their registry; defaults below apply when no override exists.
pub fn tool_action_type(tool_name: &str) -> &'static str {
    match tool_name {
        // Mutations — default Approve.
        "refresh_sensory" => "mutate-state",
        "domain_new" => "mutate-state",
        "record_subagent_outcome" => "mutate-state",
        "queue_publish" => "mutate-state",
        // v4.2 S14-S-5: secret_fetch defaults to Approve (mutate-state)
        // even though it doesn't mutate Brain state — the security
        // model requires explicit operator approval for every secret
        // access. Adopters can downgrade per-secret to `Notify` for
        // low-sensitivity public APIs via registry override.
        "secret_fetch" => "mutate-state",
        // Reads (default Auto). Listed for completeness even though
        // we don't currently wrap them — the override path stays
        // available for adopters who want stricter policy.
        "get_health_score" => "read-only",
        "get_trajectory" => "read-only",
        "get_recommendations" => "read-only",
        "validate_registry" => "read-only",
        "orient" => "read-only",
        "doctor" => "read-only",
        "explain" => "read-only",
        "get_local_awareness" => "read-only",
        "queue_consume" => "read-only",
        "queue_peek" => "read-only",
        "await_approval" => "read-only",
        // Unknown tool — bias toward safety (Approve via the
        // generic action_type). Surfaces as "needs operator attention"
        // rather than a silent allow.
        _ => "mutate-state",
    }
}

/// Resolve autonomy for `tool_name`, dispatching the side-effects
/// per the resolved level. Called as the first thing each wrapped
/// tool does.
pub async fn check_autonomy(server: &BrainServer, tool_name: &str) -> AutonomyOutcome {
    let action_type = tool_action_type(tool_name);
    let registry = server.registry();
    // v1: no hat-aware bias yet — pass null hats. The resolver
    // gracefully treats this as "no hat overrides".
    let null_hats = serde_json::Value::Null;
    let config = parse_autonomy_config(&registry.config.autonomy, None, &null_hats);
    // v1: no proposal-effectiveness data yet. The autonomy resolver
    // skips step 2 (confidence-derived level) when sample_count is 0.
    let confidence = ProposalConfidence::default();

    let level = resolve_autonomy(action_type, &config, &confidence);

    match level {
        AutonomyLevel::Auto | AutonomyLevel::Notify => AutonomyOutcome::Allow,
        AutonomyLevel::Approve => {
            let action_id = Uuid::new_v4().to_string();
            // Best-effort publish to the approvals topic. Failure to
            // write the request shouldn't auto-allow the action — we
            // still gate on `Approve` and return the action_id so
            // the agent can poll.
            let _ = publish_approval_request(
                server.project_root(),
                &action_id,
                tool_name,
                action_type,
            )
            .await;
            AutonomyOutcome::Approve { action_id, action_type }
        }
        AutonomyLevel::Blocked => AutonomyOutcome::Block {
            action_type,
            reason: format!(
                "tool '{tool_name}' is blocked at action_type '{action_type}' \
                 by safety invariants or policy"
            ),
        },
    }
}

/// Build the JSON envelope a wrapped tool returns when autonomy
/// rejects the call. Returns `None` for `Allow` (caller proceeds).
pub fn early_return_envelope(outcome: &AutonomyOutcome, tool_name: &str) -> Option<String> {
    match outcome {
        AutonomyOutcome::Allow => None,
        AutonomyOutcome::Approve { action_id, action_type } => Some(
            json!({
                "status": "pending_approval",
                "tool": tool_name,
                "action_id": action_id,
                "action_type": action_type,
                "approvals_topic": APPROVALS_TOPIC,
                "resolutions_topic": APPROVAL_RESOLUTIONS_TOPIC,
                "hint": "operator approves via `neurogrim queue publish _neurogrim/approval-resolutions ...` or the dashboard's /brains/:id/approvals page; agent polls via the await_approval MCP tool",
            })
            .to_string(),
        ),
        AutonomyOutcome::Block { action_type, reason } => Some(
            json!({
                "error": "blocked",
                "tool": tool_name,
                "action_type": action_type,
                "reason": reason,
            })
            .to_string(),
        ),
    }
}

/// Helper used by `BrainServer` mutation tools: returns the early-
/// return envelope when the autonomy outcome is non-Allow, otherwise
/// returns None and the caller proceeds.
pub async fn maybe_block(server: &BrainServer, tool_name: &str) -> Option<String> {
    let outcome = check_autonomy(server, tool_name).await;
    early_return_envelope(&outcome, tool_name)
}

/// Publish an approval request to `_neurogrim/approvals`. The
/// payload carries the `action_id` operators reference when
/// resolving on `_neurogrim/approval-resolutions`.
async fn publish_approval_request(
    project_root: &Path,
    action_id: &str,
    tool_name: &str,
    action_type: &str,
) -> Result<(), anyhow::Error> {
    let path = topic_path(project_root, APPROVALS_TOPIC);
    let payload = json!({
        "action_id": action_id,
        "tool": tool_name,
        "action_type": action_type,
        "requires_approval_by": null,
        "blast_radius": "unknown",
    });
    let msg = QueueMessage::new(APPROVALS_TOPIC, payload);
    if !Topic::is_valid(&msg.topic) {
        return Err(anyhow::anyhow!("approvals topic invalid"));
    }
    append(&path, &msg)?;
    Ok(())
}

/// Read the `_neurogrim/approval-resolutions` ledger and return the
/// most recent decision for `action_id`. Returns None when no
/// resolution has been recorded yet.
pub fn read_approval_resolution(
    project_root: &Path,
    action_id: &str,
) -> Option<ApprovalResolution> {
    let path = topic_path(project_root, APPROVAL_RESOLUTIONS_TOPIC);
    let reader = neurogrim_core::queue::JsonlQueueReader::open(&path).ok()?;
    let messages = reader.into_messages();
    // Walk newest-first.
    for msg in messages.iter().rev() {
        let aid = msg.payload.get("action_id").and_then(|v| v.as_str());
        if aid != Some(action_id) {
            continue;
        }
        let decision = msg
            .payload
            .get("decision")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let operator = msg
            .payload
            .get("operator")
            .and_then(|v| v.as_str())
            .map(String::from);
        let decided_at = msg
            .payload
            .get("decided_at")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| msg.produced_at.to_rfc3339());
        return Some(ApprovalResolution {
            action_id: action_id.to_string(),
            decision,
            operator,
            decided_at,
        });
    }
    None
}

/// One approval resolution entry. Surfaces the operator's decision
/// for an action_id.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ApprovalResolution {
    pub action_id: String,
    /// `"approve"` | `"deny"` (others surfaced for forward-compat).
    pub decision: String,
    pub operator: Option<String>,
    pub decided_at: String,
}

fn topic_path(project_root: &Path, topic: &str) -> std::path::PathBuf {
    let mut p = project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    for seg in topic.split('/') {
        if !seg.is_empty() {
            p.push(seg);
        }
    }
    p.set_extension("jsonl");
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn tool_action_type_classifies_known_tools() {
        assert_eq!(tool_action_type("queue_publish"), "mutate-state");
        assert_eq!(tool_action_type("queue_consume"), "read-only");
        assert_eq!(tool_action_type("get_health_score"), "read-only");
        assert_eq!(tool_action_type("doctor"), "read-only");
        assert_eq!(tool_action_type("domain_new"), "mutate-state");
        assert_eq!(tool_action_type("refresh_sensory"), "mutate-state");
        assert_eq!(
            tool_action_type("record_subagent_outcome"),
            "mutate-state"
        );
        assert_eq!(tool_action_type("await_approval"), "read-only");
    }

    #[test]
    fn unknown_tool_falls_back_to_mutate_state() {
        // Conservative default: an unrecognized tool is treated as
        // a mutation so it goes through approval rather than auto-
        // running.
        assert_eq!(tool_action_type("brand_new_unmapped_tool"), "mutate-state");
    }

    #[test]
    fn early_return_envelope_allow_returns_none() {
        assert_eq!(
            early_return_envelope(&AutonomyOutcome::Allow, "any_tool"),
            None
        );
    }

    #[test]
    fn early_return_envelope_block_includes_reason_and_action_type() {
        let outcome = AutonomyOutcome::Block {
            action_type: "mutate-state",
            reason: "blocked by destroy_always_blocked invariant".into(),
        };
        let s = early_return_envelope(&outcome, "domain_new").unwrap();
        assert!(s.contains("\"error\":\"blocked\""));
        assert!(s.contains("destroy_always_blocked"));
        assert!(s.contains("mutate-state"));
        assert!(s.contains("domain_new"));
    }

    #[test]
    fn early_return_envelope_approve_carries_action_id() {
        let outcome = AutonomyOutcome::Approve {
            action_id: "abc-123".into(),
            action_type: "mutate-state",
        };
        let s = early_return_envelope(&outcome, "queue_publish").unwrap();
        assert!(s.contains("\"status\":\"pending_approval\""));
        assert!(s.contains("\"action_id\":\"abc-123\""));
        assert!(s.contains("queue_publish"));
        // Hint surfaces the operator's resolution path.
        assert!(s.contains("await_approval"));
    }

    #[tokio::test]
    async fn publish_approval_request_writes_to_disk() {
        let tmp = TempDir::new().unwrap();
        publish_approval_request(
            tmp.path(),
            "test-action-id",
            "test_tool",
            "mutate-state",
        )
        .await
        .unwrap();
        let path = topic_path(tmp.path(), APPROVALS_TOPIC);
        let r = neurogrim_core::queue::JsonlQueueReader::open(&path).unwrap();
        assert_eq!(r.len(), 1);
        let messages = r.into_messages();
        assert_eq!(messages[0].topic, APPROVALS_TOPIC);
        assert_eq!(
            messages[0].payload.get("action_id").and_then(|v| v.as_str()),
            Some("test-action-id")
        );
        assert_eq!(
            messages[0].payload.get("tool").and_then(|v| v.as_str()),
            Some("test_tool")
        );
    }

    #[test]
    fn read_approval_resolution_returns_none_when_no_ledger() {
        let tmp = TempDir::new().unwrap();
        let r = read_approval_resolution(tmp.path(), "ghost");
        assert!(r.is_none());
    }

    #[test]
    fn read_approval_resolution_finds_matching_action() {
        let tmp = TempDir::new().unwrap();
        let path = topic_path(tmp.path(), APPROVAL_RESOLUTIONS_TOPIC);
        let msg = QueueMessage::new(
            APPROVAL_RESOLUTIONS_TOPIC,
            json!({
                "action_id": "abc-123",
                "decision": "approve",
                "operator": "alice",
                "decided_at": "2026-04-29T18:00:00Z",
            }),
        );
        append(&path, &msg).unwrap();
        let r = read_approval_resolution(tmp.path(), "abc-123").unwrap();
        assert_eq!(r.action_id, "abc-123");
        assert_eq!(r.decision, "approve");
        assert_eq!(r.operator.as_deref(), Some("alice"));
        assert_eq!(r.decided_at, "2026-04-29T18:00:00Z");
    }

    #[test]
    fn read_approval_resolution_returns_most_recent() {
        // If two resolutions exist for the same action_id, the
        // newer one wins. (Operators should never resolve the same
        // action twice, but if they do, the latest decision is the
        // truth.)
        let tmp = TempDir::new().unwrap();
        let path = topic_path(tmp.path(), APPROVAL_RESOLUTIONS_TOPIC);
        for op in ["alice", "bob"] {
            let msg = QueueMessage::new(
                APPROVAL_RESOLUTIONS_TOPIC,
                json!({
                    "action_id": "x",
                    "decision": "approve",
                    "operator": op,
                    "decided_at": "2026-04-29T18:00:00Z",
                }),
            );
            append(&path, &msg).unwrap();
        }
        let r = read_approval_resolution(tmp.path(), "x").unwrap();
        assert_eq!(r.operator.as_deref(), Some("bob"));
    }

    #[test]
    fn read_approval_resolution_ignores_other_action_ids() {
        let tmp = TempDir::new().unwrap();
        let path = topic_path(tmp.path(), APPROVAL_RESOLUTIONS_TOPIC);
        let msg = QueueMessage::new(
            APPROVAL_RESOLUTIONS_TOPIC,
            json!({"action_id": "other", "decision": "approve"}),
        );
        append(&path, &msg).unwrap();
        assert!(read_approval_resolution(tmp.path(), "wanted").is_none());
    }
}
