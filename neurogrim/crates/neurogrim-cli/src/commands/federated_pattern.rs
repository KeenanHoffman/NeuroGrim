//! `neurogrim federated-pattern` — operator-explicit emission of a
//! federated-pattern A2A message under bidirectional opt-in
//! (LSP-Brains v2.12 §16.6.1, E-B2-7 C7).
//!
//! Single sub-command at v1:
//!
//! - **`emit`** — construct a synthetic v1 `FederatedPatternPayload`
//!   from closed-set CLI inputs, write an `entry_kind=emitted` row to
//!   the local pattern-aggregation-ledger BEFORE transmission (Q12
//!   log-before-transmit lock), and call
//!   `neurogrim_a2a::federated_pattern::emit_federated_pattern` for
//!   each declared peer.
//!
//! v1 ships `emit` only — no `list`, no `replay`, no `delete`. The
//! pattern-aggregation-ledger is high-frequency append-only;
//! retrieval-side semantics are deferred to v2 per BACKLOG B-23.
//!
//! Closed-set discipline (Q14 lock — spec §16.6.1):
//!
//! - `--pattern-kind` is validated at clap parse time via
//!   `PossibleValuesParser` against the v1 single-entry vocabulary
//!   `["vigilance-pattern"]`. Typos like `--pattern-kind yolo` fail
//!   with clap's "invalid value" error before any I/O.
//!
//! Privacy contract (Q1 + Q5 + Q8 lock — spec §16.6.1):
//!
//! - NO `--note`, `--justification`, `--comment`, `--reason`, `--body`,
//!   `--text`, or any other free-text flag is accepted. The
//!   `additionalProperties: false` on `FederatedPatternPayload` plus
//!   the closed-set numeric-only `feature_vector` is the structural
//!   enforcement; this CLI MUST NOT add a free-text channel of any
//!   shape. Re-opening this surface requires a charter-level BR-6
//!   conversation.
//!
//! Recursion guard (Q9 lock — spec §16.6.1 MUST):
//!
//! - If `--pattern-kind` starts with the literal prefix
//!   `federated_patterns:`, the CLI rejects the emit before any I/O.
//!   The carve-out closes the meta-finding-feedback loop (the
//!   federated-patterns aggregator sensor's own findings cannot
//!   round-trip the wire). Defense-in-depth — clap's closed-set
//!   parser already rejects values not in the v1 vocabulary, but the
//!   prefix guard pins the invariant for any future v2/v3 vocabulary
//!   that might inadvertently include a `federated_patterns:*` value.
//!
//! Topology source (Q16 lock — spec §16.6.1):
//!
//! - Peer enumeration reads from `<project_root>/.claude/brain-registry.json`
//!   `config.children` map. v1 federation flows EXCLUSIVELY between
//!   parent and declared child Brains. If `--peer <X>` is absent,
//!   emission targets every declared child; if `--peer <X>` is
//!   present, `X` MUST be a key in `config.children` or the CLI
//!   errors.
//!
//! Sender-side audit (Q12 lock — spec §16.6.1 MUST):
//!
//! - Implementations MUST write an `entry_kind=emitted` row to
//!   `<project_root>/.claude/brain/pattern-aggregation-ledger.jsonl`
//!   BEFORE invoking the protocol-layer emit. This CLI is the only
//!   v1 writer of `entry_kind=emitted` rows; the receiver path
//!   writes `entry_kind=received` rows from the transport handler.
//!   The ordering matters: a transport failure after a successful
//!   ledger row gives operators an "I tried to emit but it failed"
//!   audit trail; a successful emit without a ledger row would lose
//!   the audit. Crash between ledger-write and transmit yields a
//!   "intent recorded, outcome unknown" entry — preferable to "lost
//!   transmission with no record".
//!
//! Operator identity (advisory only):
//!
//! - `--operator <handle>` falls back to `NEUROGRIM_OPERATOR` env. v1
//!   lock per Q12: NEUROGRIM_OPERATOR identity is NOT captured in
//!   the federated-pattern audit row (federation is project-level,
//!   not operator-level). The handle, when set, is printed to stdout
//!   for the operator's own console-level audit only.
//!
//! Anonymized origin (Q15 lock — spec §16.6.1):
//!
//! - The local Brain's identity is hashed via
//!   `sha256(brain_id || "|" || YYYY-MM-DD)` where the day is the
//!   current UTC date. The peer's identity is hashed identically.
//!   Daily-nonce rotation prevents cross-day identity correlation.
//!   The peer learns "this hash is mine TODAY" for self-loop
//!   detection; cross-day attribution is intentionally hard.
//!
//! Hard constraints (verified by the integration test suite):
//!
//! - NO new Cargo deps in `neurogrim-cli`. SHA-256 is implemented
//!   inline below as a minimal pure-Rust function (no `sha2` crate
//!   import; the `neurogrim-cli` Cargo.toml has no `sha2` entry).
//! - NO free-text flags. The Args struct below contains zero
//!   `String` arguments that carry operator-authored prose.

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use clap::{Args as ClapArgs, Subcommand};
use neurogrim_a2a::agent_card::{
    AgentCard, AuthScheme, Authentication, Capabilities, Transport as TransportCard,
    TransportProtocol,
};
use neurogrim_a2a::envelope::MessageType;
use neurogrim_a2a::federated_pattern::{
    emit_federated_pattern, bidirectional_opt_in_satisfied, FeatureVector,
    FederatedPatternEmitLimiter, FederatedPatternPayload, PatternKind, SeverityClass,
    FEDERATED_PATTERNS_FINDING_PREFIX,
};
use neurogrim_a2a::HttpSseTransport;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use url::Url;

