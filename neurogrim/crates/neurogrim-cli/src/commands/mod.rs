pub mod a2a_discover;
pub mod a2a_invoke;
pub mod a2a_serve;
pub mod a2a_token;
pub mod agent;
pub mod awareness;
pub mod diag;
pub mod disposition;
pub mod doctor;
pub mod domain;
pub mod domain_calibration;
pub mod explain;
pub mod federated_pattern;
pub mod federation;
pub mod health;
pub mod init;
pub mod init_scaffold;
pub mod narrate;
pub mod publish_gate;
pub mod queue;
// V5-MOD-2 Phase 4 (2026-05-02) — these CLI commands depend on
// sensor-specific helper code in `neurogrim-sensory` and only
// make sense when the corresponding sensor feature is enabled.
//   - `sca-calibrate` uses `neurogrim_sensory::supply_chain_calibration`
//   - `sca-review` uses `neurogrim_sensory::supply_chain_review`
// Without the matching sensor feature enabled, these commands
// disappear from the CLI surface entirely (clap won't see them
// because the enum variant + dispatch arm are gated).
#[cfg(feature = "sensor-supply-chain-sca")]
pub mod sca_calibrate;
#[cfg(feature = "sensor-supply-chain-review")]
pub mod sca_review;
pub mod score;
pub mod secrets;
pub mod skill;
pub mod serve;
pub mod test;
pub mod ui;
pub mod trend;
pub mod validate;
