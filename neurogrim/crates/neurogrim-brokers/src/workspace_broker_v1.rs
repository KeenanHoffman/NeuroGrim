//! A.1 — V1 Workspace Broker (canonical implementation).
//!
//! Implements [`crate::workspace::WorkspaceBroker`] for the substrate. Per
//! `docs/BROKER-SCAFFOLDING-PRE-EXECUTION-GATES.md` Gate 1 sketch + the
//! plan's A.1 scope:
//!
//! - **8 Sense pipelines** (agent reads): get-terminal-profile,
//!   get-path-conventions, get-active-processes, get-wip-state,
//!   get-build-invariants, list-child-projects, get-capability-profile,
//!   get-current-focus.
//! - **3 LocalAwareness facet pipelines** (Surfaced; mutates facts/notes
//!   on disk via the existing `neurogrim_core::awareness::LocalAwareness`
//!   atomic-replace): set-fact, add-note, remove-fact. Absorbed from the
//!   retired Phase B `LocalAwarenessBroker`.
//! - **3 Agent-contribution InnateAbility pipelines** (Surfaced; agent
//!   records back): record-terminal-recommendation, record-active-process,
//!   update-focus. Each persists as a LocalAwareness fact under the
//!   appropriate category so it survives across runs.
//!
//! ## V1 scope notes
//!
//! - **`get-active-processes`** returns the operator-recorded tracked-process
//!   set (populated via `record-active-process`). V1 does NOT enumerate
//!   live OS processes via `sysinfo` — that integration lands in a future
//!   iteration. Agents that need live PID enumeration can shell out
//!   themselves via the workspace's terminal profile.
//! - **`get-wip-state`** shells to `git status --porcelain` + `git rev-list`
//!   (cached 5s in the overlay). Pure-Rust git via libgit2 is heavier; the
//!   shell-out is simple + tested.
//! - **`get-capability-profile`** returns the [`CapabilitySnapshot`] passed
//!   at construction. The broker has no access to the live `BrokerHost`
//!   registry (that would invert ownership); the snapshot is operator-
//!   curated or host-supplied at boot.
//!
//! ## Two-write coherence
//!
//! All facts/notes mutations use the same disk-first, overlay-on-success
//! pattern proven in the Phase B `LocalAwarenessBroker`: the cold store
//! (disk) is the source of truth; the Overlay is a derived projection.
//! Failed disk writes leave Overlay at its prior state; next tick
//! reconciles by re-reading disk.

use crate::broker::{Broker, BrokerError, Role, RoleSet, WorldEvent};
use crate::extension::{ExtensionConfig, ExtensionError, Extensible};
use crate::overlay::Overlay;
use crate::pipeline::{AuditClass, EffectClass, Pipeline, Step, Tunability, Visibility};
use crate::runner::{LeafContext, LeafError};
use crate::workspace::WorkspaceBroker;
use async_trait::async_trait;
use neurogrim_core::awareness::{AwarenessCategory, LocalAwareness};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};

pub const EXTENSION_SCHEMA_VERSION: &str = "1";

// ============================================================================
// Public overlay + value types
// ============================================================================

