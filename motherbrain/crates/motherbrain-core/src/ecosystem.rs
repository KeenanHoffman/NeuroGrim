//! Fractal composition — pure types and algorithms (spec §9).
//!
//! This module holds the zero-I/O pieces of fractal composition:
//!
//! - [`ChildEntry`] / [`EcosystemRegistry`] — deserialize from the `children`
//!   block of a v2 Brain Registry (see `brain-registry-v2.schema.json` §9.2).
//! - [`ChildStatus`] — §9.5 status taxonomy (`ok` / `error` / `stale` / `disabled`).
//! - [`topological_sort`] — Kahn's algorithm over `depends_on` (§9.3). Returns the
//!   execution order: dependencies first, so by the time each child runs, every
//!   project it depends on has already produced output.
//! - [`aggregate`] — weighted-average score aggregation per §9.4, applying the
//!   freshness multiplier from §4.8 (reused from [`crate::confidence`]).
//! - [`merge_cross_project_variables`] — namespaces each child's domain
//!   variables under `child.<project_id>.` per §9.6.
//!
//! **Invariant:** this module performs no I/O. It takes child outputs already
//! in hand and produces pure results. The actual subprocess spawn / A2A call
//! lives in the sibling crate `motherbrain-ecosystem` so that `motherbrain-core`
//! stays strictly pure. If you feel tempted to `tokio::spawn` here, resist it
//! — the split exists for a reason (see the crate-level description).

use crate::agent_output::AgentOutput;
use crate::confidence::freshness_multiplier;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use thiserror::Error;
use url::Url;

// ---------------------------------------------------------------------------
// Transport selector
// ---------------------------------------------------------------------------

/// How the parent Brain reaches a child. Spec §9.1 lists two conformant
/// transports: subprocess (legacy, zero-infra) and A2A (RECOMMENDED, v2.1+).
/// MCP is NOT a child-invocation transport (§13.9 protocol split).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ChildTransport {
    /// Subprocess transport: spawn `brain_path` and parse stdout as
    /// `agent-output-v1` JSON. Preserved for starter-kit adopters and CI
    /// one-shots — don't mark as "legacy" in code; §9.1 calls it Conformant.
    Subprocess {
        /// Path to the child Brain entry point, relative to the parent project.
        brain_path: String,
    },
    /// A2A transport: POST a `snapshot.requested` envelope and await a
    /// `score.updated` reply carrying the full agent output as payload.
    A2A {
        /// Base URL of the child Brain's A2A server.
        a2a_endpoint: Url,
        /// Optional override for the Agent Card location. Default location
        /// (derived by the dispatch layer) is
        /// `{a2a_endpoint}/.well-known/agent-card.json`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_card_url: Option<Url>,
    },
}

// ---------------------------------------------------------------------------
// Child entry + registry
// ---------------------------------------------------------------------------

/// One child registration in the ecosystem. Mirrors `config.children.*` in the
/// v2 Brain Registry schema. After deserialization, exactly one transport
/// selector (`brain_path` or `a2a_endpoint`) is captured in [`Self::transport`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildEntry {
    /// Project id — the key under `config.children.{id}` in the registry.
    pub id: String,

    /// Human-readable display name. Optional; falls back to `id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// Transport selector. Built from registry fields during `from_registry_map`.
    pub transport: ChildTransport,

    /// Expected agent-output schema version this child emits.
    pub interface_version: String,

    /// Project ids this child depends on (must run after those).
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Weight in ecosystem aggregation. Schema constrains to `[0, 1]`.
    #[serde(default = "default_weight")]
    pub weight: f64,

    /// Whether this child participates. Disabled children surface as
    /// [`ChildStatus::Disabled`] and never get invoked.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_weight() -> f64 {
    1.0
}
fn default_enabled() -> bool {
    true
}

/// The full ecosystem registry — just the list of children.
///
/// Constructing from raw JSON:
/// ```no_run
/// use motherbrain_core::ecosystem::EcosystemRegistry;
/// let json = r#"{
///   "children": {
///     "alpha": {
///       "a2a_endpoint": "https://alpha.example/",
///       "interface_version": "1",
///       "weight": 0.5
///     }
///   }
/// }"#;
/// let reg: EcosystemRegistry = serde_json::from_str(json).unwrap();
/// assert_eq!(reg.children.len(), 1);
/// ```
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EcosystemRegistry {
    pub children: Vec<ChildEntry>,
}

