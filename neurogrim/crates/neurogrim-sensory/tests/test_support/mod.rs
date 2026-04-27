//! Shared helpers for `neurogrim-sensory` integration tests.
//!
//! Rust compiles each `.rs` file directly under `tests/` as a separate
//! test binary, but **subdirectories** under `tests/` are NOT
//! considered test binaries. Putting shared code at
//! `tests/test_support/mod.rs` lets each integration-test file
//! pull it in via `mod test_support;` without spinning up a phantom
//! "test_support" binary in the test runner.
//!
//! 2026-04-26 PRE-RELEASE Round 3 R3-3 fix (D3-W1): extracted the
//! identical `locate_cmdb_schema()` + `load_schema()` helpers
//! previously duplicated in both `sensor_behavior.rs:28-45` and
//! `schema_conformance.rs:32-53`. Drift-prevention: future schema-
//! path layout changes update one place, not two.

use jsonschema::JSONSchema;
use serde_json::Value;
use std::path::PathBuf;

/// Locate `cmdb-envelope-v1.schema.json` by walking known repo
/// layouts:
///
/// 1. Ecosystem layout: `<repo>/NeuroGrim/neurogrim/crates/neurogrim-sensory/`
///    → `<repo>/LSP-Brains/schemas/cmdb-envelope-v1.schema.json`
/// 2. Sibling layout (two repos side-by-side):
///    `<parent>/NeuroGrim/...` → `<parent>/LSP-Brains/schemas/cmdb-envelope-v1.schema.json`
///
/// Returns `None` when the schema isn't reachable (standalone
/// checkout). Callers are expected to skip the schema check
/// rather than fail in that case (matches the per-test pattern
/// established by Round 1 + Round 2 work).
pub fn locate_cmdb_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        // Ecosystem layout: .../NeuroGrim/neurogrim/crates/neurogrim-sensory/
        manifest_dir.join("../../../../LSP-Brains/schemas/cmdb-envelope-v1.schema.json"),
        // Standalone-sibling: .../NeuroGrim/neurogrim/crates/neurogrim-sensory/
        manifest_dir.join("../../../LSP-Brains/schemas/cmdb-envelope-v1.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Load + compile the CMDB envelope schema, if reachable. Wraps
/// `locate_cmdb_schema` + JSON parse + Draft-7 compile into a
/// single `Option<JSONSchema>` so callers can pattern-match.
pub fn load_schema() -> Option<JSONSchema> {
    let path = locate_cmdb_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}
