//! # neurogrim-brokers
//!
//! Broker framework for NeuroGrim — deterministic-dispatch substrate for agent
//! harnesses. Implements the ~12-of-38 "useful MVP" subset of the building
//! blocks documented in `BROKER-INTERNALS.md` §3.
//!
//! ## Scope (Wave 0 scaffold)
//!
//! This crate is the S*-T (Terminal-Primary) MVP — Claude Code as Primary
//! substitute, deferring the Meta lobe + local-LLM hosting. See the plan at
//! `C:/Users/koff0/.claude/plans/for-your-new-session-modular-pretzel.md` for
//! the full design.
//!
//! **In MVP scope:** Broker trait (#1), Overlay (#2a/#2b), Pipeline + Step
//! (#7/#8), Catalog (#9), Pipeline Runner (#10, single-tick), Trace Sink (#12),
//! Broker Registry (#14), Governance Composer (#19, 4 default pipelines),
//! Hot-Store Materializer (#22), Materializer Composer (#22a), Awareness
//! Materializer (#24).
//!
//! **Deferred (post-MVP):** Workflow Engine (#11), Replay (#13), Tick Source
//! (#15) — PostToolUse hook substitutes for MVP, Skill Filter (#20), Frame
//! stack (#35), Topology Broker (#17), and the Phase 8 additions (#25-#38).
//!
//! ## Module structure
//!
//! - [`broker`] — BB #1: the `Broker` trait every broker implements
//! - [`overlay`] — BB #2a `Overlay<T>` + BB #2b `WorkingState<W>`
//! - [`pipeline`] — BB #7 `Pipeline` + BB #8 `Step`
//! - [`catalog`] — BB #9: YAML loader + precondition predicate DSL
//! - [`runner`] — BB #10: single-tick Pipeline Runner
//! - [`trace`] — BB #12: minimal JSONL trace sink with snapshot deltas
//! - [`governance`] — BB #19: 4 framework-provided governance pipelines
//! - [`materializer`] — BB #22 + #22a + #24: materializers + composer
//! - [`registry`] — BB #14: Broker Registry (loads brokers from cluster manifest)

pub mod broker;
pub mod cold_store;
pub mod overlay;
pub mod pipeline;
pub mod catalog;
pub mod runner;
pub mod trace;
pub mod governance;
pub mod materializer;
pub mod registry;
pub mod work_broker;

// Re-exports for downstream consumers
pub use broker::{Broker, BrokerError, Role, RoleSet, WorldEvent};
pub use catalog::{evaluate_precondition, load_catalog, validate_catalog, CatalogError};
pub use cold_store::{ColdStore, ColdStoreError, JsonlColdStore};
pub use governance::{GovernanceComposer, GovernanceRefusal, SharedGovernance};
pub use materializer::{
    awareness::AwarenessMaterializer, hot_store::HotStoreMaterializer, MaterializerComposer,
    MaterializerError,
};
pub use overlay::{Overlay, OverlayReadGuard, WorkingState};
pub use pipeline::{
    AuditClass, EffectClass, ParamMap, Pipeline, PipelineId, Step, Tunability, Visibility,
};
pub use registry::{
    BrokerConfig, BrokerManifest, BrokerRegistry, ClusterConfig, ClusterManifest, RegistryError,
};
pub use runner::{DispatchError, DispatchOutcome, LeafContext, LeafError, PipelineRunner};
pub use trace::{SnapshotDelta, TraceError, TraceRecord, TraceSink};
pub use work_broker::{ActiveWorkOverlay, BacklogState, WorkBroker, WorkUnit, WorkUnitStatus};

// Re-export major errors for ergonomic consumer error handling
pub use anyhow::{Error, Result};