impl<'de> Deserialize<'de> for EcosystemRegistry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// Raw shape matching the schema: `{ "children": { "<id>": {...} } }`.
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            children: BTreeMap<String, RawChild>,
        }
        /// Raw child — flat fields, transport selector extracted post-hoc.
        #[derive(Deserialize)]
        struct RawChild {
            #[serde(default)]
            display_name: Option<String>,
            #[serde(default)]
            brain_path: Option<String>,
            #[serde(default)]
            a2a_endpoint: Option<Url>,
            #[serde(default)]
            agent_card_url: Option<Url>,
            interface_version: String,
            #[serde(default)]
            depends_on: Vec<String>,
            #[serde(default = "default_weight")]
            weight: f64,
            #[serde(default = "default_enabled")]
            enabled: bool,
        }

        let raw = Raw::deserialize(deserializer)?;
        let mut children = Vec::with_capacity(raw.children.len());
        for (id, c) in raw.children {
            // Spec §9.2: exactly one of a2a_endpoint / brain_path MUST be present.
            let transport = match (c.a2a_endpoint, c.brain_path) {
                (Some(endpoint), None) => ChildTransport::A2A {
                    a2a_endpoint: endpoint,
                    agent_card_url: c.agent_card_url,
                },
                (None, Some(path)) => ChildTransport::Subprocess { brain_path: path },
                (Some(_), Some(_)) => {
                    return Err(serde::de::Error::custom(format!(
                        "child {id:?}: both a2a_endpoint and brain_path set — \
                         the registry MUST pick exactly one transport"
                    )));
                }
                (None, None) => {
                    return Err(serde::de::Error::custom(format!(
                        "child {id:?}: neither a2a_endpoint nor brain_path set — \
                         a transport selector is required"
                    )));
                }
            };
            children.push(ChildEntry {
                id,
                display_name: c.display_name,
                transport,
                interface_version: c.interface_version,
                depends_on: c.depends_on,
                weight: c.weight,
                enabled: c.enabled,
            });
        }
        Ok(EcosystemRegistry { children })
    }
}

// ---------------------------------------------------------------------------
// Child status taxonomy (§9.5)
// ---------------------------------------------------------------------------

/// Per-child execution status. §9.5 defines exactly these four values.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChildStatus {
    /// Child invoked successfully and produced valid output.
    Ok,
    /// Child failed or produced invalid output.
    Error,
    /// Child's `freshness_multiplier` < 0.5 (scored_at older than ~3 days).
    Stale,
    /// Child's `enabled` is `false`.
    Disabled,
}

// ---------------------------------------------------------------------------
// Topological sort (§9.3)
// ---------------------------------------------------------------------------

/// Error returned when the dependency graph has a cycle.
#[derive(Debug, Error, PartialEq)]
pub enum EcosystemTopologyError {
    /// A cycle was detected. The spec says SHOULD fall back to registration
    /// order with a warning — we return the error so the caller owns that
    /// choice rather than silently degrading. Ids in `involved` are the ones
    /// still pending after Kahn's terminates.
    #[error("dependency cycle among children: {involved:?}")]
    Cycle { involved: Vec<String> },

    /// A `depends_on` entry referenced a project id not in the registry.
    #[error("child {child:?} depends on unknown project {unknown:?}")]
    UnknownDependency { child: String, unknown: String },
}