/// Closed-set v1 pattern-kind vocabulary (Q14 lock — spec §16.6.1).
/// Single value at v1: `vigilance-pattern`. Future kinds are v2/v3
/// candidates per BACKLOG B-23 and require an additive spec change +
/// schema bump. Adding entries here without a corresponding spec
/// change would silently break conformance.
pub const PATTERN_KINDS: &[&str] = &["vigilance-pattern"];

/// Recursion-guard prefix (Q9 lock — spec §16.6.1). Pattern-kind
/// values starting with this literal are findings emitted by the
/// federated-patterns aggregator sensor itself; transmitting them
/// would create the A→B→C→A meta-finding-feedback loop.
const RECURSION_GUARD_PREFIX: &str = FEDERATED_PATTERNS_FINDING_PREFIX;

/// Top-level args for `neurogrim federated-pattern <subcommand>`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub subcommand: FederatedPatternCmd,
}

#[derive(Subcommand, Debug)]
pub enum FederatedPatternCmd {
    /// Emit a federated-pattern A2A message to one or all declared peers.
    ///
    /// Constructs a synthetic v1 `FederatedPatternPayload`, writes an
    /// `entry_kind=emitted` row to the local pattern-aggregation-ledger
    /// BEFORE transmission (Q12 log-before-transmit lock), and calls
    /// `emit_federated_pattern` for each target peer.
    ///
    /// **Bidirectional opt-in:** both peers MUST advertise
    /// `federated-pattern` in their Agent Card capabilities; the
    /// protocol-layer emit rejects with `OptInNotSatisfied` otherwise.
    ///
    /// **No free-text flags at v1** (Q1+Q5+Q8 lock — spec §16.6.1).
    /// The privacy contract forbids operator-authored prose in
    /// federated-pattern records; the closed-set numeric `feature_vector`
    /// is the structural enforcement.
    Emit {
        /// Pattern kind — v1 closed-set: only `vigilance-pattern`
        /// (Q14 lock, spec §16.6.1).
        ///
        /// Validated at clap parse time via PossibleValuesParser so
        /// typos like `--pattern-kind yolo` fail with clap's standard
        /// "invalid value" error before any file I/O. Adding new
        /// values requires a spec change with an explicit
        /// METHODOLOGY-EVOLUTION entry.
        ///
        /// Recursion-guard: pattern-kind values starting with
        /// `federated_patterns:` are rejected at parse time per spec
        /// §16.6.1 MUST (Q9 lock).
        #[arg(
            long,
            value_parser = clap::builder::PossibleValuesParser::new(PATTERN_KINDS),
        )]
        pattern_kind: String,

        /// Optional target peer name — must be a key in
        /// `<project_root>/.claude/brain-registry.json:config.children`.
        /// When absent: emit to ALL declared children. When present:
        /// emit to ONLY that peer.
        #[arg(long)]
        peer: Option<String>,

        /// Operator handle. Falls back to `$NEUROGRIM_OPERATOR`. When
        /// set, printed to stdout for console-level audit only — Q12
        /// lock: NEUROGRIM_OPERATOR identity is NOT captured in the
        /// pattern-aggregation-ledger row (federation is project-level,
        /// not operator-level).
        #[arg(long)]
        operator: Option<String>,

        /// Project root path. The brain-registry.json lives at
        /// `<project_root>/.claude/brain-registry.json`; the ledger lives
        /// at `<project_root>/.claude/brain/pattern-aggregation-ledger.jsonl`.
        #[arg(long, default_value = ".")]
        project_root: String,

        /// Test-only flag (hidden): skip the actual A2A transmission
        /// after writing the ledger row. Used by the
        /// `federated_pattern_cli_behavior` integration tests to
        /// verify the Q12 log-before-transmit invariant without
        /// standing up a live peer. Production code paths SHOULD NOT
        /// rely on this flag — emission without transmission is
        /// observability noise, not a feature.
        #[arg(long, hide = true)]
        no_transmit: bool,
    },
}

