//! BB #24 — Awareness Materializer.
//!
//! Writes per-broker pipeline catalog routing signals
//! (description + when_to_use + currently-legal status + **per-pipeline
//! parameter schema** per ultra-pass U1) to
//! `.claude/brain/broker/segments/awareness-routing-<broker_id>.md`.
//!
//! ## MVP scope (Wave 3)
//!
//! - Stub ranking policy (operator-declared order from cluster manifest; no
//!   Autonomous tuning per ultra-pass U3). BB #20 Skill Filter lands properly
//!   when 2nd broker arrives in S1-T.
//! - **Per-pipeline parameter schema MUST be surfaced** (ultra-pass U1 closure)
//!   so the agent has the schema needed to call the single
//!   `dispatch_pipeline` MCP tool with valid params.
//! - ≤1,536 char per pipeline routing signal (per BROKER-AWARENESS.md §1).

use crate::pipeline::Pipeline;
use std::path::PathBuf;

pub struct AwarenessMaterializer {
    _broker_id: String,
    _segment_path: PathBuf,
}

impl AwarenessMaterializer {
    pub fn new(broker_id: String, segment_path: PathBuf) -> Self {
        Self {
            _broker_id: broker_id,
            _segment_path: segment_path,
        }
    }

    /// Materialize the broker's pipeline routing signals to its segment file.
    /// Wave 3 implements.
    pub fn materialize(&self, _pipelines: &[Pipeline]) -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "AwarenessMaterializer::materialize not yet implemented (Wave 3)"
        ))
    }
}
