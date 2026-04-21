//! Built-in sensory tool implementations.
//!
//! Each tool is an MCP server that produces CMDB-envelope JSON.
//! All tools implement the same contract: accept project_root, return CMDB.

pub mod cmdb;
pub mod code_quality;
pub mod coherence;
pub mod deploy_readiness;
pub mod docker_topology;
pub mod git_health;
pub mod human_comms;
pub mod secret_refs;
pub mod security_standards;
pub mod test_results;
