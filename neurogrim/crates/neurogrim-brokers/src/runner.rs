//! BB #10 — Pipeline Runner.
//!
//! Single-tick executor for MVP. No suspension; no Workflow Engine (BB #11
//! deferred to S1-T per plan scope).
//!
//! ## Wave 0 scaffold; Wave 2 implements.
//!
//! Wave 2 implementation:
//! 1. Receive a dispatch request (pipeline_id + params).
//! 2. Catalog lookup (BB #9).
//! 3. Validate params against schema.
//! 4. Evaluate preconditions against hot-store snapshot.
//! 5. Compose governance pipelines (BB #19; Wave 4) if Surfaced.
//! 6. Execute step sequence (call leaf-ops; recurse into intra-broker
//!    sub-pipelines).
//! 7. Record trace (BB #12).
//! 8. Return dispatch outcome.

use crate::pipeline::{ParamMap, PipelineId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("pipeline not found: {0}")]
    PipelineNotFound(PipelineId),

    #[error("parameter validation failed: {field}: {reason}")]
    ParamValidation { field: String, reason: String },

    #[error("precondition not met: {0}")]
    PreconditionUnmet(String),

    #[error("governance refused dispatch: {failure_reason}")]
    GovernanceRefused { failure_reason: String },

    #[error("leaf-op failed: {leaf_op}: {reason}")]
    LeafOpFailed { leaf_op: String, reason: String },

    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

/// Dispatch outcome (returned from Runner to the consumer).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DispatchOutcome {
    /// Trace ID linking to the trace-sink record (BB #12).
    pub trace_id: String,
    /// Pipeline's structured output (per its EffectClass).
    pub output: serde_json::Value,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Pipeline Runner (Wave 2 implements).
/// MVP-scope: single-tick execution; no suspension; no Workflow Engine.
pub struct PipelineRunner {
    // Wave 2 adds: catalog, governance composer, trace sink, etc.
}

impl PipelineRunner {
    pub fn new() -> Self {
        Self {}
    }

    /// Dispatch a pipeline. Wave 2 implements the full pipeline:
    /// catalog lookup → param validation → preconditions → governance →
    /// step execution → trace record.
    pub async fn dispatch(
        &self,
        _pipeline_id: PipelineId,
        _params: ParamMap,
    ) -> Result<DispatchOutcome, DispatchError> {
        Err(DispatchError::Other(anyhow::anyhow!(
            "runner::dispatch not yet implemented (Wave 2)"
        )))
    }
}

impl Default for PipelineRunner {
    fn default() -> Self {
        Self::new()
    }
}
