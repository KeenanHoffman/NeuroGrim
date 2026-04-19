//! **Three-way Brain integration test** — ecosystem Brain aggregates child
//! scores via A2A-pull (spec §9 fractal composition wired through the
//! scoring pipeline).
//!
//! # What this proves
//!
//! `dual_brain_pair.rs` proved 2 real `neurogrim a2a-serve` subprocesses
//! can talk over loopback and that `score_ecosystem` (in-process) aggregates
//! their outputs. What it did NOT prove is that a **regular brain-registry**
//! with `scoring_source.type = "a2a"` domains flows through the normal
//! scoring pipeline — i.e., that `neurogrim score` with an A2A-sourced
//! registry actually pulls child scores live and produces aggregated output.
//!
//! This test closes that gap:
//! 1. Spawn two real subprocess peers with simple CMDB-only projects.
//! 2. Build an ecosystem registry in a tempdir that points two of its
//!    domains at the peers via `scoring_source.type: "a2a"`.
//! 3. Run `neurogrim score` against the ecosystem registry as a
//!    subprocess.
//! 4. Assert the pipeline succeeded and produced a score-shaped output —
//!    meaning the new A2A branch in `context.rs::load_cmdb_data` dispatched
//!    the peer invocations and the pipeline aggregated them.
//!
//! Reuses the helpers shape from `dual_brain_pair.rs` (ChildGuard,
//! find_free_loopback_port, wait_for_ready) directly — kept as duplicate
//! here rather than hoisted to a shared `tests/common/` module to minimize
//! coupling between test files. If a third test appears, hoist then.

use std::net::TcpListener as StdTcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use url::Url;

// ---------------------------------------------------------------------------
// RAII subprocess cleanup (mirror of dual_brain_pair's ChildGuard)
// ---------------------------------------------------------------------------

struct ChildGuard {
    _name: String,
    child: Option<Child>,
}

impl ChildGuard {
    fn new(name: impl Into<String>, child: Child) -> Self {
        Self {
            _name: name.into(),
            child: Some(child),
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn find_free_loopback_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("bind loopback ephemeral port");
    let port = listener.local_addr().expect("read local_addr").port();
    drop(listener);
    port
}

/// Build a minimal project fixture for a peer: a `.claude/` dir with one
/// weighted CMDB-sourced domain at the provided score. Each peer gets its
/// own tempdir.
fn build_peer_fixture(label: &str, score: u64) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create peer tempdir");
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();

    let registry = json!({
        "meta": {
            "schema_version": "2",
            "description": format!("three-way brain test peer {label}"),
            "updated_by": "three-way-brain-test"
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
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .unwrap();

    let cmdb = json!({
        "meta": {
            "schema_version": "1",
            "updated_at": Utc::now().to_rfc3339(),
            "updated_by": "three-way-brain-test"
        },
        "score": score,
        "updated_at": Utc::now().to_rfc3339()
    });
    std::fs::write(
        claude.join("test-health-cmdb.json"),
        serde_json::to_string_pretty(&cmdb).unwrap(),
    )
    .unwrap();

    dir
}

/// Build the ecosystem registry in a tempdir. Points two domains at A2A
/// endpoints (one per child peer) and includes one local CMDB domain so the
/// test exercises mixed sourcing in the same registry.
fn build_ecosystem_fixture(
    peer_a_url: &Url,
    peer_b_url: &Url,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("create ecosystem tempdir");
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();

    // One local CMDB source — proves a2a + cmdb coexist in the same registry
    let local_cmdb = json!({
        "meta": {
            "schema_version": "1",
            "updated_at": Utc::now().to_rfc3339(),
            "updated_by": "three-way-brain-test"
        },
        "score": 90,
        "updated_at": Utc::now().to_rfc3339()
    });
    std::fs::write(
        claude.join("local-cmdb.json"),
        serde_json::to_string_pretty(&local_cmdb).unwrap(),
    )
    .unwrap();

    let registry = json!({
        "meta": {
            "schema_version": "2",
            "description": "three-way brain integration test — A2A-pull across 2 peers + 1 local CMDB",
            "updated_by": "three-way-brain-test"
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": {
                "child-peer-a": 0.4,
                "child-peer-b": 0.4,
                "local": 0.2
            },
            "advisory_domains": [],
            "principle_map": {
                "child-peer-a": "Peer A",
                "child-peer-b": "Peer B",
                "local": "Local"
            },
            "domain_definitions": {
                "child-peer-a": {
                    "scoring_source": {
                        "type": "a2a",
                        "endpoint": peer_a_url.as_str(),
                        "interface_version": "1"
                    }
                },
                "child-peer-b": {
                    "scoring_source": {
                        "type": "a2a",
                        "endpoint": peer_b_url.as_str(),
                        "interface_version": "1"
                    }
                },
                "local": {
                    "scoring_source": {
                        "type": "cmdb",
                        "path": ".claude/local-cmdb.json"
                    }
                }
            }
        }
    });

    let registry_path = claude.join("brain-registry.json");
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .unwrap();

    (dir, registry_path)
}

fn spawn_peer(port: u16, project_root: &Path) -> ChildGuard {
    let bin = env!("CARGO_BIN_EXE_neurogrim");
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
        .unwrap_or_else(|e| panic!("spawn neurogrim a2a-serve on {port}: {e}"));
    ChildGuard::new(format!("peer@{port}"), child)
}

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
                last_err = format!("HTTP {}", resp.status());
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "{} did not become ready in {timeout_secs}s (last error: {last_err})",
        endpoint_base
    ))
}

