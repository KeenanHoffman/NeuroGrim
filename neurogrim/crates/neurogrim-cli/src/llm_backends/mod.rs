//! Built-in LLM-backend factories registered by `neurogrim invoke`
//! (Feature 1, Phase 1.4 — 2026-05-09).
//!
//! Lives here, not in `neurogrim-core`, because each impl pulls
//! transport-shaped deps (reqwest for HTTP backends; `tokio::process`
//! for the codex-cli subprocess) that would break the core crate's
//! "pure logic, zero I/O" posture. neurogrim-core ships only the
//! trait + factory + registry.
//!
//! ## Built-ins shipped in Phase 1.4
//!
//! - `copilot-proxied` — talks to `neurogrim-copilot-proxy`
//!   (port 4546 by default) via OpenAI-compatible chat completions.
//!   Auth via `X-Scope-Token` from `COPILOT_PROXY_SCOPE_TOKEN` env
//!   var (issued by `proxy-cli issue --label …`).
//!
//! ## Deferred to Phase 1.5
//!
//! - `anthropic` — direct to api.anthropic.com
//! - `anthropic-proxied` — through claude-proxy on port 4545
//! - `codex-cli` — `tokio::process::Command::new("codex")` subprocess
//!
//! These three move when `neurogrim-mcp`'s `invoke_subagent` tool
//! lands and its `BrainServer` constructor wants the same registry
//! shape.

pub mod copilot_proxied;

use std::sync::Arc;

use neurogrim_core::llm_backend::{LlmBackend, LlmBackendFactory, LlmBackendRegistry};

/// Factories shipped with this build of `neurogrim`. Operators
/// register additional factories programmatically via
/// `registry.register(...)` if they ship a custom workspace binary.
pub fn built_in_factories() -> Vec<Box<dyn LlmBackendFactory>> {
    vec![Box::new(copilot_proxied::CopilotProxiedFactory::default())]
}

/// Convenience: build a registry pre-populated with this build's
/// built-ins. Equivalent to `LlmBackendRegistry::default()` followed
/// by `register_all(built_in_factories())`.
pub fn build_registry() -> LlmBackendRegistry {
    let mut r = LlmBackendRegistry::new();
    r.register_all(built_in_factories());
    r
}

/// Resolve a backend by wire-name from the built-in set + a default
/// (empty) config. The `neurogrim invoke` subcommand uses this when
/// the operator passes `--backend <name>` without other knobs.
pub fn build_default(name: &str) -> anyhow::Result<Arc<dyn LlmBackend>> {
    let registry = build_registry();
    let cfg = neurogrim_core::llm_backend::LlmBackendConfig {
        name: name.to_string(),
        options: Default::default(),
    };
    registry
        .build(&cfg)
        .ok_or_else(|| {
            let names: Vec<String> = registry
                .registered_names()
                .map(|s| s.to_string())
                .collect();
            anyhow::anyhow!(
                "no backend registered for {name:?}; registered: {names:?}"
            )
        })?
}
