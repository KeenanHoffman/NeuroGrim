//! Fractal composition dispatch — the async I/O layer for invoking child Brains
//! (spec §9 + §13.5). `motherbrain-core::ecosystem` holds the pure types and
//! algorithms; this crate hosts the transport-specific side effects.
//!
//! # Scope & invariants
//!
//! - **Transports:** Subprocess (§9.1 Conformant) and A2A (§9.1 RECOMMENDED).
//!   The sensory protocol (MCP) is deliberately absent — it is the sensory
//!   protocol, not the peer-Brain protocol (§13.9 protocol split). This
//!   crate imports zero sensory-protocol or sensory-protocol-wrapper
//!   symbols; a grep test in `tests/contract.rs` enforces the boundary.
//! - **Subprocess is NOT legacy in code.** §9.1 calls it Conformant. It is
//!   the zero-infrastructure path for starter-kit adopters and CI one-shots.
//! - **Identical inputs → identical ecosystem score regardless of transport.**
//!   The dispatch decision must never change the observable output; §9.7
//!   ("Implementations MUST produce the same ecosystem score regardless of
//!   transport") is a conformance requirement, not a nice-to-have. The
//!   contract test in `tests/contract.rs` enforces this end-to-end.
//!
//! # Entry points
//!
//! | Function | Purpose |
//! |----------|---------|
//! | [`invoke_child`] | Single-child dispatch — subprocess or A2A, returns `AgentOutput`. |
//! | [`score_ecosystem`] | Full pipeline: topo-sort, invoke all, aggregate per §9.4. |
//!
//! # Cultural notes
//!
//! `.claude/culture.yaml` applies. Honesty over apparent progress:
//! - An A2A call that times out surfaces as [`EcosystemError::A2a`] with the
//!   underlying error, not a silent zero score.
//! - A subprocess that exits non-zero is reported with its stderr excerpt —
//!   we neither swallow the output nor pretend the run succeeded.
//! - Schema validation failures name the offending field so the caller can
//!   fix their child quickly rather than hunt through opaque errors.

use motherbrain_a2a::{A2aEnvelope, HttpSseTransport, MessageType, TaskClient};
use motherbrain_core::agent_output::AgentOutput;
use motherbrain_core::ecosystem::{
    aggregate, enabled_only, topological_sort, ChildContribution, ChildEntry, ChildTransport,
    EcosystemRegistry, EcosystemScore, EcosystemTopologyError,
};
use serde_json::Value;
use std::process::Stdio;
use thiserror::Error;
use tokio::process::Command;
use url::Url;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors raised by the dispatch layer. Each variant names the failure mode
/// specifically so adopters can grep for it and respond to the actual cause.
#[derive(Debug, Error)]
pub enum EcosystemError {
    /// The subprocess exited non-zero, failed to spawn, or produced unusable
    /// output. `stderr` is carried through for diagnosis.
    #[error("subprocess child {child:?} failed: {message}")]
    Subprocess {
        child: String,
        message: String,
        #[source]
        source: Option<std::io::Error>,
    },

    /// The A2A transport returned an error. The inner error has the detail
    /// (bad status, network failure, agent-card validation, etc.).
    #[error("A2A child {child:?} failed: {source}")]
    A2a {
        child: String,
        #[source]
        source: motherbrain_a2a::A2aError,
    },

    /// The child produced JSON that doesn't deserialize into `AgentOutput`.
    /// This is also our schema-validation path: the Rust type mirrors
    /// `agent-output-v1.schema.json`, so a failed deserialize *is* a failed
    /// schema validation. Documented as such rather than claimed separately.
    #[error("child {child:?} produced invalid agent output: {source}")]
    InvalidOutput {
        child: String,
        #[source]
        source: serde_json::Error,
    },

