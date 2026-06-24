//! BB #9 — Pipeline Catalog.
//!
//! YAML loader + schema validation at load + precondition predicate DSL.
//!
//! ## Wave 0 scaffold; Wave 2 implements.
//!
//! Wave 2 design decisions (the "FIRST design choice" per ultra-pass U8):
//! 1. **Predicate DSL** — JSONPath against hot-store snapshot vs named-
//!    predicate-registry (Rust closures registered by name). JSONPath chosen
//!    for MVP (operator declares e.g., `overlay.active_work.contains(...)`
//!    expressions in YAML; no Rust changes needed when adding preconditions).
//!    Resolves BROKER-SPEC-GAPS.md gap #7.
//! 2. **Cross-broker sub_pipeline rejection** — loader rejects any
//!    `sub_pipeline:` whose target starts with a different broker_id than the
//!    catalog being loaded. Error: "cross-broker composition requires BB #27,
//!    not yet in MVP." Resolves U12 ultra-pass.
//! 3. **JSON Schema validation** — `params` field is validated as JSON Schema
//!    at load + against dispatch-time values.

use crate::pipeline::Pipeline;
use std::path::Path;

/// Load a per-broker pipeline catalog from a YAML file.
/// Wave 2 fleshes this out with validation + DSL parsing.
pub fn load_catalog(_path: &Path) -> anyhow::Result<Vec<Pipeline>> {
    Err(anyhow::anyhow!(
        "catalog::load_catalog not yet implemented (Wave 2)"
    ))
}

/// Validate a pipeline catalog against the broker's id (rejects cross-broker
/// `sub_pipeline:` references per U12 ultra-pass).
/// Wave 2 implements.
pub fn validate_catalog(_pipelines: &[Pipeline], _broker_id: &str) -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        "catalog::validate_catalog not yet implemented (Wave 2)"
    ))
}