/// Per-broker overlay shape — projects the workspace's current snapshot
/// for the agent + dashboard. JSON-serialized into
/// `.claude/brain/broker/segments/overlay-workspace.md`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceOverlay {
    pub project_root: String,
    pub terminal_profile: TerminalProfile,
    pub path_conventions: PathConventions,
    pub current_focus: Option<String>,
    pub active_processes: Vec<TrackedProcess>,
    pub fact_count: usize,
    pub note_count: usize,
    pub child_projects_count: usize,
    pub capability_summary: CapabilitySnapshot,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalProfile {
    pub primary_shell: String,
    pub available_tools: Vec<String>,
    pub os: String,
    /// Operator-curated + agent-recorded gotchas
    pub gotchas: Vec<TerminalRecommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRecommendation {
    pub pattern: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathConventions {
    pub project_root: String,
    pub scratchpad: Option<String>,
    pub logs: Option<String>,
    pub artifacts: Option<String>,
    pub secrets: Option<String>,
    pub cmdb: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedProcess {
    pub tracking_token: String,
    pub pid: Option<u32>,
    pub kind: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitySnapshot {
    pub registered_brokers: Vec<String>,
    pub registered_sensors: Vec<String>,
    pub a2a_peers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildProject {
    pub id: String,
    pub path: String,
    pub role: String,
    pub a2a_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInvariant {
    pub name: String,
    pub command: String,
    pub description: String,
}

/// Extension-declared pipeline (Tier 1 simple fact-returning pipeline).
/// At dispatch time, returns the current value of `returns_fact_key` from
/// the LocalAwareness store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionPipeline {
    pub name: String,
    pub description: String,
    pub returns_fact_key: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtensionFactDecl {
    key: String,
    value: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtensionTerminalRecDecl {
    pattern: String,
    recommendation: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtensionPipelineDecl {
    name: String,
    description: String,
    returns_fact_key: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtensionChildProjectDecl {
    id: String,
    path: String,
    #[serde(default = "default_child_role")]
    role: String,
    #[serde(default)]
    a2a_port: Option<u16>,
}

fn default_child_role() -> String {
    "submodule".to_string()
}

// ============================================================================
// Broker
// ============================================================================

pub struct WorkspaceBrokerV1 {
    id: String,
    project_root: PathBuf,
    awareness_path: PathBuf,
    overlay: Arc<Overlay<WorkspaceOverlay>>,

    // Static configuration captured at construction.
    initial_terminal_profile: TerminalProfile,
    initial_path_conventions: PathConventions,
    default_build_invariants: Vec<BuildInvariant>,
    capability_snapshot: CapabilitySnapshot,

    // Extension state (populated by apply_extension).
    extension_pipelines: Arc<RwLock<Vec<ExtensionPipeline>>>,
    extension_child_projects: Arc<RwLock<Vec<ChildProject>>>,
    extension_terminal_recs: Arc<RwLock<Vec<TerminalRecommendation>>>,

    // Live runtime state (populated at runtime via record-* leaf-ops + tick).
    tracked_processes: Arc<RwLock<Vec<TrackedProcess>>>,
}

impl WorkspaceBrokerV1 {
    /// Construct a V1 workspace broker rooted at `project_root`.
    ///
    /// - `awareness_path` — canonical LocalAwareness file location, typically
    ///   `<project_root>/.claude/brain/local-awareness.json`.
    /// - `capability_snapshot` — operator-supplied or host-derived list of
    ///   registered brokers / sensors / A2A peers. The broker reflects this
    ///   verbatim via `get-capability-profile`.
    pub fn new(
        id: impl Into<String>,
        project_root: PathBuf,
        awareness_path: PathBuf,
        capability_snapshot: CapabilitySnapshot,
    ) -> Self {
        let initial_terminal_profile = detect_terminal_profile();
        let initial_path_conventions = default_path_conventions(&project_root);
        let default_build_invariants = default_build_invariants();
        let id_str = id.into();

        let awareness = read_or_empty(&awareness_path);
        let mut overlay = WorkspaceOverlay {
            project_root: project_root.display().to_string(),
            terminal_profile: initial_terminal_profile.clone(),
            path_conventions: initial_path_conventions.clone(),
            current_focus: awareness
                .facts
                .iter()
                .find(|f| f.key == "workspace.current_focus")
                .map(|f| f.value.clone()),
            active_processes: Vec::new(),
            fact_count: awareness.facts.len(),
            note_count: awareness.notes.len(),
            child_projects_count: 0,
            capability_summary: capability_snapshot.clone(),
        };
        // Hydrate previously-recorded terminal gotchas from facts (category=Patterns,
        // key prefix `workspace.terminal_rec.`).
        for fact in awareness.facts.iter() {
            if fact.key.starts_with("workspace.terminal_rec.") {
                if let Some((pattern, rec)) = parse_terminal_rec_fact(&fact.value) {
                    overlay.terminal_profile.gotchas.push(TerminalRecommendation {
                        pattern,
                        recommendation: rec,
                    });
                }
            }
        }

        Self {
            id: id_str,
            project_root,
            awareness_path,
            overlay: Arc::new(Overlay::new(overlay)),
            initial_terminal_profile,
            initial_path_conventions,
            default_build_invariants,
            capability_snapshot,
            extension_pipelines: Arc::new(RwLock::new(Vec::new())),
            extension_child_projects: Arc::new(RwLock::new(Vec::new())),
            extension_terminal_recs: Arc::new(RwLock::new(Vec::new())),
            tracked_processes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn catalog(&self) -> Vec<Pipeline> {
        let mut pipelines = self.canonical_catalog();
        // Append extension-declared pipelines (Tier 1 simple fact-returners).
        for ext in self.extension_pipelines.read().unwrap().iter() {
            pipelines.push(Pipeline {
                id: format!("{}/{}", self.id, ext.name),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ReadOnly,
                params: serde_json::json!({"type": "object", "properties": {}}),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: format!("ext_pipeline:{}", ext.name),
                }],
                description: ext.description.clone(),
                when_to_use: "Operator-declared via Tier 1 extension.".to_string(),
                bypasses_kill_switch: false,
            });
        }
        // Canonical governance pipelines.
        pipelines.extend(
            crate::governance::GovernanceComposer::canonical_governance_pipelines(&self.id),
        );
        pipelines
    }

    fn canonical_catalog(&self) -> Vec<Pipeline> {
        let read_only = |name: &str, leaf_op: &str, description: &str| Pipeline {
            id: format!("{}/{}", self.id, name),
            visibility: Visibility::Surfaced,
            tunability: Tunability::Untunable,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::ReadOnly,
            params: serde_json::json!({"type": "object", "properties": {}}),
            preconditions: vec![],
            steps: vec![Step::Leaf {
                leaf_op: leaf_op.to_string(),
            }],
            description: description.to_string(),
            when_to_use: "Workspace onboarding cache; agent reads at session start."
                .to_string(),
            bypasses_kill_switch: false,
        };
        vec![
            read_only(
                "get-terminal-profile",
                "get_terminal_profile",
                "Returns primary shell, available tools, OS, and known gotchas.",
            ),
            read_only(
                "get-path-conventions",
                "get_path_conventions",
                "Returns workspace path conventions (scratchpad, logs, artifacts, secrets, cmdb).",
            ),
            read_only(
                "get-active-processes",
                "get_active_processes",
                "Returns operator-recorded tracked processes (PID + kind + description).",
            ),
            read_only(
                "get-wip-state",
                "get_wip_state",
                "Returns git WIP projection: branch, ahead/behind, modified/untracked counts.",
            ),
            read_only(
                "get-build-invariants",
                "get_build_invariants",
                "Returns canonical 'how to verify a change' commands for this workspace.",
            ),
            read_only(
                "list-child-projects",
                "list_child_projects",
                "Returns child projects (git submodules + extension entries + A2A peers).",
            ),
            read_only(
                "get-capability-profile",
                "get_capability_profile",
                "Returns the host's registered brokers, sensors, and A2A peers (snapshot at boot).",
            ),
            read_only(
                "get-current-focus",
                "get_current_focus",
                "Returns the operator-or-agent-declared current focus string.",
            ),
            // LocalAwareness facet — Surfaced mutators.
            Pipeline {
                id: format!("{}/set-fact", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ColdStoreWrite,
                params: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "key": {"type": "string"},
                        "value": {"type": "string"},
                        "category": {
                            "type": "string",
                            "enum": ["tool_paths", "environment", "patterns", "constraints", "general"]
                        },
                        "note": {"type": "string"}
                    },
                    "required": ["key", "value", "category"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf { leaf_op: "set_fact".to_string() }],
                description: "Upsert a fact in LocalAwareness (atomic disk replace + overlay re-projection).".to_string(),
                when_to_use: "Persistent machine knowledge future sessions should know.".to_string(),
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
                        "content": {"type": "string"},
                        "category": {
                            "type": "string",
                            "enum": ["tool_paths", "environment", "patterns", "constraints", "general"]
                        }
                    },
                    "required": ["content", "category"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf { leaf_op: "add_note".to_string() }],
                description: "Append a free-form note in LocalAwareness.".to_string(),
                when_to_use: "Observational knowledge that isn't a structured key/value fact.".to_string(),
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
                    "properties": {"key": {"type": "string"}},
                    "required": ["key"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf { leaf_op: "remove_fact".to_string() }],
                description: "Remove a fact from LocalAwareness by key.".to_string(),
                when_to_use: "When a previously-discovered fact is stale or superseded.".to_string(),
                bypasses_kill_switch: false,
            },
            // Agent-contribution InnateAbility.
            Pipeline {
                id: format!("{}/record-terminal-recommendation", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ColdStoreWrite,
                params: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string"},
                        "recommendation": {"type": "string"}
                    },
                    "required": ["pattern", "recommendation"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "record_terminal_recommendation".to_string(),
                }],
                description: "Agent records a discovered terminal gotcha for future sessions.".to_string(),
                when_to_use: "After learning that a particular command pattern fails on this machine.".to_string(),
                bypasses_kill_switch: false,
            },
            Pipeline {
                id: format!("{}/record-active-process", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::HotStoreUpdate,
                params: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pid": {"type": "integer"},
                        "kind": {"type": "string"},
                        "description": {"type": "string"}
                    },
                    "required": ["kind", "description"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf {
                    leaf_op: "record_active_process".to_string(),
                }],
                description: "Agent records a long-running process it spawned (so next session knows).".to_string(),
                when_to_use: "After spawning an IDE dev server, headless browser, or other process to outlive the session.".to_string(),
                bypasses_kill_switch: false,
            },
            Pipeline {
                id: format!("{}/update-focus", self.id),
                visibility: Visibility::Surfaced,
                tunability: Tunability::OperatorConfirmed,
                audit_class: AuditClass::Capability,
                effect_class: EffectClass::ColdStoreWrite,
                params: serde_json::json!({
                    "type": "object",
                    "properties": {"focus": {"type": "string"}},
                    "required": ["focus"]
                }),
                preconditions: vec![],
                steps: vec![Step::Leaf { leaf_op: "update_focus".to_string() }],
                description: "Agent or operator sets the current focus string for cross-session continuity.".to_string(),
                when_to_use: "When changing what the agent is working on; persists as a LocalAwareness fact.".to_string(),
                bypasses_kill_switch: false,
            },
        ]
    }

    // ============================================================================
    // Helpers
    // ============================================================================

    fn read_disk(&self) -> LocalAwareness {
        read_or_empty(&self.awareness_path)
    }

    fn write_awareness(&self, awareness: &LocalAwareness) -> Result<(), LeafError> {
        write_awareness_atomic(&self.awareness_path, awareness)
            .map_err(|e| LeafError::ExecutionFailed(format!("write awareness: {}", e)))
    }

    fn reproject_after_mutation(&self) {
        let awareness = self.read_disk();
        let mut overlay = (*self.overlay.load()).clone();
        overlay.fact_count = awareness.facts.len();
        overlay.note_count = awareness.notes.len();
        overlay.current_focus = awareness
            .facts
            .iter()
            .find(|f| f.key == "workspace.current_focus")
            .map(|f| f.value.clone());
        // Refresh terminal-recommendation gotchas from disk
        overlay.terminal_profile = self.initial_terminal_profile.clone();
        for fact in awareness.facts.iter() {
            if fact.key.starts_with("workspace.terminal_rec.") {
                if let Some((pattern, rec)) = parse_terminal_rec_fact(&fact.value) {
                    overlay.terminal_profile.gotchas.push(TerminalRecommendation {
                        pattern,
                        recommendation: rec,
                    });
                }
            }
        }
        // Append extension-declared terminal recs (from apply_extension).
        for rec in self.extension_terminal_recs.read().unwrap().iter() {
            overlay.terminal_profile.gotchas.push(rec.clone());
        }
        overlay.active_processes = self.tracked_processes.read().unwrap().clone();
        overlay.child_projects_count = self.list_child_projects_full().len();
        self.overlay.swap(overlay);
    }

    fn list_child_projects_full(&self) -> Vec<ChildProject> {
        let mut out = Vec::new();
        // Parse .gitmodules (if present)
        let gitmod_path = self.project_root.join(".gitmodules");
        if gitmod_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&gitmod_path) {
                out.extend(parse_gitmodules(&contents));
            }
        }
        // Append extension-declared
        out.extend(self.extension_child_projects.read().unwrap().iter().cloned());
        out
    }
}

// ============================================================================
// Broker impl
// ============================================================================

#[async_trait]
impl Broker for WorkspaceBrokerV1 {
    fn id(&self) -> &str {
        &self.id
    }

    fn role_set(&self) -> RoleSet {
        // V1 — Sense + InnateAbility (agent reads + records back).
        RoleSet {
            roles: vec![Role::Sense, Role::InnateAbility],
        }
    }

    async fn read_overlay(&self) -> serde_json::Value {
        let snap = self.overlay.load();
        serde_json::to_value(&*snap).unwrap_or(serde_json::Value::Null)
    }

    async fn legal_pipelines(&self) -> Vec<Pipeline> {
        self.catalog()
            .into_iter()
            .filter(|p| matches!(p.visibility, Visibility::Surfaced))
            .collect()
    }

    async fn governance_pipelines(&self) -> Vec<Pipeline> {
        crate::governance::GovernanceComposer::canonical_governance_pipelines(&self.id)
    }

    async fn tick(&self, _: WorldEvent) -> Result<(), BrokerError> {
        self.reproject_after_mutation();
        Ok(())
    }

    async fn execute_leaf(
        &self,
        name: &str,
        ctx: LeafContext,
    ) -> Result<serde_json::Value, LeafError> {
        // Extension-declared pipelines route through the ext_pipeline prefix.
        if let Some(ext_name) = name.strip_prefix("ext_pipeline:") {
            return self.execute_extension_pipeline(ext_name);
        }
        match name {
            "get_terminal_profile" => {
                let snap = self.overlay.load();
                Ok(serde_json::to_value(&snap.terminal_profile).unwrap_or(serde_json::Value::Null))
            }
            "get_path_conventions" => {
                let snap = self.overlay.load();
                Ok(serde_json::to_value(&snap.path_conventions).unwrap_or(serde_json::Value::Null))
            }
            "get_active_processes" => {
                let processes = self.tracked_processes.read().unwrap().clone();
                Ok(serde_json::to_value(&processes).unwrap_or(serde_json::Value::Null))
            }
            "get_wip_state" => Ok(get_wip_state(&self.project_root)),
            "get_build_invariants" => {
                Ok(serde_json::to_value(&self.default_build_invariants).unwrap_or(serde_json::Value::Null))
            }
            "list_child_projects" => {
                let projects = self.list_child_projects_full();
                Ok(serde_json::to_value(&projects).unwrap_or(serde_json::Value::Null))
            }
            "get_capability_profile" => {
                Ok(serde_json::to_value(&self.capability_snapshot).unwrap_or(serde_json::Value::Null))
            }
            "get_current_focus" => {
                let snap = self.overlay.load();
                Ok(serde_json::json!({"focus": snap.current_focus}))
            }
            // LocalAwareness facet
            "set_fact" => {
                let key = require_str(&ctx, "key")?.to_string();
                let value = require_str(&ctx, "value")?.to_string();
                let category = parse_category(&ctx);
                let note = ctx
                    .params
                    .get("note")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let mut awareness = self.read_disk();
                awareness.upsert_fact(&key, &value, category, note.as_deref());
                self.write_awareness(&awareness)?;
                self.reproject_after_mutation();
                Ok(serde_json::json!({"set": key}))
            }
            "add_note" => {
                let content = require_str(&ctx, "content")?.to_string();
                let category = parse_category(&ctx);
                let mut awareness = self.read_disk();
                awareness.add_note(&content, category);
                self.write_awareness(&awareness)?;
                self.reproject_after_mutation();
                Ok(serde_json::json!({"added": true}))
            }
            "remove_fact" => {
                let key = require_str(&ctx, "key")?.to_string();
                let mut awareness = self.read_disk();
                let removed = awareness.remove_fact(&key);
                if removed {
                    self.write_awareness(&awareness)?;
                    self.reproject_after_mutation();
                }
                Ok(serde_json::json!({"removed": removed}))
            }
            // Agent-contribution InnateAbility
            "record_terminal_recommendation" => {
                let pattern = require_str(&ctx, "pattern")?.to_string();
                let recommendation = require_str(&ctx, "recommendation")?.to_string();
                // Persist as a fact under category=Patterns so it survives across runs.
                let key = format!(
                    "workspace.terminal_rec.{}",
                    pattern_to_fact_key_suffix(&pattern)
                );
                let value = format!("{}\t{}", pattern, recommendation);
                let mut awareness = self.read_disk();
                awareness.upsert_fact(&key, &value, AwarenessCategory::Patterns, None);
                self.write_awareness(&awareness)?;
                self.reproject_after_mutation();
                Ok(serde_json::json!({"recorded": true, "key": key}))
            }
            "record_active_process" => {
                let pid = ctx.params.get("pid").and_then(|v| v.as_u64()).map(|n| n as u32);
                let kind = require_str(&ctx, "kind")?.to_string();
                let description = require_str(&ctx, "description")?.to_string();
                let tracking_token = uuid::Uuid::new_v4().to_string();
                let entry = TrackedProcess {
                    tracking_token: tracking_token.clone(),
                    pid,
                    kind,
                    description,
                };
                self.tracked_processes.write().unwrap().push(entry);
                self.reproject_after_mutation();
                Ok(serde_json::json!({"ok": true, "tracking_token": tracking_token}))
            }
            "update_focus" => {
                let focus = require_str(&ctx, "focus")?.to_string();
                let mut awareness = self.read_disk();
                awareness.upsert_fact(
                    "workspace.current_focus",
                    &focus,
                    AwarenessCategory::General,
                    None,
                );
                self.write_awareness(&awareness)?;
                self.reproject_after_mutation();
                Ok(serde_json::json!({"focus": focus}))
            }
            other => Err(LeafError::NotFound(other.to_string())),
        }
    }

    fn as_extensible(&self) -> Option<&dyn Extensible> {
        Some(self)
    }

    fn cmdb_path(&self) -> Option<PathBuf> {
        // Workspace broker does NOT export a CMDB — it's not a sensor.
        None
    }
}

impl WorkspaceBrokerV1 {
    fn execute_extension_pipeline(
        &self,
        ext_name: &str,
    ) -> Result<serde_json::Value, LeafError> {
        let pipelines = self.extension_pipelines.read().unwrap();
        let Some(ext) = pipelines.iter().find(|p| p.name == ext_name) else {
            return Err(LeafError::NotFound(format!("ext_pipeline:{}", ext_name)));
        };
        let awareness = self.read_disk();
        let value = awareness
            .facts
            .iter()
            .find(|f| f.key == ext.returns_fact_key)
            .map(|f| f.value.clone());
        Ok(serde_json::json!({
            "key": ext.returns_fact_key.clone(),
            "value": value,
        }))
    }
}

// ============================================================================
// WorkspaceBroker (substrate trait) impl
// ============================================================================

impl WorkspaceBroker for WorkspaceBrokerV1 {
    fn project_root(&self) -> &Path {
        &self.project_root
    }
}

// ============================================================================
// Extensible impl — Tier 1 TOML configs
// ============================================================================

#[async_trait]
impl Extensible for WorkspaceBrokerV1 {
    fn extension_schema_version(&self) -> &str {
        EXTENSION_SCHEMA_VERSION
    }

    async fn apply_extension(&self, config: &ExtensionConfig) -> Result<(), ExtensionError> {
        // [[facts]]
        if let Some(facts_array) = config.raw.get("facts").and_then(|v| v.as_array()) {
            for fact_value in facts_array {
                let decl: ExtensionFactDecl =
                    fact_value.clone().try_into().map_err(|e: toml::de::Error| {
                        ExtensionError::BrokerRejected {
                            broker_id: self.id.clone(),
                            path: config.source_path.clone(),
                            reason: format!("invalid [[facts]] entry: {}", e),
                        }
                    })?;
                let category = decl
                    .category
                    .as_deref()
                    .and_then(|s| AwarenessCategory::from_str(s).ok())
                    .unwrap_or(AwarenessCategory::General);
                let mut awareness = self.read_disk();
                awareness.upsert_fact(&decl.key, &decl.value, category, decl.note.as_deref());
                write_awareness_atomic(&self.awareness_path, &awareness).map_err(|e| {
                    ExtensionError::BrokerRejected {
                        broker_id: self.id.clone(),
                        path: config.source_path.clone(),
                        reason: format!("write awareness: {}", e),
                    }
                })?;
            }
        }
        // [[terminal_recommendations]]
        if let Some(recs) = config
            .raw
            .get("terminal_recommendations")
            .and_then(|v| v.as_array())
        {
            let mut store = self.extension_terminal_recs.write().unwrap();
            for rec_value in recs {
                let decl: ExtensionTerminalRecDecl =
                    rec_value.clone().try_into().map_err(|e: toml::de::Error| {
                        ExtensionError::BrokerRejected {
                            broker_id: self.id.clone(),
                            path: config.source_path.clone(),
                            reason: format!("invalid [[terminal_recommendations]] entry: {}", e),
                        }
                    })?;
                store.push(TerminalRecommendation {
                    pattern: decl.pattern,
                    recommendation: decl.recommendation,
                });
            }
        }
        // [[pipelines]]
        if let Some(pipelines) = config.raw.get("pipelines").and_then(|v| v.as_array()) {
            let mut store = self.extension_pipelines.write().unwrap();
            for p_value in pipelines {
                let decl: ExtensionPipelineDecl =
                    p_value.clone().try_into().map_err(|e: toml::de::Error| {
                        ExtensionError::BrokerRejected {
                            broker_id: self.id.clone(),
                            path: config.source_path.clone(),
                            reason: format!("invalid [[pipelines]] entry: {}", e),
                        }
                    })?;
                // Reject collisions with canonical pipeline names.
                let collision_id = format!("workspace/{}", decl.name);
                if Self::canonical_pipeline_ids().contains(&collision_id.as_str()) {
                    return Err(ExtensionError::BrokerRejected {
                        broker_id: self.id.clone(),
                        path: config.source_path.clone(),
                        reason: format!(
                            "extension pipeline `{}` collides with canonical pipeline ID `{}`",
                            decl.name, collision_id
                        ),
                    });
                }
                store.push(ExtensionPipeline {
                    name: decl.name,
                    description: decl.description,
                    returns_fact_key: decl.returns_fact_key,
                });
            }
        }
        // [[child_projects]]
        if let Some(children) = config.raw.get("child_projects").and_then(|v| v.as_array()) {
            let mut store = self.extension_child_projects.write().unwrap();
            for c_value in children {
                let decl: ExtensionChildProjectDecl =
                    c_value.clone().try_into().map_err(|e: toml::de::Error| {
                        ExtensionError::BrokerRejected {
                            broker_id: self.id.clone(),
                            path: config.source_path.clone(),
                            reason: format!("invalid [[child_projects]] entry: {}", e),
                        }
                    })?;
                store.push(ChildProject {
                    id: decl.id,
                    path: decl.path,
                    role: decl.role,
                    a2a_port: decl.a2a_port,
                });
            }
        }
        // Re-project after extension applied so overlay reflects new state.
        self.reproject_after_mutation();
        Ok(())
    }
}

// ============================================================================
// Free helpers
// ============================================================================

fn require_str<'a>(ctx: &'a LeafContext, key: &str) -> Result<&'a str, LeafError> {
    ctx.params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| LeafError::InputInvalid(format!("missing {}", key)))
}

fn parse_category(ctx: &LeafContext) -> AwarenessCategory {
    ctx.params
        .get("category")
        .and_then(|v| v.as_str())
        .and_then(|s| AwarenessCategory::from_str(s).ok())
        .unwrap_or(AwarenessCategory::General)
}

fn pattern_to_fact_key_suffix(pattern: &str) -> String {
    // Use a stable hash so repeated records of the same pattern overwrite
    // rather than accumulate.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    pattern.hash(&mut h);
    format!("{:x}", h.finish())
}

fn parse_terminal_rec_fact(value: &str) -> Option<(String, String)> {
    let mut parts = value.splitn(2, '\t');
    let pattern = parts.next()?.to_string();
    let recommendation = parts.next()?.to_string();
    Some((pattern, recommendation))
}

fn read_or_empty(path: &Path) -> LocalAwareness {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<LocalAwareness>(&s).ok())
        .unwrap_or_else(LocalAwareness::empty)
}

fn write_awareness_atomic(path: &Path, awareness: &LocalAwareness) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(awareness)?;
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)
}

