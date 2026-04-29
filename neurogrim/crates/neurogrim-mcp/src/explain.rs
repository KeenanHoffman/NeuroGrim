//! Bundled methodology primer (v3.2 Phase B; relocated from
//! neurogrim-cli to neurogrim-mcp in v3.2.1 so both the CLI and the
//! MCP server can expose `explain` from a single source of truth).
//!
//! Fifteen self-contained topic files ship inside the binary via
//! `include_str!`. Source: `neurogrim-mcp/data/explain/<topic>.md`.
//!
//! Each topic carries a version-stamped HTML comment header
//! (`<!-- topic: NAME — bundled in neurogrim-cli vX.Y -->`) so a
//! drift sensor can audit bundle-vs-canonical alignment over time.

const TOPIC_METHODOLOGY: &str = include_str!("../data/explain/methodology.md");
const TOPIC_DOMAIN: &str = include_str!("../data/explain/domain.md");
const TOPIC_SENSOR: &str = include_str!("../data/explain/sensor.md");
const TOPIC_HAT: &str = include_str!("../data/explain/hat.md");
const TOPIC_SCORING: &str = include_str!("../data/explain/scoring.md");
const TOPIC_FEDERATION: &str = include_str!("../data/explain/federation.md");
const TOPIC_CLI: &str = include_str!("../data/explain/cli.md");
const TOPIC_CULTURE: &str = include_str!("../data/explain/culture.md");
const TOPIC_AUTONOMY: &str = include_str!("../data/explain/autonomy.md");
const TOPIC_UI: &str = include_str!("../data/explain/ui.md");
const TOPIC_DASHBOARD_LAYOUTS: &str = include_str!("../data/explain/dashboard-layouts.md");
// v4.x topics (added retroactively in v4.3 S15-C-8 v2 — the topic .md
// files landed in S12/S13/S14/S15 but were never wired into the
// `topics()` array, so the inline-help HelpIcon couldn't resolve
// them. Wiring them now closes that regression.)
const TOPIC_PUBLISH_GATES: &str = include_str!("../data/explain/publish-gates.md");
const TOPIC_QUEUES: &str = include_str!("../data/explain/queues.md");
const TOPIC_SECRETS: &str = include_str!("../data/explain/secrets.md");
const TOPIC_COMMAND_POST: &str = include_str!("../data/explain/command-post.md");

/// Spec/methodology version this bundle was compiled against. Matches
/// the version header in each `data/explain/*.md`. Bumped manually
/// when methodology evolves enough to invalidate prior agent
/// guidance. The `--version` surface in CLI + MCP both read this.
pub const BUNDLED_VERSION: &str = "v3.5";

/// Canonical-source path relative to the workspace root, surfaced via
/// `neurogrim explain --version` and the MCP `explain --topic
/// __version__` path. Helps human auditors locate the source of truth.
pub const CANONICAL_SOURCE: &str =
    "neurogrim/crates/neurogrim-mcp/data/explain/<topic>.md";

/// All bundled topics, in display order. Each entry is
/// `(name, summary, body)`.
pub fn topics() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "methodology",
            "What is LSP Brains; the overlay framing; the 5-piece model",
            TOPIC_METHODOLOGY,
        ),
        (
            "domain",
            "Anatomy of a domain; weight tiers; when to add one",
            TOPIC_DOMAIN,
        ),
        (
            "sensor",
            "Sensor authoring contract; CMDB envelope; score formula patterns",
            TOPIC_SENSOR,
        ),
        (
            "hat",
            "The 8 declared hats and when to wear each",
            TOPIC_HAT,
        ),
        (
            "scoring",
            "Unified score, confidence, trajectory, floor gates",
            TOPIC_SCORING,
        ),
        (
            "federation",
            "A2A peers, fractal composition, read-only siblings",
            TOPIC_FEDERATION,
        ),
        (
            "cli",
            "All commands grouped by purpose (introspection / authoring / execution / bookkeeping)",
            TOPIC_CLI,
        ),
        (
            "culture",
            "culture.yaml — five values as floor-only invariants",
            TOPIC_CULTURE,
        ),
        (
            "autonomy",
            "Autonomy block: action_types, levels, safety_invariants (v3.3)",
            TOPIC_AUTONOMY,
        ),
        (
            "ui",
            "Dashboard surface: 5 pages, SSE live updates, hat lens, theme (v3.4)",
            TOPIC_UI,
        ),
        (
            "dashboard-layouts",
            "Authoring per-Brain widget layouts: catalog, sizes, common patterns, edit mode (v3.4)",
            TOPIC_DASHBOARD_LAYOUTS,
        ),
        (
            "publish-gates",
            "v4.0 publish-gates pipeline: automated/manual/e2e gate types, manifest, runner, ledger",
            TOPIC_PUBLISH_GATES,
        ),
        (
            "queues",
            "v4.1 agent coordination bus: append-only JSONL, reserved namespace, SSE live updates",
            TOPIC_QUEUES,
        ),
        (
            "secrets",
            "v4.2 encrypted secrets: four-layer model, OS-native + encrypted-file backends, single-use proxy tokens",
            TOPIC_SECRETS,
        ),
        (
            "command-post",
            "v4.3 dashboard-as-primary-editing-surface: multi-page schema, edit-via-bus, CLI parity",
            TOPIC_COMMAND_POST,
        ),
    ]
}

