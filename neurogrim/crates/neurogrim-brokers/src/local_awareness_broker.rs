//! Phase B PoC — LocalAwarenessBroker (`[Sense]` role).
//!
//! Wraps `neurogrim_core::awareness::LocalAwareness` as a substrate broker.
//! The IDE's existing LocalAwareness atomic-file model is the cold store;
//! the broker's Overlay projects the current fact set for the agent. Three
//! Surfaced pipelines mirror the LocalAwareness API: `set-fact`, `add-note`,
//! `remove-fact`.
//!
//! ## Two-write coherence (B2 mitigation)
//!
//! Each mutation leaf-op writes the LocalAwareness file FIRST (via the
//! existing atomic-replace path in
//! `neurogrim_core::awareness::LocalAwareness`'s serde) and only then
//! updates the Overlay. If the disk write fails, the leaf-op returns Err
//! and the Overlay stays at its prior snapshot — next tick re-reads disk
//! and reconciles. This avoids the coherence gap the plan's ultra-pass U17
//! flagged: a single source of truth (disk) + a derived projection
//! (Overlay) that always re-reads disk on tick.
//!
//! ## V0 scope vs Phase C absorption
//!
//! Phase B PoC: this broker lives in `neurogrim-brokers/` as a substrate-
//! shipped reference. Phase C may move it into `neurogrim-ide/` if the IDE
//! needs to extend it; the trait boundary is stable enough that a move is
//! mechanical (re-export + path change).

use crate::broker::{Broker, BrokerError, Role, RoleSet, WorldEvent};
use crate::overlay::Overlay;
use crate::pipeline::{
    AuditClass, EffectClass, Pipeline, Step, Tunability, Visibility,
};
use crate::runner::{LeafContext, LeafError};
use async_trait::async_trait;
use neurogrim_core::awareness::{AwarenessCategory, LocalAwareness};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

/// Per-broker overlay shape: a projection of the LocalAwareness file's
/// current contents. The agent sees this in current-projection.md.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalAwarenessOverlay {
    pub fact_count: usize,
    pub note_count: usize,
    /// First N fact keys (capped to keep the projection compact).
    pub recent_fact_keys: Vec<String>,
}

const RECENT_KEYS_CAP: usize = 20;

pub struct LocalAwarenessBroker {
    id: String,
    awareness_path: PathBuf,
    overlay: Arc<Overlay<LocalAwarenessOverlay>>,
}

impl LocalAwarenessBroker {
    /// Construct a new LocalAwarenessBroker reading + writing to the given
    /// awareness file path. Typical location:
    /// `<project_root>/.claude/brain/local-awareness.json`.
    ///
    /// The constructor reads the file if present; otherwise starts with an
    /// empty awareness. Initial overlay is projected immediately.
    pub fn new(id: impl Into<String>, awareness_path: PathBuf) -> Self {
        let awareness = read_or_empty(&awareness_path);
        let initial_overlay = project_overlay(&awareness);
        Self {
            id: id.into(),
            awareness_path,
            overlay: Arc::new(Overlay::new(initial_overlay)),
        }
    }

    /// Helper: read the disk file into a fresh LocalAwareness (or return
    /// empty if file doesn't exist / parse fails). Used by tick + every
    /// mutation leaf-op so the Overlay always reflects post-write state.
    fn read_disk(&self) -> LocalAwareness {
        read_or_empty(&self.awareness_path)
    }

    /// Helper: atomically write `awareness` back to disk + re-project the
    /// Overlay. Called from each mutation leaf-op after its mutation; if
    /// the disk write fails, returns Err and the Overlay is NOT swapped —
    /// next tick reconciles from disk.
    fn write_and_reproject(&self, awareness: &LocalAwareness) -> Result<(), LeafError> {
        write_awareness(&self.awareness_path, awareness)
            .map_err(|e| LeafError::ExecutionFailed(format!("write awareness: {}", e)))?;
        let new_overlay = project_overlay(awareness);
        self.overlay.swap(new_overlay);
        Ok(())
    }

