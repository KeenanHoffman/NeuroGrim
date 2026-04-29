//! Phase B — homepage dashboard layout (custom Overview).
//!
//! Lets each Brain define what its Overview page renders: which
//! widgets in what order, with size hints. Solves the user-flagged
//! "all-advisory N/A is technically correct but unhelpful as a
//! homepage" problem — the operator (or an agent) composes a layout
//! that surfaces what matters for that project.
//!
//! ## Schema
//!
//! Per-brain layout file at `<brain>/.claude/brain/dashboard-layout.json`.
//! When absent, [`default_layout_for`] generates a posture-aware
//! default (weighted vs all-advisory).
//!
//! ## Why a list-with-size-hints, not a grid
//!
//! The simpler shape (`size: "full" | "half" | "third" | "quarter"`)
//! gets us most of the layout flexibility at a fraction of the
//! engineering cost. No drag-handles to compute, no collision
//! resolution, no responsive breakpoint math. Widgets flow
//! left-to-right and wrap at row width 1.0; the operator (or agent)
//! controls layout by ordering + size hints. The next-pass edit
//! mode can upgrade to a true 12-col grid if that simpler shape
//! proves limiting.

use neurogrim_core::registry::BrainRegistry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Body of `GET /api/brains/:id/dashboard-layout`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DashboardLayoutResponse {
    pub schema_version: String,
    pub brain_id: String,
    /// True when the layout came from `dashboard-layout.json` on
    /// disk. False when the response is the synthesized default
    /// (no file present) — the frontend renders a "this is the
    /// default; click 'Customize' to edit" hint in that case.
    pub is_default: bool,
    pub widgets: Vec<WidgetSpec>,
}

/// One widget on the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct WidgetSpec {
    /// Stable id within the layout — used by the frontend as a
    /// React key and (in the next-pass edit mode) as a target for
    /// move/remove operations.
    pub id: String,
    /// Widget kind. Matches a component name in the frontend's
    /// widget registry (`components/widgets/index.ts`). Stringly-typed
    /// at the wire so unknown widget types render gracefully (the
    /// frontend shows a "[unknown widget]" placeholder rather than
    /// crashing the page).
    pub widget_type: String,
    /// One of "full" | "half" | "third" | "quarter". Maps to a
    /// fractional row width — full=12/12, half=6/12, third=4/12,
    /// quarter=3/12. Widgets flow left-to-right; if a widget would
    /// overflow the current row, it starts a new row.
    pub size: String,
    /// Optional title override for the widget header. None →
    /// widget uses its own default title.
    pub title: Option<String>,
    /// Widget-specific configuration (JSON object). Each widget
    /// type interprets this differently; e.g., `domain-card` uses
    /// `{ "domain": "child-neurogrim" }`, `markdown-note` uses
    /// `{ "content": "..." }`.
    ///
    /// Typed as `Record<string, unknown>` in the generated TS
    /// bindings — ts-rs doesn't have an automatic mapping for
    /// `serde_json::Value`. Each frontend widget component is
    /// responsible for narrowing its slice of the config object.
    #[ts(type = "Record<string, unknown>")]
    pub config: serde_json::Value,
}

/// Path to a brain's layout file. Returns the path even when the
/// file doesn't exist (caller decides whether to read or fall
/// back to default).
pub fn layout_file_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("dashboard-layout.json")
}

/// Read the layout from disk if present; return Some(parsed) on
/// success, None otherwise. Malformed files are logged and treated
/// as missing — operators get the default layout rather than a
/// blank page when their JSON has a typo.
pub fn read_layout(project_root: &Path) -> Option<DashboardLayoutResponse> {
    let path = layout_file_path(project_root);
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<DashboardLayoutResponse>(trimmed) {
        Ok(mut parsed) => {
            // The file always represents the operator's intent, so
            // clear the is_default flag regardless of how the file
            // was authored.
            parsed.is_default = false;
            Some(parsed)
        }
        Err(e) => {
            tracing::warn!(
                "dashboard-layout.json at {:?} could not be parsed ({e}); falling back to default layout",
                path
            );
            None
        }
    }
}

