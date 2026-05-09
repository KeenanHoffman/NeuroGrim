//! `neurogrim invoke` — dispatch a prompt to a registered LLM backend
//! and append the outcome to `<project>/.claude/brain/subagent-outcomes.jsonl`
//! (Feature 1, Phase 1.4 — 2026-05-09).
//!
//! Designed for shelling-out from a Claude Code subagent flow:
//! the operator (or an agent) runs
//! `neurogrim invoke --backend copilot-proxied --model gpt-4o --role rubber-duck --prompt-file plan.md`,
//! captures the JSON envelope on stdout, parses out the `text` field,
//! continues the workflow. The subagent-outcomes ledger keeps the
//! lifecycle observable for the existing `subagent-health` domain.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;
use uuid::Uuid;

use neurogrim_core::llm_backend::LlmInvokeOptions;

use crate::llm_backends;
use crate::roles::RolesRegistry;

#[derive(Debug, Args)]
pub struct InvokeArgs {
    /// Backend wire-name. When neither --backend nor --role resolves a
    /// backend, falls back to the registry's `fallback_backend`
    /// (default `copilot-proxied`).
    #[arg(long)]
    pub backend: Option<String>,

    /// Model identifier (backend-specific). E.g. `gpt-4o`,
    /// `claude-opus-4`, `claude-3-5-sonnet`. When --role resolves a
    /// model, this becomes optional; explicit --model overrides the
    /// role's default.
    #[arg(long)]
    pub model: Option<String>,

    /// Hat / role label. When set, looks up the role in
    /// `.claude/agent-roles.yaml` (or the bundled defaults) and uses
    /// its backend / model / system_prompt as defaults. Explicit
    /// --backend / --model / --system flags still override.
    /// Always recorded in the outcome ledger so `subagent-health` can
    /// attribute outcomes per hat.
    #[arg(long)]
    pub role: Option<String>,

    /// Read the prompt from stdin (default behavior when neither
    /// --from-stdin nor --prompt-file is set).
    #[arg(long, conflicts_with = "prompt_file")]
    pub from_stdin: bool,

    /// Read the prompt from a file. Mutually exclusive with --from-stdin.
    #[arg(long, conflicts_with = "from_stdin")]
    pub prompt_file: Option<PathBuf>,

    /// Inline prompt. Smallest path for trivial calls. If set, takes
    /// precedence over both --from-stdin and --prompt-file.
    #[arg(long)]
    pub prompt: Option<String>,

    /// Optional system prompt (prepended as a system message for
    /// chat-shaped backends).
    #[arg(long)]
    pub system: Option<String>,

    /// Hard cap on output tokens.
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Per-call timeout in seconds (whole-request, not per-token).
    #[arg(long, default_value_t = 120)]
    pub timeout_seconds: u64,

    /// Project root for the outcome ledger. Defaults to the current
    /// working directory.
    #[arg(long)]
    pub project_root: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct InvokeEnvelope {
    outcome_id: String,
    text: String,
    tokens_in: u32,
    tokens_out: u32,
    backend: String,
    model: String,
    duration_ms: u64,
}

pub async fn run(args: InvokeArgs) -> Result<()> {
    let prompt = read_prompt(&args)?;

    // Role resolver: when --role is set, pull defaults from the bundled
    // role registry overlaid with <project>/.claude/agent-roles.yaml.
    // CLI flags override the resolved role.
    let cwd = args
        .project_root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let roles = RolesRegistry::load(&cwd)
        .with_context(|| "loading agent-roles registry")?;
    let resolved_role = args.role.as_deref().and_then(|name| {
        let role = roles.resolve(name);
        if role.is_none() {
            tracing::warn!(
                "role {name:?} not found in registry; available: {:?}",
                roles.names()
            );
        }
        role
    });

    // Compose (backend, model, system_prompt) — explicit flag > role
    // default > registry fallback.
    let backend_name: String = args
        .backend
        .clone()
        .or_else(|| resolved_role.map(|r| r.backend.clone()))
        .unwrap_or_else(|| roles.fallback_backend.clone());
    let model: String = args
        .model
        .clone()
        .or_else(|| resolved_role.map(|r| r.model.clone()))
        .unwrap_or_else(|| roles.fallback_model.clone());
    let system_prompt: Option<String> = args
        .system
        .clone()
        .or_else(|| resolved_role.and_then(|r| r.system_prompt.clone()));

    let backend = llm_backends::build_default(&backend_name)?;

    let options = LlmInvokeOptions {
        max_tokens: args.max_tokens,
        system_prompt,
        temperature: None,
        timeout: Some(Duration::from_secs(args.timeout_seconds)),
    };

    let response = backend
        .invoke(&prompt, &model, &options)
        .await
        .with_context(|| format!("invoking backend {backend_name}"))?;

    let outcome_id = format!("ulid_{}", Uuid::new_v4().simple());
    let envelope = InvokeEnvelope {
        outcome_id: outcome_id.clone(),
        text: response.text.clone(),
        tokens_in: response.tokens_in,
        tokens_out: response.tokens_out,
        backend: response.backend.clone(),
        model: response.model.clone(),
        duration_ms: response.duration_ms,
    };

    // Append a row to the subagent-outcomes ledger. Same JSONL schema
    // neurogrim-mcp::record_subagent_outcome writes (see S13-B-5);
    // the refactor that extracts a shared writer to neurogrim-core
    // is Phase 1.4b, deferred. For now both call sites maintain the
    // same shape directly.
    let project_root = args
        .project_root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    if let Err(e) = append_outcome(&project_root, &outcome_id, &args, &response) {
        // Non-fatal — observability shouldn't block the call envelope.
        tracing::warn!(error = %e, "subagent-outcomes append failed");
    }

    println!("{}", serde_json::to_string(&envelope)?);
    Ok(())
}

fn read_prompt(args: &InvokeArgs) -> Result<String> {
    if let Some(p) = args.prompt.as_deref() {
        return Ok(p.to_string());
    }
    if let Some(path) = args.prompt_file.as_ref() {
        return std::fs::read_to_string(path)
            .with_context(|| format!("reading prompt from {}", path.display()));
    }
    // Default: stdin. `--from-stdin` is honored explicitly + is also
    // the behavior when neither --prompt nor --prompt-file is set.
    let mut buf = String::new();
    use std::io::Read;
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("reading prompt from stdin")?;
    Ok(buf)
}

fn append_outcome(
    project_root: &std::path::Path,
    outcome_id: &str,
    args: &InvokeArgs,
    response: &neurogrim_core::llm_backend::LlmResponse,
) -> Result<()> {
    let dir = project_root.join(".claude").join("brain");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join("subagent-outcomes.jsonl");
    let row = serde_json::json!({
        "ts":            chrono::Utc::now().to_rfc3339(),
        "outcome_id":    outcome_id,
        "capability":    "llm-invoke",
        "worn_hat":      args.role.clone().unwrap_or_default(),
        "status":        "success",
        "backend":       response.backend,
        "model":         response.model,
        "tokens_in":     response.tokens_in,
        "tokens_out":    response.tokens_out,
        "duration_ms":   response.duration_ms,
    });
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&row)?)?;
    Ok(())
}
