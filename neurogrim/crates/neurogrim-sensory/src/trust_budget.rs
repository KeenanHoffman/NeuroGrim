//! Trust-budget sensor — per-Brain trust-surface declaration validator.
//!
//! E-B2-4 C3 (2026-04-27). Static-only validator (per Q9 + Q1) that reads
//! `<project_root>/trust-budget.toml` (Q5 lock: repo root, committed),
//! validates it against `trust-budget-v1.schema.json` (LSP-Brains, embedded
//! at compile time via `include_str!`), then cross-references the declared
//! surface against the actual surface across three dimensions:
//!
//! - **Crates** — read via `supply_chain_sca::lockfile::cargo::parse` (Q2
//!   lock, no re-implementation). Filtered to direct dependencies only
//!   (Q1 lock); transitive crates are deferred to B-23. The workspace
//!   `Cargo.toml` is checked at `<root>/Cargo.toml` first; if absent we
//!   fall back to `<root>/neurogrim/Cargo.toml` (NeuroGrim's two-level
//!   layout where the workspace lives under a `neurogrim/` subdirectory).
//! - **Shell-outs** — `std::fs::read_dir` + `std::fs::read_to_string`
//!   over `<project_root>/scripts/*.sh` and `*.ps1` (Q1 lock — zero new
//!   deps; no `walkdir`). The scanner runs a two-pass discipline: pass 1
//!   collects every locally-defined bash function (`name() { ... }`);
//!   pass 2 emits only first-word invocations that are NOT defined in
//!   the same script and that match a closed-set vocabulary of real
//!   external commands. Heuristic by design — the goal is "no obvious
//!   non-commands" rather than "perfect coverage."
//! - **External services** — best-effort scan of `https://` URLs across
//!   `*.rs`, `*.sh`, `*.py` files under `<project_root>` (and the
//!   workspace under `<project_root>/neurogrim/` when present). Skips
//!   build output (`target/`), version-control internals (`.git/`),
//!   and test fixtures (`tests/fixtures/`). The operator's declaration
//!   is the source of truth; the actual surface is a sanity-check.
//!
//! All findings emit `points: 0` (advisory per Q4). Two finding kinds per
//! surface (per Q8):
//!
//! - `trust_budget:undeclared:<surface>:<item>` — actual surface item not
//!   in declared (operator forgot to update `trust-budget.toml`).
//! - `trust_budget:overdeclared:<surface>:<item>` — declared item not in
//!   actual (declaration is stale).
//!
//! Plus three meta-finding kinds:
//!
//! - `trust_budget:declaration:missing` — no `trust-budget.toml` at root.
//! - `trust_budget:declaration:malformed` — file exists but TOML parse fails.
//! - `trust_budget:declaration:invalid:<error_path>` — TOML parses but
//!   schema validation fails (one finding per validation error).
//! - `trust_budget:degraded:no_cargo_lock` — neither `<root>/Cargo.lock`
//!   nor `<root>/neurogrim/Cargo.lock` exists (e.g., LSP-Brains spec repo,
//!   python-starter); crate-drift detection is skipped.
//!
//! Plus C3a per-hat composition findings:
//!
//! - `trust_budget:per_hat:violation_potential:<hat>:<tool>` — hat declares
//!   `forbidden_tools` containing a script-shellout token that the
//!   workspace declares in `declared_shell_outs`. Static "could-be"
//!   conflict; runtime enforcement is deferred to BACKLOG B-23.
//! - `trust_budget:per_hat:network_undeclared:<hat>:<fqdn>` — hat declares
//!   `network_targets.allowed` containing an FQDN not in the workspace's
//!   `declared_external_services`.
//!
//! # Recursion-guard (Q7 hard rule)
//!
//! This file MUST be pure file-read + JSON-Schema-validate. No
//! shell-execution surfaces — the test in
//! `tests/trust_budget_sensor_behavior.rs::recursion_guard_no_command_in_validator_span`
//! reads the source file at test time and grep-checks for forbidden
//! patterns from a closed list (the patterns are deliberately not
//! enumerated in this comment so they don't trip the test against
//! itself; see the test source for the canonical list).

use crate::cmdb::{build_cmdb, Finding};
use crate::supply_chain_sca::lockfile::cargo as cargo_lockfile;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ── Closed-vocabulary documentation anchors (Q1) ────────────────────────────
//
// These const arrays mirror the schema's `definitions.Ecosystem.enum` and
// `definitions.TrustPosture.enum`. Drift is caught by the existing
// `trust_budget_schema_conformance::closed_set_enums_have_exactly_locked_entries`
// test. The runtime enforcement is the schema's job; these constants are
// documentation anchors so future readers don't have to follow the
// `include_str!` chain to learn the vocabulary.
const TRUST_BUDGET_ECOSYSTEM_VOCABULARY: &[&str] =
    &["cargo", "pypi", "npm", "system"];

const TRUST_BUDGET_TRUST_POSTURE_VOCABULARY: &[&str] = &[
    "api_only",
    "official_registry",
    "operator_audited",
    "vendor_attested",
];

// ── Embedded schema ─────────────────────────────────────────────────────────

/// Embedded trust-budget contract schema. Path is relative to this source
/// file (`src/trust_budget.rs`); the build fails at compile time if the
/// schema is missing. Five hops: src → crate → crates → neurogrim →
/// NeuroGrim → ecosystem root.
const TRUST_BUDGET_SCHEMA_JSON: &str = include_str!(
    "../../../../../LSP-Brains/schemas/trust-budget-v1.schema.json"
);

// ── Shell-out vocabulary ────────────────────────────────────────────────────

