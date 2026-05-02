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
//! Idempotent: `try_init()` returns `Err` rather than panicking on
//! double-init, so a second call within the same process is a
//! silent no-op. Subcommands invoked from `main()` therefore do
//! not need to coordinate with each other.

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Options for centralized tracing setup.
///
/// Phase 0 only carries `enable_diag` as a placeholder — the field
/// is read but no diagnostics layer is attached yet. V5-FOUND-1
/// Phase 2 will attach the diagnostics `Layer` when this flag is
/// `true` (set from `NEUROGRIM_DIAG=1` env var or a top-level
/// `--diag` CLI flag, both wired in `main()`).
#[derive(Debug, Default, Clone)]
pub struct TracingOpts {
    /// V5-FOUND-1 Phase 2 hook: when `true`, the diagnostics
    /// `Layer` will be attached to capture spans into
    /// `.claude/brain/diagnostics.jsonl`. Phase 0 ignores this
    /// field; Phase 2 will read it.
    pub enable_diag: bool,
}

/// Initialize the global tracing subscriber.
///
/// Idempotent — second and subsequent calls within the same
/// process are silent no-ops (`try_init` returns `Err` rather than
/// panicking).
pub fn setup_tracing(opts: TracingOpts) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer();
    // V5-FOUND-1 Phase 2 will read this and conditionally attach
    // the diagnostics Layer here. For now the field is unused.
    let _ = opts.enable_diag;
    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
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