/// Build a posture-aware default layout for a Brain.
///
/// Two postures are recognized:
///
/// - **Weighted**: at least one domain has weight > 0. Default
///   shows the gauge-centric layout (matches v3.4 Phase 1.1
///   Overview): identity → score-gauge / strongest-signals /
///   top-recommendations.
///
/// - **All-advisory**: every domain has weight 0. Default lifts
///   `child-*` domains to first-class status (a `domain-card`
///   per child) since those are the only signals carrying real
///   data on an all-advisory host. Falls back to the weighted
///   layout when no `child-*` domains are declared.
pub fn default_layout_for(brain_id: &str, registry: &BrainRegistry) -> DashboardLayoutResponse {
    let weights = &registry.config.domain_weights;
    let weighted = weights.values().any(|w| *w > 0.0);

    let mut widgets: Vec<WidgetSpec> = Vec::new();

    if weighted {
        // Gauge-centric layout for opinionated Brains.
        widgets.push(WidgetSpec {
            id: "identity".to_string(),
            widget_type: "identity".to_string(),
            size: "full".to_string(),
            title: None,
            config: serde_json::json!({}),
        });
        widgets.push(WidgetSpec {
            id: "score-gauge".to_string(),
            widget_type: "score-gauge".to_string(),
            size: "third".to_string(),
            title: None,
            config: serde_json::json!({}),
        });
        widgets.push(WidgetSpec {
            id: "strongest-signals".to_string(),
            widget_type: "strongest-signals".to_string(),
            size: "third".to_string(),
            title: None,
            config: serde_json::json!({ "count": 3 }),
        });
        widgets.push(WidgetSpec {
            id: "top-recommendations".to_string(),
            widget_type: "top-recommendations".to_string(),
            size: "third".to_string(),
            title: None,
            config: serde_json::json!({ "count": 3 }),
        });
    } else {
        // All-advisory default: lift child-* domains to first-class
        // cards if any are declared, otherwise fall back to the
        // gauge layout (which will correctly render N/A).
        widgets.push(WidgetSpec {
            id: "identity".to_string(),
            widget_type: "identity".to_string(),
            size: "full".to_string(),
            title: None,
            config: serde_json::json!({}),
        });

        let child_domains: Vec<String> = weights
            .keys()
            .filter(|name| name.starts_with("child-"))
            .cloned()
            .collect();

        if !child_domains.is_empty() {
            widgets.push(WidgetSpec {
                id: "advisory-note".to_string(),
                widget_type: "markdown-note".to_string(),
                size: "full".to_string(),
                title: Some("Observe-only posture".to_string()),
                config: serde_json::json!({
                    "content": "This Brain is **all-advisory** by design — its unified score is structurally N/A. The signals below are pulled live from declared child Brains via A2A; click any one to drill into that child's full dashboard."
                }),
            });
            // Best layout: 4 child cards as quarters fit one row,
            // anything else falls to thirds (3 fit a row + wrap).
            let size = if child_domains.len() == 4 { "quarter" } else { "third" };
            let mut sorted = child_domains.clone();
            sorted.sort();
            for (i, dom) in sorted.iter().enumerate() {
                widgets.push(WidgetSpec {
                    id: format!("child-card-{i}"),
                    widget_type: "domain-card".to_string(),
                    size: size.to_string(),
                    title: None,
                    config: serde_json::json!({ "domain": dom }),
                });
            }
            widgets.push(WidgetSpec {
                id: "strongest-signals".to_string(),
                widget_type: "strongest-signals".to_string(),
                size: "half".to_string(),
                title: Some("Strongest ecosystem signals".to_string()),
                config: serde_json::json!({ "count": 5 }),
            });
            widgets.push(WidgetSpec {
                id: "top-recommendations".to_string(),
                widget_type: "top-recommendations".to_string(),
                size: "half".to_string(),
                title: None,
                config: serde_json::json!({ "count": 3 }),
            });
        } else {
            // No child-* domains: pure all-advisory with no
            // federation — same as weighted layout, the gauge
            // shows N/A honestly.
            widgets.push(WidgetSpec {
                id: "score-gauge".to_string(),
                widget_type: "score-gauge".to_string(),
                size: "third".to_string(),
                title: None,
                config: serde_json::json!({}),
            });
            widgets.push(WidgetSpec {
                id: "strongest-signals".to_string(),
                widget_type: "strongest-signals".to_string(),
                size: "third".to_string(),
                title: None,
                config: serde_json::json!({ "count": 3 }),
            });
            widgets.push(WidgetSpec {
                id: "top-recommendations".to_string(),
                widget_type: "top-recommendations".to_string(),
                size: "third".to_string(),
                title: None,
                config: serde_json::json!({ "count": 3 }),
            });
        }
    }

    DashboardLayoutResponse {
        schema_version: "1".to_string(),
        brain_id: brain_id.to_string(),
        is_default: true,
        widgets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_core::registry::BrainRegistry;

    fn registry_with_weights(domains: &[(&str, f64)]) -> BrainRegistry {
        let definitions: serde_json::Value = serde_json::Value::Object(
            domains
                .iter()
                .map(|(k, _)| {
                    (
                        k.to_string(),
                        serde_json::json!({
                            "principle": "x",
                            "scoring_source": null,
                            "exported_variables": {}
                        }),
                    )
                })
                .collect(),
        );
        let weights: serde_json::Value = serde_json::Value::Object(
            domains
                .iter()
                .map(|(k, w)| (k.to_string(), serde_json::json!(*w)))
                .collect(),
        );
        let raw = serde_json::json!({
            "meta": {
                "schema_version": "2",
                "description": "test",
                "updated_by": "test"
            },
            "tools": {},
            "data_sources": {},
            "config": {
                "domain_weights": weights,
                "domain_definitions": definitions,
            }
        });
        BrainRegistry::from_json(&raw.to_string()).unwrap()
    }

    #[test]
    fn weighted_brain_gets_gauge_centric_layout() {
        let r = registry_with_weights(&[("test-health", 0.4), ("code-quality", 0.3)]);
        let layout = default_layout_for("neurogrim", &r);
        let types: Vec<&str> = layout.widgets.iter().map(|w| w.widget_type.as_str()).collect();
        assert_eq!(types, vec![
            "identity",
            "score-gauge",
            "strongest-signals",
            "top-recommendations",
        ]);
        assert!(layout.is_default);
    }

    #[test]
    fn all_advisory_with_children_gets_child_card_layout() {
        let r = registry_with_weights(&[
            ("agent-behavior", 0.0),
            ("child-neurogrim", 0.0),
            ("child-lsp-brains", 0.0),
            ("child-job-hunt", 0.0),
            ("child-python-starter", 0.0),
        ]);
        let layout = default_layout_for("ecosystem", &r);
        let types: Vec<&str> = layout.widgets.iter().map(|w| w.widget_type.as_str()).collect();
        // identity + advisory-note + 4 domain-cards + strongest + top-recs
        assert_eq!(types[0], "identity");
        assert_eq!(types[1], "markdown-note");
        let card_count = types.iter().filter(|t| **t == "domain-card").count();
        assert_eq!(card_count, 4);
        // 4 children → all sized as "quarter" so they fit one row.
        let card_sizes: Vec<&str> = layout
            .widgets
            .iter()
            .filter(|w| w.widget_type == "domain-card")
            .map(|w| w.size.as_str())
            .collect();
        for s in card_sizes {
            assert_eq!(s, "quarter");
        }
    }

    #[test]
    fn all_advisory_with_three_children_uses_thirds() {
        let r = registry_with_weights(&[
            ("child-a", 0.0),
            ("child-b", 0.0),
            ("child-c", 0.0),
        ]);
        let layout = default_layout_for("eco", &r);
        let card_sizes: Vec<&str> = layout
            .widgets
            .iter()
            .filter(|w| w.widget_type == "domain-card")
            .map(|w| w.size.as_str())
            .collect();
        for s in card_sizes {
            assert_eq!(s, "third");
        }
    }

    #[test]
    fn all_advisory_no_children_falls_back_to_gauge_layout() {
        let r = registry_with_weights(&[("agent-behavior", 0.0)]);
        let layout = default_layout_for("solo", &r);
        let types: Vec<&str> = layout.widgets.iter().map(|w| w.widget_type.as_str()).collect();
        assert!(types.contains(&"score-gauge"));
        assert!(!types.contains(&"domain-card"));
    }

    #[test]
    fn read_layout_returns_none_when_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_layout(tmp.path()).is_none());
    }

    #[test]
    fn read_layout_parses_valid_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude/brain")).unwrap();
        let body = serde_json::json!({
            "schema_version": "1",
            "brain_id": "test",
            "is_default": false,
            "widgets": [
                {
                    "id": "w1",
                    "widget_type": "score-gauge",
                    "size": "full",
                    "title": null,
                    "config": {}
                }
            ]
        });
        std::fs::write(
            layout_file_path(tmp.path()),
            serde_json::to_string_pretty(&body).unwrap(),
        )
        .unwrap();
        let layout = read_layout(tmp.path()).expect("parse");
        assert_eq!(layout.widgets.len(), 1);
        assert_eq!(layout.widgets[0].widget_type, "score-gauge");
        assert!(!layout.is_default);
    }

    #[test]
    fn read_layout_returns_none_on_malformed_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude/brain")).unwrap();
        std::fs::write(layout_file_path(tmp.path()), "{ not json").unwrap();
        // Malformed → None (logged, but doesn't panic).
        assert!(read_layout(tmp.path()).is_none());
    }

    #[test]
    fn read_layout_forces_is_default_false_even_if_file_says_true() {
        // Defensive: if someone hand-edits the file and leaves
        // is_default: true in there, the response must still
        // report it as a real on-disk layout.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude/brain")).unwrap();
        let body = serde_json::json!({
            "schema_version": "1",
            "brain_id": "test",
            "is_default": true,
            "widgets": []
        });
        std::fs::write(
            layout_file_path(tmp.path()),
            serde_json::to_string(&body).unwrap(),
        )
        .unwrap();
        let layout = read_layout(tmp.path()).unwrap();
        assert!(!layout.is_default);
    }
}