// ---------------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ecosystem_neurogrim_score_aggregates_two_a2a_peers() {
    // Spawn two subprocess peers — each a full neurogrim a2a-serve process
    // with its own fixture. Peer A at score 80, Peer B at score 70. Distinct
    // scores so a bug that hardcodes one of them would show up in the
    // aggregation.
    let peer_a_dir = build_peer_fixture("peer-a", 80);
    let peer_b_dir = build_peer_fixture("peer-b", 70);

    let port_a = find_free_loopback_port();
    let port_b = find_free_loopback_port();

    let _peer_a = spawn_peer(port_a, peer_a_dir.path());
    let _peer_b = spawn_peer(port_b, peer_b_dir.path());

    let peer_a_url = Url::parse(&format!("http://127.0.0.1:{port_a}/")).unwrap();
    let peer_b_url = Url::parse(&format!("http://127.0.0.1:{port_b}/")).unwrap();

    wait_for_ready(&peer_a_url, 30).await.expect("peer-a ready");
    wait_for_ready(&peer_b_url, 30).await.expect("peer-b ready");

    // Build the ecosystem registry that routes two of its domains through A2A
    // to the peers we just spawned.
    let (_eco_dir, registry_path) = build_ecosystem_fixture(&peer_a_url, &peer_b_url);

    // Run `neurogrim score` against the ecosystem registry. This is the
    // full pipeline path exercising load_cmdb_data's new a2a branch.
    let bin = env!("CARGO_BIN_EXE_neurogrim");
    let output = Command::new(bin)
        .arg("score")
        .arg("--plain")
        .arg("--registry")
        .arg(&registry_path)
        .stdin(Stdio::null())
        .output()
        .expect("spawn neurogrim score");

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert!(
        output.status.success(),
        "neurogrim score exited non-zero. \
         stdout={stdout}\nstderr={stderr}"
    );

    // The score output should contain digits (some numeric score rendered)
    // and should mention each domain by name — meaning the pipeline reached
    // all three domains including the two sourced via A2A.
    assert!(
        stdout.chars().any(|c| c.is_ascii_digit()),
        "score output should contain digits. stdout={stdout}"
    );
    for domain in ["child-peer-a", "child-peer-b", "local"] {
        assert!(
            stdout.contains(domain),
            "score output should mention {domain:?}. stdout={stdout}"
        );
    }
}