fn default_path_conventions(project_root: &Path) -> PathConventions {
    PathConventions {
        project_root: project_root.display().to_string(),
        scratchpad: Some(".claude/brain/scratchpad".to_string()),
        logs: None,
        artifacts: Some("target".to_string()),
        secrets: Some(".claude/secrets/secret-refs.json".to_string()),
        cmdb: Some(".claude".to_string()),
    }
}

fn default_build_invariants() -> Vec<BuildInvariant> {
    vec![
        BuildInvariant {
            name: "cargo-check".to_string(),
            command: "cargo check --workspace --all-targets".to_string(),
            description: "Verify the workspace compiles + tests typecheck.".to_string(),
        },
        BuildInvariant {
            name: "cargo-test".to_string(),
            command: "cargo test --workspace --all-targets".to_string(),
            description: "Run the full workspace test suite.".to_string(),
        },
        BuildInvariant {
            name: "neurogrim-doctor".to_string(),
            command: "neurogrim doctor".to_string(),
            description: "Verify the Brain's configuration is sound.".to_string(),
        },
    ]
}

fn detect_terminal_profile() -> TerminalProfile {
    let os = std::env::consts::OS.to_string();
    // V1: declare conservative defaults. Operators override via the
    // extensions/workspace/*.toml mechanism (terminal_recommendations).
    let (primary_shell, default_tools) = match os.as_str() {
        "windows" => (
            "powershell".to_string(),
            vec!["git", "cargo", "rustc"].into_iter().map(String::from).collect(),
        ),
        "linux" | "macos" => (
            "bash".to_string(),
            vec!["git", "cargo", "rustc", "make"]
                .into_iter()
                .map(String::from)
                .collect(),
        ),
        _ => (
            "unknown".to_string(),
            vec!["git", "cargo"].into_iter().map(String::from).collect(),
        ),
    };
    TerminalProfile {
        primary_shell,
        available_tools: default_tools,
        os,
        gotchas: Vec::new(),
    }
}

