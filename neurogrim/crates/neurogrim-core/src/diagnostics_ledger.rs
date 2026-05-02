//! Append-only JSONL ledger for diagnostics events
//! (V5-FOUND-1 Phase 1).
//!
//! Implements the writer + reader for `diagnostics-ledger-v1.schema.json`.
//! One row per timed operation across a closed-set of `kind`s
//! (build, test, cargo, scoring, mcp_dispatch, a2a_post, a2a_sse,
//! dashboard_route, diag_synthesis). See the schema for the full
//! contract and the V5-FOUND-1 plan
//! (`.claude/plans/v5-found-1-diagnostic-monitor.md`) for design
//! rationale.
//!
//! # Privacy floor
//!
//! The ledger is gitignored runtime state and **never** carries
//! free-text from prompts, tool args, peer payloads, request
//! bodies, or operator notes. Two layers of structural
//! enforcement:
//!
//! 1. Schema: `additionalProperties: false` on the entry root and
//!    the `extras` object; the union of allowed `extras` properties
//!    is documented per-kind.
//! 2. Writer: [`validate_entry`] rejects any `extras` key in
//!    [`FORBIDDEN_EXTRAS_KEYS`] (the negative list — `prompt`,
//!    `args`, `payload`, `text`, `body`, `note`, `comment`,
//!    `reason`) AND rejects any `extras` key not in the
//!    per-kind positive list returned by [`allowed_extras_for`].
//!
//! Re-opening the privacy floor (e.g., adding free-text fields to
//! the ledger) requires an explicit charter-level conversation,
//! mirroring the invocation-ledger discipline at
//! `docs/invocation-ledger.md:26-39`.
//!
//! # Append-only / atomicity
//!
//! Each [`append`] call performs a single
//! `OpenOptions::create(true).append(true)`-write of one line +
//! newline. Writes ≤ PIPE_BUF (4KB on POSIX) are atomic; on
//! Windows, append-mode opens use `FILE_APPEND_DATA` access right
//! with the same atomicity guarantee for short writes. A single
//! diagnostics entry serializes well under that bound (~300 bytes
//! typical), so concurrent writers from multiple CLI invocations
//! cannot interleave a single entry. Same discipline as
//! [`crate::calibration_ledger`] and the
//! `record-skill-invocation.sh` PostToolUse hook.
//!
//! # Phase 1 posture
//!
//! V5-FOUND-1 Phase 1 ships the writer, reader, and schema. The
//! tracing-subscriber Layer that produces entries from spans is
//! Phase 2; the actual span instrumentation across crates is
//! Phase 3. Phase 1's writer can be called directly for tests
//! and operator-facing tooling; production-path emission goes
//! through the Phase 2 Layer.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// Forbidden keys in `extras` — the structural privacy floor.
///
/// Any entry whose `extras` map contains one of these keys is
/// rejected at write time by [`validate_entry`]. Re-opening this
/// list requires a charter-level conversation.
pub const FORBIDDEN_EXTRAS_KEYS: &[&str] = &[
    "prompt", "args", "payload", "text", "body", "note", "comment", "reason",
];

/// Default ledger filename relative to a project root's
/// `.claude/brain/` directory. Callers compose the full path via
/// [`default_ledger_path`].
pub const DIAGNOSTICS_LEDGER_FILENAME: &str = "diagnostics.jsonl";

/// Compose the canonical ledger path under a project root:
/// `<project_root>/.claude/brain/diagnostics.jsonl`.
pub fn default_ledger_path(project_root: &Path) -> std::path::PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join(DIAGNOSTICS_LEDGER_FILENAME)
}

/// Closed-set discriminator for the surface this event was
/// emitted from. Adding a variant is a v2 schema bump and must
/// also extend [`allowed_extras_for`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Reserved for cargo-build instrumentation; not yet wired.
    Build,
    /// `neurogrim test` end-to-end run.
    Test,
    /// Child cargo subprocess invocation under a test span.
    Cargo,
    /// `run_scoring` end-to-end path in neurogrim-mcp.
    Scoring,
    /// Per-server MCP sensory dispatch (per-server granularity in
    /// V5-FOUND-1 per plan-critic; per-tool deferred to v5.5).
    McpDispatch,
    /// A2A inbound POST handling.
    A2aPost,
    /// A2A SSE event emission.
    A2aSse,
    /// Dashboard axum route handler.
    DashboardRoute,
    /// Reserved for V5-FOUND-1.1 (`neurogrim diag synthesize`).
    DiagSynthesis,
}

