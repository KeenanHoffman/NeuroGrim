//! BB #7 `Pipeline` + BB #8 `Step`.
//!
//! The universal unit. Per BROKER-INTERNALS.md §1.3: a `Pipeline` is a
//! serializable struct passed to consumers — `Serialize + Deserialize + Clone`,
//! lives in YAML on disk, in the Pipeline Catalog in RAM, in the audit ledger
//! after dispatch.
//!
//! ## Concrete field types (BROKER-SPEC-GAPS.md gap #2 resolution; Wave 1)
//!
//! - `PipelineId` = operator-assigned String (`<broker_id>/<pipeline_name>`)
//!   — chosen over hash-derived IDs to avoid the R-O-10 silent-breakage risk
//!   on framework upgrades.
//! - `ParamSchema` = `serde_json::Value` validated as JSON Schema at load time
//!   (Wave 2: BB #9 Catalog loader). Dispatch-time param validation also
//!   uses JSON Schema (Wave 2).
//! - `ParamMap` = `serde_json::Map<String, serde_json::Value>` (Wave 2).
//! - `GovernancePolicy` = a struct declaring which framework-provided
//!   governance pipelines compose into this pipeline (Wave 4: BB #19).
//! - `EffectClass` = enum for idempotency reasoning + audit grouping; values
//!   pinned in Wave 2.

use serde::{Deserialize, Serialize};

/// Operator-assigned pipeline identifier. Format: `<broker_id>/<pipeline_name>`.
pub type PipelineId = String;

/// Parameter map for dispatch (validated against ParamSchema at dispatch time).
pub type ParamMap = serde_json::Map<String, serde_json::Value>;

/// Pipeline visibility classification (per BROKER-CONTRACT.md Glossary).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    /// Tier 1 — LLM-facing; appears in `legal_pipelines()` ranking + in
    /// `current-projection.md` awareness routing.
    Surfaced,
    /// Tier 2 — broker plumbing; traced + governed but not surfaced.
    Internal,
    /// Tier 3 (A14) — agent-invokable via the broker host's dispatch ceremony
    /// for audit-completeness, but NOT enumerated in `current-projection.md`
    /// awareness routing. Used for pipelines that are IDE-facing infrastructure
    /// (e.g., browser overlay primitives the IDE host calls directly) where the
    /// agent shouldn't see them as choices but the operator wants every
    /// invocation in trace.jsonl. Per V0-RETRO ultra-pass U6.
    AuditOnly,
}

/// Pipeline tunability tier (per BROKER-INTERNALS.md §4).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Tunability {
    /// Code change required; manifest cannot tune.
    Untunable,
    /// Operator edits config files / Brain UI; reload-catalog applies.
    OperatorOnly,
    /// LLM proposes via tuning pipeline → proposal ledger → operator confirms.
    OperatorConfirmed,
    /// LLM tunes directly within declared bounds; reversible.
    Autonomous,
}

/// Audit class per BB #20 Skill Filter classifier exclusion rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuditClass {
    /// Capability work — counted by hygiene classifier.
    Capability,
    /// Governance work — counted by hygiene classifier.
    Governance,
    /// Meta-observation — excluded from hygiene feed to prevent self-referential
    /// inflation (per §2.3 closure).
    MetaObservation,
}

/// Effect class for idempotency reasoning + audit grouping.
/// Wave 2 finalizes the enum variants.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EffectClass {
    /// Pure read; no state mutation.
    ReadOnly,
    /// Updates broker-internal state (hot store; working state).
    HotStoreUpdate,
    /// Writes to cold store (durable; survives broker restart).
    ColdStoreWrite,
    /// External world-effect (dispatch to Effector; user-visible action).
    WorldEffect,
}