fn get_wip_state(project_root: &Path) -> serde_json::Value {
    // Shell out to git for V1 (avoids libgit2 dep). Returns shape-stable
    // empty projection if git is missing or the dir isn't a repo.
    let status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_root)
        .output();
    let Ok(status) = status_output else {
        return serde_json::json!({"present": false});
    };
    if !status.status.success() {
        return serde_json::json!({"present": false});
    }
    let lines = String::from_utf8_lossy(&status.stdout);
    let modified_count = lines.lines().filter(|l| !l.starts_with("??")).count();
    let untracked_count = lines.lines().filter(|l| l.starts_with("??")).count();

    let branch_output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok();
    let branch = branch_output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "HEAD".to_string());

    serde_json::json!({
        "present": true,
        "branch": branch,
        "modified_count": modified_count,
        "untracked_count": untracked_count,
    })
}

fn parse_gitmodules(contents: &str) -> Vec<ChildProject> {
    // Minimal parser — we don't need full INI semantics, just the path field
    // from each `[submodule "<name>"]` block.
    let mut out = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_path: Option<String> = None;
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("[submodule \"") {
            if let (Some(id), Some(path)) = (current_id.take(), current_path.take()) {
                out.push(ChildProject {
                    id,
                    path,
                    role: "submodule".to_string(),
                    a2a_port: None,
                });
            }
            current_id = rest.strip_suffix("\"]").map(|s| s.to_string());
            current_path = None;
        } else if let Some(rest) = trimmed.strip_prefix("path = ") {
            current_path = Some(rest.to_string());
        }
    }
    if let (Some(id), Some(path)) = (current_id, current_path) {
        out.push(ChildProject {
            id,
            path,
            role: "submodule".to_string(),
            a2a_port: None,
        });
    }
    out
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::ParamMap;
    use tempfile::TempDir;

    fn make_broker(tmp: &TempDir) -> Arc<WorkspaceBrokerV1> {
        let project_root = tmp.path().to_path_buf();
        let awareness_path = project_root.join(".claude/brain/local-awareness.json");
        Arc::new(WorkspaceBrokerV1::new(
            "workspace",
            project_root,
            awareness_path,
            CapabilitySnapshot {
                registered_brokers: vec!["workspace".to_string()],
                registered_sensors: vec!["coherence".to_string()],
                a2a_peers: Vec::new(),
            },
        ))
    }

    async fn dispatch(
        broker: &WorkspaceBrokerV1,
        leaf: &str,
        params: ParamMap,
    ) -> Result<serde_json::Value, LeafError> {
        let ctx = LeafContext {
            broker_id: broker.id.clone(),
            pipeline_id: format!("workspace/{}", leaf),
            params,
            overlay_snapshot: serde_json::Value::Null,
            frame: crate::frame::Frame::default(),
        };
        broker.execute_leaf(leaf, ctx).await
    }

    #[tokio::test]
    async fn workspace_v1_role_set_is_sense_plus_innate() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        assert!(broker.role_set().contains(&Role::Sense));
        assert!(broker.role_set().contains(&Role::InnateAbility));
    }

    #[tokio::test]
    async fn workspace_v1_catalog_has_14_canonical_pipelines() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let canonical: Vec<String> = broker
            .canonical_catalog()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(canonical.len(), 14);
        for expected_id in WorkspaceBrokerV1::canonical_pipeline_ids() {
            assert!(
                canonical.contains(&expected_id.to_string()),
                "canonical catalog missing {}",
                expected_id
            );
        }
    }

    #[tokio::test]
    async fn workspace_v1_get_terminal_profile_returns_os() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let result = dispatch(&broker, "get_terminal_profile", ParamMap::new())
            .await
            .unwrap();
        assert!(result["os"].is_string());
        assert!(result["primary_shell"].is_string());
    }

    #[tokio::test]
    async fn workspace_v1_get_capability_profile_returns_snapshot() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let result = dispatch(&broker, "get_capability_profile", ParamMap::new())
            .await
            .unwrap();
        let brokers = result["registered_brokers"].as_array().unwrap();
        assert!(brokers.iter().any(|v| v == "workspace"));
    }

    #[tokio::test]
    async fn workspace_v1_set_fact_persists_to_disk_and_reprojects() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let mut params = ParamMap::new();
        params.insert("key".into(), serde_json::Value::String("my.key".into()));
        params.insert("value".into(), serde_json::Value::String("hello".into()));
        params.insert("category".into(), serde_json::Value::String("general".into()));
        dispatch(&broker, "set_fact", params).await.unwrap();
        // Disk reflects it.
        let disk_contents =
            std::fs::read_to_string(tmp.path().join(".claude/brain/local-awareness.json")).unwrap();
        assert!(disk_contents.contains("my.key"));
        assert!(disk_contents.contains("hello"));
        // Overlay fact_count incremented.
        let overlay = broker.read_overlay().await;
        assert_eq!(overlay["fact_count"], 1);
    }

    #[tokio::test]
    async fn workspace_v1_update_focus_persists_and_surfaces() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let mut params = ParamMap::new();
        params.insert(
            "focus".into(),
            serde_json::Value::String("test focus".into()),
        );
        dispatch(&broker, "update_focus", params).await.unwrap();
        let result = dispatch(&broker, "get_current_focus", ParamMap::new())
            .await
            .unwrap();
        assert_eq!(result["focus"], "test focus");
    }

    #[tokio::test]
    async fn workspace_v1_record_active_process_returns_token_and_persists() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let mut params = ParamMap::new();
        params.insert("kind".into(), serde_json::Value::String("ide".into()));
        params.insert(
            "description".into(),
            serde_json::Value::String("running IDE dev server".into()),
        );
        params.insert(
            "pid".into(),
            serde_json::Value::Number(serde_json::Number::from(12345)),
        );
        let result = dispatch(&broker, "record_active_process", params)
            .await
            .unwrap();
        assert!(result["tracking_token"].is_string());
        let processes = dispatch(&broker, "get_active_processes", ParamMap::new())
            .await
            .unwrap();
        let arr = processes.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["pid"], 12345);
    }

    #[tokio::test]
    async fn workspace_v1_record_terminal_recommendation_persists_across_reload() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path().to_path_buf();
        let awareness_path = project_root.join(".claude/brain/local-awareness.json");

        // Round 1: record a recommendation
        {
            let broker = WorkspaceBrokerV1::new(
                "workspace",
                project_root.clone(),
                awareness_path.clone(),
                CapabilitySnapshot::default(),
            );
            let mut params = ParamMap::new();
            params.insert(
                "pattern".into(),
                serde_json::Value::String("^bash.*head".into()),
            );
            params.insert(
                "recommendation".into(),
                serde_json::Value::String("use PowerShell Get-Content -TotalCount".into()),
            );
            let ctx = LeafContext {
                broker_id: "workspace".into(),
                pipeline_id: "workspace/record-terminal-recommendation".into(),
                params,
                overlay_snapshot: serde_json::Value::Null,
                frame: crate::frame::Frame::default(),
            };
            broker
                .execute_leaf("record_terminal_recommendation", ctx)
                .await
                .unwrap();
        }
        // Round 2: re-instantiate broker; gotcha should hydrate from disk.
        let broker2 = WorkspaceBrokerV1::new(
            "workspace",
            project_root,
            awareness_path,
            CapabilitySnapshot::default(),
        );
        let result = broker2
            .execute_leaf(
                "get_terminal_profile",
                LeafContext {
                    broker_id: "workspace".into(),
                    pipeline_id: "workspace/get-terminal-profile".into(),
                    params: ParamMap::new(),
                    overlay_snapshot: serde_json::Value::Null,
                    frame: crate::frame::Frame::default(),
                },
            )
            .await
            .unwrap();
        let gotchas = result["gotchas"].as_array().unwrap();
        assert!(
            gotchas.iter().any(|g| g["pattern"] == "^bash.*head"),
            "gotcha should hydrate from disk; got: {:?}",
            gotchas
        );
    }

    #[tokio::test]
    async fn workspace_v1_list_child_projects_parses_gitmodules() {
        let tmp = TempDir::new().unwrap();
        let gitmod = "[submodule \"NeuroGrim\"]\n\tpath = NeuroGrim\n\turl = https://example.com/foo.git\n[submodule \"LSP-Brains\"]\n\tpath = LSP-Brains\n";
        std::fs::write(tmp.path().join(".gitmodules"), gitmod).unwrap();
        let broker = make_broker(&tmp);
        let result = dispatch(&broker, "list_child_projects", ParamMap::new())
            .await
            .unwrap();
        let arr = result.as_array().unwrap();
        assert!(arr.iter().any(|p| p["id"] == "NeuroGrim" && p["path"] == "NeuroGrim"));
        assert!(arr
            .iter()
            .any(|p| p["id"] == "LSP-Brains" && p["path"] == "LSP-Brains"));
    }

    #[tokio::test]
    async fn workspace_v1_get_build_invariants_returns_defaults() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let result = dispatch(&broker, "get_build_invariants", ParamMap::new())
            .await
            .unwrap();
        let arr = result.as_array().unwrap();
        assert!(arr.iter().any(|i| i["name"] == "cargo-check"));
        assert!(arr.iter().any(|i| i["name"] == "neurogrim-doctor"));
    }

    #[tokio::test]
    async fn workspace_v1_extension_facts_apply_at_boot() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let ext_toml = r#"