/// Closed-set vocabulary of real external commands the shell-out scanner
/// will emit. Anything not in this list is treated as either a builtin,
/// a locally-defined function, or noise. Heuristic by design — the goal
/// is "no obvious non-commands" rather than "perfect coverage." Operators
/// can still declare additional commands in `trust-budget.toml`; the
/// scanner just won't surface them as drift findings unless they appear
/// in actual scripts via this vocabulary.
const SHELL_OUT_VOCABULARY: &[&str] = &[
    "curl", "wget", "git", "cargo", "bash", "sh", "python", "python3",
    "py", "pip", "pip3", "npm", "yarn", "pnpm", "node", "docker",
    "make", "cmake", "tar", "unzip", "zip", "cp", "mv", "rm", "mkdir",
    "cat", "grep", "awk", "sed", "sort", "uniq", "head", "tail", "find",
    "xargs", "twine", "rustup", "cargo-audit", "ssh", "scp", "rsync",
    "jq",
];

/// Bash builtins / control words / very-short tokens that should never
/// be emitted as shell-outs. Distinct from `SHELL_OUT_VOCABULARY` so the
/// builtin filter can be applied even for tokens that pass the syntactic
/// shape test.
const SHELL_BUILTINS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "do", "done",
    "case", "esac", "in", "function", "return", "exit", "shift",
    "break", "continue", "local", "set", "unset", "export", "declare",
    "readonly", "echo", "printf", "read", "test", "trap", "wait",
    "source", "eval", "let", "true", "false", "cd", "pwd", "type",
    "command", "help", "alias", "unalias", "history", "jobs", "fg",
    "bg", "kill", "sleep", "time", "times", "umask", "ulimit", "hash",
    "getopts", "select", "until", "{", "}", "(", ")", "[", "]", "[[",
    "]]",
];

// ── Public analysis entry point ─────────────────────────────────────────────

/// Analyze the trust-budget for a Brain at `project_root`.
///
/// Mirrors the `analyze_capability_hygiene` shape — accepts `&str` to
/// match the existing CLI dispatch convention; the path is canonicalized
/// internally where possible.
///
/// Returns a CMDB envelope (cmdb-envelope-v1.schema.json) carrying the
/// trust-budget score (advisory, weight 0.0), findings list, and an
/// `extras` block with declared/actual breakdowns for each surface.
pub async fn analyze_trust_budget(project_root: &str) -> Value {
    let root_raw = PathBuf::from(project_root);
    let root = root_raw.canonicalize().unwrap_or(root_raw);
    analyze_trust_budget_path(&root)
}

/// Path-typed implementation of `analyze_trust_budget`. Separated out so
/// integration tests can exercise the path-typed surface without a UTF-8
/// round-trip through `&str`.
pub fn analyze_trust_budget_path(root: &Path) -> Value {
    let mut findings: Vec<Finding> = Vec::new();
    let mut declaration_present = false;

    // ── Phase 1: read declared surface ────────────────────────────────
    let declared_path = root.join("trust-budget.toml");
    let declared_outcome = read_declared(&declared_path);

    let declared = match declared_outcome {
        DeclaredOutcome::Missing => {
            findings.push(Finding {
                name: "trust_budget:declaration:missing".to_string(),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "No `trust-budget.toml` at `{}`. Per Q4 (permissive default), \
                     proceeding with empty declared sets — the sensor still computes \
                     the actual surface and reports drift findings as \
                     `trust_budget:undeclared:*`. Author a `trust-budget.toml` to \
                     enable two-way drift signaling.",
                    declared_path.display()
                )),
            });
            DeclaredSurface::default()
        }
        DeclaredOutcome::Malformed(reason) => {
            findings.push(Finding {
                name: "trust_budget:declaration:malformed".to_string(),
                status: "error".to_string(),
                points: 0,
                detail: Some(format!(
                    "`{}` exists but failed TOML parse: {reason}. Aborting drift \
                     analysis for this Brain — fix the syntax and re-run.",
                    declared_path.display()
                )),
            });
            // Abort gracefully — return envelope with that one finding.
            return build_cmdb(
                "trust-budget",
                100,
                findings,
                Some(meta_extras(&declared_path, &DeclaredSurface::default(), &ActualSurface::default(), false)),
                None,
            );
        }
        DeclaredOutcome::Found { value, surface } => {
            declaration_present = true;
            // ── Phase 2: validate against schema ──────────────────────
            if let Some(schema) = compile_trust_budget_schema_inline() {
                if let Err(errors) = schema.validate(&value) {
                    for err in errors {
                        let path_str = err.instance_path.to_string();
                        // Sanitize the path — remove leading slash,
                        // replace remaining slashes with `:` so the
                        // finding name remains a single
                        // colon-separated identifier.
                        let sanitized = path_str
                            .trim_start_matches('/')
                            .replace('/', ":");
                        let key = if sanitized.is_empty() {
                            "<root>".to_string()
                        } else {
                            sanitized
                        };
                        findings.push(Finding {
                            name: format!("trust_budget:declaration:invalid:{key}"),
                            status: "error".to_string(),
                            points: 0,
                            detail: Some(format!(
                                "schema validation failed at {path_str}: {err}"
                            )),
                        });
                    }
                }
            }
            // Even on validation failure, proceed with the parseable
            // surface — the declared sets are extracted directly from
            // the TOML, which is best-effort permissive.
            surface
        }
    };

    // ── Phase 3: compute actual surface ────────────────────────────────
    let actual = compute_actual_surface(root, &mut findings);

    // ── Phase 4: drift comparison ──────────────────────────────────────
    emit_drift_findings(&declared, &actual, &mut findings);

    // ── C3a: per-hat composition ───────────────────────────────────────
    emit_per_hat_findings(root, &declared, &mut findings);

    // ── Score (advisory; weight 0.0) ───────────────────────────────────
    //
    // Q4: "advisory weight 0.0" — the unified Brain score is unaffected
    // because the registry weight is 0.0 regardless of what we return
    // here. We still produce a meaningful score for the dashboard:
    // 100 if zero `:undeclared:*` or `:overdeclared:*` findings emitted
    // (the trust-budget is "in alignment"); otherwise lightly penalized.
    let drift_count = findings
        .iter()
        .filter(|f| {
            let n = &f.name;
            n.starts_with("trust_budget:undeclared:")
                || n.starts_with("trust_budget:overdeclared:")
        })
        .count();
    let score: u8 = (100i32 - (drift_count as i32 * 2)).clamp(0, 100) as u8;

    let extras = meta_extras(&declared_path, &declared, &actual, declaration_present);

    build_cmdb("trust-budget", score, findings, Some(extras), None)
}

