//! A2A (Agent2Agent) peer protocol for MotherBrain.
//!
//! This crate implements the Brain-to-Brain peer communication protocol as specified in
//! LSP Brains spec v2.1 (`spec/LSP-BRAINS-SPEC.md` §13, Appendix G). It is the normative
//! transport for:
//!
//! - Fractal composition (spec §9): parent Brain invoking child Brain as a peer.
//! - Dual brain (spec §10): local Brain ↔ external Brain coordination.
//!
//! **Scope boundary:** this crate handles *Brain-to-Brain* traffic only. Sensory tool
//! invocation and Brain-as-tool exposure to LLM agents live in `motherbrain-mcp`. See
//! `METHODOLOGY-EVOLUTION.md` §6 for the protocol-split rationale.
//!
//! **Invariant (CI-enforced):** this crate MUST NOT import from `rmcp` or
//! `motherbrain-mcp`. The two protocols are orthogonal.
//!
//! # Module Map
//!
//! | Module | Responsibility | Corresponds to spec |
//! |--------|---------------|---------------------|
//! | `envelope` | A2A message envelope types | `a2a-envelope-v1.schema.json`, §13.3, G.5 |
//! | `agent_card` | Agent Card types + publication | `agent-card-v1.schema.json`, §13.2, G.2 |
//! | `client` | TaskClient trait + HTTP+SSE impl | §13.3, G.3 |
//! | `server` | TaskServer trait + axum impl | §13.3, G.4 |
//! | `transport` | Pluggable transport layer (HTTP+SSE, JSON-RPC) | §13.5, G.6 |
//! | `error` | A2A error types | G.8 |
//!
//! # Status
//!
//! Stage 6 (S6-DB-1) — crate scaffold. The envelope + agent_card types are stabilized
//! against the v2.1 schemas; client, server, and transport modules are under construction.
//! See `roadmap/epics/S6-dual-brain-a2a.md`.

pub mod agent_card;
pub mod client;
pub mod envelope;
pub mod error;
pub mod server;
pub mod transport;

pub use agent_card::AgentCard;
pub use client::TaskClient;
pub use envelope::{A2aEnvelope, MessageType};
pub use error::A2aError;
pub use server::TaskServer;
pub use transport::{HttpSseTransport, JsonRpcTransport, TaskAccepted, Transport};