[extension]
schema_version = "1"

[[facts]]
key = "team.region"
value = "us-west-2"
category = "general"
"#;
        let config = ExtensionConfig {
            source_path: PathBuf::from("/tmp/test.toml"),
            target_broker_id: "workspace".into(),
            schema_version: "1".into(),
            authored_by: None,
            raw: toml::from_str(ext_toml).unwrap(),
        };
        broker.apply_extension(&config).await.unwrap();
        // Disk reflects it.
        let disk_contents =
            std::fs::read_to_string(tmp.path().join(".claude/brain/local-awareness.json")).unwrap();
        assert!(disk_contents.contains("team.region"));
        assert!(disk_contents.contains("us-west-2"));
    }

    #[tokio::test]
    async fn workspace_v1_extension_terminal_recs_appear_in_profile() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let ext_toml = r#"
[extension]
schema_version = "1"

[[terminal_recommendations]]
pattern = "deploy.*"
recommendation = "Use aws-vault exec prod -- deploy.sh"
"#;
        let config = ExtensionConfig {
            source_path: PathBuf::from("/tmp/test.toml"),
            target_broker_id: "workspace".into(),
            schema_version: "1".into(),
            authored_by: None,
            raw: toml::from_str(ext_toml).unwrap(),
        };
        broker.apply_extension(&config).await.unwrap();
        let result = dispatch(&broker, "get_terminal_profile", ParamMap::new())
            .await
            .unwrap();
        let gotchas = result["gotchas"].as_array().unwrap();
        assert!(gotchas.iter().any(|g| g["pattern"] == "deploy.*"));
    }

    #[tokio::test]
    async fn workspace_v1_extension_pipeline_dispatches_and_returns_fact() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);

        // Pre-set a fact
        let mut params = ParamMap::new();
        params.insert("key".into(), serde_json::Value::String("api.url".into()));
        params.insert(
            "value".into(),
            serde_json::Value::String("https://api.example.com".into()),
        );
        params.insert("category".into(), serde_json::Value::String("general".into()));
        dispatch(&broker, "set_fact", params).await.unwrap();

        // Apply extension declaring a pipeline that returns this fact
        let ext_toml = r#"