// ── Declared-surface readers ─────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct DeclaredSurface {
    /// Crate names from `declared_crates`. Keyed by `name` only (the
    /// schema disambiguates `(name, ecosystem)` but for cargo lockfile
    /// drift we only need the cargo names; pypi/npm declared entries
    /// are surfaced via the breakdown JSON but not drift-checked
    /// because the corresponding actual lockfile parsers belong to
    /// other sensor surfaces).
    crates: BTreeSet<String>,
    /// Names of crates marked `seeded: true` — these suppress the
    /// `:undeclared:crate:<name>` finding when the crate also appears
    /// in the actual surface (E4-1 mitigation).
    seeded_crates: BTreeSet<String>,
    /// All declared crate entries, grouped for the breakdown JSON.
    crate_entries: Vec<(String, String, bool)>, // (name, ecosystem, seeded)
    /// Shell-out command names from `declared_shell_outs`.
    shell_outs: BTreeSet<String>,
    /// FQDNs from `declared_external_services`.
    services: BTreeSet<String>,
}

enum DeclaredOutcome {
    Missing,
    Malformed(String),
    Found { value: Value, surface: DeclaredSurface },
}

fn read_declared(path: &Path) -> DeclaredOutcome {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            // Distinguish "file does not exist" from "file exists but
            // can't be read." NotFound is the missing-declaration path
            // (advisory finding); other errors collapse into malformed
            // (operator's responsibility to fix permissions etc.).
            if e.kind() == std::io::ErrorKind::NotFound {
                return DeclaredOutcome::Missing;
            }
            return DeclaredOutcome::Malformed(format!("could not read file: {e}"));
        }
    };
    let toml_value: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(e) => return DeclaredOutcome::Malformed(format!("TOML parse error: {e}")),
    };
    let json_value: Value = match serde_json::to_value(toml_value) {
        Ok(v) => v,
        Err(e) => return DeclaredOutcome::Malformed(format!("TOML→JSON convert: {e}")),
    };

    let surface = extract_declared_surface(&json_value);
    DeclaredOutcome::Found {
        value: json_value,
        surface,
    }
}

fn extract_declared_surface(value: &Value) -> DeclaredSurface {
    let mut s = DeclaredSurface::default();

    // declared_crates: array of { name, ecosystem, seeded?, notes? }
    if let Some(arr) = value.get("declared_crates").and_then(|v| v.as_array()) {
        for item in arr {
            let Some(name) = item.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let ecosystem = item
                .get("ecosystem")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let seeded = item
                .get("seeded")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            s.crate_entries
                .push((name.to_string(), ecosystem.to_string(), seeded));
            // For drift purposes we only track cargo-ecosystem names
            // (the actual surface comes from `Cargo.lock`). Other
            // ecosystems (pypi/npm) are present in the breakdown JSON
            // for visibility but not drift-checked here.
            if ecosystem == "cargo" {
                s.crates.insert(name.to_string());
                if seeded {
                    s.seeded_crates.insert(name.to_string());
                }
            }
        }
    }

    if let Some(arr) = value.get("declared_shell_outs").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(cmd) = item.get("command_name").and_then(|v| v.as_str()) {
                s.shell_outs.insert(cmd.to_string());
            }
        }
    }

    if let Some(arr) = value
        .get("declared_external_services")
        .and_then(|v| v.as_array())
    {
        for item in arr {
            if let Some(fqdn) = item.get("fqdn").and_then(|v| v.as_str()) {
                s.services.insert(fqdn.to_string());
            }
        }
    }

    s
}

// ── Actual-surface computation ──────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct ActualSurface {
    crates: BTreeSet<String>,
    shell_outs: BTreeSet<String>,
    services: BTreeSet<String>,
}

