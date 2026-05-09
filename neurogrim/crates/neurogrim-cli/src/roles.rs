//! Agent-role registry — `<project>/.claude/agent-roles.yaml`
//! (v2-Feature 4, 2026-05-09).
//!
//! Maps named roles (e.g., `advisor`, `critic-hat`, `rubber-duck`) to
//! a backend + model + system prompt triple. Used by the
//! `neurogrim invoke --role <name>` resolver to source defaults
//! without making operators paste backend/model/system args every call.
//!
//! Loading rules:
//! - `<project>/.claude/agent-roles.yaml` if present + parseable
//! - Otherwise the BUNDLED_DEFAULTS (always available) — operators get
//!   reasonable behavior on fresh checkouts without authoring YAML
//! - Operator-authored entries OVERLAY (replace) bundled entries by name;
//!   bundled roles the operator didn't override remain available
//!
//! Schema validated at load time; malformed entries don't crash —
//! they're skipped with a tracing::warn so the rest of the role
//! registry stays usable.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// Bundled defaults — always available even when no operator config
/// exists. The four roles below cover the v2-Feature 4 launch set;
/// operators extend by editing `.claude/agent-roles.yaml`.
const BUNDLED_DEFAULTS_YAML: &str = include_str!("agent-roles.defaults.yaml");

/// One role entry. Mirrors the YAML schema documented in the
/// `agent-roles.defaults.yaml` example.
#[derive(Debug, Clone, Deserialize)]
pub struct Role {
    pub backend: String,
    pub model: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

/// Top-level YAML shape. `version` is currently 1; future bumps invalidate
/// configs that don't update.
#[derive(Debug, Clone, Deserialize)]
pub struct RolesFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub roles: HashMap<String, Role>,
    #[serde(default)]
    pub fallback_backend: Option<String>,
    #[serde(default)]
    pub fallback_model: Option<String>,
}

fn default_version() -> u32 {
    1
}

/// In-memory registry that callers query. Built from bundled defaults
/// + optional operator overlay.
#[derive(Debug, Clone)]
pub struct RolesRegistry {
    roles: HashMap<String, Role>,
    pub fallback_backend: String,
    pub fallback_model: String,
}

impl RolesRegistry {
    /// Load from a project root. Returns BUNDLED_DEFAULTS overlaid with
    /// `<project>/.claude/agent-roles.yaml` when that file exists +
    /// parses cleanly.
    pub fn load(project_root: &Path) -> anyhow::Result<Self> {
        // Always start with the bundled defaults — operators get sane
        // behavior on a fresh checkout.
        let bundled: RolesFile = serde_yaml::from_str(BUNDLED_DEFAULTS_YAML)
            .map_err(|e| anyhow::anyhow!("parsing bundled agent-roles defaults: {e}"))?;
        let mut roles = bundled.roles.clone();
        let mut fallback_backend = bundled
            .fallback_backend
            .clone()
            .unwrap_or_else(|| "copilot-proxied".to_string());
        let mut fallback_model = bundled
            .fallback_model
            .clone()
            .unwrap_or_else(|| "gpt-4o".to_string());

        let path = project_root.join(".claude").join("agent-roles.yaml");
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(text) => match serde_yaml::from_str::<RolesFile>(&text) {
                    Ok(parsed) => {
                        for (name, role) in parsed.roles {
                            roles.insert(name, role);
                        }
                        if let Some(b) = parsed.fallback_backend {
                            fallback_backend = b;
                        }
                        if let Some(m) = parsed.fallback_model {
                            fallback_model = m;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "agent-roles.yaml at {} failed to parse: {e}; using bundled defaults only",
                            path.display()
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "agent-roles.yaml at {} present but unreadable: {e}; using bundled defaults only",
                        path.display()
                    );
                }
            }
        }

        Ok(RolesRegistry {
            roles,
            fallback_backend,
            fallback_model,
        })
    }

    pub fn resolve(&self, name: &str) -> Option<&Role> {
        self.roles.get(name)
    }

    pub fn names(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.roles.keys().map(|s| s.as_str()).collect();
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn bundled_defaults_parse() {
        let bundled: RolesFile = serde_yaml::from_str(BUNDLED_DEFAULTS_YAML)
            .expect("bundled YAML must parse");
        assert_eq!(bundled.version, 1);
        assert!(
            bundled.roles.contains_key("advisor"),
            "bundled defaults must declare an `advisor` role"
        );
        assert!(
            bundled.roles.contains_key("critic-hat"),
            "bundled defaults must declare a `critic-hat` role"
        );
        assert!(
            bundled.roles.contains_key("rubber-duck"),
            "bundled defaults must declare a `rubber-duck` role"
        );
    }

    #[test]
    fn registry_loads_without_project_yaml() {
        let dir = TempDir::new().unwrap();
        let reg = RolesRegistry::load(dir.path()).expect("registry loads");
        assert!(reg.resolve("advisor").is_some());
        assert!(reg.resolve("nonexistent").is_none());
    }

    #[test]
    fn registry_overlay_replaces_bundled_entry_by_name() {
        let dir = TempDir::new().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("agent-roles.yaml"),
            r#"
version: 1
roles:
  advisor:
    backend: anthropic
    model: claude-opus-4-7
    system_prompt: "Operator-overridden advisor."
  custom-hat:
    backend: ollama
    model: qwen3.5:1.7b
"#,
        )
        .unwrap();

        let reg = RolesRegistry::load(dir.path()).expect("registry loads");
        let advisor = reg.resolve("advisor").expect("advisor present");
        assert_eq!(advisor.backend, "anthropic");
        assert_eq!(advisor.model, "claude-opus-4-7");
        // Operator-only role available
        assert!(reg.resolve("custom-hat").is_some());
        // Bundled role NOT overridden by operator stays available
        assert!(reg.resolve("rubber-duck").is_some());
    }

    #[test]
    fn registry_skips_malformed_yaml() {
        let dir = TempDir::new().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("agent-roles.yaml"), "not: valid: yaml: at: all: ::: ").unwrap();
        let reg = RolesRegistry::load(dir.path()).expect("load returns Ok even on bad operator yaml");
        // Bundled defaults still available
        assert!(reg.resolve("advisor").is_some());
    }
}
