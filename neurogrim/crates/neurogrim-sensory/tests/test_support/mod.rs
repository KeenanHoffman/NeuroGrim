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

/// Locate `agent-output-v1.schema.json` via the same repo-layout
/// candidates as `locate_cmdb_schema`. Returns `None` when the
/// schema isn't reachable (standalone checkout).
///
/// Added in E-B2-1 C7 alongside the schema's relax + add
/// (additionalProperties: false → true; +unified_confidence) so
/// agent-output schema conformance can be tested in lockstep with
/// cmdb-envelope conformance.
pub fn locate_agent_output_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../../LSP-Brains/schemas/agent-output-v1.schema.json"),
        manifest_dir.join("../../../LSP-Brains/schemas/agent-output-v1.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Load + compile the agent-output v1 schema, if reachable.
pub fn load_agent_output_schema() -> Option<JSONSchema> {
    let path = locate_agent_output_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}

/// Locate `domain-calibration-ledger-v1.schema.json` via the same
/// repo-layout candidates as the cmdb + agent-output helpers. Returns
/// `None` when the schema isn't reachable (standalone checkout).
///
/// Added in E-B2-2 C1 alongside the new unified calibration-ledger
/// schema. Same skip-when-absent convention as the other helpers.
pub fn locate_calibration_ledger_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../../LSP-Brains/schemas/domain-calibration-ledger-v1.schema.json"),
        manifest_dir.join("../../../LSP-Brains/schemas/domain-calibration-ledger-v1.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Load + compile the domain-calibration-ledger v1 schema, if reachable.
pub fn load_calibration_ledger_schema() -> Option<JSONSchema> {
    let path = locate_calibration_ledger_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}

/// Locate `hat-contract-v1.schema.json` via the same repo-layout
/// candidates as the other helpers. Returns `None` when the schema
/// isn't reachable (standalone checkout).
///
/// Added in E-B2-3 C1 alongside the new persona-hat contract schema.
/// Persona-hat (Hat-model B per spec §5.4.1, Q5 = 5c) — distinct from
/// the registry-hat schema (Hat-model A; embedded in
/// `brain-registry-v2.schema.json:413-433`).
pub fn locate_hat_contract_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../../LSP-Brains/schemas/hat-contract-v1.schema.json"),
        manifest_dir.join("../../../LSP-Brains/schemas/hat-contract-v1.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Load + compile the hat-contract v1 schema, if reachable.
pub fn load_hat_contract_schema() -> Option<JSONSchema> {
    let path = locate_hat_contract_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}

/// Read the raw JSON of `hat-contract-v1.schema.json` (uncompiled).
/// Used by closed-set discipline pinning tests that need to inspect
/// the schema's `definitions.ToolName.enum` directly rather than
/// validate against it.
pub fn read_hat_contract_schema_value() -> Option<Value> {
    let path = locate_hat_contract_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Locate a hat-contract fixture file by name. Fixtures live at
/// `<crate>/tests/fixtures/<name>` and are crate-local (unlike schemas,
/// which live in the sibling LSP-Brains repo). Always reachable.
///
/// Added in E-B2-3 C3 alongside the three pre-canned hat-contract
/// fixtures (valid, invalid-vocabulary, missing-frontmatter).
pub fn locate_hat_contract_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Read a hat-contract fixture file as a single string. Caller decides
/// how to interpret it (split frontmatter, etc.).
pub fn read_hat_contract_fixture(name: &str) -> String {
    let path = locate_hat_contract_fixture(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

/// Locate `trust-budget-v1.schema.json` via the same repo-layout
/// candidates as the other helpers. Returns `None` when the schema
/// isn't reachable (standalone checkout).
///
/// Added in E-B2-4 C1+C2 alongside the new per-Brain trust-budget
/// schema. Mirrors the cross-repo `include_str!` ordering risk
/// (E4-9): test skips with `eprintln` when the LSP-Brains submodule
/// pointer hasn't been bumped yet.
pub fn locate_trust_budget_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../../LSP-Brains/schemas/trust-budget-v1.schema.json"),
        manifest_dir.join("../../../LSP-Brains/schemas/trust-budget-v1.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Load + compile the trust-budget v1 schema, if reachable.
pub fn load_trust_budget_schema() -> Option<JSONSchema> {
    let path = locate_trust_budget_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}

/// Read the raw JSON of `trust-budget-v1.schema.json` (uncompiled).
/// Used by closed-set discipline pinning tests that need to inspect
/// `definitions.Ecosystem.enum` and `definitions.TrustPosture.enum`
/// directly rather than validate against them.
pub fn read_trust_budget_schema_value() -> Option<Value> {
    let path = locate_trust_budget_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Locate a trust-budget fixture file by name. Fixtures live at
/// `<crate>/tests/fixtures/<name>` and are crate-local. Always
/// reachable; panics if not found (the four E-B2-4 C2 fixtures
/// are committed as part of the same chunk that ships this helper).
pub fn locate_trust_budget_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Locate `invocation-ledger-v1.schema.json` via the same repo-layout
/// candidates as the other helpers. Returns `None` when the schema
/// isn't reachable (standalone checkout).
///
/// Added in E-B2-6 C1+C2 alongside the new invocation-ledger schema
/// (the first-ever schema for the `.claude/brain/invocation-ledger.jsonl`
/// ledger; previously the format was defined only by
/// `record-skill-invocation.sh:56-58`'s fixed printf line + the prose
/// at `docs/invocation-ledger.md:26-39`). Mirrors the cross-repo
/// `include_str!` ordering risk: test skips with `eprintln` when the
/// LSP-Brains submodule pointer hasn't been bumped yet.
pub fn locate_invocation_ledger_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../../LSP-Brains/schemas/invocation-ledger-v1.schema.json"),
        manifest_dir.join("../../../LSP-Brains/schemas/invocation-ledger-v1.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Load + compile the invocation-ledger v1 schema, if reachable.
pub fn load_invocation_ledger_schema() -> Option<JSONSchema> {
    let path = locate_invocation_ledger_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}

/// Read the raw JSON of `invocation-ledger-v1.schema.json` (uncompiled).
/// Used by the closed-set discipline pinning test that inspects
/// `definitions.Disposition.enum` directly rather than validating
/// against it.
pub fn read_invocation_ledger_schema_value() -> Option<Value> {
    let path = locate_invocation_ledger_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Locate an invocation-ledger fixture file by name. Fixtures live at
/// `<crate>/tests/fixtures/<name>` and are crate-local (unlike schemas,
/// which live in the sibling LSP-Brains repo). Always reachable;
/// panics if not found (the six E-B2-6 C2 fixtures are committed as
/// part of the same chunk that ships this helper).
pub fn locate_invocation_ledger_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Locate `brain-registry-v2.schema.json` via the same repo-layout
/// candidates as the other helpers. Returns `None` when the schema
/// isn't reachable (standalone checkout).
///
/// Added in E-B2-2 C5 alongside the new optional `calibration_trigger`
/// discriminated-union field + `enable_calibration_writes` config gate.
pub fn locate_brain_registry_schema() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../../LSP-Brains/schemas/brain-registry-v2.schema.json"),
        manifest_dir.join("../../../LSP-Brains/schemas/brain-registry-v2.schema.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Load + compile the brain-registry v2 schema, if reachable.
pub fn load_brain_registry_schema() -> Option<JSONSchema> {
    let path = locate_brain_registry_schema()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&value)
        .ok()
}

/// Locate a calibration-ledger fixture file by name. Fixtures live at
/// `<crate>/tests/fixtures/<name>` and are crate-local (unlike schemas,
/// which live in the sibling LSP-Brains repo). Always reachable.
///
/// Added in E-B2-2 C8 alongside the three pre-canned ledger fixtures
/// (pending-only, pending+triaged, malformed).
pub fn locate_calibration_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Read all lines of a calibration-ledger fixture as raw strings.
/// Caller decides how to interpret them (parse vs skip on malformed).
pub fn read_calibration_fixture_lines(name: &str) -> Vec<String> {
    let path = locate_calibration_fixture(name);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|s| s.to_string())
        .collect()
}
