//! In-tree implementations of [`neurogrim_core::test_runner::TestRunner`]
//! (V5-FOUND-4 Phase 2, 2026-05-04).
//!
//! Lives in `neurogrim-cli` (not `neurogrim-core`) to avoid a cyclic
//! dep — Forks B1/C1 leave `build_cargo_args` and
//! `parse_nextest_output` in `commands::test`, where their existing
//! 25+ unit tests already live. NextestRunner imports them via
//! same-crate `super::test::*`. The trait surface itself stays in
//! `neurogrim-core::test_runner` (where the SDK re-export reaches).
//!
//! AgentDrivenRunner is intentionally NOT in this module —
//! v5.5 BACKLOG B-51 covers that work once a Rust LLM client lands.

pub mod nextest;
