//! **S6-DB-4 Dual Brain Pair Integration Test** — the end-to-end proof that
//! two real `motherbrain a2a-serve` processes can talk A2A over loopback and
//! aggregate into an ecosystem score (spec §9 fractal composition, §13 A2A).
//!
//! # What this file proves
//!
//! Previous tests exercised:
//! - the A2A wire protocol (in-process `TaskServer` + `TaskClient`, 19 tests)
//! - the ecosystem dispatch pipeline with an in-process server (6 contract tests)
//! - the CLI wiring (2 in-process integration tests)
//!
//! What was *missing* — and what this file adds — is the live proof that when
//! the **binary** is invoked, two real processes over a real TCP socket
//! produce the same round-trip the in-process tests proved. Spec §9.7 is a
//! conformance requirement; without this layer of evidence we could ship a
//! regression in the CLI wiring and every other test would still pass.
//!
//! # Explicit non-goals (scope honesty)
//!
//! - **Proactive emission (`score.updated` → `ecosystem.scored`) is OUT OF
//!   SCOPE for this phase.** S6-DB-3's server infrastructure responds to
//!   incoming `snapshot.requested` but does not proactively emit. Implementing
//!   a timer/hook that fires `score.updated` to peers is future work (S6-DB-5
//!   or later). This test file proves the *request-response* direction of
//!   §10.4 only. Look for the `TODO(S6-DB-5+)` comments to find the seams
//!   where emission would slot in.
//!
//! - **Cross-host transport** — we only exercise loopback (127.0.0.1). The
//!   HTTP transport itself works the same over any network, but auth,
//!   retries, and timeouts under real network conditions belong to a later
//!   hardening pass.
//!
//! # Schema validation claim
//!
//! The S6-DB-2 contract tests establish that Rust deserialization into
//! `AgentOutput` / `A2aEnvelope` is equivalent to JSON Schema validation
//! against `agent-output-v1.schema.json` / `a2a-envelope-v1.schema.json` —
//! the Rust types were code-generated from those schemas. We rely on that
//! equivalence here rather than pulling in a full JSON Schema validator crate
//! (cheaper, and the two are guaranteed in lock-step). A dedicated test
//! (`dual_brain_envelope_validates_against_schema_at_every_hop`) documents
//! this by asserting the structural invariants a schema would catch.
//!
//! # Cultural notes (`.claude/culture.yaml`)
//!
//! - **Honesty:** subprocess stderr is captured and surfaced on test failure.
//!   Reliability notes embedded where the test makes a trade-off (port-bind
//!   race documented, not hidden).
//! - **Integrity:** `ChildGuard` RAII kills the subprocess on drop, so a
//!   panic mid-test still cleans up the child.
//! - **Critical-but-kind:** assertions name what went wrong and how to find
//!   the log line that shows it, so a future debugger isn't left guessing.

use std::net::TcpListener as StdTcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use chrono::Utc;
use motherbrain_core::agent_output::AgentOutput;
use motherbrain_core::ecosystem::{ChildEntry, ChildStatus, ChildTransport, EcosystemRegistry};
use motherbrain_ecosystem::score_ecosystem;
use serde_json::json;
use url::Url;

// ---------------------------------------------------------------------------
// RAII subprocess cleanup
// ---------------------------------------------------------------------------

/// Wraps a `std::process::Child` so `Drop` kills the process. If a test
/// panics after spawning a peer but before explicitly stopping it, the OS
/// still reaps the child.
///
/// On Windows, `Child::kill()` calls `TerminateProcess`, which is an
/// ungraceful shutdown. That's the right call for tests — we're not
/// measuring clean-shutdown behavior here, we're making sure stray
/// `motherbrain.exe` processes don't accumulate across test runs.
struct ChildGuard {
    name: String,
    child: Option<Child>,
}

impl ChildGuard {
    fn new(name: impl Into<String>, child: Child) -> Self {
        Self {
            name: name.into(),
            child: Some(child),
        }
    }

