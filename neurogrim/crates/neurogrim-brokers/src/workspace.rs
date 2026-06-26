//! A.0.2 — `WorkspaceBroker` substrate trait.
//!
//! Per `docs/BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md` Gate 1 (U1),
//! the trait is intentionally thin — concrete impls (A.1) bring overlay,
//! pipelines, LocalAwareness wiring, etc. The substrate just defines what
//! "workspace broker" MEANS at the type level:
//!
//! 1. It's a [`Broker`] (dispatchable via the standard `BrokerHost`).
//! 2. It's [`Extensible`] (operators can extend with TOML configs at
//!    `<cluster_manifest_dir>/extensions/workspace/*.toml`).
//! 3. It exposes its workspace's project root (the lay-of-the-land anchor).
//! 4. It declares the canonical 14-pipeline V1 surface every workspace
//!    broker must support (so the agent's onboarding query set is stable
//!    across consumers).
//!
//! ## V1 vs V2 scope (per the plan)
//!
//! V1 (this trait, [Sense + InnateAbility]): agent onboarding cache,
//! operator-curated knowledge, LocalAwareness absorption, agent-recorded
//! learnings. **In scope.**
//!
//! V2 Workspace Manager [Embodiment]: coordinates Effector brokers,
//! owns the Workspace Queue, requires the full LLM harness. **Out of
//! scope** — sequenced post-V1; the V1 broker is forward-compatible
//! either way (V2 absorbs V1 OR sits adjacent to it).
//!
//! ## Canonical pipeline IDs
//!
//! Every WorkspaceBroker MUST register pipelines matching the 14 IDs in
//! [`WorkspaceBroker::canonical_pipeline_ids`]. This is a contract: agents
//! discover workspace facts by dispatching these well-known IDs, so the
//! API stays uniform across Brains. Per-Brain impls MAY add extra
//! pipelines (e.g., the IDE adds pane / window pipelines); the canonical
//! 14 are the floor.

use crate::broker::Broker;
use crate::extension::Extensible;
use std::path::Path;

/// Substrate trait every workspace broker implements. Concrete impls
/// (NeuroGrim's own, the ecosystem Brain's, cereGrim's eventually) compose
/// this with their own [`Broker`] + [`Extensible`] implementations.
///
/// V1 [Sense + InnateAbility] — agent onboarding cache.
pub trait WorkspaceBroker: Broker + Extensible {
    /// Project root this workspace broker is rooted at. Used by other
    /// brokers' leaf-ops (e.g., sensory brokers reading CMDBs) when they
    /// need to know "where am I."
    fn project_root(&self) -> &Path;

    /// The 14 canonical pipeline IDs every V1 workspace broker registers.
    /// Per-Brain concrete impls compose this list with any consumer-
    /// specific additions in their `legal_pipelines()` return.
    ///
    /// Ordering: alphabetical for stability across implementations.
    fn canonical_pipeline_ids() -> &'static [&'static str]
    where
        Self: Sized,
    {
        &[
            // Sense pipelines (Internal — agent reads, doesn't dispatch)
            "workspace/get-active-processes",
            "workspace/get-build-invariants",
            "workspace/get-capability-profile",
            "workspace/get-current-focus",
            "workspace/get-path-conventions",
            "workspace/get-terminal-profile",
            "workspace/get-wip-state",
            "workspace/list-child-projects",
            // LocalAwareness facet pipelines (Surfaced — facts/notes mutation)
            "workspace/add-note",
            "workspace/remove-fact",
            "workspace/set-fact",
            // Agent-contribution InnateAbility pipelines (Surfaced)
            "workspace/record-active-process",
            "workspace/record-terminal-recommendation",
            "workspace/update-focus",
        ]
    }
}

// NOTE: trait-default-method tests run at A.1 when WorkspaceBrokerV1 ships
// as the first concrete impl. The trait surface here is intentionally
// minimal (one fn + one default associated fn); meaningful tests require
// a concrete broker that satisfies the Broker + Extensible supertraits.
