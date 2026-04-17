//! Built-in sensory tool implementations.
//!
//! Each tool is an MCP server that produces CMDB-envelope JSON.
//! All tools implement the same contract: accept project_root, return CMDB.

pub mod git_health;
pub mod test_results;
pub mod code_quality;
pub mod deploy_readiness;
pub mod security_standards;
pub mod coherence;
pub mod human_comms;
pub mod secret_refs;
pub mod cmdb;
