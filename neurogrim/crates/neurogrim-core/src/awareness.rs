//! Local machine awareness — a persistent, gitignored fact store for
//! machine-specific knowledge that agents discover and should not forget.
//!
//! Lives at `.claude/brain/local-awareness.json`. Never committed to git.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// The full local awareness store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAwareness {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub facts: Vec<AwarenessFact>,
    #[serde(default)]
    pub notes: Vec<AwarenessNote>,
}

/// A discrete key-value fact about the local machine or project environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwarenessFact {
    /// Dot-namespaced key, e.g. `tool_paths.cargo` or `environment.shell`.
    pub key: String,
    /// The fact's value as a string.
    pub value: String,
    #[serde(default)]
    pub category: AwarenessCategory,
    pub discovered_at: DateTime<Utc>,
    /// Optional human-readable context explaining why this fact matters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A free-form note about a local machine pattern, quirk, or constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwarenessNote {
    pub content: String,
    #[serde(default)]
    pub category: AwarenessCategory,
    pub discovered_at: DateTime<Utc>,
}

/// Semantic categories for facts and notes.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AwarenessCategory {
    /// Absolute paths to tools that are not on PATH.
    ToolPaths,
    /// Local environment variables or OS-level quirks.
    Environment,
    /// Known behavioral patterns (e.g. exit codes that look like errors but aren't).
    Patterns,
    /// Hard constraints of the local environment (no network, restricted permissions, etc.).
    Constraints,
    #[default]
    General,
}

impl AwarenessCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ToolPaths => "tool_paths",
            Self::Environment => "environment",
            Self::Patterns => "patterns",
            Self::Constraints => "constraints",
            Self::General => "general",
        }
    }
}

impl FromStr for AwarenessCategory {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().replace('-', "_").as_str() {
            "tool_paths" | "toolpaths" | "tools" => Self::ToolPaths,
            "environment" | "env" => Self::Environment,
            "patterns" | "pattern" => Self::Patterns,
            "constraints" | "constraint" => Self::Constraints,
            _ => Self::General,
        })
    }
}

impl std::fmt::Display for AwarenessCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl LocalAwareness {
    /// Create an empty awareness store with schema_version "1".
    pub fn empty() -> Self {
        Self {
            schema_version: "1".to_string(),
            updated_at: None,
            facts: vec![],
            notes: vec![],
        }
    }

    /// Add or update a fact. If a fact with the same key already exists, it is replaced.
    /// Returns the index of the upserted fact.
    pub fn upsert_fact(
        &mut self,
        key: &str,
        value: &str,
        category: AwarenessCategory,
        note: Option<&str>,
    ) -> usize {
        let now = Utc::now();
        if let Some(idx) = self.facts.iter().position(|f| f.key == key) {
            self.facts[idx].value = value.to_string();
            self.facts[idx].category = category;
            self.facts[idx].note = note.map(|s| s.to_string());
            // preserve original discovered_at — the fact was first seen then
            self.updated_at = Some(now);
            idx
        } else {
            self.facts.push(AwarenessFact {
                key: key.to_string(),
                value: value.to_string(),
                category,
                discovered_at: now,
                note: note.map(|s| s.to_string()),
            });
            self.updated_at = Some(now);
            self.facts.len() - 1
        }
    }

    /// Remove a fact by key. Returns `true` if a fact was removed,
    /// `false` if no fact with that key existed.
    ///
    /// Needed for tri-state inherit semantics on consumers (e.g., a
    /// permission matrix where a missing fact means "inherit from
    /// default"): expressing "back to inherit" requires a real delete
    /// primitive, not an "inherit" sentinel value.
    pub fn remove_fact(&mut self, key: &str) -> bool {
        if let Some(idx) = self.facts.iter().position(|f| f.key == key) {
            self.facts.remove(idx);
            self.updated_at = Some(Utc::now());
            true
        } else {
            false
        }
    }

    /// Append a free-form note.
    pub fn add_note(&mut self, content: &str, category: AwarenessCategory) {
        self.notes.push(AwarenessNote {
            content: content.to_string(),
            category,
            discovered_at: Utc::now(),
        });
        self.updated_at = Some(Utc::now());
    }

    /// Get a fact by key.
    pub fn get_fact(&self, key: &str) -> Option<&AwarenessFact> {
        self.facts.iter().find(|f| f.key == key)
    }