    /// Take ownership so the caller can wait on exit explicitly. Useful at
    /// the end of the happy-path test where we want to surface stderr if the
    /// subprocess died early.
    fn into_inner(mut self) -> Child {
        self.child.take().expect("child already taken")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Best-effort kill. `kill` returns an error if the process already
            // exited — that's fine. We never panic from Drop; tests with a
            // genuine failure should see the original failure, not a drop
            // secondary explosion.
            let _ = child.kill();
            let _ = child.wait();
            // No println! here — silent cleanup keeps test output readable.
            let _ = &self.name; // suppress dead_code if we ever stop using it
        }
    }
}

// ---------------------------------------------------------------------------
// Test helpers — port allocation, fixture building, subprocess spawn, wait
// ---------------------------------------------------------------------------

/// Bind an OS-picked ephemeral port on 127.0.0.1, read the assigned port,
/// and drop the listener so the subprocess can re-bind it.
///
/// **Race honesty:** between the drop and the subprocess's bind, another
/// process on the host could theoretically grab the port. In practice, on
/// loopback, with immediate re-bind by the child we spawn a few milliseconds
/// later, the window is small enough for test purposes. If this test ever
/// becomes flaky on a busy CI host, the right fix is a CLI enhancement to
/// `a2a-serve` that accepts `--port 0` and prints the bound port to stdout.
/// That's proposed as future work — not implemented here to keep this patch
/// scoped to the test harness.
fn find_free_loopback_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("bind loopback ephemeral port");
    let port = listener.local_addr().expect("read local_addr").port();
    drop(listener);
    port
}

/// Create a minimal project root under a tempdir, structured like a real
/// Brain project: `.claude/brain-registry.json` + a single CMDB stub.
///
/// The caller owns the returned `TempDir` — dropping it removes the whole
/// tree. Keep it alive for the duration of the test or the subprocess will
/// start failing mid-run.
///
/// # Fixture choices (scoring-honest)
///
/// We declare exactly one weighted domain (`test-health`) with a fixed CMDB
/// score of `80`. Single-domain keeps `weight_sum` trivially `1.0` (required
/// by `BrainRegistry::validate`). The `80` isn't magic — it's a non-zero,
/// non-round number chosen so an all-zeros fallback would be visible. This
/// is a plumbing test, not a scoring-math test.
fn build_minimal_project_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create tempdir for project root");
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).expect("create .claude/");

    let registry_json = json!({
        "meta": {
            "schema_version": "2",
            "description": "Pair integration test fixture",
            "updated_by": "dual-brain-pair-test"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": { "test-health": 1.0 },
            "advisory_domains": [],
            "principle_map": { "test-health": "Test Health" },
            "domain_definitions": {
                "test-health": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/test-health-cmdb.json"
                    }
                }
            }
        }
    });
    std::fs::write(
        claude.join("brain-registry.json"),
        serde_json::to_string_pretty(&registry_json).unwrap(),
    )
    .expect("write brain-registry.json");

    // CMDB stub. `updated_at` is RFC 3339 so the scorer parses it as a real
    // timestamp. `score: 80` matches the fixture comment above.
    let cmdb_json = json!({
        "meta": {
            "schema_version": "1",
            "updated_at": Utc::now().to_rfc3339(),
            "updated_by": "dual-brain-pair-test"
        },
        "score": 80,
        "updated_at": Utc::now().to_rfc3339()
    });
    std::fs::write(
        claude.join("test-health-cmdb.json"),
        serde_json::to_string_pretty(&cmdb_json).unwrap(),
    )
    .expect("write test-health-cmdb.json");

    dir
}