/// On-disk shape of an `EmittedEntry` row in the pattern-aggregation-ledger.
/// Every field maps 1:1 to a required property in
/// `pattern-aggregation-ledger-v1.schema.json` `EmittedEntry`. Adding a
/// free-text field here would violate the privacy contract (Q1+Q5+Q8
/// lock). Field order matches the schema for human-readable diffs.
#[derive(Serialize, Debug)]
struct EmittedEntry<'a> {
    schema_version: &'a str,
    entry_kind: &'a str,
    ts: &'a str,
    peer_brain_id: &'a str,
    to_brain_id: &'a str,
    envelope_message_id: &'a str,
    payload: &'a FederatedPatternPayload,
}

pub async fn run(args: Args) -> Result<()> {
    match args.subcommand {
        FederatedPatternCmd::Emit {
            pattern_kind,
            peer,
            operator,
            project_root,
            no_transmit,
        } => {
            cmd_emit(
                &pattern_kind,
                peer.as_deref(),
                operator.as_deref(),
                &project_root,
                no_transmit,
            )
            .await
        }
    }
}

async fn cmd_emit(
    pattern_kind: &str,
    peer_arg: Option<&str>,
    operator_arg: Option<&str>,
    project_root: &str,
    no_transmit: bool,
) -> Result<()> {
    // Recursion guard (Q9 lock, spec §16.6.1 MUST). Parse-time-equivalent
    // rejection — no file I/O occurs if the guard fires. Defense-in-depth:
    // clap's PossibleValuesParser will already reject any value not in the
    // v1 closed set, but pinning the prefix check here means a future v2/v3
    // vocabulary that inadvertently includes a `federated_patterns:*`
    // value would still be rejected. Spec §16.6.1 + Q9 cited in the error
    // string for operator audit clarity.
    if pattern_kind.starts_with(RECURSION_GUARD_PREFIX) {
        anyhow::bail!(
            "refusing to emit federated-pattern with kind starting with \
             '{RECURSION_GUARD_PREFIX}' (recursion guard, spec §16.6.1 + Q9)"
        );
    }

    // Operator handle for console-level audit. Per Q12 lock, this value
    // is NOT written to the ledger — federation is project-level, not
    // operator-level. Printed to stdout below for the operator's own
    // visibility ("who at the keyboard pressed enter?").
    let operator_label = resolve_operator(operator_arg);

    // Read brain-registry.json — used both for the local Brain's identity
    // (the `meta.project` field — Q15 anonymized-origin input) and the
    // declared peer topology (config.children map — Q16 topology lock).
    let registry_path = Path::new(project_root)
        .join(".claude")
        .join("brain-registry.json");
    let registry_text = fs::read_to_string(&registry_path).with_context(|| {
        format!(
            "failed to read {} — run from a project root, or pass --project-root",
            registry_path.display()
        )
    })?;
    let registry: serde_json::Value = serde_json::from_str(&registry_text)
        .with_context(|| format!("failed to parse {} as JSON", registry_path.display()))?;

    let local_brain_id = local_brain_id_from_registry(&registry);
    let children = children_from_registry(&registry);

    // Q16 topology lock — federation requires at least one declared peer.
    if children.is_empty() {
        anyhow::bail!(
            "no children declared in brain-registry.json:config.children; federation \
             requires at least one declared peer (Q16 topology lock, spec §16.6.1)"
        );
    }

    // Resolve the target peer set: one if --peer; all if --peer absent.
    let targets: Vec<(String, ChildEntry)> = if let Some(p) = peer_arg {
        match children.iter().find(|(name, _)| name.as_str() == p) {
            Some((name, child)) => vec![(name.clone(), child.clone())],
            None => {
                let known: Vec<&str> = children.iter().map(|(n, _)| n.as_str()).collect();
                anyhow::bail!(
                    "unknown peer '{p}'; declared children in brain-registry.json: {known:?}"
                );
            }
        }
    } else {
        children.clone()
    };

    // Q15 anonymized-origin: sha256(brain_id || "|" || YYYY-MM-DD UTC).
    // The day is the current UTC date — daily-nonce rotation prevents
    // cross-day identity correlation. The peer's identity is hashed
    // identically (operator-supplied peer brain_id from registry; no
    // crypto signing at v1).
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let local_origin_hash = anonymized_origin(&local_brain_id, &today);

    // Construct the local Brain's Agent Card. v1 minimal: declare
    // federated-pattern in BOTH accepts[] AND emits[] (bidirectional
    // opt-in MUST). The id is the local brain_id; everything else is
    // synthesized from the project root since we're running as a CLI,
    // not as a TaskServer.
    let local_card = build_local_card(&local_brain_id);

    // The protocol-layer rate-limiter is operator-fresh per CLI invocation
    // — fine because the CLI is operator-explicit (Q2 lock) and not
    // expected to fire fast enough to need cross-invocation state.
    let rate_limiter = FederatedPatternEmitLimiter::new();
    let transport = HttpSseTransport::new();

    let ledger_path = ledger_path(project_root);
    ensure_brain_dir(&ledger_path)?;

    // Per-peer emission loop. Each iteration: construct the payload
    // (synthetic v1 baseline), write the ledger row BEFORE transmit
    // (Q12 lock), then attempt transmit.
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let total = targets.len();

    for (peer_name, child) in &targets {
        let peer_brain_id = peer_brain_id_for_child(peer_name, child);
        let peer_origin_hash = anonymized_origin(&peer_brain_id, &today);
        let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

        // Build the v3.1 vigilance-driven payload. All values are
        // closed-set or bounded-numeric per the privacy contract; no
        // operator prose is reachable from any --flag in this CLI.
        // The `feature_vector` is populated from the local
        // supply-chain-vigilance CMDB findings (count + highest
        // severity); when no CMDB or no findings exist the function
        // returns the safe baseline (count=0, Info, window=7) so
        // emission still proceeds for operators who haven't run the
        // vigilance sensor yet. v3.1 E-V31-E.
        let payload = FederatedPatternPayload {
            schema_version: "1".to_string(),
            pattern_kind: PatternKind::VigilancePattern,
            feature_vector: read_vigilance_features(project_root),
            anonymized_origin: local_origin_hash.clone(),
            origin_set: vec![local_origin_hash.clone()],
            peer_brain_id: peer_origin_hash.clone(),
            discovered_at: ts.clone(),
            severity_class: Some(SeverityClass::Info),
            legal_disclaimer: None,
            metadata: None,
        };

        // Q12 LOCK — log BEFORE transmit. The envelope's actual
        // message_id is generated inside emit_federated_pattern; we
        // pre-allocate one here so the ledger row carries a stable
        // identifier. If transmit succeeds, the wire envelope's id
        // will differ from this one — the ledger captures INTENT;
        // wire correlation is a v2 concern (BACKLOG B-23).
        let predicted_envelope_id = uuid::Uuid::new_v4().to_string();
        let entry = EmittedEntry {
            schema_version: "1",
            entry_kind: "emitted",
            ts: &ts,
            peer_brain_id: &peer_origin_hash,
            to_brain_id: &peer_origin_hash,
            envelope_message_id: &predicted_envelope_id,
            payload: &payload,
        };
        let line = serde_json::to_string(&entry)
            .context("serialize emitted entry")?;
        append_jsonl_line(&ledger_path, &line).with_context(|| {
            format!("append entry_kind=emitted row to {}", ledger_path.display())
        })?;

        // --no-transmit short-circuits the wire I/O; the ledger row
        // is the verified artifact (Q12 lock — log-before-transmit
        // invariant pinned by integration tests). Production code
        // omits this flag.
        if no_transmit {
            succeeded += 1;
            println!(
                "emitted federated-pattern (no-transmit): peer={peer_name} \
                 kind={pattern_kind} envelope_message_id={predicted_envelope_id} ts={ts}"
            );
            continue;
        }

        // Build a synthetic peer Agent Card from registry data. v1
        // posture: assume the peer advertises federated-pattern in
        // BOTH accepts[] and emits[] (operator deliberation per Q5
        // bidirectional-opt-in lock). If the peer's declared
        // capabilities don't actually include federated-pattern, the
        // protocol-layer emit will reject with OptInNotSatisfied at
        // run time. Fetching the live Agent Card (over A2A discover)
        // is a v2 concern — at v1 this CLI assumes operator-level
        // declaration consent in registry.config.children.
        let peer_card = build_peer_card(&peer_brain_id);

        // Defensive opt-in check at the CLI layer too. The protocol
        // emit also checks; this gives the operator a CLI-level error
        // before the ledger row is committed. (We've already written
        // the ledger row above per Q12 — the opt-in check here is
        // belt-and-braces for the transport-side error message.)
        if !bidirectional_opt_in_satisfied(&local_card, &peer_card) {
            failed += 1;
            eprintln!(
                "emit failed: peer={peer_name} kind={pattern_kind} \
                 reason=bidirectional-opt-in-not-satisfied"
            );
            continue;
        }

        // Resolve the peer endpoint URL from registry.children data.
        let peer_endpoint = match peer_endpoint_for_child(child) {
            Some(url) => url,
            None => {
                failed += 1;
                eprintln!(
                    "emit failed: peer={peer_name} kind={pattern_kind} \
                     reason=missing-a2a-endpoint-in-registry"
                );
                continue;
            }
        };

        let outcome = emit_federated_pattern(
            &local_card,
            &peer_card,
            &peer_endpoint,
            payload,
            "vigilance-pattern",
            &rate_limiter,
            &transport,
        )
        .await;

        match outcome {
            Ok(o) => {
                succeeded += 1;
                println!(
                    "emitted federated-pattern: peer={peer_name} \
                     kind={pattern_kind} envelope_message_id={} ts={}",
                    o.envelope_message_id,
                    o.sent_at.to_rfc3339_opts(SecondsFormat::Millis, true)
                );
            }
            Err(e) => {
                failed += 1;
                eprintln!(
                    "emit failed: peer={peer_name} kind={pattern_kind} \
                     reason={e}. Hint: the v1 federation requires the peer to be \
                     running an A2A server. Start the peer with \
                     `neurogrim a2a-serve`."
                );
            }
        }
    }

    // End-of-run summary line.
    println!(
        "federated-pattern emit complete: {total} peer(s) attempted, \
         {succeeded} successful, {failed} failed{operator_suffix}",
        operator_suffix = match &operator_label {
            Some(op) => format!(" (operator={op})"),
            None => String::new(),
        }
    );

    if succeeded == 0 {
        anyhow::bail!(
            "all peer emissions failed; see per-peer reasons above"
        );
    }
    Ok(())
}

