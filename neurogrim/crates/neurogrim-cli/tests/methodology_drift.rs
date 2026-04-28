//! v3.2 Phase B.5 — sanity check that the bundled methodology primer
//! is well-formed.
//!
//! Each bundled topic file at
//! `crates/neurogrim-cli/data/explain/<topic>.md` is loaded into the
//! binary via `include_str!` (see `commands/explain.rs`). This test
//! reads them from disk and checks structural invariants the bundle's
//! own unit tests cannot catch: file presence, version-stamp uniformity,
//! content non-emptiness.
//!
//! Replaces the heavier "byte-compare bundled chunks vs canonical AGENT-
//! PRIMER.md" design from the original plan; per-topic files ARE the
//! canonical source under v3.2, and a runtime drift sensor is deferred
//! to v3.3 BACKLOG.

use std::fs;
use std::path::PathBuf;

const TOPICS: &[&str] = &[
    "methodology",
    "domain",
    "sensor",
    "hat",
    "scoring",
    "federation",
    "cli",
    "culture",
];

fn data_explain_dir() -> PathBuf {
    // v3.2.1 — explain data files moved from neurogrim-cli/data/explain
    // to neurogrim-mcp/data/explain so the MCP server can expose
    // `explain` from the same source of truth.
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .parent()
        .unwrap()
        .join("neurogrim-mcp")
        .join("data")
        .join("explain")
}

#[test]
fn every_topic_file_exists() {
    let dir = data_explain_dir();
    for topic in TOPICS {
        let path = dir.join(format!("{topic}.md"));
        assert!(
            path.is_file(),
            "expected bundled topic file at {}",
            path.display()
        );
    }
}

#[test]
fn every_topic_has_version_header() {
    let dir = data_explain_dir();
    for topic in TOPICS {
        let path = dir.join(format!("{topic}.md"));
        let content = fs::read_to_string(&path).unwrap();
        let first = content.lines().next().unwrap_or("");
        assert!(
            first.starts_with("<!-- topic:") && first.contains(topic),
            "topic file {} missing version-header marker; first line: {first:?}",
            path.display()
        );
    }
}

#[test]
fn every_topic_has_substantive_content() {
    let dir = data_explain_dir();
    for topic in TOPICS {
        let path = dir.join(format!("{topic}.md"));
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.len() > 800,
            "topic '{topic}' is too short ({} bytes); expected at least ~800",
            content.len()
        );
        assert!(
            content.contains("##"),
            "topic '{topic}' has no markdown subheadings; not a primer-shaped doc"
        );
    }
}

#[test]
fn version_headers_are_uniform() {
    // All bundled topics should declare the same compiled-in version.
    let dir = data_explain_dir();
    let mut versions = Vec::new();
    for topic in TOPICS {
        let path = dir.join(format!("{topic}.md"));
        let content = fs::read_to_string(&path).unwrap();
        let first = content.lines().next().unwrap_or("");
        // Header shape: `<!-- topic: NAME — bundled in neurogrim-cli vX.Y -->`
        let token = first.rsplit("neurogrim-cli ").nth(0).unwrap_or("");
        let version_str: String = token
            .chars()
            .take_while(|c| !c.is_whitespace())
            .collect();
        versions.push((topic, version_str));
    }
    let baseline = &versions[0].1.clone();
    for (topic, v) in &versions {
        assert_eq!(
            v, baseline,
            "topic '{topic}' has version '{v}', expected '{baseline}'"
        );
    }
}

#[test]
fn no_topic_references_unknown_command() {
    // Smoke check: each bundled topic should only reference real
    // `neurogrim` subcommands. Helps catch typos when topics drift
    // ahead of the binary.
    let known_commands = [
        "agent",
        "score",
        "health",
        "trend",
        "narrate",
        "validate",
        "doctor",
        "explain",
        "serve",
        "sensory",
        "init",
        "awareness",
        "domain",
        "skill",
        "federation",
        "a2a-serve",
        "a2a-invoke",
        "a2a-discover",
        "a2a-token",
        "disposition",
        "domain-calibration",
        "federated-pattern",
        "sca-review",
        "sca-calibrate",
    ];
    let dir = data_explain_dir();
    for topic in TOPICS {
        let path = dir.join(format!("{topic}.md"));
        let content = fs::read_to_string(&path).unwrap();
        // Find every `neurogrim WORD` mention and check WORD is known.
        for cap in content.split("`neurogrim ").skip(1) {
            // Skip flag-shaped tokens (`neurogrim --help`, `neurogrim --version`).
            if cap.starts_with('-') {
                continue;
            }
            // Take everything until next backtick or whitespace.
            let cmd: String = cap
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if cmd.is_empty() {
                continue;
            }
            assert!(
                known_commands.iter().any(|k| *k == cmd),
                "topic '{topic}' references unknown subcommand 'neurogrim {cmd}'"
            );
        }
    }
}