/// Spawn `motherbrain a2a-serve --port N --project-root <dir>` as a real
/// subprocess. Returns a `ChildGuard` that kills the process on drop.
///
/// We locate the binary via `env!("CARGO_BIN_EXE_motherbrain")` — cargo
/// exports that for integration tests of binary crates, so we don't have to
/// hard-code a path relative to `target/`.
///
/// Stdio: stdin null, stdout/stderr piped. We leak stdout (the server's
/// human banner) but capture stderr so a crash surfaces with its error
/// message on assertion failure.
fn spawn_peer_server(port: u16, project_root: &Path) -> ChildGuard {
    let bin = env!("CARGO_BIN_EXE_motherbrain");
    let child = Command::new(bin)
        .arg("a2a-serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--project-root")
        .arg(project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn motherbrain a2a-serve on port {port}: {e}"));
    ChildGuard::new(format!("peer@{port}"), child)
}

/// Poll `{endpoint_base}/.well-known/agent-card.json` until 200 OK or the
/// deadline passes.
///
/// `endpoint_base` should be the authority root (e.g. `http://127.0.0.1:NNNN`)
/// — we append the well-known path per RFC 5785 (same rule the A2A client
/// uses; see S6-DB-3's URL bug fix for the history).
///
/// Returns `Err(String)` with a human-readable reason on timeout. The
/// message names the port and the last error we saw, which keeps the
/// failure actionable without needing to re-run with tracing enabled.
async fn wait_for_ready(endpoint_base: &Url, timeout_secs: u64) -> Result<(), String> {
    let card_url = endpoint_base
        .join("/.well-known/agent-card.json")
        .map_err(|e| format!("cannot build well-known URL from {endpoint_base}: {e}"))?;
    let client = reqwest::Client::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut last_err: String = "no attempts made".into();
    while std::time::Instant::now() < deadline {
        match client.get(card_url.clone()).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => {
                last_err = format!("HTTP {} from {}", resp.status(), card_url);
            }
            Err(e) => {
                last_err = format!("connect error to {}: {}", card_url, e);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "peer at {} did not become ready within {}s (last: {})",
        endpoint_base, timeout_secs, last_err
    ))
}

/// Drain whatever stderr the subprocess has already produced and return it
/// as a string. Used on assertion failure so the test report names the
/// actual failure from the child's perspective rather than just the
/// parent's network-level symptom.
fn drain_stderr_nonblocking(child: &mut Child) -> String {
    use std::io::Read;
    let mut buf = Vec::new();
    if let Some(stderr) = child.stderr.as_mut() {
        // Best-effort — we don't care if the read blocks briefly. This is
        // only called on the failure path where the subprocess is already
        // being torn down.
        let _ = stderr.read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Build the A2A endpoint URL the child server publishes at `host:port`.
/// Matches the format in `commands/a2a_serve.rs` so tests don't drift if
/// the server's endpoint convention changes.
fn endpoint_for_port(port: u16) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}/a2a/v1/")).expect("static URL must parse")
}

/// Authority root URL — what we hand to `wait_for_ready` to hit the
/// well-known agent card location.
fn authority_root_for_port(port: u16) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}/")).expect("static URL must parse")
}