/// Topological sort over children's `depends_on` (spec §9.3, Kahn's algorithm).
///
/// Returns children in execution order — dependencies appear before dependents.
/// So for a chain `A -> B -> C` (C depends_on B, B depends_on A), the returned
/// order is `[A, B, C]`. This is the order the caller should iterate when
/// invoking children: by the time you get to C, B has already run.
///
/// Ties (same in-degree) are broken by registration order (stable). Empty
/// input returns an empty Vec.
pub fn topological_sort(
    children: &[ChildEntry],
) -> Result<Vec<ChildEntry>, EcosystemTopologyError> {
    if children.is_empty() {
        return Ok(Vec::new());
    }

    // Look up id -> index for stable order preservation. BTreeMap so iteration
    // over "roots" (indegree=0) is deterministic by insertion order.
    let by_id: HashMap<&str, usize> = children
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();

    // Validate all `depends_on` targets exist before doing any work.
    for c in children {
        for dep in &c.depends_on {
            if !by_id.contains_key(dep.as_str()) {
                return Err(EcosystemTopologyError::UnknownDependency {
                    child: c.id.clone(),
                    unknown: dep.clone(),
                });
            }
        }
    }

    let mut indegree: Vec<usize> = children.iter().map(|c| c.depends_on.len()).collect();
    // For each child, the list of dependents (children that depend on it).
    // Built once up front so the Kahn loop is O(V+E).
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); children.len()];
    for (i, c) in children.iter().enumerate() {
        for dep in &c.depends_on {
            let j = by_id[dep.as_str()];
            dependents[j].push(i);
        }
    }

    // Seed the queue with every indegree-0 child in registration order. This
    // gives ties a predictable resolution, which makes tests reproducible.
    let mut queue: VecDeque<usize> = VecDeque::new();
    for (i, &d) in indegree.iter().enumerate() {
        if d == 0 {
            queue.push_back(i);
        }
    }

    let mut ordered: Vec<ChildEntry> = Vec::with_capacity(children.len());
    while let Some(i) = queue.pop_front() {
        ordered.push(children[i].clone());
        for &j in &dependents[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                queue.push_back(j);
            }
        }
    }

    if ordered.len() != children.len() {
        // Whatever still has indegree > 0 is part of (or downstream of) a cycle.
        let involved: Vec<String> = indegree
            .iter()
            .enumerate()
            .filter(|(_, &d)| d > 0)
            .map(|(i, _)| children[i].id.clone())
            .collect();
        return Err(EcosystemTopologyError::Cycle { involved });
    }
    Ok(ordered)
}

// ---------------------------------------------------------------------------
// Score aggregation (§9.4)
// ---------------------------------------------------------------------------

/// One child's contribution to the aggregate. The dispatch layer fills these
/// in and hands them to [`aggregate`].
#[derive(Debug, Clone)]
pub struct ChildContribution {
    /// Project id from the registry.
    pub id: String,
    /// Child's `weight` from the registry.
    pub weight: f64,
    /// Outcome. `Ok(output)` feeds into aggregation; `Err(_)` records the
    /// failure and excludes the child from the weighted average.
    pub output: Result<AgentOutput, String>,
    /// Whether the child was disabled in the registry (so we can set
    /// [`ChildStatus::Disabled`] without calling the transport).
    pub disabled: bool,
}

/// Aggregated ecosystem score (§9.4) plus per-child status (§9.5).
#[derive(Debug, Clone, Serialize)]
pub struct EcosystemScore {
    /// Weighted-average score, rounded to `[0, 100]`.
    pub ecosystem_score: u8,
    /// Raw (unrounded) weighted average. Exposed for tests and observability;
    /// the rounded `ecosystem_score` is what callers should publish.
    pub ecosystem_score_raw: f64,
    /// Per-child status. Keyed by project id.
    pub child_statuses: BTreeMap<String, ChildStatus>,
    /// Per-child error messages (only present for [`ChildStatus::Error`]).
    pub child_errors: BTreeMap<String, String>,
    /// Timestamp at which aggregation was performed.
    pub aggregated_at: DateTime<Utc>,
}