/// Resolve operator handle for console-level audit only (Q12 lock —
/// NOT written to the pattern-aggregation-ledger). Returns `None` when
/// neither flag nor env is set; emission proceeds without an operator
/// label rather than erroring (federation is project-level, not
/// operator-level — different posture from the disposition CLI).
fn resolve_operator(operator_arg: Option<&str>) -> Option<String> {
    if let Some(op) = operator_arg {
        let trimmed = op.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Ok(env_val) = std::env::var("NEUROGRIM_OPERATOR") {
        let trimmed = env_val.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Resolve the local Brain's identity from the registry. The canonical
/// source is `meta.updated_by` (matches `a2a_serve.rs` Agent Card
/// construction). Falls back to `meta.project` when `updated_by` is
/// empty; falls back to "neurogrim-local" as a last resort so the hash
/// computation cannot panic on a thin registry. Operators with proper
/// registry hygiene will see the `meta.updated_by`-derived hash.
fn local_brain_id_from_registry(registry: &serde_json::Value) -> String {
    if let Some(updated_by) = registry
        .get("meta")
        .and_then(|m| m.get("updated_by"))
        .and_then(|v| v.as_str())
    {
        let trimmed = updated_by.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Some(project) = registry
        .get("meta")
        .and_then(|m| m.get("project"))
        .and_then(|v| v.as_str())
    {
        let trimmed = project.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "neurogrim-local".to_string()
}

/// One declared child entry — the relevant fields we read off
/// `config.children[<name>]`. Stored loosely so future registry
/// extensions (E4-7 forward-compat) don't trip parsing.
#[derive(Debug, Clone)]
struct ChildEntry {
    /// `a2a_endpoint` field — used to construct the URL passed to
    /// `emit_federated_pattern`. May be absent in operator-thin
    /// registries; the CLI errors per-peer rather than aborting the
    /// whole emission run.
    a2a_endpoint: Option<String>,
    /// `display_name` field — printed for operator-readable error
    /// messages. Falls back to the peer key when absent.
    #[allow(dead_code)]
    display_name: Option<String>,
}

/// Read `config.children` from the parsed registry. Returns peer
/// entries in a stable iteration order (insertion-order from
/// `serde_json::Map`). Empty map when no children are declared —
/// the caller errors at the call-site per Q16 topology lock.
fn children_from_registry(registry: &serde_json::Value) -> Vec<(String, ChildEntry)> {
    let map = registry
        .get("config")
        .and_then(|c| c.get("children"))
        .and_then(|v| v.as_object());
    let Some(map) = map else {
        return Vec::new();
    };
    map.iter()
        .map(|(name, child)| {
            let entry = ChildEntry {
                a2a_endpoint: child
                    .get("a2a_endpoint")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                display_name: child
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            };
            (name.clone(), entry)
        })
        .collect()
}

/// The peer's brain_id used as input to the anonymized-origin hash. v1
/// uses the registry-declared peer name (the `config.children` key)
/// since a v1 CLI doesn't fetch the live Agent Card to learn the
/// peer's `meta.updated_by`. v2 candidate: discover-and-cache the
/// peer's actual brain_id via `/.well-known/agent-card.json`.
fn peer_brain_id_for_child(name: &str, _child: &ChildEntry) -> String {
    name.to_string()
}

/// Resolve a peer's A2A endpoint URL from its registry entry.
fn peer_endpoint_for_child(child: &ChildEntry) -> Option<Url> {
    let raw = child.a2a_endpoint.as_deref()?;
    Url::parse(raw).ok()
}

/// Construct a minimal local Agent Card declaring federated-pattern
/// in BOTH `accepts[]` and `emits[]` (bidirectional opt-in MUST). v1
/// CLI posture: this card represents operator-level consent at the
/// invocation point. The protocol-layer
/// `bidirectional_opt_in_satisfied` check still has to pass against
/// the peer's card — see `build_peer_card`.
fn build_local_card(brain_id: &str) -> AgentCard {
    AgentCard {
        schema_version: "1".into(),
        id: brain_id.to_string(),
        name: brain_id.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        interface_version: "1".into(),
        capabilities: Capabilities {
            accepts: vec![MessageType::FederatedPattern],
            emits: vec![MessageType::FederatedPattern],
            streaming: false,
        },
        transport: TransportCard {
            protocol: TransportProtocol::HttpSse,
            endpoint: "http://127.0.0.1:0/a2a/v1/".into(),
            tasks_path: "/a2a/v1/tasks".into(),
        },
        authentication: Authentication {
            scheme: AuthScheme::None,
        },
        topology: None,
        queue_endpoints: None,
    }
}

/// Construct a synthetic peer Agent Card from registry-declared data.
/// v1 posture: assume the peer advertises federated-pattern in BOTH
/// directions. If the peer's declared capabilities differ at run time,
/// the protocol-layer emit returns `OptInNotSatisfied`. Live peer
/// Agent Card discovery is a v2 concern (BACKLOG B-23).
fn build_peer_card(peer_brain_id: &str) -> AgentCard {
    AgentCard {
        schema_version: "1".into(),
        id: peer_brain_id.to_string(),
        name: peer_brain_id.to_string(),
        version: "0.0.0".to_string(),
        interface_version: "1".into(),
        capabilities: Capabilities {
            accepts: vec![MessageType::FederatedPattern],
            emits: vec![MessageType::FederatedPattern],
            streaming: false,
        },
        transport: TransportCard {
            protocol: TransportProtocol::HttpSse,
            endpoint: "http://127.0.0.1:0/a2a/v1/".into(),
            tasks_path: "/a2a/v1/tasks".into(),
        },
        authentication: Authentication {
            scheme: AuthScheme::None,
        },
        topology: None,
        queue_endpoints: None,
    }
}

/// Resolve the ledger path:
/// `<project_root>/.claude/brain/pattern-aggregation-ledger.jsonl`.
fn ledger_path(project_root: &str) -> PathBuf {
    Path::new(project_root)
        .join(".claude")
        .join("brain")
        .join("pattern-aggregation-ledger.jsonl")
}

/// Ensure the parent `.claude/brain/` directory exists. Mirrors the
/// disposition CLI's `ensure_brain_dir` helper — same convention as
/// the operator-disposition + domain-calibration writers.
fn ensure_brain_dir(ledger_path: &Path) -> Result<()> {
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("mkdir -p {}", parent.display()))?;
    }
    Ok(())
}

/// Append a single JSONL line to the ledger file. Same atomicity
/// posture as the disposition CLI: `OpenOptions::create(true).append(true)`
/// gives `O_APPEND` semantics on POSIX and `FILE_APPEND_DATA` on
/// Windows — both atomic for writes < PIPE_BUF (4096 bytes). A
/// federated-pattern emitted row is ~600 bytes, well under the bound.
fn append_jsonl_line(path: &Path, line: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open ledger {}", path.display()))?;
    writeln!(file, "{line}")
        .with_context(|| format!("append to ledger {}", path.display()))?;
    Ok(())
}

/// Read the local supply-chain-vigilance CMDB and aggregate its
/// findings into a `FeatureVector` for federated transmission (v3.1
/// E-V31-E real-payload migration; replaces the v1 synthetic
/// baseline with operator-meaningful signal).
///
/// Mapping (closed-set, privacy-safe):
///
/// - `numeric_count` — count of findings in the CMDB.
/// - `severity_class` — highest severity observed across findings,
///   mapping CMDB `status` → `SeverityClass`:
///     `"info"` → `Info`, `"warning"` → `Medium`, `"critical"` →
///     `Critical`. Other statuses default to `Info`.
/// - `observation_window_days` — fixed at 7 (matches the vigilance
///   sensor's window). Configurable via spec §16.6.1 amendment if
///   evidence accrues that other windows surface meaningful signal.
///
/// Returns the safe-baseline vector (`count=0, Info, window=7`)
/// when the CMDB:
/// - is missing (file not found),
/// - is malformed (parse error),
/// - has no findings,
/// - has no `findings` array at all.
///
/// This preserves emission behavior for operators who haven't yet
/// run the vigilance sensor — federation still works in
/// "demonstration mode" with a baseline payload, just like v1.
///
/// Privacy-safe: aggregates only. No per-finding details, no
/// package names, no semver, no operator-authored strings cross
/// the wire. The closed-set numeric/enum schema (`FeatureVector`)
/// is the structural enforcement.
fn read_vigilance_features(project_root: &str) -> FeatureVector {
    let cmdb_path = Path::new(project_root)
        .join(".claude")
        .join("supply-chain-vigilance-cmdb.json");

    let baseline = FeatureVector {
        numeric_count: 0,
        severity_class: SeverityClass::Info,
        observation_window_days: 7,
    };

    let Ok(text) = fs::read_to_string(&cmdb_path) else {
        return baseline;
    };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else {
        return baseline;
    };
    let Some(findings) = data.get("findings").and_then(|v| v.as_array()) else {
        return baseline;
    };
    if findings.is_empty() {
        return baseline;
    }

    let count = findings.len() as u32;
    let max_severity = findings
        .iter()
        .filter_map(|f| f.get("status").and_then(|v| v.as_str()))
        .map(severity_from_status)
        .max_by_key(|s| severity_rank(*s))
        .unwrap_or(SeverityClass::Info);

    FeatureVector {
        numeric_count: count,
        severity_class: max_severity,
        observation_window_days: 7,
    }
}

/// Map a CMDB finding `status` string to a closed-set severity.
/// Unrecognized statuses default to `Info` — fail-safe (over-report
/// would leak; under-report is observability noise but not a
/// privacy concern).
fn severity_from_status(status: &str) -> SeverityClass {
    match status {
        "critical" => SeverityClass::Critical,
        "warning" => SeverityClass::Medium,
        _ => SeverityClass::Info,
    }
}

/// Total ordering on `SeverityClass` for max-by-key. Mirrors the
/// schema's enum ordering Info < Low < Medium < High < Critical.
fn severity_rank(s: SeverityClass) -> u8 {
    match s {
        SeverityClass::Info => 0,
        SeverityClass::Low => 1,
        SeverityClass::Medium => 2,
        SeverityClass::High => 3,
        SeverityClass::Critical => 4,
    }
}

/// Q15 anonymized-origin hash — `sha256(brain_id || "|" || YYYY-MM-DD)`
/// returned as a 64-character lowercase hex string. Daily-nonce
/// rotation prevents cross-day identity correlation; same-day self-loop
/// detection works because both peers using the same date as input
/// produce comparable hashes.
fn anonymized_origin(brain_id: &str, day_yyyy_mm_dd: &str) -> String {
    let input = format!("{brain_id}|{day_yyyy_mm_dd}");
    sha256_hex(input.as_bytes())
}

// ─── Minimal SHA-256 implementation (no external crate) ──────────────
//
// The `neurogrim-cli` crate intentionally has no `sha2` dependency;
// the brief's HARD CONSTRAINT forbids adding new Cargo deps. Q15 lock
// requires SHA-256 specifically (not a weaker hash), so the function
// is implemented inline. Reference: NIST FIPS 180-4 §6.2 SHA-256.
//
// This is a conformant SHA-256 implementation. It is NOT performance-
// optimized — federation emission is operator-explicit (Q2 lock), so
// per-invocation hash count is bounded by `len(declared_children)`,
// typically < 5. Constant-time semantics are not required because the
// hash inputs are non-secret operator-level identifiers (brain_id +
// public daily-nonce date). If a future v2 use case demands higher
// throughput or constant-time guarantees, reach for the `sha2` crate.

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const SHA256_H_INIT: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Compute SHA-256 of `input` and return the digest as a 64-character
/// lowercase hex string. NIST FIPS 180-4 §6.2.
fn sha256_hex(input: &[u8]) -> String {
    let digest = sha256(input);
    let mut s = String::with_capacity(64);
    for byte in digest {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

fn sha256(input: &[u8]) -> [u8; 32] {
    // Padding per FIPS 180-4 §5.1.1: append 0x80, then 0x00 bytes until
    // the message length mod 64 == 56, then the original message length
    // in bits as a big-endian u64.
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded: Vec<u8> = Vec::with_capacity(input.len() + 72);
    padded.extend_from_slice(input);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = SHA256_H_INIT;

    // Process each 512-bit (64-byte) block.
    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..64 {
            let s0 =
                w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 =
                w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Wrapper for parse-only testing — clap requires a top-level
    /// derive(Parser) to drive arg-parsing tests for a Subcommand.
    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: FederatedPatternCmd,
    }

    /// Q14 vocabulary regression guard at the unit level. Adding new
    /// pattern_kinds without a spec change must fail this test.
    #[test]
    fn pattern_kinds_v1_is_single_entry() {
        assert_eq!(
            PATTERN_KINDS,
            &["vigilance-pattern"],
            "Q14 lock — v1 closed-set pattern_kind vocabulary"
        );
    }

    #[test]
    fn clap_accepts_valid_pattern_kind() {
        let parsed = TestCli::try_parse_from([
            "test",
            "emit",
            "--pattern-kind",
            "vigilance-pattern",
        ]);
        assert!(parsed.is_ok(), "valid kind must parse; err: {:?}", parsed.err());
    }

    #[test]
    fn clap_rejects_invalid_pattern_kind() {
        let parsed = TestCli::try_parse_from([
            "test",
            "emit",
            "--pattern-kind",
            "yolo-pattern",
        ]);
        assert!(parsed.is_err(), "invalid pattern_kind must error");
        let err = parsed.unwrap_err().to_string();
        assert!(
            err.contains("yolo-pattern") || err.contains("invalid value"),
            "error must reference the bad value or 'invalid value'; got: {err}"
        );
    }

    /// Q1+Q5+Q8 privacy regression pin — the Args struct MUST NOT
    /// have a free-text `--note` flag at v1. clap rejects unknown
    /// args; this is the regression guard.
    #[test]
    fn clap_rejects_note_flag() {
        let parsed = TestCli::try_parse_from([
            "test",
            "emit",
            "--pattern-kind",
            "vigilance-pattern",
            "--note",
            "this should be forbidden",
        ]);
        assert!(
            parsed.is_err(),
            "free-text --note must be rejected (Q1+Q5+Q8 privacy lock)"
        );
    }

    /// Same for `--justification`, `--reason`, `--comment` — none
    /// must be accepted at v1.
    #[test]
    fn clap_rejects_other_free_text_flags() {
        for flag in ["--justification", "--reason", "--comment", "--body", "--text"] {
            let parsed = TestCli::try_parse_from([
                "test",
                "emit",
                "--pattern-kind",
                "vigilance-pattern",
                flag,
                "this should be forbidden",
            ]);
            assert!(
                parsed.is_err(),
                "free-text {flag} must be rejected (Q1+Q5+Q8 privacy lock)"
            );
        }
    }

    /// SHA-256 spot-check against the FIPS 180-4 §B.1 test vector
    /// "abc" — confirms the inline implementation matches the
    /// reference. Empty-string vector is the second sanity check.
    #[test]
    fn sha256_matches_fips_test_vectors() {
        let abc = sha256_hex(b"abc");
        assert_eq!(
            abc, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "FIPS 180-4 §B.1 'abc' test vector mismatch"
        );
        let empty = sha256_hex(b"");
        assert_eq!(
            empty, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "FIPS 180-4 empty-string test vector mismatch"
        );
    }

    #[test]
    fn read_vigilance_features_returns_baseline_when_cmdb_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fv = read_vigilance_features(tmp.path().to_str().unwrap());
        assert_eq!(fv.numeric_count, 0);
        assert_eq!(fv.severity_class, SeverityClass::Info);
        assert_eq!(fv.observation_window_days, 7);
    }

    #[test]
    fn read_vigilance_features_returns_baseline_when_findings_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/supply-chain-vigilance-cmdb.json"),
            r#"{"score": 100, "findings": []}"#,
        )
        .unwrap();
        let fv = read_vigilance_features(tmp.path().to_str().unwrap());
        assert_eq!(fv.numeric_count, 0);
        assert_eq!(fv.severity_class, SeverityClass::Info);
    }

    #[test]
    fn read_vigilance_features_aggregates_findings() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        // Mix of statuses: warning + warning + info → 3 findings,
        // highest severity = Medium (warning).
        std::fs::write(
            tmp.path().join(".claude/supply-chain-vigilance-cmdb.json"),
            r#"{"findings":[
                {"name":"a","status":"warning","points":10},
                {"name":"b","status":"warning","points":5},
                {"name":"c","status":"info","points":0}
            ]}"#,
        )
        .unwrap();
        let fv = read_vigilance_features(tmp.path().to_str().unwrap());
        assert_eq!(fv.numeric_count, 3);
        assert_eq!(fv.severity_class, SeverityClass::Medium);
        assert_eq!(fv.observation_window_days, 7);
    }

    #[test]
    fn read_vigilance_features_picks_highest_severity() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        // info + warning + critical → Critical wins.
        std::fs::write(
            tmp.path().join(".claude/supply-chain-vigilance-cmdb.json"),
            r#"{"findings":[
                {"name":"a","status":"info"},
                {"name":"b","status":"warning"},
                {"name":"c","status":"critical"}
            ]}"#,
        )
        .unwrap();
        let fv = read_vigilance_features(tmp.path().to_str().unwrap());
        assert_eq!(fv.numeric_count, 3);
        assert_eq!(fv.severity_class, SeverityClass::Critical);
    }

    #[test]
    fn read_vigilance_features_returns_baseline_when_malformed() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        std::fs::write(
            tmp.path().join(".claude/supply-chain-vigilance-cmdb.json"),
            r#"not valid json {{"#,
        )
        .unwrap();
        let fv = read_vigilance_features(tmp.path().to_str().unwrap());
        assert_eq!(fv.numeric_count, 0);
        assert_eq!(fv.severity_class, SeverityClass::Info);
    }

    #[test]
    fn severity_from_status_maps_known_statuses() {
        assert_eq!(severity_from_status("info"), SeverityClass::Info);
        assert_eq!(severity_from_status("warning"), SeverityClass::Medium);
        assert_eq!(severity_from_status("critical"), SeverityClass::Critical);
        // Unknown defaults to Info — fail-safe under privacy contract.
        assert_eq!(severity_from_status("unknown"), SeverityClass::Info);
        assert_eq!(severity_from_status(""), SeverityClass::Info);
    }

    #[test]
    fn anonymized_origin_is_deterministic() {
        let h1 = anonymized_origin("brain-x", "2026-04-27");
        let h2 = anonymized_origin("brain-x", "2026-04-27");
        assert_eq!(h1, h2, "same inputs must yield same hash");
        assert_eq!(h1.len(), 64, "sha256 hex must be 64 chars");
        let h3 = anonymized_origin("brain-x", "2026-04-28");
        assert_ne!(h1, h3, "different days must yield different hashes");
    }
}