/// Look up a topic body by name. Returns `Some(body)` when the topic
/// is bundled; `None` otherwise. Caller renders the error.
pub fn lookup(name: &str) -> Option<&'static str> {
    topics().iter().find(|(k, _, _)| *k == name).map(|t| t.2)
}

/// Comma-separated list of available topic names. Used in error
/// messages from both CLI and MCP layers.
pub fn topic_names() -> Vec<&'static str> {
    topics().iter().map(|(k, _, _)| *k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_count_is_15() {
        assert_eq!(topics().len(), 15);
    }

    #[test]
    fn topic_names_are_unique() {
        let mut names = topic_names();
        names.sort();
        let n = names.len();
        names.dedup();
        assert_eq!(n, names.len(), "duplicate topic name");
    }

    #[test]
    fn all_topics_have_non_empty_bodies() {
        for (name, _summary, body) in topics() {
            assert!(
                body.len() > 200,
                "topic '{name}' bundled body is too short ({} bytes)",
                body.len()
            );
        }
    }

    #[test]
    fn all_topics_carry_version_header() {
        for (name, _summary, body) in topics() {
            let first = body.lines().next().unwrap_or("");
            assert!(
                first.starts_with("<!-- topic:") && first.contains(BUNDLED_VERSION),
                "topic '{name}' is missing the bundled-version header marker; first line: {first:?}"
            );
        }
    }

    #[test]
    fn methodology_topic_mentions_overlay() {
        assert!(TOPIC_METHODOLOGY.contains("overlay"));
        assert!(TOPIC_METHODOLOGY.contains("nervous system"));
    }

    #[test]
    fn domain_topic_describes_weight_tiers() {
        assert!(TOPIC_DOMAIN.contains("Weighted"));
        assert!(TOPIC_DOMAIN.contains("Advisory"));
        assert!(TOPIC_DOMAIN.contains("Stub"));
    }

    #[test]
    fn cli_topic_lists_command_families() {
        assert!(TOPIC_CLI.contains("Introspection"));
        assert!(TOPIC_CLI.contains("Authoring"));
        assert!(TOPIC_CLI.contains("Execution"));
        assert!(TOPIC_CLI.contains("Bookkeeping"));
    }

    #[test]
    fn lookup_finds_known_topic() {
        assert!(lookup("methodology").is_some());
        assert!(lookup("nonexistent").is_none());
    }

    #[test]
    fn dashboard_layouts_topic_lists_widget_catalog_and_patterns() {
        // Regression guard: agents authoring layouts pull this
        // topic for the widget catalog + size hints + common
        // patterns. If a future edit drops the catalog table or
        // the pattern walkthroughs, the topic becomes a stub.
        // Spot-check the load-bearing sections.
        for widget_type in [
            "identity",
            "score-gauge",
            "strongest-signals",
            "top-recommendations",
            "domain-card",
            "markdown-note",
        ] {
            assert!(
                TOPIC_DASHBOARD_LAYOUTS.contains(widget_type),
                "dashboard-layouts topic missing widget type {widget_type}"
            );
        }
        for size in ["full", "half", "third", "quarter"] {
            assert!(
                TOPIC_DASHBOARD_LAYOUTS.contains(size),
                "dashboard-layouts topic missing size {size}"
            );
        }
        assert!(
            TOPIC_DASHBOARD_LAYOUTS.contains("Common patterns")
                || TOPIC_DASHBOARD_LAYOUTS.contains("common patterns"),
            "dashboard-layouts topic must walk through common patterns"
        );
        assert!(
            TOPIC_DASHBOARD_LAYOUTS.contains("Edit mode") || TOPIC_DASHBOARD_LAYOUTS.contains("edit mode"),
            "dashboard-layouts topic must describe edit mode"
        );
    }

    #[test]
    fn ui_topic_describes_the_five_pages_and_sse() {
        // Regression guard: the v3.4 dashboard's value prop is the
        // five pages + live updates. If a future edit accidentally
        // strips those out, the topic would still be "valid markdown
        // with the version header" but uselessly thin.
        assert!(TOPIC_UI.contains("Overview"));
        assert!(TOPIC_UI.contains("Domains"));
        assert!(TOPIC_UI.contains("Federation"));
        assert!(TOPIC_UI.contains("Skills"));
        assert!(
            TOPIC_UI.contains("SSE") || TOPIC_UI.contains("Server-Sent Events"),
            "ui topic should mention SSE / Server-Sent Events"
        );
        assert!(
            TOPIC_UI.contains("hat") && TOPIC_UI.contains("lens"),
            "ui topic should describe the hat lens picker"
        );
    }
}