    pub fn catalog(&self) -> Vec<Pipeline> {
        let mut pipelines = vec![
            Pipeline {
                id: format!("{}/set-fact", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ColdStoreWrite,
                params: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" },
                        "value": { "type": "string" },
                        "category": {
                            "type": "string",
                            "enum": ["tool_paths", "environment", "patterns", "constraints", "general"]
                        },
                        "note": { "type": "string" }
                    },
                    "required": ["key", "value", "category"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "set_fact".to_string(),
                }],
                description: "Upsert a fact in LocalAwareness (atomic disk replace + overlay re-projection).".to_string(),
                when_to_use: "When you discover persistent machine knowledge that future sessions should know (tool paths, env quirks, patterns, constraints).".to_string(),
                bypasses_kill_switch: false,
            },
            Pipeline {
                id: format!("{}/add-note", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ColdStoreWrite,
                params: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string" },
                        "category": {
                            "type": "string",
                            "enum": ["tool_paths", "environment", "patterns", "constraints", "general"]
                        }
                    },
                    "required": ["content", "category"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "add_note".to_string(),
                }],
                description: "Append a free-form note in LocalAwareness.".to_string(),
                when_to_use: "When the knowledge is observational rather than a structured key/value fact.".to_string(),
                bypasses_kill_switch: false,
            },
            Pipeline {
                id: format!("{}/remove-fact", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ColdStoreWrite,
                params: serde_json::json!({
                    "type": "object",
                    "properties": { "key": { "type": "string" } },
                    "required": ["key"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "remove_fact".to_string(),
                }],
                description: "Remove a fact from LocalAwareness by key.".to_string(),
                when_to_use: "When a previously-discovered fact is stale or has been superseded; supports the tri-state inherit semantics for permission matrices.".to_string(),
                bypasses_kill_switch: false,
            },
        ];
        // Canonical governance pipelines per BB #19.
        pipelines.extend(crate::governance::GovernanceComposer::canonical_governance_pipelines(&self.id));
        pipelines
    }
}

fn read_or_empty(path: &std::path::Path) -> LocalAwareness {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<LocalAwareness>(&s).ok())
        .unwrap_or_else(LocalAwareness::empty)
}

fn write_awareness(path: &std::path::Path, awareness: &LocalAwareness) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Atomic-replace via tempfile + rename: write to a sibling temp file
    // then atomic rename. Matches the IDE's existing atomic-replace
    // contract (Phase B PoC; production may borrow IDE's
    // `RealLocalAwareness` directly via a trait once C1 lands).
    let tmp_path = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(awareness)?;
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)
}

fn project_overlay(awareness: &LocalAwareness) -> LocalAwarenessOverlay {
    LocalAwarenessOverlay {
        fact_count: awareness.facts.len(),
        note_count: awareness.notes.len(),
        recent_fact_keys: awareness
            .facts
            .iter()
            .rev()
            .take(RECENT_KEYS_CAP)
            .map(|f| f.key.clone())
            .collect(),
    }
}

#[async_trait]
impl Broker for LocalAwarenessBroker {
    fn id(&self) -> &str {
        &self.id
    }

    fn role_set(&self) -> RoleSet {
        RoleSet::single(Role::Sense)
    }