/// Build a minimal parent `AgentOutput` for use as the "parent" side of
/// `score_ecosystem`. A plain score with no domains — the parent's own
/// contribution to aggregation is controlled via `parent_weight`.
fn minimal_parent_output(score: u8) -> AgentOutput {
    let value = json!({
        "schema_version": "1",
        "scored_at": Utc::now().to_rfc3339(),
        "score": score,
        "domains": {},
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {},
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    });
    serde_json::from_value(value).expect("minimal AgentOutput must deserialize")
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

/// **Headline test.** Two real `motherbrain a2a-serve` processes, each
/// serving its own project root, aggregated through `score_ecosystem`.
///
/// This is the request-response direction of §9.7 end-to-end. If this passes,
/// the CLI wiring + the real scoring pipeline + the A2A transport all agree
/// on the wire contract when run as *processes*, not just as in-process
/// async tasks.
#[tokio::test]
async fn fractal_composition_end_to_end_over_loopback() {
    // --- Project roots ---
    let dir_alpha = build_minimal_project_root();
    let dir_beta = build_minimal_project_root();

    // --- Ports ---
    let port_alpha = find_free_loopback_port();
    // Re-bind-drop again for beta. The two calls don't share state, so the
    // second picks a different ephemeral port with overwhelming probability.
    // Documented above: this is the acceptable race we take on.
    let port_beta = find_free_loopback_port();
    assert_ne!(
        port_alpha, port_beta,
        "ephemeral allocator returned duplicate ports — extraordinarily rare, \
         but retry the test run if it does"
    );

    // --- Spawn peers ---
    let mut guard_alpha = Some(spawn_peer_server(port_alpha, dir_alpha.path()));
    let mut guard_beta = Some(spawn_peer_server(port_beta, dir_beta.path()));

    // --- Readiness barrier ---
    // Use a short helper closure so a timeout drains stderr and surfaces it
    // in the panic message — the most useful possible failure mode.
    let alpha_root = authority_root_for_port(port_alpha);
    let beta_root = authority_root_for_port(port_beta);
    if let Err(e) = wait_for_ready(&alpha_root, 10).await {
        let stderr =
            drain_stderr_nonblocking(guard_alpha.as_mut().unwrap().child.as_mut().unwrap());
        panic!("alpha peer never became ready: {e}\n---stderr---\n{stderr}");
    }
    if let Err(e) = wait_for_ready(&beta_root, 10).await {
        let stderr = drain_stderr_nonblocking(guard_beta.as_mut().unwrap().child.as_mut().unwrap());
        panic!("beta peer never became ready: {e}\n---stderr---\n{stderr}");
    }

    // --- Registry wiring ---
    let registry = EcosystemRegistry {
        children: vec![
            ChildEntry {
                id: "alpha".into(),
                display_name: Some("Alpha (subprocess peer)".into()),
                transport: ChildTransport::A2A {
                    a2a_endpoint: endpoint_for_port(port_alpha),
                    agent_card_url: None,
                },
                interface_version: "1".into(),
                depends_on: vec![],
                weight: 1.0,
                enabled: true,
            },
            ChildEntry {
                id: "beta".into(),
                display_name: Some("Beta (subprocess peer)".into()),
                transport: ChildTransport::A2A {
                    a2a_endpoint: endpoint_for_port(port_beta),
                    agent_card_url: None,
                },
                interface_version: "1".into(),
                depends_on: vec![],
                weight: 1.0,
                enabled: true,
            },
        ],
    };

    // --- Parent + pipeline ---
    let parent = minimal_parent_output(80);
    let result = score_ecosystem(parent, 1.0, registry).await;

    // On error, surface both stderrs so the failure report names what the
    // subprocesses actually said.
    if let Err(e) = &result {
        let alpha_stderr =
            drain_stderr_nonblocking(guard_alpha.as_mut().unwrap().child.as_mut().unwrap());
        let beta_stderr =
            drain_stderr_nonblocking(guard_beta.as_mut().unwrap().child.as_mut().unwrap());
        panic!(
            "score_ecosystem failed: {e}\n\
             ---alpha stderr---\n{alpha_stderr}\n\
             ---beta stderr---\n{beta_stderr}"
        );
    }
    let score = result.unwrap();

    // --- Assertions ---
    // 1. Both children present with Ok status — the core §9.7 conformance
    //    claim at the process boundary.
    assert_eq!(
        score.child_statuses.len(),
        2,
        "expected 2 child_statuses, got {}: {:?}",
        score.child_statuses.len(),
        score.child_statuses
    );
    assert_eq!(
        score.child_statuses.get("alpha"),
        Some(&ChildStatus::Ok),
        "alpha child must be Ok; full statuses: {:?}",
        score.child_statuses
    );
    assert_eq!(
        score.child_statuses.get("beta"),
        Some(&ChildStatus::Ok),
        "beta child must be Ok; full statuses: {:?}",
        score.child_statuses
    );

    // 2. No per-child errors recorded.
    assert!(
        score.child_errors.is_empty(),
        "expected no child_errors; got: {:?}",
        score.child_errors
    );

    // 3. ecosystem_score is a valid u8 in [0, 100]. u8 type bounds it on
    //    the top side via `clamp` in `aggregate`; we also check the lower
    //    bound is non-zero since both children produced score 80 and the
    //    parent is 80 too — aggregate should be around 80.
    //    (We avoid asserting an exact number here because the freshness
    //    and confidence multipliers depend on clock + domain shape; the
    //    contract test `two_children_mixed_transports_hand_computed_aggregate`
    //    pins the exact arithmetic with a hand-computed expectation.)
    assert!(
        score.ecosystem_score <= 100,
        "ecosystem_score must be <= 100; got {}",
        score.ecosystem_score
    );
    assert!(
        score.ecosystem_score > 0,
        "ecosystem_score should be > 0 for 3 healthy contributors at score 80; got {}",
        score.ecosystem_score
    );

    // --- Cleanup + post-mortem ---
    // Take the guards out so we can wait on each child and check its
    // stderr didn't contain a panic. Panics in the child would not fail
    // this test otherwise (the child still serves the snapshot), so we
    // explicitly check.
    let mut child_alpha = guard_alpha.take().unwrap().into_inner();
    let mut child_beta = guard_beta.take().unwrap().into_inner();
    let _ = child_alpha.kill();
    let _ = child_beta.kill();
    let alpha_err = drain_stderr_nonblocking(&mut child_alpha);
    let beta_err = drain_stderr_nonblocking(&mut child_beta);
    let _ = child_alpha.wait();
    let _ = child_beta.wait();
    assert!(
        !alpha_err.contains("panicked at"),
        "alpha subprocess panicked:\n{alpha_err}"
    );
    assert!(
        !beta_err.contains("panicked at"),
        "beta subprocess panicked:\n{beta_err}"
    );

    // Keep tempdirs alive to here explicitly — dropping them earlier would
    // remove the registry before the child shuts down, and we'd see scary
    // "file not found" noise in stderr that isn't actually a bug.
    drop(dir_alpha);
    drop(dir_beta);
}

/// **Negative path.** One live peer, one pointed at a port nobody bound.
/// The live peer succeeds; the dead one is reported as an error without
/// crashing the harness.
///
/// This is the operational failure mode an adopter is most likely to hit:
/// a child project registered in their ecosystem config but whose peer
/// process isn't running. The pipeline must surface the problem rather
/// than silently drop to a lower score.
#[tokio::test]
async fn dual_brain_peer_unreachable_is_reported_cleanly() {
    let dir_alive = build_minimal_project_root();
    let port_alive = find_free_loopback_port();
    // Pick a second port and *do not* spawn a peer for it. The allocator
    // will give us a port that was momentarily bound, so the OS won't
    // think it's still in use — there's just nothing listening.
    let port_dead = find_free_loopback_port();

    let mut guard_alive = Some(spawn_peer_server(port_alive, dir_alive.path()));

    let alive_root = authority_root_for_port(port_alive);
    if let Err(e) = wait_for_ready(&alive_root, 10).await {
        let stderr =
            drain_stderr_nonblocking(guard_alive.as_mut().unwrap().child.as_mut().unwrap());
        panic!("alive peer never became ready: {e}\n---stderr---\n{stderr}");
    }

    let registry = EcosystemRegistry {
        children: vec![
            ChildEntry {
                id: "alive".into(),
                display_name: None,
                transport: ChildTransport::A2A {
                    a2a_endpoint: endpoint_for_port(port_alive),
                    agent_card_url: None,
                },
                interface_version: "1".into(),
                depends_on: vec![],
                weight: 1.0,
                enabled: true,
            },
            ChildEntry {
                id: "dead".into(),
                display_name: None,
                transport: ChildTransport::A2A {
                    a2a_endpoint: endpoint_for_port(port_dead),
                    agent_card_url: None,
                },
                interface_version: "1".into(),
                depends_on: vec![],
                weight: 1.0,
                enabled: true,
            },
        ],
    };

    let parent = minimal_parent_output(75);
    let score = score_ecosystem(parent, 1.0, registry)
        .await
        .expect("pipeline must not crash when one peer is unreachable");

    // The live peer succeeded.
    assert_eq!(
        score.child_statuses.get("alive"),
        Some(&ChildStatus::Ok),
        "alive peer must be Ok; full statuses: {:?}",
        score.child_statuses
    );
    // The dead peer surfaced as Error — and the error message names the
    // failure, not a generic "something went wrong".
    assert_eq!(
        score.child_statuses.get("dead"),
        Some(&ChildStatus::Error),
        "dead peer must be Error; full statuses: {:?}",
        score.child_statuses
    );
    let dead_msg = score
        .child_errors
        .get("dead")
        .expect("dead peer must have an error message attached");
    // The message should mention A2A transport so adopters can grep for
    // the failure mode. `discover` returns `AgentCardUnreachable` for an
    // unreachable endpoint; either the string "A2A" (from the wrapping
    // variant) or "unreachable"/"connection" (from the inner text) is
    // kind enough to the reader.
    assert!(
        dead_msg.to_lowercase().contains("a2a")
            || dead_msg.to_lowercase().contains("unreachable")
            || dead_msg.to_lowercase().contains("connection"),
        "dead peer error should name the A2A failure; got: {dead_msg}"
    );

    drop(guard_alive);
    drop(dir_alive);
}

/// **Schema-validation-at-every-hop.** After a successful round-trip, pluck
/// the child's `AgentOutput` and assert the shape a JSON Schema validator
/// would catch. This documents the equivalence claim from S6-DB-2: Rust
/// deserialization into `AgentOutput` / `A2aEnvelope` *is* schema validation,
/// because the Rust types were generated from the schemas.
///
/// If this test ever diverges from `agent-output-v1.schema.json`, one of
/// two things has happened: the Rust types drifted from the schema (bug —
/// regenerate), or the schema evolved to v2 (bump `schema_version` here
/// and add a migration test). Both cases are actionable from this failure.
#[tokio::test]
async fn dual_brain_envelope_validates_against_schema_at_every_hop() {
    let dir = build_minimal_project_root();
    let port = find_free_loopback_port();
    let mut guard = Some(spawn_peer_server(port, dir.path()));

    let root = authority_root_for_port(port);
    if let Err(e) = wait_for_ready(&root, 10).await {
        let stderr = drain_stderr_nonblocking(guard.as_mut().unwrap().child.as_mut().unwrap());
        panic!("peer never became ready: {e}\n---stderr---\n{stderr}");
    }

    // We invoke through `score_ecosystem` — same path the headline test
    // uses — but with a single child so we can inspect its output directly.
    // We need a way to get the child's agent_output back; `score_ecosystem`
    // aggregates it away. For this test we use the lower-level
    // `motherbrain_ecosystem::invoke_child` which returns the raw
    // `AgentOutput` before aggregation.
    let entry = ChildEntry {
        id: "under-test".into(),
        display_name: None,
        transport: ChildTransport::A2A {
            a2a_endpoint: endpoint_for_port(port),
            agent_card_url: None,
        },
        interface_version: "1".into(),
        depends_on: vec![],
        weight: 1.0,
        enabled: true,
    };
    let child_output = motherbrain_ecosystem::invoke_child(&entry)
        .await
        .expect("invoke_child over real subprocess must succeed");

    // --- Structural invariants a JSON Schema validator would check ---

    // `schema_version == "1"` — the live contract version.
    assert_eq!(
        child_output.schema_version, "1",
        "agent-output schema_version must be \"1\"; got {:?}",
        child_output.schema_version
    );

    // `scored_at` is RFC 3339 — the aggregator relies on this to compute
    // freshness. A malformed value would surface as a child Error.
    let scored_at =
        chrono::DateTime::parse_from_rfc3339(&child_output.scored_at).unwrap_or_else(|e| {
            panic!(
                "scored_at must be RFC 3339; got {:?}, err: {e}",
                child_output.scored_at
            )
        });
    // And it's recent — within a reasonable window around now. If this
    // fires, the child's clock is drifting or the scoring pipeline is
    // reporting stale output. Either is a real bug, not test flakiness.
    let now = Utc::now();
    let delta = (now - scored_at.with_timezone(&Utc)).num_seconds().abs();
    assert!(
        delta < 300,
        "scored_at should be within 5 minutes of now; delta={delta}s"
    );

    // `domains` is a map (even if empty in our fixture — the minimal
    // CMDB path still populates a single domain). The Rust type is
    // `HashMap<String, AgentDomain>`, so the existence of this field
    // with the right shape is guaranteed by deserialization; we still
    // exercise it to document the invariant.
    // Our fixture declares one weighted domain (`test-health`), so the
    // child should have scored it.
    assert!(
        child_output.domains.contains_key("test-health"),
        "child should have scored the `test-health` domain declared in its fixture; \
         got domains: {:?}",
        child_output.domains.keys().collect::<Vec<_>>()
    );

    // Overall score is bounded [0, 100].
    assert!(
        child_output.score <= 100,
        "score must be <= 100; got {}",
        child_output.score
    );

    // Envelope-level validation is embedded in `TaskClient::invoke` itself:
    // it rejects responses whose envelope `schema_version != "1"`, and the
    // fact that `invoke_child` returned `Ok` above is proof the envelope
    // passed that check. Documented here because the spec calls this out
    // as a per-hop validation requirement.

    drop(guard);
    drop(dir);
}
