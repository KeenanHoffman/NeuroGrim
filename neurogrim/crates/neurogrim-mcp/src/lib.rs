//! MCP client + server integration for NeuroGrim.
//!
//! The Brain acts as both:
//! - MCP Server: exposes scoring tools to AI agents
//! - MCP Client: discovers and invokes sensory tool servers

pub mod client;
pub mod server;
pub mod transport;

pub use client::{invoke_sensory_servers, SensoryClient, SensoryResult};
pub use server::BrainServer;