    /// True if there is no content (no facts, no notes).
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty() && self.notes.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_awareness_serializes_to_valid_json() {
        let a = LocalAwareness::empty();
        let json = serde_json::to_string(&a).unwrap();
        let back: LocalAwareness = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, "1");
        assert!(back.facts.is_empty());
        assert!(back.notes.is_empty());
        assert!(back.updated_at.is_none());
    }

    #[test]
    fn category_roundtrips_via_serde() {
        for (cat, expected) in [
            (AwarenessCategory::ToolPaths, "tool_paths"),
            (AwarenessCategory::Environment, "environment"),
            (AwarenessCategory::Patterns, "patterns"),
            (AwarenessCategory::Constraints, "constraints"),
            (AwarenessCategory::General, "general"),
        ] {
            let json = serde_json::to_string(&cat).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let back: AwarenessCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(back, cat);
        }
    }

    #[test]
    fn fact_missing_category_defaults_to_general() {
        let json = r#"{"key":"k","value":"v","discovered_at":"2025-01-01T00:00:00Z"}"#;
        let fact: AwarenessFact = serde_json::from_str(json).unwrap();
        assert_eq!(fact.category, AwarenessCategory::General);
    }

    #[test]
    fn upsert_adds_new_fact() {
        let mut a = LocalAwareness::empty();
        a.upsert_fact(
            "cargo_path",
            "/usr/bin/cargo",
            AwarenessCategory::ToolPaths,
            None,
        );
        assert_eq!(a.facts.len(), 1);
        assert_eq!(a.facts[0].key, "cargo_path");
        assert_eq!(a.facts[0].value, "/usr/bin/cargo");
        assert!(a.updated_at.is_some());
    }

    #[test]
    fn upsert_replaces_existing_fact() {
        let mut a = LocalAwareness::empty();
        a.upsert_fact(
            "cargo_path",
            "/old/path",
            AwarenessCategory::ToolPaths,
            None,
        );
        a.upsert_fact(
            "cargo_path",
            "/new/path",
            AwarenessCategory::ToolPaths,
            Some("updated"),
        );
        assert_eq!(a.facts.len(), 1, "should not duplicate");
        assert_eq!(a.facts[0].value, "/new/path");
        assert_eq!(a.facts[0].note.as_deref(), Some("updated"));
    }

    #[test]
    fn remove_fact_returns_true_and_drops_the_fact() {
        let mut a = LocalAwareness::empty();
        a.upsert_fact(
            "cargo_path",
            "/usr/bin/cargo",
            AwarenessCategory::ToolPaths,
            None,
        );
        a.upsert_fact("other", "x", AwarenessCategory::General, None);
        let before = a.updated_at;
        // sleep one tick to make updated_at observably newer
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(a.remove_fact("cargo_path"));
        assert_eq!(a.facts.len(), 1);
        assert_eq!(a.facts[0].key, "other");
        assert!(a.updated_at > before);
    }

    #[test]
    fn remove_fact_returns_false_when_missing() {
        let mut a = LocalAwareness::empty();
        a.upsert_fact("other", "x", AwarenessCategory::General, None);
        let before = a.updated_at;
        assert!(!a.remove_fact("not_there"));
        assert_eq!(a.facts.len(), 1, "no fact removed");
        assert_eq!(a.updated_at, before, "updated_at not touched on no-op");
    }

    #[test]
    fn add_note_appends_to_notes_not_facts() {
        let mut a = LocalAwareness::empty();
        a.add_note(
            "cargo exits 1 on Windows — expected",
            AwarenessCategory::Patterns,
        );
        assert!(a.facts.is_empty());
        assert_eq!(a.notes.len(), 1);
        assert_eq!(a.notes[0].category, AwarenessCategory::Patterns);
    }

    #[test]
    fn get_fact_returns_correct_value() {
        let mut a = LocalAwareness::empty();
        a.upsert_fact(
            "tool_paths.cargo",
            "C:\\cargo.exe",
            AwarenessCategory::ToolPaths,
            None,
        );
        a.upsert_fact("other", "x", AwarenessCategory::General, None);
        let f = a.get_fact("tool_paths.cargo").unwrap();
        assert_eq!(f.value, "C:\\cargo.exe");
    }

    #[test]
    fn category_from_str_aliases() {
        assert_eq!(
            "tools".parse::<AwarenessCategory>().unwrap(),
            AwarenessCategory::ToolPaths
        );
        assert_eq!(
            "env".parse::<AwarenessCategory>().unwrap(),
            AwarenessCategory::Environment
        );
        assert_eq!(
            "unknown".parse::<AwarenessCategory>().unwrap(),
            AwarenessCategory::General
        );
    }

    #[test]
    fn awareness_deserializes_from_spec_example() {
        let json = r#"{
            "schema_version": "1",
            "facts": [{
                "key": "tool_paths.cargo",
                "value": "C:\\\\Users\\\\test\\\\.rustup\\\\toolchains\\\\stable\\\\bin\\\\cargo.exe",
                "category": "tool_paths",
                "discovered_at": "2026-04-12T10:00:00Z",
                "note": "Not on PATH in bash; use full path"
            }],
            "notes": [{
                "content": "cargo test exits 1 on Windows due to file lock warning",
                "category": "patterns",
                "discovered_at": "2026-04-12T10:00:00Z"
            }]
        }"#;
        let a: LocalAwareness = serde_json::from_str(json).unwrap();
        assert_eq!(a.facts[0].key, "tool_paths.cargo");
        assert_eq!(a.notes[0].category, AwarenessCategory::Patterns);
    }
}