fn compute_actual_surface(root: &Path, findings: &mut Vec<Finding>) -> ActualSurface {
    let mut s = ActualSurface::default();

    // ── Crates: reuse `supply_chain_sca::lockfile::cargo::parse`,
    // filtered to direct dependencies only (Q1 lock; transitive deferred
    // to B-23). NeuroGrim's repo has its workspace at `<root>/neurogrim/`,
    // not at `<root>/` directly — we try both layouts.
    //
    // Locate the workspace root holding `Cargo.lock`. Strategy:
    //   1. If `<root>/Cargo.lock` exists, the project root IS the
    //      workspace root (single-level layout — covers every test
    //      fixture and most adopters).
    //   2. Else if `<root>/neurogrim/Cargo.lock` exists, use that
    //      (NeuroGrim's two-level layout where the workspace lives
    //      under a `neurogrim/` subdirectory).
    //   3. Else: emit a degraded-mode advisory finding and skip crate
    //      detection silently.
    //
    // Once the workspace root is located, we apply the direct-dep filter
    // when a `Cargo.toml` is also present at that root (NeuroGrim layout
    // and most real Rust projects). When `Cargo.toml` is missing or
    // produces an empty direct-dep set (e.g., the synthetic test fixtures
    // that author only a `Cargo.lock`), we fall back to the full
    // lockfile surface so the operator still sees drift findings.
    let workspace_root: Option<PathBuf> = if root.join("Cargo.lock").is_file() {
        Some(root.to_path_buf())
    } else if root.join("neurogrim").join("Cargo.lock").is_file() {
        Some(root.join("neurogrim"))
    } else {
        None
    };

    match workspace_root {
        Some(ws) => {
            // Lockfile actual surface (all crates.io packages, transitive included).
            let lockfile_crates: BTreeSet<String> = match cargo_lockfile::parse(&ws) {
                Ok(pkgs) => pkgs.into_iter().map(|p| p.name).collect(),
                Err(_) => BTreeSet::new(),
            };
            // Direct-dep set: union of workspace.dependencies + each
            // member's [dependencies] / [dev-dependencies] / [build-dependencies].
            // Returns an empty set if `Cargo.toml` is missing or
            // unparseable — fallback handled below.
            let direct_deps = collect_direct_deps(&ws);

            if direct_deps.is_empty() {
                // No direct-dep manifest available — surface the full
                // lockfile so drift findings are still emitted. This is
                // the synthetic-fixture path (test authors only the
                // `Cargo.lock`); real Rust projects will have a
                // `Cargo.toml` and take the filter branch below.
                s.crates = lockfile_crates;
            } else {
                // Filter to direct ∩ lockfile (Q1 lock).
                for name in &direct_deps {
                    if lockfile_crates.contains(name) {
                        s.crates.insert(name.clone());
                    }
                }
            }
        }
        None => {
            findings.push(Finding {
                name: "trust_budget:degraded:no_cargo_lock".to_string(),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "Neither `{}/Cargo.lock` nor `{}/neurogrim/Cargo.lock` exists. \
                     Crate-drift detection skipped — this is expected for non-Rust \
                     Brains (LSP-Brains spec repo, python-starter). The shell-out \
                     and external-service surfaces are still computed.",
                    root.display(),
                    root.display()
                )),
            });
        }
    }

    // ── Shell-outs: scan `<root>/scripts/*.sh` and `*.ps1`.
    let scripts_dir = root.join("scripts");
    if scripts_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if ext != "sh" && ext != "ps1" {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    scan_script_for_shell_outs(&content, &mut s.shell_outs);
                    // Scripts may also reference external services via
                    // URLs in HTTP fetch / log strings. Reuse the URL
                    // scanner so the service surface stays correct.
                    scan_text_for_https_urls(&content, &mut s.services);
                }
            }
        }
    }

    // ── External services: scan source files (Rust + shell + Python)
    // for HTTPS URL string literals. The scan is broadly under
    // `<project_root>` but skips build output (`target/`), VCS internals
    // (`.git/`), and test fixtures (`tests/fixtures/`). When the
    // workspace is at `<root>/neurogrim/`, the recursive scan lands
    // there naturally — no need for a separate hard-coded sub-tree list.
    scan_dir_recursive_for_urls(root, &mut s.services, 0);

    s
}

/// Collect the union of direct-dependency crate names declared across
/// the workspace manifest and each workspace member's manifest. The
/// result is a flat name-set used to filter the lockfile's transitive
/// surface down to the v1 (Q1 lock) direct-only surface.
///
/// Strategy:
///   1. Read `<workspace_root>/Cargo.toml`. Extract:
///      - `[workspace.dependencies]` keys (workspace-pinned deps).
///      - `[workspace.members]` array (paths to per-crate manifests).
///      - `[dependencies]` / `[dev-dependencies]` / `[build-dependencies]`
///        keys (single-crate non-workspace layout fallback).
///   2. For each workspace member path, read that crate's `Cargo.toml`
///      and union in its `[dependencies]` / `[dev-dependencies]` /
///      `[build-dependencies]` keys.
fn collect_direct_deps(workspace_root: &Path) -> BTreeSet<String> {
    let mut deps: BTreeSet<String> = BTreeSet::new();

    let manifest_path = workspace_root.join("Cargo.toml");
    let manifest_raw = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(_) => return deps,
    };
    let manifest: toml::Value = match toml::from_str(&manifest_raw) {
        Ok(v) => v,
        Err(_) => return deps,
    };

    // Workspace-level deps (NeuroGrim layout).
    if let Some(ws) = manifest.get("workspace") {
        if let Some(ws_deps) = ws.get("dependencies").and_then(|v| v.as_table()) {
            for key in ws_deps.keys() {
                deps.insert(key.clone());
            }
        }
        if let Some(members) = ws.get("members").and_then(|v| v.as_array()) {
            for member in members {
                let Some(member_str) = member.as_str() else {
                    continue;
                };
                let member_manifest = workspace_root
                    .join(member_str)
                    .join("Cargo.toml");
                let Ok(member_raw) = std::fs::read_to_string(&member_manifest) else {
                    continue;
                };
                let Ok(member_toml): Result<toml::Value, _> =
                    toml::from_str(&member_raw)
                else {
                    continue;
                };
                add_dep_table_keys(&member_toml, &mut deps);
            }
        }
    }

    // Single-crate non-workspace layout fallback (covers e.g. simple
    // adopter projects like the python-starter if it ever grows a
    // Cargo.toml — defense-in-depth).
    add_dep_table_keys(&manifest, &mut deps);

    // Filter out workspace-internal crate names (those declared as
    // workspace members) so they don't show up as undeclared. The
    // member entries are paths like `crates/neurogrim-core`; the
    // package name is by convention the last path component, but
    // could differ. Best-effort: strip both the path-stem name and
    // the actual `[package].name` from each member manifest.
    let internal_names = collect_workspace_internal_names(workspace_root, &manifest);
    for name in &internal_names {
        deps.remove(name);
    }

    deps
}

