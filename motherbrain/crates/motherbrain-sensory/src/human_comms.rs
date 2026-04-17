//! Human communications sensory tool — persistent human model.
//!
//! Reads two YAML preference files and merges them to produce a CMDB that
//! captures how a specific human wants to receive information from agents:
//!
//!   ~/.claude/human-comms.yaml        — user-scoped defaults (never committed)
//!   {project_root}/.claude/human-comms.yaml  — project-scoped overrides (committed)
//!
//! Merge rule: project-scoped fields win over user-scoped fields.
//! All preference values are flattened into top-level CMDB fields so they
//! become domain variables available to correlation rules and agent output.
//!
//! Score = preference completeness:
//!   +25 pts per top-level block (communication, format, signals, interaction)
//!   that has ≥1 key explicitly defined in the merged result.

use crate::cmdb::{build_cmdb, Finding};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct HumanCommsServer {
    tool_router: ToolRouter<Self>,
}
impl HumanCommsServer {
    pub fn new() -> Self {
        Self { tool_router: Self::tool_router() }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckHumanCommsParams {
    pub project_root: String,
}

#[tool_router]
impl HumanCommsServer {
    #[tool(description = "Check human communication preferences: reads ~/.claude/human-comms.yaml \
        (user-scoped defaults) and {project_root}/.claude/human-comms.yaml (project-scoped \
        overrides), merges them, and scores preference completeness. Returns CMDB-envelope JSON \
        with all preferences flattened as domain variables.")]
    async fn check_human_comms(&self, Parameters(p): Parameters<CheckHumanCommsParams>) -> String {
        serde_json::to_string_pretty(&analyze_human_comms(&p.project_root).await)
            .unwrap_or_default()
    }
}

impl ServerHandler for HumanCommsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Human communications sensory tool. Merges user-scoped and project-scoped \
                preference YAML files into a scored CMDB of communication preferences."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Public analysis entry point ───────────────────────────────────────────────

pub async fn analyze_human_comms(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings: Vec<Finding> = Vec::new();
    let mut score: i32 = 0;
    let mut extras: Vec<(&str, Value)> = Vec::new();

    // ── Step 1: Locate and load both preference files ─────────────────────────

    let user_home_path = user_home();
    let user_yaml_path = user_home_path.as_ref().map(|h| h.join(".claude/human-comms.yaml"));
    let project_yaml_path = root.join(".claude/human-comms.yaml");

    let (user_prefs, has_user_defaults) = load_yaml(user_yaml_path.as_deref()).await;
    let (project_prefs, has_project_overrides) = load_yaml(Some(&project_yaml_path)).await;

    // ── Step 2: Merge — project-scoped wins over user-scoped ──────────────────
    let merged = deep_merge(user_prefs, project_prefs);

    // ── Step 3: Score completeness — 25 pts per defined block ─────────────────
    let comm_block    = merged.get("communication");
    let format_block  = merged.get("format");
    let signals_block = merged.get("signals");
    let interact_block = merged.get("interaction");

    let comm_defined    = block_has_keys(comm_block);
    let format_defined  = block_has_keys(format_block);
    let signals_defined = block_has_keys(signals_block);
    let interact_defined = block_has_keys(interact_block);

    if comm_defined    { score += 25; } else {
        findings.push(Finding {
            name: "communication_preferences".into(),
            status: "undefined".into(),
            points: 0,
            detail: Some("No communication preferences set. Add `communication:` block to human-comms.yaml.".into()),
        });
    }
    if format_defined  { score += 25; } else {
        findings.push(Finding {
            name: "format_preferences".into(),
            status: "undefined".into(),
            points: 0,
            detail: Some("No format preferences set. Add `format:` block to human-comms.yaml.".into()),
        });
    }
    if signals_defined { score += 25; } else {
        findings.push(Finding {
            name: "signal_preferences".into(),
            status: "undefined".into(),
            points: 0,
            detail: Some("No signal preferences set. Add `signals:` block to human-comms.yaml.".into()),
        });
    }
    if interact_defined { score += 25; } else {
        findings.push(Finding {
            name: "interaction_preferences".into(),
            status: "undefined".into(),
            points: 0,
            detail: Some("No interaction preferences set. Add `interaction:` block to human-comms.yaml.".into()),
        });
    }

    let preferences_complete = score == 100;

    // ── Step 4: Extract and flatten individual preferences ────────────────────

    // communication block
    let include_urls = get_bool(&merged, "communication", "include_urls", false);
    let verbosity    = get_str(&merged, "communication", "verbosity", "standard");
    let lead_with    = get_str(&merged, "communication", "lead_with", "answer");

    // format block
    let code_blocks    = get_str(&merged, "format", "code_blocks", "always");
    let lists_vs_prose = get_str(&merged, "format", "lists_vs_prose", "lists");
    let emoji          = get_str(&merged, "format", "emoji", "never");

    // signals block
    let proactive_hat_suggestions = get_bool(&merged, "signals", "proactive_hat_suggestions", true);
    let alert_on_correlation_fire = get_bool(&merged, "signals", "alert_on_correlation_fire", true);
    let include_why_context       = get_bool(&merged, "signals", "include_why_context", true);

    // interaction block
    let ask_one_question           = get_bool(&merged, "interaction", "ask_one_question", true);
    let confirm_completed_steps    = get_bool(&merged, "interaction", "confirm_completed_steps", false);
    let acknowledge_hat_announcements = get_bool(&merged, "interaction", "acknowledge_hat_announcements", true);

    // per_hat block (kept as nested JSON object)
    let per_hat = merged.get("per_hat").cloned().unwrap_or(json!({}));

    // ── Step 5: Add positive findings for defined preferences ─────────────────
    if preferences_complete {
        findings.push(Finding {
            name: "communication_contract".into(),
            status: "complete".into(),
            points: 100,
            detail: Some("All preference categories explicitly defined — agent has a full communication contract.".into()),
        });
    } else if score > 0 {
        findings.push(Finding {
            name: "communication_contract".into(),
            status: "partial".into(),
            points: score,
            detail: Some(format!("{}/4 preference blocks defined.", score / 25)),
        });
    }

    // ── Step 6: Assemble extras ───────────────────────────────────────────────

    // Provenance
    extras.push(("has_user_defaults",     Value::Bool(has_user_defaults)));
    extras.push(("has_project_overrides", Value::Bool(has_project_overrides)));
    extras.push(("preferences_complete",  Value::Bool(preferences_complete)));

    // communication
    extras.push(("include_urls", Value::Bool(include_urls)));
    extras.push(("verbosity",    Value::String(verbosity)));
    extras.push(("lead_with",    Value::String(lead_with)));

    // format
    extras.push(("code_blocks",    Value::String(code_blocks)));
    extras.push(("lists_vs_prose", Value::String(lists_vs_prose)));
    extras.push(("emoji",          Value::String(emoji)));

    // signals
    extras.push(("proactive_hat_suggestions", Value::Bool(proactive_hat_suggestions)));
    extras.push(("alert_on_correlation_fire", Value::Bool(alert_on_correlation_fire)));
    extras.push(("include_why_context",       Value::Bool(include_why_context)));

    // interaction
    extras.push(("ask_one_question",            Value::Bool(ask_one_question)));
    extras.push(("confirm_completed_steps",     Value::Bool(confirm_completed_steps)));
    extras.push(("acknowledge_hat_announcements", Value::Bool(acknowledge_hat_announcements)));

    // per_hat (nested, not a domain variable — for agent consumption)
    extras.push(("per_hat", per_hat));

    build_cmdb("check-human-comms", score.clamp(0, 100) as u8, findings, Some(extras))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolve user home directory: $HOME (Unix) or $USERPROFILE (Windows).
fn user_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

/// Read a YAML file and parse it to `serde_json::Value`.
/// Returns `(json!({}), false)` if the file is missing or unparseable.
async fn load_yaml(path: Option<&Path>) -> (Value, bool) {
    let p = match path {
        Some(p) => p,
        None => return (json!({}), false),
    };
    let text = match tokio::fs::read_to_string(p).await {
        Ok(t) => t,
        Err(_) => return (json!({}), false),
    };
    match serde_yaml::from_str::<serde_json::Value>(&text) {
        Ok(v) => (v, true),
        Err(_) => (json!({}), false),
    }
}

/// Deep merge two JSON objects (base ← override).
///
/// For each top-level key: override wins.
/// For `per_hat`: merged one level deeper (per hat name, override wins per hat).
fn deep_merge(base: Value, over: Value) -> Value {
    let base_obj = match base.as_object() {
        Some(o) => o.clone(),
        None => return over,
    };
    let over_obj = match over.as_object() {
        Some(o) => o.clone(),
        None => return Value::Object(base_obj),
    };

    let mut result = base_obj;

    for (key, over_val) in over_obj {
        if key == "per_hat" {
            // One level deeper: merge per hat name
            let base_per_hat = result.get("per_hat").cloned().unwrap_or(json!({}));
            let base_hats = base_per_hat.as_object().cloned().unwrap_or_default();
            let over_hats = over_val.as_object().cloned().unwrap_or_default();
            let mut merged_hats = base_hats;
            for (hat, hat_over) in over_hats {
                let hat_base = merged_hats.get(&hat).cloned().unwrap_or(json!({}));
                // Merge individual hat fields: override wins
                let merged_hat = merge_objects(hat_base, hat_over);
                merged_hats.insert(hat, merged_hat);
            }
            result.insert("per_hat".to_string(), Value::Object(merged_hats));
        } else {
            // Top-level block: override wins entirely
            result.insert(key, over_val);
        }
    }

    Value::Object(result)
}

/// Merge two JSON objects field-by-field: override wins per field.
fn merge_objects(base: Value, over: Value) -> Value {
    let mut result = base.as_object().cloned().unwrap_or_default();
    if let Some(over_obj) = over.as_object() {
        for (k, v) in over_obj {
            result.insert(k.clone(), v.clone());
        }
    }
    Value::Object(result)
}

/// Returns true if the value is a non-empty JSON object.
fn block_has_keys(v: Option<&Value>) -> bool {
    v.and_then(|v| v.as_object()).map(|o| !o.is_empty()).unwrap_or(false)
}

/// Extract a bool from `merged[block][field]`, with a default.
fn get_bool(merged: &Value, block: &str, field: &str, default: bool) -> bool {
    merged
        .get(block)
        .and_then(|b| b.get(field))
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

/// Extract a string from `merged[block][field]`, with a default.
fn get_str(merged: &Value, block: &str, field: &str, default: &str) -> String {
    merged
        .get(block)
        .and_then(|b| b.get(field))
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}
