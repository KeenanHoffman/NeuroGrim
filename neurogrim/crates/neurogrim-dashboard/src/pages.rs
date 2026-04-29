//! Multi-page dashboard configuration (v4.3 S15-C-1).
//!
//! Supersedes v3.4's single-page `dashboard-layout.json`. The new
//! shape lets a Brain declare multiple named pages — built-in
//! (`overview`, `services`, `logs`, `settings`) plus operator-
//! defined custom pages — that share the same widget catalog and
//! routing machinery.
//!
//! ## v1 scope (this stage)
//!
//! - Schema declared + ts-rs bindings.
//! - Backward-compat read: when only the v3.4
//!   `dashboard-layout.json` exists, [`read_dashboard_pages`]
//!   synthesizes a v2 config with the old layout under
//!   `pages.overview`.
//! - The current dashboard-layout endpoints continue to operate
//!   unchanged. Migration of those endpoints + custom-page CRUD
//!   ship in S15-C-6 (next session).
//!
//! ## File format (v2)
//!
//! `<brain>/.claude/brain/dashboard-pages.json`:
//!
//! ```json
//! {
//!   "schema_version": "2",
//!   "brain_id": "alpha",
//!   "pages": {
//!     "overview": [...widgets...],
//!     "custom-pc-state": [...widgets...]
//!   },
//!   "page_order": ["overview", "services", "logs", "settings", "custom-pc-state"]
//! }
//! ```
//!
//! Built-in pages don't strictly require entries in `pages` — the
//! frontend's hardcoded routes for Overview / Services / Logs /
//! Settings render regardless. The `pages` map is exclusively
//! about widget content (the Overview layout, custom pages); the
//! `page_order` is purely a sidebar-ordering hint.

use crate::layout::{read_layout, WidgetSpec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Default reserved page IDs that the frontend always renders
/// (regardless of whether the Brain's config declares them). These
/// are the built-in pages introduced in v4.3.
pub const BUILTIN_PAGE_IDS: &[&str] =
    &["overview", "services", "logs", "settings", "approvals", "publish-gates"];

/// v2 multi-page config. Replaces v3.4's single
/// `DashboardLayoutResponse` for adopters who opt into the new
/// shape; the old format still works via [`read_dashboard_pages`]'s
/// backward-compat path.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DashboardPagesConfig {
    pub schema_version: String,
    pub brain_id: String,
    /// Per-page widget content. Keys are page IDs (`overview`,
    /// `custom-pc-state`, etc.); values are the widget lists. A
    /// page with no entry in this map renders empty (custom pages)
    /// or falls back to its built-in default (Overview).
    pub pages: HashMap<String, Vec<WidgetSpec>>,
    /// Sidebar ordering. Front-of-list = top-of-sidebar. Built-in
    /// pages appear in this list when the operator wants to
    /// reorder them; missing built-ins use their default position.
    pub page_order: Vec<String>,
}