/// Aggregate parent + children into the ecosystem score.
///
/// Formula (§9.4):
/// ```text
/// child_effective = child_score * (child_confidence / 100) * freshness_multiplier(scored_at)
/// ecosystem = weighted_avg(parent_score * parent_weight, child_effective[i] * child_weight[i])
/// ```
///
/// Honesty notes:
/// - If a child errors out, it's excluded from the average and reported as
///   `ChildStatus::Error`. We do NOT fabricate a zero for it — that would
///   quietly bias the average downward and hide the failure.
/// - If there are no successful contributions, the ecosystem score equals
///   the parent score (weighted by `parent_weight`, which cancels out).
/// - `child_confidence` is read from the child's `domains` map as a simple
///   arithmetic mean of declared domain confidences. If the child reports
///   no domains, we treat confidence as 100 (full) — a child that knows
///   nothing about anything is unusual and is the caller's problem to detect;
///   we don't invent a penalty.
pub fn aggregate(
    parent: &AgentOutput,
    parent_weight: f64,
    contributions: &[ChildContribution],
    now: DateTime<Utc>,
) -> EcosystemScore {
    let mut child_statuses = BTreeMap::new();
    let mut child_errors = BTreeMap::new();

    // Parent term: parent_score * parent_weight. Parent has no freshness
    // multiplier in §9.4 — it's scored "now" by construction.
    let mut weighted_sum = parent.score as f64 * parent_weight;
    let mut weight_sum = parent_weight;

    for c in contributions {
        if c.disabled {
            child_statuses.insert(c.id.clone(), ChildStatus::Disabled);
            continue;
        }
        let out = match &c.output {
            Ok(o) => o,
            Err(msg) => {
                child_statuses.insert(c.id.clone(), ChildStatus::Error);
                child_errors.insert(c.id.clone(), msg.clone());
                continue;
            }
        };

        // Parse the scored_at timestamp for freshness. A malformed timestamp
        // is an error — we don't silently default to "fresh" (that would be
        // the quiet-bias failure mode we're avoiding).
        let scored_at = match DateTime::parse_from_rfc3339(&out.scored_at) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                child_statuses.insert(c.id.clone(), ChildStatus::Error);
                child_errors.insert(
                    c.id.clone(),
                    format!("invalid scored_at {:?}: {e}", out.scored_at),
                );
                continue;
            }
        };
        let fresh = freshness_multiplier(Some(scored_at), now);

        // §9.5: stale when freshness_multiplier < 0.5. We still INCLUDE the
        // stale child in the average (§9.4 applies freshness as a dampener);
        // the status is just a visibility flag for the caller.
        let status = if fresh < 0.5 {
            ChildStatus::Stale
        } else {
            ChildStatus::Ok
        };
        child_statuses.insert(c.id.clone(), status);

        let confidence = mean_domain_confidence(out);
        let effective = out.score as f64 * (confidence / 100.0) * fresh;
        weighted_sum += effective * c.weight;
        weight_sum += c.weight;
    }

    let ecosystem_score_raw = if weight_sum > 0.0 {
        weighted_sum / weight_sum
    } else {
        // Degenerate case: parent_weight = 0 and no successful children.
        // Report 0 rather than NaN — integrity over cleverness.
        0.0
    };
    let ecosystem_score = ecosystem_score_raw.round().clamp(0.0, 100.0) as u8;

    EcosystemScore {
        ecosystem_score,
        ecosystem_score_raw,
        child_statuses,
        child_errors,
        aggregated_at: now,
    }
}

/// Arithmetic mean of domain confidences in the agent output. Empty domains
/// -> 100 (see `aggregate` docs for why).
fn mean_domain_confidence(out: &AgentOutput) -> f64 {
    if out.domains.is_empty() {
        return 100.0;
    }
    let sum: f64 = out.domains.values().map(|d| d.confidence as f64).sum();
    sum / out.domains.len() as f64
}

// ---------------------------------------------------------------------------
// Cross-project variable merging (§9.6)
// ---------------------------------------------------------------------------

/// Merge each child's `domain_variables` into the parent namespace with the
/// prefix `child.<project_id>.` (spec §9.6). The parent's own variables are
/// NOT included here — the caller decides how to compose these with its own
/// set. This keeps the function a pure projection.
pub fn merge_cross_project_variables(
    contributions: &[ChildContribution],
) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for c in contributions {
        if c.disabled {
            continue;
        }
        let Ok(output) = &c.output else { continue };
        for (k, v) in &output.domain_variables {
            out.insert(format!("child.{}.{}", c.id, k), v.clone());
        }
    }
    out
}

