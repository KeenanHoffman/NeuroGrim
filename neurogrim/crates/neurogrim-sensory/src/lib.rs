//! Built-in sensory tool implementations.
//!
//! Each tool is an MCP server that produces CMDB-envelope JSON.
//! All tools implement the same contract: accept project_root, return CMDB.

pub mod agent_behavior;
pub mod capability_hygiene;
pub mod cmdb;
pub mod code_quality;
pub mod coherence;
pub mod deploy_readiness;
pub mod docker_topology;
pub mod domain_calibration;
pub mod git_health;
pub mod human_comms;
pub mod operator_calibration;
pub mod secret_refs;
pub mod security_standards;
pub mod skill_coherence;
pub mod supply_chain_calibration;
pub mod supply_chain_review;
pub mod supply_chain_sca;
pub mod supply_chain_vigilance;
pub mod test_results;
pub mod trust_budget;