    /// Dependency graph has a cycle or an unknown dependency. Surfaced from
    /// `motherbrain-core::ecosystem::topological_sort`.
    #[error("ecosystem topology error: {0}")]
    Topology(#[from] EcosystemTopologyError),

    /// Returned when an A2A response's payload is missing or is not a JSON
    /// object we can deserialize as `AgentOutput`. Distinct from
    /// `InvalidOutput` so the caller can tell a *transport-layer* problem
    /// from a *child-produced-bad-output* problem.
    #[error("A2A child {child:?} returned unexpected envelope: {message}")]
    UnexpectedA2aResponse { child: String, message: String },

    /// The peer's Agent Card doesn't declare a message type we need to send
    /// (spec §9.7 step 2). Surfaced as its own variant so adopters can tell
    /// "peer refused us" from "peer is broken" — both are recoverable, but
    /// they call for different fixes.
    #[error(
        "A2A peer {brain_id:?} does not accept message type {missing:?} \
         (check peer's Agent Card capabilities.accepts)"
    )]
    CapabilityMismatch { brain_id: String, missing: String },

    /// The peer's Agent Card declares a different `interface_version` than
    /// the registry entry expected (spec §9.7 step 2 + §6). We refuse to
    /// invoke — silently accepting a mismatched interface would mean the
    /// caller gets an `AgentOutput` shape they're not prepared for.
    #[error(
        "A2A interface_version mismatch: registry expected {expected:?}, peer advertises {got:?}"
    )]
    InterfaceVersionMismatch { expected: String, got: String },
}

// ---------------------------------------------------------------------------
// Single-child dispatch
// ---------------------------------------------------------------------------

/// Invoke one child Brain and return its `AgentOutput`.
///
/// Dispatches on the transport variant:
/// - [`ChildTransport::Subprocess`] — spawn the command, capture stdout,
///   parse as `AgentOutput` JSON. stderr is forwarded into the error on
///   non-zero exit.
/// - [`ChildTransport::A2A`] — build a `snapshot.requested` envelope and
///   call the peer via `motherbrain-a2a::TaskClient`. The reply envelope's
///   payload is expected to be the child's agent-output JSON (spec §9.7).
///
/// Disabled children are NOT invoked here; the caller is expected to filter
/// with [`motherbrain_core::ecosystem::enabled_only`] or by inspecting
/// `entry.enabled` before calling. We treat `invoke_child` as "do the work"
/// and keep the policy (enable/disable) in the caller — fewer implicit rules
/// is fewer surprises.
pub async fn invoke_child(entry: &ChildEntry) -> Result<AgentOutput, EcosystemError> {
    match &entry.transport {
        ChildTransport::Subprocess { brain_path } => invoke_subprocess(&entry.id, brain_path).await,
        ChildTransport::A2A {
            a2a_endpoint,
            agent_card_url,
        } => {
            invoke_a2a(
                &entry.id,
                a2a_endpoint,
                agent_card_url.as_ref(),
                &entry.interface_version,
            )
            .await
        }
    }
}

