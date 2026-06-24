//! BB #1 — Broker capsule.
//!
//! The `Broker` trait every broker implements. Defines the contract surface
//! between framework + broker authors. Wave 1 fleshes this out per
//! BROKER-SPEC-GAPS.md gap #1 resolution.
//!
//! ## Trait shape (Wave 0 sketch; Wave 1 finalizes)
//!
//! ```ignore
//! #[async_trait::async_trait]
//! pub trait Broker: Send + Sync {
//!     type OverlayShape: Send + Sync + serde::Serialize;
//!     type WorkingState: Send;
//!
//!     /// Returns the consumer-facing read-only Overlay state.
//!     /// Per BROKER-CONTRACT.md §"The Overlay contract": atomic-swap,
//!     /// no-torn-read, versioned.
//!     async fn read_overlay(&self) -> Self::OverlayShape;
//!
//!     /// Returns the currently-legal pipelines per BROKER-CONTRACT.md
//!     /// central invariant.
//!     async fn legal_pipelines(&self, state: &Self::WorkingState)
//!         -> Vec<crate::Pipeline>;
//!
//!     /// Returns the governance-pipelines sidecar per §4 reachability
//!     /// channel split (LB-3 closure).
//!     async fn governance_pipelines(&self) -> Vec<crate::Pipeline>;
//!
//!     /// Tick handler. Brokers re-project Overlays in response to world
//!     /// events (per BB #15 Tick Source — MVP uses PostToolUse hook).
//!     async fn tick(&mut self, event: WorldEvent) -> Result<(), BrokerError>;
//!
//!     /// Role-set declaration per BROKER-CONTRACT.md role-set framing.
//!     fn role_set(&self) -> RoleSet;
//! }
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// World events that drive broker tick re-projection. MVP shape; Wave 1
/// finalizes per BROKER-SPEC-GAPS.md gap #4 resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEvent {
    /// Bus topic the event arrived on (per BB #4 queue conventions).
    pub topic: String,
    /// Payload (typed at the consuming broker's discretion).
    pub payload: serde_json::Value,
    /// ISO 8601 timestamp.
    pub ts: String,
}

/// Role-set per BROKER-CONTRACT.md §"Broker roles".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleSet {
    pub roles: Vec<Role>,
}

/// One of the three architectural role classes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Sense,
    InnateAbility,
    Embodiment,
}

/// Errors brokers can produce. Wave 1 expands this enum per spec gaps.
#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("broker not initialized: {0}")]
    NotInitialized(String),

    #[error("cold-store unavailable: {0}")]
    ColdStoreUnavailable(String),

    #[error("tick handling failed: {0}")]
    TickFailed(String),

    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

/// Marker trait the actual `Broker` definition lands on in Wave 1.
/// This placeholder exists so consuming crates can begin to import
/// `neurogrim_brokers::Broker` ahead of Wave 1 without breaking compilation
/// when Wave 1 lands the real trait.
pub trait Broker: Send + Sync {
    fn role_set(&self) -> RoleSet;
}