    async fn read_overlay(&self) -> serde_json::Value {
        let snap = self.overlay.load();
        serde_json::to_value(&*snap).unwrap_or(serde_json::Value::Null)
    }

    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        // V0: every Surfaced pipeline is legal. S1-T may filter by capability
        // (e.g., `ide-remote-actions-allowed` LocalAwareness fact).
        self.catalog()
            .into_iter()
            .filter(|p| matches!(p.visibility, Visibility::Surfaced))
            .collect()
    }

    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        crate::governance::GovernanceComposer::canonical_governance_pipelines(&self.id)
    }

    async fn tick(&self, _event: WorldEvent) -> Result<(), BrokerError> {
        // Reconcile from disk on every tick — sole source of truth lives
        // on disk; Overlay is a derived projection.
        let awareness = self.read_disk();
        self.overlay.swap(project_overlay(&awareness));
        Ok(())
    }

    async fn execute_leaf(
        &self,
        name: &str,
        ctx: LeafContext,
    ) -> Result<serde_json::Value, LeafError> {
        match name {
            "set_fact" => {
                let key = ctx
                    .params
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LeafError::InputInvalid("missing key".into()))?
                    .to_string();
                let value = ctx
                    .params
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LeafError::InputInvalid("missing value".into()))?
                    .to_string();
                let category = parse_category(&ctx)?;
                let note = ctx
                    .params
                    .get("note")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                // Read disk → mutate → write disk → re-project overlay.
                let mut awareness = self.read_disk();
                awareness.upsert_fact(&key, &value, category, note.as_deref());
                self.write_and_reproject(&awareness)?;
                Ok(serde_json::json!({"set": key}))
            }
            "add_note" => {
                let content = ctx
                    .params
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LeafError::InputInvalid("missing content".into()))?
                    .to_string();
                let category = parse_category(&ctx)?;
                let mut awareness = self.read_disk();
                awareness.add_note(&content, category);
                self.write_and_reproject(&awareness)?;
                Ok(serde_json::json!({"added": true}))
            }
            "remove_fact" => {
                let key = ctx
                    .params
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LeafError::InputInvalid("missing key".into()))?
                    .to_string();
                let mut awareness = self.read_disk();
                let removed = awareness.remove_fact(&key);
                if removed {
                    self.write_and_reproject(&awareness)?;
                }
                Ok(serde_json::json!({"removed": removed}))
            }
            "arm_kill_switch" | "disengage_kill_switch" => {
                // LocalAwarenessBroker doesn't host its own GovernanceComposer
                // — these are framework pipelines that should NOT be in its
                // catalog. If invoked, return NotFound (the host's central
                // GovernanceComposer handles arm/disengage via the Work
                // Broker or another InnateAbility broker).
                Err(LeafError::NotFound(name.to_string()))
            }
            other => Err(LeafError::NotFound(other.to_string())),
        }
    }
}

