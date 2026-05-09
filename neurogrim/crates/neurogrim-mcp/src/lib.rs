//! MCP client + server integration for NeuroGrim.
//!
//! The Brain acts as both:
//! - MCP Server: exposes scoring tools to AI agents
//! - MCP Client: discovers and invokes sensory tool servers

pub mod autonomy;
pub mod client;
pub mod context;
pub mod doctor;
pub mod domain;
pub mod explain;
// Feature 1, Phase 1.5 (2026-05-09) — LLM-subagent dispatch. Wraps
// the `neurogrim-core::llm_backend` trait with mcp-specific factory
// registrations (currently `copilot-proxied` HTTP backend).
pub mod llm_backends;
pub mod prose;
pub mod proxy_tokens;
pub mod publish_gates;
// V5-MOD-1 Phase 3 (2026-05-02): factory-registry global for the
// scoring-source dispatch sites (context.rs, server.rs, doctor.rs).
pub mod scoring_source_registry;
pub mod server;
pub mod transport;

pub use client::{invoke_sensory_servers, SensoryClient, SensoryResult};
pub use server::BrainServer;