/// Add every key from a manifest's `[dependencies]`, `[dev-dependencies]`,
/// and `[build-dependencies]` sections to `out`.
fn add_dep_table_keys(manifest: &toml::Value, out: &mut BTreeSet<String>) {
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = manifest.get(section).and_then(|v| v.as_table()) {
            for key in table.keys() {
                out.insert(key.clone());
            }
        }
    }
}

/// Collect the `[package].name` for every workspace member so they can
/// be filtered out of the direct-dep set (workspace-internal crates
/// shouldn't show as undeclared third-party deps).
fn collect_workspace_internal_names(
    workspace_root: &Path,
    workspace_manifest: &toml::Value,
) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    let Some(members) = workspace_manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    else {
        return names;
    };
    for member in members {
        let Some(member_str) = member.as_str() else {
            continue;
        };
        // Default name = last path component (e.g., `crates/foo` → `foo`).
        if let Some(stem) = std::path::Path::new(member_str)
            .file_name()
            .and_then(|s| s.to_str())
        {
            names.insert(stem.to_string());
        }
        // Authoritative name = `[package].name` in the member manifest.
        let manifest_path = workspace_root.join(member_str).join("Cargo.toml");
        if let Ok(raw) = std::fs::read_to_string(&manifest_path) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&raw) {
                if let Some(name) = parsed
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                {
                    names.insert(name.to_string());
                }
            }
        }
    }
    names
}

/// Two-pass scan of a script body for external-command invocations.
/// Pass 1 collects every locally-defined bash function. Pass 2 walks
/// non-comment, non-function-definition lines and emits the first
/// command-shaped token only when:
///   * it's not a shell builtin / control word,
///   * it's not a function defined in the same script,
///   * it's in the closed-set `SHELL_OUT_VOCABULARY` (or matches a
///     command-shape that's clearly external — but v1 is conservative
///     and uses the closed set to avoid false positives).
fn scan_script_for_shell_outs(content: &str, out: &mut BTreeSet<String>) {
    // Pass 1: collect locally-defined function names. Bash function
    // syntax is `name() { ... }` or `function name { ... }`; we only
    // recognize the parenthesized form (the dominant convention in
    // NeuroGrim's scripts).
    let local_functions = collect_local_function_names(content);

    // Pass 2: scan invocations.
    for raw_line in content.lines() {
        let trimmed = raw_line.trim_start();
        // Skip comments.
        if trimmed.starts_with('#') {
            continue;
        }
        // Skip blank lines.
        if trimmed.is_empty() {
            continue;
        }
        // Skip function definitions (whole line, not just first token —
        // the parens are the unambiguous marker).
        if is_function_definition_line(trimmed) {
            continue;
        }
        // Skip variable assignments (`VAR=value`, `local VAR=value`,
        // `export VAR=value`, `readonly VAR=value`).
        if is_variable_assignment_line(trimmed) {
            continue;
        }
        // Pluck candidate command tokens from the line. We extract the
        // first command-shaped token after stripping common shell
        // prefixes (`if`, `then`, `&&`, `||`, `;`, `(`, etc.). The
        // helper returns None if no candidate is found.
        if let Some(candidate) = extract_command_candidate(trimmed) {
            // Filter against:
            //   - shell builtins / control words
            //   - locally-defined functions
            //   - capitalization heuristic (commands are lowercase /
            //     kebab-case; capitalized words are prose / class names)
            //   - the closed-set SHELL_OUT_VOCABULARY (v1 conservatism)
            let lower = candidate.to_ascii_lowercase();
            if SHELL_BUILTINS.iter().any(|b| *b == lower.as_str()) {
                continue;
            }
            if local_functions.contains(candidate.as_str()) {
                continue;
            }
            // Drop tokens with ANY uppercase letter — real shell command
            // names are lowercase or kebab-case. Filters `Inspect`,
            // `See`, `BASH_SOURCE`, etc.
            if candidate.chars().any(|c| c.is_ascii_uppercase()) {
                continue;
            }
            // Drop very short tokens (1-char) that are almost certainly
            // regex/markup artifacts (e.g., a stray `d`).
            if candidate.len() < 2 {
                continue;
            }
            // Closed-set vocabulary check. Operators get exactly the
            // baseline of well-known external commands; anything beyond
            // the baseline is silent at v1 (conservative — better to
            // miss a real external command than to flood the output
            // with false positives).
            if SHELL_OUT_VOCABULARY.iter().any(|v| *v == lower.as_str()) {
                out.insert(lower);
            }
        }
    }
}

/// Extract every locally-defined bash function name. Recognizes the
/// parenthesized form `name() { ... }` (the dominant convention in
/// NeuroGrim's scripts) on a single line. Whitespace is allowed between
/// the name and the parens, between the parens, and before the brace.
fn collect_local_function_names(content: &str) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for raw_line in content.lines() {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(name) = parse_function_definition(trimmed) {
            names.insert(name);
        }
    }
    names
}