impl EventKind {
    /// Stable wire-name string matching the schema enum.
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::Build => "build",
            EventKind::Test => "test",
            EventKind::Cargo => "cargo",
            EventKind::Scoring => "scoring",
            EventKind::McpDispatch => "mcp_dispatch",
            EventKind::A2aPost => "a2a_post",
            EventKind::A2aSse => "a2a_sse",
            EventKind::DashboardRoute => "dashboard_route",
            EventKind::DiagSynthesis => "diag_synthesis",
        }
    }
}

/// Closed-set outcome enum. Adding a variant is a v2 schema bump.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Err,
    Timeout,
    Cancelled,
}

impl Outcome {
    /// Parse a wire-string into the typed enum. Used by the
    /// V5-FOUND-1 Phase 2 tracing Layer to interpret an `outcome`
    /// span field (set by instrumented code via `span.record(
    /// "outcome", "err")`). Returns `None` for unknown values; the
    /// Layer falls back to `Ok` in that case.
    pub fn from_str(s: &str) -> Option<Outcome> {
        match s {
            "ok" => Some(Outcome::Ok),
            "err" => Some(Outcome::Err),
            "timeout" => Some(Outcome::Timeout),
            "cancelled" => Some(Outcome::Cancelled),
            _ => None,
        }
    }
}

/// One diagnostics-ledger entry. Mirrors
/// `diagnostics-ledger-v1.schema.json` exactly.
///
/// Construct via [`DiagnosticsEntry::new`] or fields directly;
/// always validate via [`validate_entry`] (called by [`append`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticsEntry {
    /// Schema version. v1 is locked at 1.
    pub schema_version: u32,
    /// UUID-v4 unique to this event.
    pub event_id: String,
    /// ISO 8601 UTC timestamp at span enter.
    pub ts_start: String,
    /// Span duration in whole milliseconds.
    pub duration_ms: u64,
    /// Closed-set surface discriminator.
    pub kind: EventKind,
    /// Span name within the kind (stable identifier).
    pub name: String,
    /// Closed-set outcome.
    pub outcome: Outcome,
    /// Span nesting depth (0 = top-level).
    pub depth: u32,
    /// Parent span's `event_id`, or null for top-level spans.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent_event_id: Option<String>,
    /// Closed-set positive list per `kind`, validated at write
    /// time. Values must be JSON primitives or short closed-set
    /// strings — never free text.
    pub extras: BTreeMap<String, serde_json::Value>,
}

impl DiagnosticsEntry {
    /// Construct a new entry with `schema_version=1`, a fresh
    /// UUID-v4, and `parent_event_id=None`. Caller fills in
    /// `ts_start`, `duration_ms`, `kind`, `name`, `outcome`,
    /// `depth`, and `extras`.
    ///
    /// Equivalent to constructing the struct field-by-field; this
    /// helper just centralizes the schema_version + event_id
    /// defaults so callers don't repeat them.
    pub fn new(
        ts_start: impl Into<String>,
        duration_ms: u64,
        kind: EventKind,
        name: impl Into<String>,
        outcome: Outcome,
        depth: u32,
        extras: BTreeMap<String, serde_json::Value>,
    ) -> Self {
        DiagnosticsEntry {
            schema_version: 1,
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_start: ts_start.into(),
            duration_ms,
            kind,
            name: name.into(),
            outcome,
            depth,
            parent_event_id: None,
            extras,
        }
    }
}

/// Per-kind allowed keys in `extras`. Synced with the schema's
/// `extras.properties` documentation. Adding a key for a kind is
/// **not** a schema bump (the union is already in the schema), but
/// adding a key not yet in the schema's `extras.properties` IS a
/// schema bump.
pub fn allowed_extras_for(kind: EventKind) -> &'static [&'static str] {
    match kind {
        EventKind::Build => &["target"],
        EventKind::Test => &["test_count", "fail_count", "ignored_count"],
        EventKind::Cargo => &["cmd", "exit_code"],
        EventKind::Scoring => &["domains_count", "score", "confidence"],
        EventKind::McpDispatch => &["server_name", "tool_count", "fail_count"],
        EventKind::A2aPost => &["peer_id_hash", "status_code"],
        EventKind::A2aSse => &["peer_id_hash", "status_code"],
        EventKind::DashboardRoute => &["route_name", "status_code"],
        EventKind::DiagSynthesis => &["baseline_value_ms", "target_value_ms", "action_count"],
    }
}

