//! Centralized `tracing-subscriber` initialization for the
//! neurogrim CLI.
//!
//! V5-FOUND-1 Phase 0 (plan-critic finding): the project previously
//! initialized tracing per-command via
//! `tracing_subscriber::fmt().try_init()` calls inside individual
//! subcommands (`a2a_discover.rs`, `a2a_invoke.rs`, `a2a_serve.rs`).
//! That pattern is incompatible with attaching a custom `Layer`
//! (e.g., the V5-FOUND-1 Phase 2 diagnostics Layer) because
//! `try_init` installs a global `Subscriber` that cannot accept
//! additional layers afterwards. This module is the single
//! initialization site; subcommands MUST NOT call `try_init`
//! themselves.
//!
//! Behavior matches the prior per-command pattern when
//! `enable_diag` is `false`: `EnvFilter` from `RUST_LOG` /
//! default-env, falling back to `"info"` if unset; `fmt` layer to
//! stderr (clap default).
//!
//! When `enable_diag` is `true` (Phase 2), the diagnostics
//! [`crate::diagnostics_layer::DiagnosticsLayer`] is attached to
//! the registry chain. Spans whose name is in the closed table at
//! [`crate::diagnostics_layer::kind_for_span_name`] emit one
//! ledger entry per span on close.
//!
//! Idempotent: `try_init()` returns `Err` rather than panicking on
//! double-init, so a second call within the same process is a
//! silent no-op. Subcommands invoked from `main()` therefore do
//! not need to coordinate with each other.

use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Options for centralized tracing setup.
///
/// `enable_diag` is read from `NEUROGRIM_DIAG=1` env var (or a
/// top-level `--diag` CLI flag in a follow-on iteration) and
/// gates whether the diagnostics Layer is attached.
#[derive(Debug, Default, Clone)]
pub struct TracingOpts {
    /// When `true`, attach the diagnostics `Layer` (V5-FOUND-1
    /// Phase 2) so mapped spans emit ledger entries to
    /// `<project_root>/.claude/brain/diagnostics.jsonl`.
    pub enable_diag: bool,
}

/// Initialize the global tracing subscriber.
///
/// Idempotent — second and subsequent calls within the same
/// process are silent no-ops (`try_init` returns `Err` rather than
/// panicking).
///
/// When `opts.enable_diag` is `false`, the subscriber chain is
/// `Registry → EnvFilter → fmt` (no diag overhead). When `true`,
/// the diagnostics Layer is appended; the ledger path is composed
/// from the current working directory (`<cwd>/.claude/brain/
/// diagnostics.jsonl`), matching the existing CWD-as-project-root
/// convention used by other CLI subcommands (e.g., `disposition
/// record --project-root .`).
pub fn setup_tracing(opts: TracingOpts) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    // CLI logs go to stderr (matches the module-doc-comment contract
    // and the clap convention). stdout is reserved for command output
    // and — critically — for MCP-on-stdio servers (`broker-serve`,
    // `serve`) whose JSON-RPC framing is corrupted by stray stdout
    // bytes from a tracing fmt layer.
    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    // Conditional diag layer: `Option<L>` implements `Layer<S>`
    // when `L: Layer<S>` (tracing-subscriber std impl), so this
    // is the canonical pattern for opt-in layers.
    let diag_layer = if opts.enable_diag {
        Some(crate::diagnostics_layer::DiagnosticsLayer::new(
            PathBuf::from("."),
        ))
    } else {
        None
    };

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(diag_layer)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calling `setup_tracing` twice must not panic. The second
    /// call is a silent no-op because `try_init` returns `Err`
    /// when a subscriber is already installed.
    ///
    /// Note: this test installs a global subscriber that persists
    /// for the rest of the test binary's lifetime, so other tests
    /// in this module that depend on a clean tracing state would
    /// need to live in their own test binary. We don't have any
    /// such tests today; if a future test needs to capture span
    /// output, refactor with `#[cfg(test)]` indirection.
    #[test]
    fn setup_tracing_is_idempotent() {
        setup_tracing(TracingOpts::default());
        setup_tracing(TracingOpts::default());
        setup_tracing(TracingOpts {
            enable_diag: true,
        });
    }
}