/// Return `Some(name)` if the line is a bash function definition of the
/// form `name() ...` (with optional whitespace and trailing `{`).
fn parse_function_definition(line: &str) -> Option<String> {
    // Find the position of `()` if any. The name is whatever precedes
    // the optional whitespace before `(`.
    let paren_idx = line.find('(')?;
    // Validate it's `()` not some other parenthesized thing.
    let after = &line[paren_idx + 1..];
    let after_trim = after.trim_start();
    if !after_trim.starts_with(')') {
        return None;
    }
    // Validate name is a legal bash identifier.
    let name_part = &line[..paren_idx];
    let name = name_part.trim();
    if name.is_empty() {
        return None;
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some(name.to_string())
}

/// Treat the line as a function-definition line if `parse_function_definition`
/// succeeds. (Wrapper for clarity at call sites.)
fn is_function_definition_line(line: &str) -> bool {
    parse_function_definition(line).is_some()
}

/// Variable assignment: a line whose first whitespace-delimited token
/// is either `VAR=...` or `local VAR=...` / `export VAR=...` /
/// `readonly VAR=...` / `declare VAR=...`.
fn is_variable_assignment_line(line: &str) -> bool {
    let mut tokens = line.split_whitespace();
    let Some(first) = tokens.next() else {
        return false;
    };
    if first.contains('=') && !first.starts_with('=') {
        return true;
    }
    if matches!(first, "local" | "export" | "readonly" | "declare") {
        if let Some(second) = tokens.next() {
            if second.contains('=') && !second.starts_with('=') {
                return true;
            }
        }
    }
    false
}

/// Extract the most plausible command-name token from a line of shell
/// code. The line is assumed already trimmed at the front. The function
/// strips common shell control prefixes (`if`, `then`, `else`, `&&`,
/// `||`, `;`, `(`, `do`, `while`, etc.) and returns the next token that
/// looks like a command name.
fn extract_command_candidate(line: &str) -> Option<String> {
    // Split on whitespace; we'll walk forward, skipping prefix tokens
    // until we find something command-shaped.
    let mut iter = line.split_whitespace();
    while let Some(tok) = iter.next() {
        // Strip terminating shell punctuation that'd otherwise glue to
        // the token (e.g., `(`, `{`).
        let cleaned = tok
            .trim_start_matches(|c: char| {
                matches!(c, '(' | '{' | '[' | '!' | '"' | '\'' | '$' | '`')
            })
            .trim_end_matches(|c: char| {
                matches!(c, ';' | '&' | '|' | ')' | '}' | ']' | '"' | '\'')
            });
        if cleaned.is_empty() {
            continue;
        }
        // Skip control / connector tokens — keep walking.
        let lower = cleaned.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "if" | "then" | "else" | "elif" | "fi" | "for" | "while"
                | "do" | "done" | "case" | "esac" | "in" | "until"
                | "&&" | "||" | ";" | "{" | "}" | "(" | ")"
                | "!" | "time" | "exec" | "function" | "return"
                | "select"
        ) {
            continue;
        }
        // Skip `command` / `builtin` / `eval` wrappers — peel them off
        // and continue to the actual command.
        if matches!(lower.as_str(), "command" | "builtin" | "eval" | "exec") {
            continue;
        }
        // Skip leading flags or env-var assignments inline (`FOO=bar`
        // would have been caught by is_variable_assignment_line, but a
        // leading inline assignment can precede a command).
        if cleaned.starts_with('-') {
            continue;
        }
        if cleaned.contains('=') && !cleaned.starts_with('=') {
            // Inline env assignment: `FOO=bar cmd ...` — keep walking.
            continue;
        }
        // Strip leading `./` or absolute-path prefix (e.g.,
        // `./scripts/foo.sh build` → `foo.sh`).
        let bare = cleaned.rsplit('/').next().unwrap_or(cleaned);
        // Validate command-name shape.
        if !is_command_shaped(bare) {
            return None;
        }
        return Some(bare.to_string());
    }
    None
}

