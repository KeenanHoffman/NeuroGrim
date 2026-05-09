//! Built-in LLM-backend factories registered by the `invoke_subagent`
//! MCP tool (Feature 1, Phase 1.5 — 2026-05-09).
//!
//! Currently a near-clone of `neurogrim-cli/src/llm_backends/`. The
//! cleanup-discipline-aligned refactor — extract this module to
//! `neurogrim-core` behind an `llm-copilot-proxied` feature so both
//! cli and mcp consume one copy — is tracked in
//! `D:/Brains/copilot-proxy/DEFERRED-WORK.md` as Phase 1.5b.

pub mod copilot_proxied;

use neurogrim_core::llm_backend::{LlmBackendFactory, LlmBackendRegistry};

/// Factories the MCP `invoke_subagent` tool ships with.
pub fn built_in_factories() -> Vec<Box<dyn LlmBackendFactory>> {
    vec![Box::new(copilot_proxied::CopilotProxiedFactory::default())]
}

/// Build a registry pre-populated with the mcp built-ins.
pub fn build_registry() -> LlmBackendRegistry {
    let mut r = LlmBackendRegistry::new();
    r.register_all(built_in_factories());
    r
}