[extension]
schema_version = "1"

[[pipelines]]
name = "get-api-url"
description = "Returns the API URL."
returns_fact_key = "api.url"
"#;
        let config = ExtensionConfig {
            source_path: PathBuf::from("/tmp/test.toml"),
            target_broker_id: "workspace".into(),
            schema_version: "1".into(),
            authored_by: None,
            raw: toml::from_str(ext_toml).unwrap(),
        };
        broker.apply_extension(&config).await.unwrap();

        // Dispatch the extension pipeline
        let result = broker
            .execute_extension_pipeline("get-api-url")
            .unwrap();
        assert_eq!(result["key"], "api.url");
        assert_eq!(result["value"], "https://api.example.com");
    }

    #[tokio::test]
    async fn workspace_v1_extension_pipeline_name_collision_rejected() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let ext_toml = r#"
[extension]
schema_version = "1"

[[pipelines]]
name = "set-fact"
description = "Would collide with canonical workspace/set-fact pipeline."
returns_fact_key = "x.y"
"#;
        let config = ExtensionConfig {
            source_path: PathBuf::from("/tmp/test.toml"),
            target_broker_id: "workspace".into(),
            schema_version: "1".into(),
            authored_by: None,
            raw: toml::from_str(ext_toml).unwrap(),
        };
        let err = broker.apply_extension(&config).await.unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("collides with canonical pipeline ID"),
            "expected collision error, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn workspace_v1_extension_child_projects_appear_in_listing() {
        let tmp = TempDir::new().unwrap();
        let broker = make_broker(&tmp);
        let ext_toml = r#"
[extension]
schema_version = "1"

[[child_projects]]
id = "external-tool"
path = "../external-tool"
role = "tool"
"#;
        let config = ExtensionConfig {
            source_path: PathBuf::from("/tmp/test.toml"),
            target_broker_id: "workspace".into(),
            schema_version: "1".into(),
            authored_by: None,
            raw: toml::from_str(ext_toml).unwrap(),
        };
        broker.apply_extension(&config).await.unwrap();
        let result = dispatch(&broker, "list_child_projects", ParamMap::new())
            .await
            .unwrap();
        let arr = result.as_array().unwrap();
        assert!(
            arr.iter().any(|p| p["id"] == "external-tool"),
            "child project from extension should appear in listing"
        );
    }
}