/// Return true if `s` looks like a plausible command name: starts with
/// an ASCII letter or underscore, contains only `[a-zA-Z0-9_.\-]`.
fn is_command_shaped(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Best-effort scan for `https://[a-zA-Z0-9.-]+` patterns. Captures the
/// FQDN portion (up to the first `/` or whitespace).
fn scan_text_for_https_urls(content: &str, out: &mut BTreeSet<String>) {
    // Manual scan; deliberately simple to avoid regex deps.
    let bytes = content.as_bytes();
    let needle = b"https://";
    let mut i = 0usize;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let start = i + needle.len();
            let mut end = start;
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_alphanumeric() || b == b'.' || b == b'-' {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > start {
                let fqdn = &content[start..end];
                // Filter: require at least one dot AND a TLD-shaped tail
                // (≥2 chars, alphabetic). Rules out `https://localhost`,
                // bare-IP placeholders, and the noise tokens like `...`
                // or `.` that surface in the source.
                if is_plausible_fqdn(fqdn) {
                    out.insert(fqdn.to_ascii_lowercase());
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
}

/// Return true if `fqdn` has the shape of a real FQDN: contains a dot,
/// the trailing label (after the final dot) is at least 2 ASCII letters,
/// and no leading/trailing dot.
fn is_plausible_fqdn(fqdn: &str) -> bool {
    if fqdn.starts_with('.') || fqdn.ends_with('.') {
        return false;
    }
    let Some(last_dot) = fqdn.rfind('.') else {
        return false;
    };
    let tld = &fqdn[last_dot + 1..];
    if tld.len() < 2 {
        return false;
    }
    if !tld.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    true
}

/// Recursively scan a directory tree for HTTPS URLs in source-shaped
/// files (`.rs`, `.sh`, `.py`). Hand-rolled instead of `walkdir` to
/// honor Q1 (zero new deps). Bounded recursion depth is a defensive
/// guard against runaway symlink loops.
///
/// Skips:
///   * `target/` (Rust build output)
///   * `.git/` (VCS internals)
///   * `tests/fixtures/` (intentional test data, not real surface)
///   * Hidden directories (`.something/`)
///   * `node_modules/` (npm vendor)
fn scan_dir_recursive_for_urls(dir: &Path, out: &mut BTreeSet<String>, depth: usize) {
    if depth > 16 {
        return;
    }
    // Skip directories that are build output, VCS internals, vendor,
    // or test fixtures. Check the tail component name.
    if let Some(name) = dir.file_name().and_then(|s| s.to_str()) {
        if matches!(name, "target" | ".git" | "node_modules") {
            return;
        }
        if depth > 0 && name.starts_with('.') {
            return;
        }
    }
    // Skip `tests/fixtures` specifically — best-effort match on the
    // path's tail two components.
    if let (Some(parent), Some(name)) = (
        dir.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()),
        dir.file_name().and_then(|s| s.to_str()),
    ) {
        if parent == "tests" && name == "fixtures" {
            return;
        }
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_recursive_for_urls(&path, out, depth + 1);
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if !matches!(ext, "rs" | "sh" | "py") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                scan_text_for_https_urls(&content, out);
            }
        }
    }
}

// ── Drift emission ──────────────────────────────────────────────────────────

fn emit_drift_findings(
    declared: &DeclaredSurface,
    actual: &ActualSurface,
    findings: &mut Vec<Finding>,
) {
    // Crates
    for name in &actual.crates {
        if !declared.crates.contains(name) && !declared.seeded_crates.contains(name) {
            findings.push(Finding {
                name: format!("trust_budget:undeclared:crate:{name}"),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "`{name}` is in `Cargo.lock` but missing from \
                     `trust-budget.toml#declared_crates`. Either add it (with an \
                     `ecosystem = \"cargo\"`) or, for first-run noise, mark a \
                     `seeded = true` entry to suppress (E4-1)."
                )),
            });
        }
    }
    for name in &declared.crates {
        if !actual.crates.contains(name) {
            findings.push(Finding {
                name: format!("trust_budget:overdeclared:crate:{name}"),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "`{name}` is declared in `trust-budget.toml#declared_crates` \
                     but not present in `Cargo.lock`. Declaration is stale; \
                     remove it or restore the dependency."
                )),
            });
        }
    }

    // Shell-outs
    for name in &actual.shell_outs {
        if !declared.shell_outs.contains(name) {
            findings.push(Finding {
                name: format!("trust_budget:undeclared:shell_out:{name}"),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "`{name}` is invoked from `scripts/*.sh|ps1` but missing \
                     from `trust-budget.toml#declared_shell_outs`. Either \
                     declare it or remove the invocation."
                )),
            });
        }
    }
    for name in &declared.shell_outs {
        if !actual.shell_outs.contains(name) {
            findings.push(Finding {
                name: format!("trust_budget:overdeclared:shell_out:{name}"),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "`{name}` is declared in `trust-budget.toml#declared_shell_outs` \
                     but no `scripts/*.sh|ps1` line invokes it. Declaration is \
                     stale; remove it or add a script that uses it."
                )),
            });
        }
    }

    // External services
    for fqdn in &actual.services {
        if !declared.services.contains(fqdn) && !service_matches_wildcard(fqdn, &declared.services) {
            findings.push(Finding {
                name: format!("trust_budget:undeclared:service:{fqdn}"),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "`https://{fqdn}` appears in source code but is not declared \
                     in `trust-budget.toml#declared_external_services`. Either \
                     declare it (with `purpose` + `trust_posture`) or remove \
                     the call site. Best-effort scan — false positives possible \
                     for example/comment URLs."
                )),
            });
        }
    }
    for fqdn in &declared.services {
        // Wildcard declarations (`*.example.com`) match many actual
        // FQDNs — don't fire overdeclared in that case.
        if fqdn.contains('*') {
            continue;
        }
        if !actual.services.contains(fqdn) {
            findings.push(Finding {
                name: format!("trust_budget:overdeclared:service:{fqdn}"),
                status: "neutral".to_string(),
                points: 0,
                detail: Some(format!(
                    "`{fqdn}` is declared in `trust-budget.toml#declared_external_services` \
                     but no source-code HTTPS URL references it. Declaration may \
                     be stale; verify and remove if no longer used."
                )),
            });
        }
    }
}

/// Check whether `fqdn` matches any wildcard pattern in `declared` (e.g.,
/// `osv.dev` matches `*.dev`). v1 supports left-prefix wildcards only.
fn service_matches_wildcard(fqdn: &str, declared: &BTreeSet<String>) -> bool {
    for pattern in declared {
        if let Some(suffix) = pattern.strip_prefix("*.") {
            if fqdn.ends_with(suffix)
                && fqdn.len() > suffix.len()
                && fqdn.as_bytes()[fqdn.len() - suffix.len() - 1] == b'.'
            {
                return true;
            }
        }
    }
    false
}

// ── C3a per-hat composition ─────────────────────────────────────────────────

