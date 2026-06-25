//! `neurogrim broker-scaffold` — Phase C9 prerequisite per plan §C9.
//!
//! Emits a Pipeline skeleton + leaf-op match-arm stub for a given broker
//! verb. Designed for the IDE's IdeAction consolidation (40+ enum variants
//! → 40+ Surfaced pipelines on appropriate brokers), where hand-authoring
//! every pipeline + leaf-op signature would be repetitive purgatory.
//!
//! ## V0 scope
//!
//! Single-pipeline scaffold (one invocation = one pipeline + one leaf-op).
//! Future versions may take an enum source file + emit pipelines for every
//! variant in one pass; for now, one verb at a time keeps the operator in
//! the loop on each scaffold.
//!
//! ## Usage
//!
//! ```bash
//! neurogrim broker-scaffold \
//!     --broker-id browser-kill-switch \
//!     --pipeline-name arm-kill-switch \
//!     --visibility surfaced \
//!     --audit-class governance \
//!     --leaf-op arm_kill_switch
//! ```
//!
//! Emits the Pipeline literal (paste into `broker.catalog()`) + leaf-op
//! match-arm stub (paste into `Broker::execute_leaf`).

use anyhow::Result;

#[derive(Debug)]
pub struct ScaffoldArgs {
    pub broker_id: String,
    pub pipeline_name: String,
    pub visibility: String,
    pub audit_class: String,
    pub leaf_op: String,
    pub description: String,
    pub when_to_use: String,
    pub params_schema_json: String,
}

pub fn run(args: ScaffoldArgs) -> Result<()> {
    let visibility_rust = match args.visibility.as_str() {
        "surfaced" => "Visibility::Surfaced",
        "internal" => "Visibility::Internal",
        "audit-only" => "Visibility::AuditOnly",
        other => {
            anyhow::bail!(
                "unknown visibility `{}`; expected: surfaced | internal | audit-only",
                other
            )
        }
    };
    let audit_class_rust = match args.audit_class.as_str() {
        "capability" => "AuditClass::Capability",
        "governance" => "AuditClass::Governance",
        "meta-observation" => "AuditClass::MetaObservation",
        other => {
            anyhow::bail!(
                "unknown audit-class `{}`; expected: capability | governance | meta-observation",
                other
            )
        }
    };

    let pipeline_skeleton = format!(
        r#"// === scaffolded pipeline literal — paste into broker.catalog() ===
Pipeline {{
    id: format!("{{}}/{name}", self.id),
    visibility: {visibility},
    tunability: Tunability::OperatorConfirmed,
    audit_class: {audit_class},
    effect_class: EffectClass::HotStoreUpdate, // FIXME: classify effect
    params: serde_json::json!({params_schema}),
    preconditions: vec![],
    steps: vec![Step::Leaf {{
        leaf_op: "{leaf_op}".to_string(),
    }}],
    description: "{description}".to_string(),
    when_to_use: "{when_to_use}".to_string(),
    bypasses_kill_switch: false, // set true ONLY for arm/disengage governance pipelines
}},"#,
        name = args.pipeline_name,
        visibility = visibility_rust,
        audit_class = audit_class_rust,
        params_schema = args.params_schema_json,
        leaf_op = args.leaf_op,
        description = args.description,
        when_to_use = args.when_to_use,
    );

    let leaf_op_skeleton = format!(
        r#"// === scaffolded leaf-op match-arm — paste into Broker::execute_leaf ===
"{leaf_op}" => {{
    // FIXME: implement {leaf_op}. Read params via ctx.params.get("<field>");
    // mutate working state under lock; return JSON-encoded result.
    Ok(serde_json::json!({{"todo": "{leaf_op}"}}))
}}"#,
        leaf_op = args.leaf_op,
    );

    println!("{}", pipeline_skeleton);
    println!();
    println!("{}", leaf_op_skeleton);
    println!();
    println!(
        "// NOTE: scaffolder is intentionally MINIMAL — review the FIXMEs above"
    );
    println!("// before pasting. Per plan §C9, ~30-40% of IdeAction variants will need");
    println!(
        "// bespoke leaf-op shapes (Tauri AppHandle, WebView2 access, IDE-internal"
    );
    println!("// state) that the scaffolder can't infer. Use this for the mechanical");
    println!("// majority + hand-author the bespoke shapes separately.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_args() -> ScaffoldArgs {
        ScaffoldArgs {
            broker_id: "browser-kill-switch".to_string(),
            pipeline_name: "arm-kill-switch".to_string(),
            visibility: "surfaced".to_string(),
            audit_class: "governance".to_string(),
            leaf_op: "arm_kill_switch".to_string(),
            description: "Arm the browser kill switch.".to_string(),
            when_to_use: "Operator-controlled emergency halt.".to_string(),
            params_schema_json: "{}".to_string(),
        }
    }

    #[test]
    fn scaffold_succeeds_for_valid_args() {
        run(fixture_args()).unwrap();
    }

    #[test]
    fn scaffold_rejects_unknown_visibility() {
        let mut args = fixture_args();
        args.visibility = "private".to_string();
        let err = run(args).unwrap_err();
        assert!(err.to_string().contains("unknown visibility"));
    }

    #[test]
    fn scaffold_rejects_unknown_audit_class() {
        let mut args = fixture_args();
        args.audit_class = "secret".to_string();
        let err = run(args).unwrap_err();
        assert!(err.to_string().contains("unknown audit-class"));
    }
}