/// Validate an entry against schema-equivalent intrinsic
/// constraints (privacy floor + per-kind extras + non-empty name +
/// schema_version=1). Called by [`append`] before writing.
pub fn validate_entry(entry: &DiagnosticsEntry) -> Result<()> {
    if entry.schema_version != 1 {
        bail!(
            "diagnostics ledger: schema_version must be 1; got {}",
            entry.schema_version
        );
    }
    if entry.name.is_empty() {
        bail!("diagnostics ledger: name must be non-empty");
    }
    let allowed = allowed_extras_for(entry.kind);
    for k in entry.extras.keys() {
        if FORBIDDEN_EXTRAS_KEYS.contains(&k.as_str()) {
            bail!(
                "diagnostics ledger: forbidden key '{}' in extras (privacy floor violation). \
                 Forbidden keys: {:?}",
                k,
                FORBIDDEN_EXTRAS_KEYS
            );
        }
        if !allowed.contains(&k.as_str()) {
            bail!(
                "diagnostics ledger: key '{}' not allowed in extras for kind={:?}; \
                 allowed: {:?}",
                k,
                entry.kind,
                allowed
            );
        }
    }
    Ok(())
}

/// Append a new entry to the ledger file at `path`. Validates
/// schema-equivalent intrinsic constraints before writing; creates
/// the file (and parent directory) if missing.
///
/// Atomic write semantics: single `OpenOptions::append`-write of
/// `line + '\n'`. Writes ≤ PIPE_BUF (4KB) are POSIX-atomic.
/// Windows append-mode opens have the same guarantee for short
/// writes via `FILE_APPEND_DATA`.
pub fn append(path: &Path, entry: &DiagnosticsEntry) -> Result<()> {
    validate_entry(entry).context("diagnostics ledger entry validation")?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("diagnostics ledger: create parent dir")?;
    }

    let line = serde_json::to_string(entry)
        .context("diagnostics ledger: serialize entry")?;
    if line.contains('\n') {
        bail!("diagnostics ledger entry must serialize to a single line; got multi-line JSON");
    }

    // Build line + newline as a single byte buffer and issue ONE
    // write_all. Calling writeln!(f, "{}", line) instead would
    // invoke write_fmt → multiple write_str calls (one per format
    // segment), breaking the atomic-append guarantee on Windows
    // (FILE_APPEND_DATA atomicity is per-write, not per-formatter-
    // invocation). Linux O_APPEND has the same per-write semantics;
    // a single write_all keeps both platforms safe.
    let mut payload = line.into_bytes();
    payload.push(b'\n');

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("diagnostics ledger: open {} for append", path.display()))?;
    f.write_all(&payload)
        .context("diagnostics ledger: write line")?;
    f.flush().context("diagnostics ledger: flush")?;
    Ok(())
}