/// Walk `<root>/.claude/skills/hats/*.md` (skip `SKILL.md`) and emit
/// composition findings cross-referencing each hat's `forbidden_tools`
/// and `network_targets.allowed` against the workspace's
/// `declared_shell_outs` and `declared_external_services`.
fn emit_per_hat_findings(
    root: &Path,
    declared: &DeclaredSurface,
    findings: &mut Vec<Finding>,
) {
    let hats_dir = root.join(".claude").join("skills").join("hats");
    if !hats_dir.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(&hats_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut hats: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == "SKILL.md" || name.starts_with("README") || name.starts_with('.') {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(body) = std::fs::read_to_string(&path) {
            hats.push((stem.to_string(), body));
        }
    }
    hats.sort_by(|a, b| a.0.cmp(&b.0));

    for (hat_name, body) in hats {
        let frontmatter = match extract_frontmatter(&body) {
            Some(s) => s,
            None => continue,
        };
        let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&frontmatter) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let json_value: Value = match serde_json::to_value(yaml_value) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Forbidden tools — emit `violation_potential` for each
        // declared shell-out token that the hat lists as forbidden.
        // Maps the closed-set hat-contract vocabulary to script-token
        // names: in v1 we treat the literal token as the cross-reference
        // key (e.g., a hat with `forbidden_tools: [Bash]` matches a
        // declared shell-out with `command_name = "Bash"`). The
        // operator-friendly common case is when an operator declares
        // a shell-script `command_name` matching the hat-contract
        // token — at which point the static analysis flags the
        // potential conflict.
        if let Some(forbidden) = json_value
            .get("forbidden_tools")
            .and_then(|v| v.as_array())
        {
            for tool in forbidden {
                let Some(tok) = tool.as_str() else {
                    continue;
                };
                if declared.shell_outs.contains(tok) {
                    findings.push(Finding {
                        name: format!(
                            "trust_budget:per_hat:violation_potential:{hat_name}:{tok}"
                        ),
                        status: "neutral".to_string(),
                        points: 0,
                        detail: Some(format!(
                            "Hat `{hat_name}` declares `{tok}` in `forbidden_tools`, \
                             and the workspace declares `{tok}` in \
                             `declared_shell_outs`. Static potential for the hat \
                             to invoke a forbidden command. Runtime enforcement is \
                             deferred to BACKLOG B-23 (per E4-2)."
                        )),
                    });
                }
            }
        }

        // Network targets — emit `network_undeclared` for each
        // hat-allowed FQDN not in the workspace's declared services.
        if let Some(allowed) = json_value
            .pointer("/network_targets/allowed")
            .and_then(|v| v.as_array())
        {
            for fq in allowed {
                let Some(fqdn) = fq.as_str() else {
                    continue;
                };
                if !declared.services.contains(fqdn) {
                    findings.push(Finding {
                        name: format!(
                            "trust_budget:per_hat:network_undeclared:{hat_name}:{fqdn}"
                        ),
                        status: "neutral".to_string(),
                        points: 0,
                        detail: Some(format!(
                            "Hat `{hat_name}` declares `{fqdn}` in \
                             `network_targets.allowed` but the workspace's \
                             `trust-budget.toml#declared_external_services` does \
                             not include it. Drift signal: either the hat \
                             allow-list is fictional or the workspace \
                             declaration is incomplete."
                        )),
                    });
                }
            }
        }
    }
}

/// Extract YAML frontmatter delimited by `---` lines at the start of a
/// markdown file. Mirrors `extract_hat_frontmatter` from
/// `capability_hygiene.rs` (deliberate duplication — the production
/// extractor is small enough that the duplication cost is lower than
/// the refactor cost; mirrors the comment in `capability_hygiene.rs`
/// at the same function).
fn extract_frontmatter(markdown: &str) -> Option<String> {
    let mut lines = markdown.split_inclusive('\n');
    let first = lines.next()?.trim_end();
    if first != "---" {
        return None;
    }
    let mut yaml = String::new();
    for line in lines {
        if line.trim_end() == "---" {
            return Some(yaml);
        }
        yaml.push_str(line);
    }
    None
}

// ── Schema compile helper ────────────────────────────────────────────────────

fn compile_trust_budget_schema_inline() -> Option<jsonschema::JSONSchema> {
    let parsed: Value = serde_json::from_str(TRUST_BUDGET_SCHEMA_JSON).ok()?;
    jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&parsed)
        .ok()
}

// ── Extras / breakdown JSON ─────────────────────────────────────────────────

fn meta_extras(
    declared_path: &Path,
    declared: &DeclaredSurface,
    actual: &ActualSurface,
    declaration_present: bool,
) -> Vec<(&'static str, Value)> {
    // Group declared crates by ecosystem for the breakdown JSON.
    let mut declared_crates_by_ecosystem: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (name, ecosystem, seeded) in &declared.crate_entries {
        declared_crates_by_ecosystem
            .entry(ecosystem.clone())
            .or_default()
            .push(json!({
                "name": name,
                "seeded": seeded,
            }));
    }

    let breakdown = json!({
        "declared": {
            "crates": declared_crates_by_ecosystem,
            "crate_count": declared.crate_entries.len(),
            "shell_outs": declared.shell_outs.iter().collect::<Vec<_>>(),
            "shell_out_count": declared.shell_outs.len(),
            "services": declared.services.iter().collect::<Vec<_>>(),
            "service_count": declared.services.len(),
        },
        "actual": {
            "crates": actual.crates.iter().collect::<Vec<_>>(),
            "crate_count": actual.crates.len(),
            "shell_outs": actual.shell_outs.iter().collect::<Vec<_>>(),
            "shell_out_count": actual.shell_outs.len(),
            "services": actual.services.iter().collect::<Vec<_>>(),
            "service_count": actual.services.len(),
        },
        "declaration_present": declaration_present,
        "declaration_path": declared_path.display().to_string(),
        "vocabulary": {
            "ecosystem": TRUST_BUDGET_ECOSYSTEM_VOCABULARY,
            "trust_posture": TRUST_BUDGET_TRUST_POSTURE_VOCABULARY,
        },
    });

    vec![("trust_budget_breakdown", breakdown)]
}