/// Small helper: classify a child's status based purely on whether it's
/// enabled/errored/stale, given its output (if any) and the current clock.
/// Exposed so the dispatch layer can report a status before calling
/// [`aggregate`] (e.g., for an Agent Card mismatch).
pub fn classify_child(
    enabled: bool,
    output: Option<&AgentOutput>,
    error: Option<&str>,
    now: DateTime<Utc>,
) -> ChildStatus {
    if !enabled {
        return ChildStatus::Disabled;
    }
    if error.is_some() {
        return ChildStatus::Error;
    }
    let Some(out) = output else {
        return ChildStatus::Error;
    };
    let Ok(scored_at) = DateTime::parse_from_rfc3339(&out.scored_at) else {
        return ChildStatus::Error;
    };
    let fresh = freshness_multiplier(Some(scored_at.with_timezone(&Utc)), now);
    if fresh < 0.5 {
        ChildStatus::Stale
    } else {
        ChildStatus::Ok
    }
}

/// Filter children to just the enabled ones, preserving order. Convenience
/// for dispatch layers that want to skip disabled entries before sorting.
pub fn enabled_only(children: &[ChildEntry]) -> Vec<ChildEntry> {
    let mut v: Vec<ChildEntry> = children.iter().filter(|c| c.enabled).cloned().collect();
    // Drop disabled-project dependencies too so Kahn's doesn't trip on them.
    let alive: HashSet<&str> = v.iter().map(|c| c.id.as_str()).collect();
    let alive_owned: HashSet<String> = alive.iter().map(|s| s.to_string()).collect();
    for c in &mut v {
        c.depends_on.retain(|d| alive_owned.contains(d));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_output::{AgentDomain, AgentOutput};
    use chrono::Duration;
    use serde_json::json;

    // -------- Fixtures --------

    /// Build a minimal agent output with a single domain at the given score
    /// and confidence, scored at the given timestamp.
    fn fixture_output(score: u8, confidence: u8, scored_at: DateTime<Utc>) -> AgentOutput {
        let mut domains = HashMap::new();
        domains.insert(
            "health".to_string(),
            AgentDomain {
                score,
                effective_score: score,
                confidence,
                weight: 1.0,
                trajectory: None,
            },
        );
        AgentOutput {
            schema_version: "1".into(),
            scored_at: scored_at.to_rfc3339(),
            score,
            domains,
            dirty_gates: vec![],
            stale_artifacts: vec![],
            domain_variables: HashMap::new(),
            top_recommendations: vec![],
            correlations_fired: vec![],
            incident_patterns: vec![],
            skipped_temporal: vec![],
            proposal_effectiveness: None,
            trajectory: None,
            current_hat: None,
            current_persona: None,
        }
    }

    fn child(id: &str, deps: &[&str], weight: f64) -> ChildEntry {
        ChildEntry {
            id: id.to_string(),
            display_name: None,
            transport: ChildTransport::Subprocess {
                brain_path: format!("./bin/{id}"),
            },
            interface_version: "1".into(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            weight,
            enabled: true,
        }
    }

    // -------- Topological sort --------

    #[test]
    fn topo_empty_returns_empty() {
        let got = topological_sort(&[]).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn topo_single_node_returns_single() {
        let nodes = [child("a", &[], 1.0)];
        let got = topological_sort(&nodes).unwrap();
        assert_eq!(got.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["a"]);
    }

    #[test]
    fn topo_linear_chain_dependencies_first() {
        // C depends on B, B depends on A. Expected order: A, B, C.
        // (dependencies first — by the time C runs, B has run.)
        let nodes = [
            child("c", &["b"], 1.0),
            child("a", &[], 1.0),
            child("b", &["a"], 1.0),
        ];
        let got = topological_sort(&nodes).unwrap();
        let ids: Vec<_> = got.iter().map(|c| c.id.clone()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn topo_diamond_preserves_registration_order_for_ties() {
        // A -> B, A -> C, B -> D, C -> D.  (D depends on B and C.)
        // Valid order is A first, then B/C in either order, then D.
        // We assert: A first, D last, and that B comes before D, C comes before D.
        let nodes = [
            child("a", &[], 1.0),
            child("b", &["a"], 1.0),
            child("c", &["a"], 1.0),
            child("d", &["b", "c"], 1.0),
        ];
        let got = topological_sort(&nodes).unwrap();
        let ids: Vec<_> = got.iter().map(|c| c.id.clone()).collect();
        assert_eq!(ids[0], "a", "A must run first");
        assert_eq!(ids[3], "d", "D must run last");
        let pos_b = ids.iter().position(|x| x == "b").unwrap();
        let pos_c = ids.iter().position(|x| x == "c").unwrap();
        let pos_d = ids.iter().position(|x| x == "d").unwrap();
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn topo_cycle_returns_error() {
        // A -> B -> A (mutual dependency).
        let nodes = [
            child("a", &["b"], 1.0),
            child("b", &["a"], 1.0),
        ];
        let err = topological_sort(&nodes).unwrap_err();
        match err {
            EcosystemTopologyError::Cycle { involved } => {
                assert_eq!(involved.len(), 2);
                assert!(involved.contains(&"a".to_string()));
                assert!(involved.contains(&"b".to_string()));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn topo_self_loop_is_cycle() {
        // A child that depends on itself is a 1-node cycle. Kahn's should
        // never dequeue it (indegree starts at 1), so we get Cycle.
        let nodes = [child("a", &["a"], 1.0)];
        let err = topological_sort(&nodes).unwrap_err();
        assert!(matches!(err, EcosystemTopologyError::Cycle { .. }));
    }

    #[test]
    fn topo_unknown_dependency_errors() {
        let nodes = [child("a", &["ghost"], 1.0)];
        let err = topological_sort(&nodes).unwrap_err();
        match err {
            EcosystemTopologyError::UnknownDependency { child, unknown } => {
                assert_eq!(child, "a");
                assert_eq!(unknown, "ghost");
            }
            other => panic!("expected UnknownDependency, got {other:?}"),
        }
    }

    // -------- Aggregation --------

    #[test]
    fn aggregate_no_children_equals_parent_score() {
        // Spec §9.4: with no children, ecosystem = parent.
        let now = Utc::now();
        let parent = fixture_output(80, 100, now);
        let result = aggregate(&parent, 1.0, &[], now);
        assert_eq!(result.ecosystem_score, 80);
        assert!(result.child_statuses.is_empty());
    }

    #[test]
    fn aggregate_two_children_fresh_weighted_average() {
        // Parent = 90 @ weight 1.0.
        // Child A = 60 @ confidence 100, weight 1.0, scored now.
        // Child B = 40 @ confidence 100, weight 0.5, scored now.
        // Expected:
        //   parent term  = 90 * 1.0 = 90
        //   child A term = 60 * 1.0 * 1.0 (fresh) * 1.0 (weight) = 60
        //   child B term = 40 * 1.0 * 1.0 (fresh) * 0.5 (weight) = 20
        //   weighted_sum = 90 + 60 + 20 = 170
        //   weight_sum   = 1.0 + 1.0 + 0.5 = 2.5
        //   ecosystem    = 170 / 2.5 = 68
        let now = Utc::now();
        let parent = fixture_output(90, 100, now);
        let contribs = vec![
            ChildContribution {
                id: "a".into(),
                weight: 1.0,
                output: Ok(fixture_output(60, 100, now)),
                disabled: false,
            },
            ChildContribution {
                id: "b".into(),
                weight: 0.5,
                output: Ok(fixture_output(40, 100, now)),
                disabled: false,
            },
        ];
        let result = aggregate(&parent, 1.0, &contribs, now);
        // Hand-computed expectation:
        assert!(
            (result.ecosystem_score_raw - 68.0).abs() < 1e-9,
            "expected raw 68.0, got {}",
            result.ecosystem_score_raw
        );
        assert_eq!(result.ecosystem_score, 68);
        assert_eq!(result.child_statuses["a"], ChildStatus::Ok);
        assert_eq!(result.child_statuses["b"], ChildStatus::Ok);
    }

    #[test]
    fn aggregate_applies_freshness_multiplier() {
        // A 5-day-old child should have its score halved (freshness 0.5).
        // Parent = 100 @ weight 0, child = 100 @ weight 1, 5 days old.
        // With parent_weight 0: ecosystem = child_effective
        //   = 100 * 1.0 * 0.5 = 50.
        let now = Utc::now();
        let parent = fixture_output(100, 100, now);
        let contribs = vec![ChildContribution {
            id: "stale-ish".into(),
            weight: 1.0,
            output: Ok(fixture_output(100, 100, now - Duration::days(5))),
            disabled: false,
        }];
        let result = aggregate(&parent, 0.0, &contribs, now);
        assert_eq!(result.ecosystem_score, 50);
        // freshness 0.5 is the boundary — NOT stale (stale is < 0.5).
        assert_eq!(result.child_statuses["stale-ish"], ChildStatus::Ok);
    }

    #[test]
    fn aggregate_flags_stale_child() {
        // 10 days old => freshness 0.25 < 0.5 => Stale.
        let now = Utc::now();
        let parent = fixture_output(100, 100, now);
        let contribs = vec![ChildContribution {
            id: "old".into(),
            weight: 1.0,
            output: Ok(fixture_output(80, 100, now - Duration::days(10))),
            disabled: false,
        }];
        let result = aggregate(&parent, 1.0, &contribs, now);
        assert_eq!(result.child_statuses["old"], ChildStatus::Stale);
    }

    #[test]
    fn aggregate_excludes_errored_child_from_average() {
        // Errored child must not silently contribute 0 — that would bias
        // the ecosystem downward. It's excluded, with status=Error.
        let now = Utc::now();
        let parent = fixture_output(100, 100, now);
        let contribs = vec![
            ChildContribution {
                id: "good".into(),
                weight: 1.0,
                output: Ok(fixture_output(50, 100, now)),
                disabled: false,
            },
            ChildContribution {
                id: "bad".into(),
                weight: 1.0,
                output: Err("subprocess exited 1".into()),
                disabled: false,
            },
        ];
        let result = aggregate(&parent, 1.0, &contribs, now);
        // Only parent + good contributed: (100 + 50) / 2 = 75.
        assert_eq!(result.ecosystem_score, 75);
        assert_eq!(result.child_statuses["good"], ChildStatus::Ok);
        assert_eq!(result.child_statuses["bad"], ChildStatus::Error);
        assert!(result.child_errors.contains_key("bad"));
    }

    #[test]
    fn aggregate_marks_disabled_children_without_invoking() {
        let now = Utc::now();
        let parent = fixture_output(100, 100, now);
        let contribs = vec![ChildContribution {
            id: "off".into(),
            weight: 1.0,
            // Even if we had output, disabled short-circuits.
            output: Err("should not be consulted".into()),
            disabled: true,
        }];
        let result = aggregate(&parent, 1.0, &contribs, now);
        // Parent alone: 100.
        assert_eq!(result.ecosystem_score, 100);
        assert_eq!(result.child_statuses["off"], ChildStatus::Disabled);
    }

    #[test]
    fn aggregate_applies_confidence_dampener() {
        // child_effective = score * (confidence/100) * freshness.
        // 100 @ 50% confidence, fresh, weight 1.0, parent_weight 0 => 50.
        let now = Utc::now();
        let parent = fixture_output(0, 100, now);
        let contribs = vec![ChildContribution {
            id: "half".into(),
            weight: 1.0,
            output: Ok(fixture_output(100, 50, now)),
            disabled: false,
        }];
        let result = aggregate(&parent, 0.0, &contribs, now);
        assert_eq!(result.ecosystem_score, 50);
    }

    // -------- Cross-project variables --------

    #[test]
    fn cross_project_variables_are_prefixed() {
        let now = Utc::now();
        let mut out_a = fixture_output(50, 100, now);
        out_a.domain_variables.insert("deploy_blocking".into(), json!(3));
        out_a
            .domain_variables
            .insert("any_stale".into(), json!(true));
        let contribs = vec![ChildContribution {
            id: "alpha".into(),
            weight: 1.0,
            output: Ok(out_a),
            disabled: false,
        }];
        let merged = merge_cross_project_variables(&contribs);
        assert_eq!(merged["child.alpha.deploy_blocking"], json!(3));
        assert_eq!(merged["child.alpha.any_stale"], json!(true));
    }

    #[test]
    fn cross_project_variables_skips_disabled_and_errored() {
        let now = Utc::now();
        let mut ok_out = fixture_output(50, 100, now);
        ok_out.domain_variables.insert("v".into(), json!(1));
        let contribs = vec![
            ChildContribution {
                id: "ok-one".into(),
                weight: 1.0,
                output: Ok(ok_out),
                disabled: false,
            },
            ChildContribution {
                id: "dis".into(),
                weight: 1.0,
                output: Err("ignored".into()),
                disabled: true,
            },
            ChildContribution {
                id: "err".into(),
                weight: 1.0,
                output: Err("boom".into()),
                disabled: false,
            },
        ];
        let merged = merge_cross_project_variables(&contribs);
        assert_eq!(merged.len(), 1);
        assert!(merged.contains_key("child.ok-one.v"));
    }

    // -------- Registry deserialization --------

    #[test]
    fn registry_deserializes_mixed_transports() {
        // Lifted from spec §9.2 example.
        let json = r#"{
            "children": {
                "project-alpha": {
                    "display_name": "Project Alpha",
                    "a2a_endpoint": "https://alpha.internal/a2a/v1/",
                    "interface_version": "1",
                    "depends_on": [],
                    "weight": 1.0,
                    "enabled": true
                },
                "project-beta-legacy": {
                    "display_name": "Project Beta (subprocess)",
                    "brain_path": "relative/path/to/brain-entry",
                    "interface_version": "1",
                    "depends_on": [],
                    "weight": 0.5,
                    "enabled": true
                }
            }
        }"#;
        let reg: EcosystemRegistry = serde_json::from_str(json).unwrap();
        assert_eq!(reg.children.len(), 2);
        // BTreeMap iteration => alphabetical order by id.
        assert_eq!(reg.children[0].id, "project-alpha");
        assert!(matches!(
            reg.children[0].transport,
            ChildTransport::A2A { .. }
        ));
        assert_eq!(reg.children[1].id, "project-beta-legacy");
        assert!(matches!(
            reg.children[1].transport,
            ChildTransport::Subprocess { .. }
        ));
        assert_eq!(reg.children[1].weight, 0.5);
    }

    #[test]
    fn registry_rejects_missing_transport_selector() {
        let json = r#"{
            "children": {
                "nope": { "interface_version": "1" }
            }
        }"#;
        let err = serde_json::from_str::<EcosystemRegistry>(json).unwrap_err();
        assert!(err.to_string().contains("transport selector"));
    }

    #[test]
    fn registry_rejects_dual_transport_selector() {
        let json = r#"{
            "children": {
                "dual": {
                    "a2a_endpoint": "https://example/",
                    "brain_path": "./brain",
                    "interface_version": "1"
                }
            }
        }"#;
        let err = serde_json::from_str::<EcosystemRegistry>(json).unwrap_err();
        assert!(err.to_string().contains("both"));
    }

    #[test]
    fn classify_child_status_table() {
        let now = Utc::now();
        let out_fresh = fixture_output(80, 100, now);
        let out_stale = fixture_output(80, 100, now - Duration::days(10));

        assert_eq!(classify_child(false, None, None, now), ChildStatus::Disabled);
        assert_eq!(
            classify_child(true, None, Some("x"), now),
            ChildStatus::Error
        );
        assert_eq!(
            classify_child(true, Some(&out_fresh), None, now),
            ChildStatus::Ok
        );
        assert_eq!(
            classify_child(true, Some(&out_stale), None, now),
            ChildStatus::Stale
        );
    }

    #[test]
    fn enabled_only_drops_disabled_and_their_dep_edges() {
        let mut nodes = vec![
            child("a", &[], 1.0),
            child("b", &["a"], 1.0),
            child("c", &["a", "b"], 1.0),
        ];
        nodes[1].enabled = false; // drop b
        let kept = enabled_only(&nodes);
        assert_eq!(kept.len(), 2);
        let c_entry = kept.iter().find(|c| c.id == "c").unwrap();
        // depends_on `b` was dropped because b is disabled.
        assert_eq!(c_entry.depends_on, vec!["a".to_string()]);
    }
}
