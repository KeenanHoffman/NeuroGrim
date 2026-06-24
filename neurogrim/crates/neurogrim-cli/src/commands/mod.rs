// Wave 5.5 (S*-T MVP broker harness; 2026-06-24) — broker-serve subcommand
// loads cluster manifest + starts MCP server with single dispatch_pipeline
// tool. See C:/Users/koff0/.claude/plans/for-your-new-session-modular-pretzel.md
// for the full design.
pub mod broker_serve;
pub mod broker_init;
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
// Feature 1, Phase 1.4 (2026-05-09) — `neurogrim invoke` dispatches
// a prompt to a registered LLM backend (initially `copilot-proxied`,
// talking to D:/Brains/copilot-proxy on port 4546).
pub mod invoke;
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
// V5-FOUND-4 Phase 2 (2026-05-04) — NextestRunner impl of
// neurogrim_core::test_runner::TestRunner. Lives here (not in
// neurogrim-core) to avoid a cyclic dep with commands::test's
// build_cargo_args + parse_nextest_output (Forks B1/C1).
pub mod test_runner_impls;
pub mod ui;
pub mod trend;
pub mod validate;