impl DashboardPagesConfig {
    /// Empty default for a fresh Brain: all built-ins in order, no
    /// custom pages, no widget overrides.
    pub fn default_for(brain_id: &str) -> Self {
        Self {
            schema_version: "2".into(),
            brain_id: brain_id.to_string(),
            pages: HashMap::new(),
            page_order: BUILTIN_PAGE_IDS.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Synthesize a v2 config from a v3.4 `DashboardLayoutResponse`.
    /// The single layout becomes `pages.overview`; the page order
    /// is the default built-in list.
    pub fn from_v3_4_layout(
        brain_id: &str,
        widgets: Vec<WidgetSpec>,
    ) -> Self {
        let mut pages = HashMap::new();
        pages.insert("overview".to_string(), widgets);
        Self {
            schema_version: "2".into(),
            brain_id: brain_id.to_string(),
            pages,
            page_order: BUILTIN_PAGE_IDS.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// True iff `page_id` is a reserved built-in. Used by the
    /// custom-page-creation flow (S15-C-6) to reject collisions.
    pub fn is_builtin(page_id: &str) -> bool {
        BUILTIN_PAGE_IDS.contains(&page_id)
    }
}

/// On-disk path for the v2 config. Distinct file name from v3.4's
/// `dashboard-layout.json` so the backward-compat read can detect
/// which format the operator has authored.
pub fn pages_file_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("dashboard-pages.json")
}

/// Atomically write the v2 config. Temp file + rename so concurrent
/// readers see either the old or the new file, never partial.
pub fn save_dashboard_pages(
    project_root: &Path,
    config: &DashboardPagesConfig,
) -> std::io::Result<()> {
    let path = pages_file_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Validate that a page name is acceptable for custom pages:
/// - kebab-case (lowercase + digits + hyphens; must start with a letter)
/// - max 64 chars
/// - NOT a reserved built-in id
pub fn is_valid_custom_page_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    if DashboardPagesConfig::is_builtin(name) {
        return false;
    }
    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut prev = b' ';
    for &b in bytes {
        let ok = matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-');
        if !ok {
            return false;
        }
        if b == b'-' && prev == b'-' {
            return false;
        }
        prev = b;
    }
    if bytes.last() == Some(&b'-') {
        return false;
    }
    true
}

/// Read the multi-page config. Three branches:
///
/// 1. v2 file exists → parse + return.
/// 2. v3.4 layout file exists → synthesize a v2 config with the
///    old layout under `pages.overview`. Operators don't lose
///    their work when they upgrade.
/// 3. Neither → return [`DashboardPagesConfig::default_for`].
pub fn read_dashboard_pages(project_root: &Path, brain_id: &str) -> DashboardPagesConfig {
    let v2_path = pages_file_path(project_root);
    if let Ok(text) = std::fs::read_to_string(&v2_path) {
        let trimmed = text.trim_start_matches('\u{FEFF}');
        if let Ok(cfg) = serde_json::from_str::<DashboardPagesConfig>(trimmed) {
            return cfg;
        }
        tracing::warn!(
            "v2 dashboard-pages.json at {} failed to parse; falling back to v3.4 layout",
            v2_path.display()
        );
    }
    if let Some(old) = read_layout(project_root) {
        return DashboardPagesConfig::from_v3_4_layout(brain_id, old.widgets);
    }
    DashboardPagesConfig::default_for(brain_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn write_v3_4_layout(dir: &TempDir, widgets: Vec<WidgetSpec>) {
        let path = dir
            .path()
            .join(".claude")
            .join("brain")
            .join("dashboard-layout.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body = serde_json::json!({
            "schema_version": "1",
            "brain_id": "alpha",
            "is_default": false,
            "widgets": widgets,
        });
        std::fs::write(&path, body.to_string()).unwrap();
    }

    fn make_widget(id: &str) -> WidgetSpec {
        WidgetSpec {
            id: id.to_string(),
            widget_type: "score-gauge".to_string(),
            size: "half".to_string(),
            title: None,
            config: json!({}),
        }
    }

    #[test]
    fn default_for_returns_empty_pages_with_builtin_order() {
        let cfg = DashboardPagesConfig::default_for("alpha");
        assert_eq!(cfg.schema_version, "2");
        assert_eq!(cfg.brain_id, "alpha");
        assert!(cfg.pages.is_empty());
        assert_eq!(
            cfg.page_order,
            BUILTIN_PAGE_IDS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn is_builtin_recognizes_reserved_ids() {
        assert!(DashboardPagesConfig::is_builtin("overview"));
        assert!(DashboardPagesConfig::is_builtin("services"));
        assert!(DashboardPagesConfig::is_builtin("logs"));
        assert!(DashboardPagesConfig::is_builtin("settings"));
        assert!(!DashboardPagesConfig::is_builtin("custom-pc-state"));
        assert!(!DashboardPagesConfig::is_builtin("operator-named"));
    }

    #[test]
    fn from_v3_4_layout_promotes_old_widgets_to_overview_page() {
        let widgets = vec![make_widget("a"), make_widget("b")];
        let cfg = DashboardPagesConfig::from_v3_4_layout("alpha", widgets.clone());
        assert_eq!(cfg.brain_id, "alpha");
        assert_eq!(cfg.pages.len(), 1);
        assert_eq!(cfg.pages.get("overview").map(|v| v.len()), Some(2));
        assert_eq!(cfg.pages.get("overview").unwrap()[0].id, "a");
    }

    #[test]
    fn read_returns_default_when_no_files() {
        let dir = TempDir::new().unwrap();
        let cfg = read_dashboard_pages(dir.path(), "alpha");
        assert!(cfg.pages.is_empty());
        assert_eq!(cfg.schema_version, "2");
    }

    #[test]
    fn read_reads_v3_4_layout_when_only_old_file_exists() {
        let dir = TempDir::new().unwrap();
        write_v3_4_layout(&dir, vec![make_widget("x"), make_widget("y")]);
        let cfg = read_dashboard_pages(dir.path(), "alpha");
        assert_eq!(cfg.pages.get("overview").map(|v| v.len()), Some(2));
        // v2 schema_version even though the source was v3.4 —
        // synthesizing a forward-compatible view.
        assert_eq!(cfg.schema_version, "2");
    }

    #[test]
    fn read_prefers_v2_file_over_v3_4_when_both_present() {
        let dir = TempDir::new().unwrap();
        write_v3_4_layout(&dir, vec![make_widget("v3-4-widget")]);
        // Now write a v2 file with different content.
        let v2 = DashboardPagesConfig {
            schema_version: "2".into(),
            brain_id: "alpha".into(),
            pages: {
                let mut m = HashMap::new();
                m.insert("overview".to_string(), vec![make_widget("v2-widget")]);
                m.insert(
                    "custom-pc-state".to_string(),
                    vec![make_widget("custom-widget")],
                );
                m
            },
            page_order: vec![
                "overview".to_string(),
                "custom-pc-state".to_string(),
            ],
        };
        let v2_path = pages_file_path(dir.path());
        std::fs::create_dir_all(v2_path.parent().unwrap()).unwrap();
        std::fs::write(&v2_path, serde_json::to_string(&v2).unwrap()).unwrap();

        let cfg = read_dashboard_pages(dir.path(), "alpha");
        assert_eq!(cfg.pages.len(), 2);
        assert_eq!(cfg.pages.get("overview").unwrap()[0].id, "v2-widget");
        assert!(cfg.pages.contains_key("custom-pc-state"));
    }

    #[test]
    fn read_falls_through_to_default_when_v2_file_is_corrupt() {
        let dir = TempDir::new().unwrap();
        let v2_path = pages_file_path(dir.path());
        std::fs::create_dir_all(v2_path.parent().unwrap()).unwrap();
        std::fs::write(&v2_path, "not json at all").unwrap();
        // Corrupt v2 → falls back to v3.4 (none here) → default.
        let cfg = read_dashboard_pages(dir.path(), "alpha");
        assert!(cfg.pages.is_empty());
    }

    #[test]
    fn pages_file_path_uses_documented_location() {
        let p = pages_file_path(Path::new("/proj"));
        assert_eq!(
            p,
            Path::new("/proj/.claude/brain/dashboard-pages.json")
        );
    }

    // ── Custom-page-name validation tests (S15-C-6) ─────────────

    #[test]
    fn is_valid_custom_page_name_accepts_kebab() {
        assert!(is_valid_custom_page_name("custom-pc-state"));
        assert!(is_valid_custom_page_name("a"));
        assert!(is_valid_custom_page_name("alpha-beta-gamma"));
        assert!(is_valid_custom_page_name("page1"));
    }

    #[test]
    fn is_valid_custom_page_name_rejects_reserved_builtins() {
        for builtin in BUILTIN_PAGE_IDS {
            assert!(
                !is_valid_custom_page_name(builtin),
                "should reject builtin: {builtin}"
            );
        }
    }

    #[test]
    fn is_valid_custom_page_name_rejects_malformed() {
        assert!(!is_valid_custom_page_name(""));
        assert!(!is_valid_custom_page_name("UPPER"));
        assert!(!is_valid_custom_page_name("Has-Caps"));
        assert!(!is_valid_custom_page_name("1starts-with-digit"));
        assert!(!is_valid_custom_page_name("-leads-with-dash"));
        assert!(!is_valid_custom_page_name("trails-dash-"));
        assert!(!is_valid_custom_page_name("double--dash"));
        assert!(!is_valid_custom_page_name("has space"));
        assert!(!is_valid_custom_page_name("has_underscore"));
        assert!(!is_valid_custom_page_name("has.dot"));
        assert!(!is_valid_custom_page_name(&"x".repeat(65)));
    }

    #[test]
    fn save_dashboard_pages_writes_atomically() {
        let dir = TempDir::new().unwrap();
        let cfg = DashboardPagesConfig {
            schema_version: "2".into(),
            brain_id: "alpha".into(),
            pages: {
                let mut m = HashMap::new();
                m.insert(
                    "custom-page".to_string(),
                    vec![make_widget("test")],
                );
                m
            },
            page_order: vec!["overview".into(), "custom-page".into()],
        };
        save_dashboard_pages(dir.path(), &cfg).unwrap();
        // Re-read and verify.
        let read = read_dashboard_pages(dir.path(), "alpha");
        assert_eq!(read.pages.len(), 1);
        assert!(read.pages.contains_key("custom-page"));
    }
}