/// Read the entire ledger from `path`. Returns empty Vec if the
/// file doesn't exist (first-run posture).
///
/// Malformed lines are logged + skipped, not propagated as errors —
/// the goal is to never block the reader on a bad ledger entry.
/// Same posture as [`crate::calibration_ledger::read_all`].
///
/// Lines whose `schema_version` is not 1 are also skipped (logged
/// at warn level). When v2 lands, this reader will need to be
/// updated to accept either version explicitly.
pub fn read_all(path: &Path) -> Result<Vec<DiagnosticsEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let f = File::open(path)
        .with_context(|| format!("diagnostics ledger: open {}", path.display()))?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for (i, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(
                    "diagnostics ledger: read error on line {}: {:#}",
                    i + 1,
                    e
                );
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<DiagnosticsEntry>(&line) {
            Ok(entry) => {
                if entry.schema_version != 1 {
                    tracing::warn!(
                        "diagnostics ledger: skipping line {} with schema_version {} (v1 reader)",
                        i + 1,
                        entry.schema_version
                    );
                    continue;
                }
                out.push(entry);
            }
            Err(e) => {
                tracing::warn!(
                    "diagnostics ledger: parse failed on line {} ({:#}); content: {}",
                    i + 1,
                    e,
                    line.chars().take(120).collect::<String>()
                );
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn sample_entry(kind: EventKind, extras: BTreeMap<String, serde_json::Value>) -> DiagnosticsEntry {
        DiagnosticsEntry::new(
            "2026-05-02T00:00:00Z".to_string(),
            42,
            kind,
            "score_pipeline.run".to_string(),
            Outcome::Ok,
            0,
            extras,
        )
    }

    #[test]
    fn happy_path_append_and_read() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let mut extras = BTreeMap::new();
        extras.insert("score".to_string(), json!(85.5));
        extras.insert("domains_count".to_string(), json!(12));
        extras.insert("confidence".to_string(), json!(90.0));
        let entry = sample_entry(EventKind::Scoring, extras);
        append(&path, &entry).expect("append should succeed");

        let read_back = read_all(&path).expect("read_all should succeed");
        assert_eq!(read_back.len(), 1);
        assert_eq!(read_back[0], entry);
    }

    #[test]
    fn rejects_forbidden_key_prompt() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let mut extras = BTreeMap::new();
        extras.insert("prompt".to_string(), json!("would be a privacy violation"));
        let entry = sample_entry(EventKind::Scoring, extras);
        let err = append(&path, &entry).expect_err("forbidden key must be rejected");
        let msg = format!("{:#}", err);
        assert!(msg.contains("forbidden key"), "unexpected error: {}", msg);
        assert!(msg.contains("prompt"), "should name the offending key: {}", msg);
        assert!(!path.exists(), "ledger file must not be created on rejection");
    }

    #[test]
    fn rejects_all_forbidden_keys() {
        // Sweep the full FORBIDDEN_EXTRAS_KEYS list to ensure each
        // is rejected. Catches drift if someone narrows the list.
        let dir = TempDir::new().unwrap();
        for forbidden in FORBIDDEN_EXTRAS_KEYS {
            let path = dir.path().join(format!("diag-{}.jsonl", forbidden));
            let mut extras = BTreeMap::new();
            extras.insert((*forbidden).to_string(), json!("content"));
            let entry = sample_entry(EventKind::Scoring, extras);
            let err = append(&path, &entry).unwrap_err_or_else_panic(forbidden);
            let msg = format!("{:#}", err);
            assert!(
                msg.contains(forbidden),
                "rejection message must name the offending key '{}': {}",
                forbidden,
                msg
            );
        }
    }

    /// Test helper: like `expect_err` but with a forbidden-key
    /// hint in the panic message so failures point at which sweep
    /// iteration regressed.
    trait UnwrapErrOrPanic<T, E> {
        fn unwrap_err_or_else_panic(self, forbidden_key: &str) -> E;
    }

    impl<T, E> UnwrapErrOrPanic<T, E> for Result<T, E> {
        fn unwrap_err_or_else_panic(self, forbidden_key: &str) -> E {
            match self {
                Ok(_) => panic!(
                    "forbidden key '{}' must be rejected but append returned Ok",
                    forbidden_key
                ),
                Err(e) => e,
            }
        }
    }

    #[test]
    fn rejects_unknown_extras_key_for_kind() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        // `tool_count` is allowed for mcp_dispatch but not for scoring.
        let mut extras = BTreeMap::new();
        extras.insert("tool_count".to_string(), json!(3));
        let entry = sample_entry(EventKind::Scoring, extras);
        let err = append(&path, &entry).expect_err("unknown extras key must be rejected");
        let msg = format!("{:#}", err);
        assert!(msg.contains("not allowed"), "unexpected error: {}", msg);
        assert!(msg.contains("tool_count"), "should name the offending key: {}", msg);
    }

    #[test]
    fn rejects_empty_name() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let mut entry = sample_entry(EventKind::Scoring, BTreeMap::new());
        entry.name = String::new();
        let err = append(&path, &entry).expect_err("empty name must be rejected");
        assert!(
            format!("{:#}", err).contains("name must be non-empty"),
            "unexpected error: {:#}",
            err
        );
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let mut entry = sample_entry(EventKind::Scoring, BTreeMap::new());
        entry.schema_version = 2;
        let err = append(&path, &entry).expect_err("wrong schema_version must be rejected");
        assert!(
            format!("{:#}", err).contains("schema_version must be 1"),
            "unexpected error: {:#}",
            err
        );
    }

    #[test]
    fn read_all_skips_malformed_and_wrong_version_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        // Write a valid v1 entry first.
        let entry = sample_entry(EventKind::Scoring, {
            let mut m = BTreeMap::new();
            m.insert("score".to_string(), json!(80.0));
            m
        });
        append(&path, &entry).unwrap();

        // Then append: a malformed line, an empty line, and a v2
        // future-schema-version line. The reader should skip all
        // three and return only the v1 entry.
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "this is not json").unwrap();
        writeln!(f).unwrap();
        writeln!(
            f,
            "{}",
            json!({
                "schema_version": 2,
                "event_id": "00000000-0000-4000-8000-000000000000",
                "ts_start": "2026-05-02T00:00:00Z",
                "duration_ms": 0,
                "kind": "scoring",
                "name": "future.event",
                "outcome": "ok",
                "depth": 0,
                "extras": {}
            })
        )
        .unwrap();
        drop(f);

        let read_back = read_all(&path).unwrap();
        assert_eq!(read_back.len(), 1, "only the v1 entry should survive");
        assert_eq!(read_back[0].name, "score_pipeline.run");
    }

    #[test]
    fn read_all_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist.jsonl");
        let read_back = read_all(&path).unwrap();
        assert!(read_back.is_empty());
    }

    #[test]
    fn concurrent_writers_produce_well_formed_lines() {
        // Two threads each append 100 entries; resulting ledger has
        // 200 well-formed lines, parseable by read_all.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");

        let path_a = path.clone();
        let path_b = path.clone();
        let handle_a = std::thread::spawn(move || {
            for i in 0..100 {
                let mut m = BTreeMap::new();
                m.insert("test_count".to_string(), json!(i));
                let entry = DiagnosticsEntry::new(
                    "2026-05-02T00:00:00Z".to_string(),
                    1,
                    EventKind::Test,
                    format!("test.thread_a.{}", i),
                    Outcome::Ok,
                    0,
                    m,
                );
                append(&path_a, &entry).unwrap();
            }
        });
        let handle_b = std::thread::spawn(move || {
            for i in 0..100 {
                let mut m = BTreeMap::new();
                m.insert("score".to_string(), json!(75.0));
                let entry = DiagnosticsEntry::new(
                    "2026-05-02T00:00:00Z".to_string(),
                    1,
                    EventKind::Scoring,
                    format!("scoring.thread_b.{}", i),
                    Outcome::Ok,
                    0,
                    m,
                );
                append(&path_b, &entry).unwrap();
            }
        });
        handle_a.join().unwrap();
        handle_b.join().unwrap();

        let read_back = read_all(&path).unwrap();
        assert_eq!(read_back.len(), 200, "all 200 entries must be readable");
    }

    #[test]
    fn allowed_extras_are_complete() {
        // Sanity: every variant of EventKind has at least one
        // allowed extras key (we don't ship a kind with zero
        // schema-documented extras at v1).
        let kinds = [
            EventKind::Build,
            EventKind::Test,
            EventKind::Cargo,
            EventKind::Scoring,
            EventKind::McpDispatch,
            EventKind::A2aPost,
            EventKind::A2aSse,
            EventKind::DashboardRoute,
            EventKind::DiagSynthesis,
        ];
        for k in kinds {
            assert!(
                !allowed_extras_for(k).is_empty(),
                "kind {:?} should have at least one allowed extras key",
                k
            );
        }
    }

    #[test]
    fn default_ledger_path_composes_correctly() {
        let p = default_ledger_path(Path::new("/tmp/myproj"));
        assert!(p.to_string_lossy().ends_with(".claude/brain/diagnostics.jsonl")
            || p.to_string_lossy().ends_with(".claude\\brain\\diagnostics.jsonl"));
    }

    #[test]
    fn parent_event_id_round_trips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("diagnostics.jsonl");
        let mut entry = sample_entry(EventKind::Cargo, {
            let mut m = BTreeMap::new();
            m.insert("cmd".to_string(), json!("test"));
            m.insert("exit_code".to_string(), json!(0));
            m
        });
        entry.parent_event_id = Some("11111111-1111-4111-8111-111111111111".to_string());
        append(&path, &entry).unwrap();
        let read_back = read_all(&path).unwrap();
        assert_eq!(read_back.len(), 1);
        assert_eq!(
            read_back[0].parent_event_id.as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
    }
}