fn parse_category(ctx: &LeafContext) -> Result<AwarenessCategory, LeafError> {
    let s = ctx
        .params
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("general");
    Ok(AwarenessCategory::from_str(s).unwrap_or(AwarenessCategory::General))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::GovernanceComposer;
    use crate::pipeline::ParamMap;
    use crate::runner::PipelineRunner;
    use crate::trace::TraceSink;
    use tempfile::TempDir;

    fn make_runner_governance() -> (Arc<TraceSink>, Arc<GovernanceComposer>) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        (
            Arc::new(TraceSink::new(path)),
            Arc::new(GovernanceComposer::new(1000)),
        )
    }

    #[tokio::test]
    async fn local_awareness_broker_role_set_is_sense() {
        let tmp = TempDir::new().unwrap();
        let broker = LocalAwarenessBroker::new("local-awareness", tmp.path().join("aware.json"));
        assert!(broker.role_set().contains(&Role::Sense));
        assert!(!broker.role_set().contains(&Role::Embodiment));
    }

    #[tokio::test]
    async fn local_awareness_broker_initial_overlay_is_empty() {
        let tmp = TempDir::new().unwrap();
        let broker = LocalAwarenessBroker::new("local-awareness", tmp.path().join("aware.json"));
        let overlay = broker.read_overlay().await;
        assert_eq!(overlay["fact_count"], 0);
        assert_eq!(overlay["note_count"], 0);
    }

    #[tokio::test]
    async fn local_awareness_broker_set_fact_persists_to_disk_and_overlay() {
        let tmp = TempDir::new().unwrap();
        let aware_path = tmp.path().join("aware.json");
        let broker = Arc::new(LocalAwarenessBroker::new("la", aware_path.clone()));
        let catalog = broker.catalog();
        let (sink, governance) = make_runner_governance();
        let runner = PipelineRunner::new(sink, governance);

        let dyn_broker: Arc<dyn Broker> = broker.clone();
        let mut params = ParamMap::new();
        params.insert("key".into(), serde_json::Value::String("cargo_path".into()));
        params.insert("value".into(), serde_json::Value::String("/usr/bin/cargo".into()));
        params.insert("category".into(), serde_json::Value::String("tool_paths".into()));
        runner
            .dispatch(dyn_broker, &catalog, "la/set-fact".to_string(), params)
            .await
            .unwrap();

        // Disk file written
        assert!(aware_path.exists());
        let on_disk = std::fs::read_to_string(&aware_path).unwrap();
        assert!(on_disk.contains("cargo_path"));
        assert!(on_disk.contains("/usr/bin/cargo"));

        // Overlay re-projected
        let overlay = broker.read_overlay().await;
        assert_eq!(overlay["fact_count"], 1);
        let keys = overlay["recent_fact_keys"].as_array().unwrap();
        assert_eq!(keys[0], "cargo_path");
    }

    /// B2 coherence test: when the disk write FAILS (simulated via a
    /// read-only parent dir), the leaf-op returns Err AND the Overlay is
    /// NOT updated. The next tick re-reads from disk (still empty) and
    /// reconciles cleanly.
    #[tokio::test]
    async fn b2_failed_disk_write_leaves_overlay_unchanged() {
        let tmp = TempDir::new().unwrap();
        // Use a path that cannot be created — child of a non-existent dir
        // chain where intermediate creation fails on Windows due to invalid
        // characters. We force this by using NUL byte in path which is
        // invalid everywhere.
        let aware_path = tmp.path().join("does\0not\0work").join("aware.json");
        let broker = Arc::new(LocalAwarenessBroker::new("la", aware_path));
        let catalog = broker.catalog();
        let (sink, governance) = make_runner_governance();
        let runner = PipelineRunner::new(sink, governance);

        let overlay_before = broker.read_overlay().await;
        let dyn_broker: Arc<dyn Broker> = broker.clone();
        let mut params = ParamMap::new();
        params.insert("key".into(), serde_json::Value::String("x".into()));
        params.insert("value".into(), serde_json::Value::String("y".into()));
        params.insert("category".into(), serde_json::Value::String("general".into()));
        // Dispatch fails because the leaf-op returns LeafError::ExecutionFailed.
        let result = runner
            .dispatch(dyn_broker, &catalog, "la/set-fact".to_string(), params)
            .await;
        assert!(result.is_err(), "dispatch should refuse when disk write fails");
        // Overlay unchanged.
        let overlay_after = broker.read_overlay().await;
        assert_eq!(overlay_before, overlay_after);
    }

    #[tokio::test]
    async fn local_awareness_broker_remove_fact_drops_from_overlay() {
        let tmp = TempDir::new().unwrap();
        let aware_path = tmp.path().join("aware.json");
        let broker = Arc::new(LocalAwarenessBroker::new("la", aware_path));
        let catalog = broker.catalog();
        let (sink, governance) = make_runner_governance();
        let runner = PipelineRunner::new(sink, governance);

        // Set a fact first.
        let dyn_broker: Arc<dyn Broker> = broker.clone();
        let mut params = ParamMap::new();
        params.insert("key".into(), serde_json::Value::String("k1".into()));
        params.insert("value".into(), serde_json::Value::String("v1".into()));
        params.insert("category".into(), serde_json::Value::String("general".into()));
        runner
            .dispatch(dyn_broker.clone(), &catalog, "la/set-fact".to_string(), params)
            .await
            .unwrap();
        assert_eq!(broker.read_overlay().await["fact_count"], 1);

        // Remove it.
        let mut params = ParamMap::new();
        params.insert("key".into(), serde_json::Value::String("k1".into()));
        runner
            .dispatch(dyn_broker, &catalog, "la/remove-fact".to_string(), params)
            .await
            .unwrap();
        assert_eq!(broker.read_overlay().await["fact_count"], 0);
    }
}