/// Subprocess transport. The child Brain's entry point is spawned with no
/// arguments and its stdout is parsed as `agent-output-v1` JSON.
///
/// Honesty: we do NOT set `PATH` or a working directory — the caller's
/// environment is used as-is. If adopters need isolation, they build it on
/// top; pretending we sandbox when we don't would be worse than naming the
/// limit.
async fn invoke_subprocess(
    child_id: &str,
    brain_path: &str,
) -> Result<AgentOutput, EcosystemError> {
    tracing::debug!(child = %child_id, path = %brain_path, "spawning subprocess child");

    // `brain_path` can be either a literal executable or a shell-style command.
    // We go with the literal-executable interpretation — the spec gives us a
    // `brain_path`, not a `brain_command`. If adopters need shell features,
    // they can wrap in a script.
    //
    // We split on whitespace so a simple "cmd arg1 arg2" string works; this
    // matches the pragmatic behavior of most process-invocation APIs. If
    // `brain_path` is empty, `Command::new("")` fails with a clear error.
    let mut parts = brain_path.split_whitespace();
    let program = parts.next().ok_or_else(|| EcosystemError::Subprocess {
        child: child_id.into(),
        message: "brain_path is empty".into(),
        source: None,
    })?;
    let args: Vec<&str> = parts.collect();

    let output = Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| EcosystemError::Subprocess {
            child: child_id.into(),
            message: format!("failed to spawn {program:?}: {e}"),
            source: Some(e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(EcosystemError::Subprocess {
            child: child_id.into(),
            message: format!(
                "exit status {:?}, stderr: {}",
                output.status.code(),
                stderr.trim()
            ),
            source: None,
        });
    }

    // Deserialize — per the crate docs, this IS the schema validation. Any
    // field shape mismatch surfaces as serde_json::Error with a helpful
    // location pointer.
    let agent_output: AgentOutput =
        serde_json::from_slice(&output.stdout).map_err(|e| EcosystemError::InvalidOutput {
            child: child_id.into(),
            source: e,
        })?;
    Ok(agent_output)
}

/// A2A transport. Builds a `snapshot.requested` envelope targeting the peer
/// and awaits the reply. The reply's `payload` is interpreted as the child's
/// full agent output per spec §9.7 step 4.
///
/// Spec §9.7 steps 1–2 + Appendix G.3 step 1 REQUIRE Agent Card discovery
/// before the first POST:
///
/// 1. Fetch `{endpoint}/.well-known/agent-card.json` (or the override from
///    the registry entry).
/// 2. Verify the peer's Agent Card declares `snapshot.requested` in
///    `capabilities.accepts` — refuse up front rather than POST and wait
///    for a 405.
/// 3. Verify `interface_version` matches what the registry said to expect —
///    a quiet mismatch would silently corrupt the caller's assumptions about
///    the `AgentOutput` shape.
///
/// The discovered card is held only for these two checks; we don't cache
/// across calls (yet). Caching is safe when we add it because the `discover`
/// call itself is idempotent.
async fn invoke_a2a(
    child_id: &str,
    endpoint: &Url,
    agent_card_url: Option<&Url>,
    expected_interface_version: &str,
) -> Result<AgentOutput, EcosystemError> {
    tracing::debug!(child = %child_id, endpoint = %endpoint, "dispatching A2A child");

    let client: TaskClient<HttpSseTransport> = TaskClient::new_http();

    // Step 1: discover the Agent Card. Transport failures bubble as A2a.
    let card = client
        .discover_at(endpoint, agent_card_url)
        .await
        .map_err(|e| EcosystemError::A2a {
            child: child_id.into(),
            source: e,
        })?;

    // Step 2a: verify the peer accepts snapshot.requested. We do the check
    // here rather than relying on a 405 response because the spec calls for
    // pre-flight validation — and because surfacing a typed error is kinder
    // than a generic transport failure.
    if !card
        .capabilities
        .accepts
        .contains(&MessageType::SnapshotRequested)
    {
        return Err(EcosystemError::CapabilityMismatch {
            brain_id: card.id,
            missing: "snapshot.requested".into(),
        });
    }

    // Step 2b: verify the interface_version the peer advertises matches what
    // the registry entry expected. String equality is deliberate — we treat
    // interface_version as an opaque label, not a semver range (spec §6).
    if card.interface_version != expected_interface_version {
        return Err(EcosystemError::InterfaceVersionMismatch {
            expected: expected_interface_version.to_string(),
            got: card.interface_version,
        });
    }

    // snapshot.requested payload per spec §9.7 step 3.
    let request = A2aEnvelope::new(
        "motherbrain-ecosystem",
        MessageType::SnapshotRequested,
        serde_json::json!({ "scope": "score" }),
    );

    let response = client
        .invoke(endpoint, request)
        .await
        .map_err(|e| EcosystemError::A2a {
            child: child_id.into(),
            source: e,
        })?;

    // Accept either score.updated (§9.7 normative) or snapshot.delivered
    // (the reply form used by TaskServer when responding to snapshot.requested).
    // The spec text in §9.7 step 4 says score.updated; the TaskServer examples
    // in motherbrain-a2a test with snapshot.delivered as the reply_to form.
    // We accept both rather than reject one — the message type doesn't
    // change the payload shape, and refusing snapshot.delivered would break
    // working interop with the sibling crate's reference server.
    match response.message_type {
        MessageType::ScoreUpdated | MessageType::SnapshotDelivered => {}
        other => {
            return Err(EcosystemError::UnexpectedA2aResponse {
                child: child_id.into(),
                message: format!("expected score.updated or snapshot.delivered, got {other:?}"),
            });
        }
    }

    // Per §9.7 step 5: payload IS the agent output. We deserialize directly.
    let agent_output: AgentOutput =
        serde_json::from_value(response.payload).map_err(|e| EcosystemError::InvalidOutput {
            child: child_id.into(),
            source: e,
        })?;
    Ok(agent_output)
}

// ---------------------------------------------------------------------------
// Full-pipeline entry point
// ---------------------------------------------------------------------------

/// Run the full fractal-composition pipeline for one parent + its registry:
///
/// 1. Drop disabled children and their inbound dep edges.
/// 2. Topological sort (§9.3) — dependencies first.
/// 3. Invoke each child via [`invoke_child`]. Failures are captured as
///    per-child errors; they do NOT abort the whole ecosystem pass.
/// 4. Aggregate (§9.4) into [`EcosystemScore`].
///
/// The `parent_weight` is the weight assigned to the parent's own score in
/// the aggregation — callers typically pass `1.0` to give the parent one
/// full share.
///
/// Honesty: this is **sequential** in v1. Parallel dispatch of independent
/// children is an optimization, not a correctness requirement; we ship the
/// correct sequential version first rather than race to parallelism we'd
/// have to test twice.
pub async fn score_ecosystem(
    parent_output: AgentOutput,
    parent_weight: f64,
    registry: EcosystemRegistry,
) -> Result<EcosystemScore, EcosystemError> {
    let alive = enabled_only(&registry.children);
    // Disabled children still appear in child_statuses — so collect them
    // separately and feed them in as `disabled: true` contributions.
    let disabled: Vec<&ChildEntry> = registry.children.iter().filter(|c| !c.enabled).collect();

    let ordered = topological_sort(&alive)?;

    let mut contributions: Vec<ChildContribution> =
        Vec::with_capacity(ordered.len() + disabled.len());
    for entry in &ordered {
        let result = invoke_child(entry).await;
        let output = match result {
            Ok(out) => Ok(out),
            // We down-convert to string here so the `aggregate` call — which
            // lives in the pure core — can stay dependency-free. The full
            // `EcosystemError` variant is still available to callers who
            // call `invoke_child` directly.
            Err(e) => Err(e.to_string()),
        };
        contributions.push(ChildContribution {
            id: entry.id.clone(),
            weight: entry.weight,
            output,
            disabled: false,
        });
    }
    for entry in disabled {
        contributions.push(ChildContribution {
            id: entry.id.clone(),
            weight: entry.weight,
            output: Err("disabled".into()),
            disabled: true,
        });
    }

    let now = chrono::Utc::now();
    Ok(aggregate(
        &parent_output,
        parent_weight,
        &contributions,
        now,
    ))
}

/// Expose the cross-project variable merge from the core for adopters who
/// want it alongside the dispatch pipeline. Thin passthrough — we just keep
/// all the ecosystem surface discoverable from one crate.
pub use motherbrain_core::ecosystem::merge_cross_project_variables;

// Re-export the status enum so callers don't need two imports.
pub use motherbrain_core::ecosystem::ChildStatus;

// ---------------------------------------------------------------------------
// Value helper for test fixtures
// ---------------------------------------------------------------------------

/// Produce the minimum JSON that deserializes as `AgentOutput`. Useful for
/// fixtures in downstream tests. Not part of the crate's runtime surface;
/// we gate on `cfg(test)` ... but keeping it public behind `cfg(feature)`
/// would be cleaner. For now, leave unexported and re-implement in tests
/// that need it.
#[doc(hidden)]
pub fn _fixture_agent_output_json(score: u8, scored_at_rfc3339: &str) -> Value {
    serde_json::json!({
        "schema_version": "1",
        "scored_at": scored_at_rfc3339,
        "score": score,
        "domains": {},
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {},
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    })
}

// ---------------------------------------------------------------------------
// In-crate unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn score_ecosystem_empty_registry_equals_parent() {
        let parent: AgentOutput =
            serde_json::from_value(_fixture_agent_output_json(75, &Utc::now().to_rfc3339()))
                .unwrap();
        let result = score_ecosystem(parent, 1.0, EcosystemRegistry { children: vec![] })
            .await
            .unwrap();
        assert_eq!(result.ecosystem_score, 75);
        assert!(result.child_statuses.is_empty());
    }

    #[tokio::test]
    async fn score_ecosystem_captures_subprocess_failure() {
        // Child path that won't exist. We assert:
        //   - pipeline does NOT propagate the error (it's captured)
        //   - child shows up with status=Error
        //   - aggregation excludes the child (parent alone == parent score)
        let parent: AgentOutput =
            serde_json::from_value(_fixture_agent_output_json(80, &Utc::now().to_rfc3339()))
                .unwrap();
        let registry = EcosystemRegistry {
            children: vec![ChildEntry {
                id: "broken".into(),
                display_name: None,
                transport: ChildTransport::Subprocess {
                    brain_path: "definitely-not-a-real-binary-xyz".into(),
                },
                interface_version: "1".into(),
                depends_on: vec![],
                weight: 1.0,
                enabled: true,
            }],
        };
        let result = score_ecosystem(parent, 1.0, registry).await.unwrap();
        assert_eq!(
            result.ecosystem_score, 80,
            "failed child must not bias the average"
        );
        assert_eq!(result.child_statuses["broken"], ChildStatus::Error);
    }
}