/// A pipeline — the universal unit per BB #7.
/// Wave 2 fills in the full schema; this is the Wave 0 skeleton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: PipelineId,
    pub visibility: Visibility,
    pub tunability: Tunability,
    pub audit_class: AuditClass,
    pub effect_class: EffectClass,

    /// JSON Schema validated at catalog load + at dispatch time.
    #[serde(default)]
    pub params: serde_json::Value,

    /// Precondition predicate expressions (Wave 2: DSL decision; BROKER-SPEC-GAPS
    /// gap #7).
    #[serde(default)]
    pub preconditions: Vec<String>,

    /// Ordered step sequence.
    pub steps: Vec<Step>,

    /// Description shown to agent in `current-projection.md` awareness segment.
    #[serde(default)]
    pub description: String,

    /// When-to-use hint shown to agent (~512 chars per BROKER-AWARENESS.md §1).
    #[serde(default)]
    pub when_to_use: String,

    /// A1.5 / B-64 / V0-RETRO ultra-pass U2 — escape hatch for the
    /// kill-switch bootstrap paradox. When `true`, the framework's
    /// `check-kill-switch` governance pre-dispatch step is SKIPPED for this
    /// pipeline, allowing it to dispatch even while armed. Only framework-
    /// provided governance pipelines (`arm-kill-switch`, `disengage-kill-
    /// switch`) should set this; the catalog loader rejects user-authored
    /// pipelines with `bypasses_kill_switch = true` AND
    /// `audit_class != AuditClass::Governance` to prevent agents defining
    /// bypass routes.
    #[serde(default)]
    pub bypasses_kill_switch: bool,
    // Wave 4 adds: governance: GovernancePolicy (BB #19)
}

/// One step in a pipeline (per BB #8).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "step_type", rename_all = "kebab-case")]
pub enum Step {
    /// Calls a Rust leaf-op function (Tier 3 plain function).
    Leaf { leaf_op: String },
    /// Composes another pipeline as a sub-step.
    /// MVP: intra-broker only (Wave 2 catalog loader rejects cross-broker
    /// `sub_pipeline:` refs per BB #27 deferral; U12 ultra-pass).
    SubPipeline {
        sub_pipeline: PipelineId,
        #[serde(default)]
        params: ParamMap,
    },
    /// Run inner step iff predicate evaluates true (Wave 2).
    Guard {
        predicate: String,
        inner: Box<Step>,
    },
    /// If/else over hot store (Wave 2).
    Branch {
        predicate: String,
        if_true: Box<Step>,
        if_false: Box<Step>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_serde_round_trip() {
        let p = Pipeline {
            id: "work-broker/dispatch-work-unit".to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorConfirmed,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::HotStoreUpdate,
            params: serde_json::json!({"type": "object", "properties": {"work_unit_id": {"type": "string"}}}),
            preconditions: vec!["work_unit_exists".to_string()],
            steps: vec![Step::Leaf {
                leaf_op: "claim_work_unit".to_string(),
            }],
            description: "Dispatch a work unit from the active backlog.".to_string(),
            when_to_use: "When the operator is ready to start the next work item.".to_string(),
            bypasses_kill_switch: false,
        };
        let yaml = serde_yaml::to_string(&p).unwrap();
        let parsed: Pipeline = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.id, p.id);
        assert_eq!(parsed.visibility, Visibility::Surfaced);
        assert!(!parsed.bypasses_kill_switch);
    }

    #[test]
    fn pipeline_bypasses_kill_switch_defaults_to_false_via_serde() {
        // A1.5 / B-64: existing manifest YAML without the new field must
        // deserialize cleanly with the field defaulting to false.
        let yaml = r#"
id: t/legacy
visibility: surfaced
tunability: operator-only
audit_class: capability
effect_class: read-only
steps: []
"#;
        let p: Pipeline = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.id, "t/legacy");
        assert!(!p.bypasses_kill_switch);
    }

    #[test]
    fn pipeline_audit_only_visibility_round_trips() {
        // A14 / U6: the new AuditOnly visibility class.
        let yaml = r#"
id: t/internal-but-audited
visibility: audit-only
tunability: untunable
audit_class: capability
effect_class: hot-store-update
steps: []
"#;
        let p: Pipeline = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.visibility, Visibility::AuditOnly);
        let back = serde_yaml::to_string(&p).unwrap();
        assert!(back.contains("visibility: audit-only"));
    }
}
