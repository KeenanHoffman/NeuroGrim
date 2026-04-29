//! `neurogrim ui` — launch the v3.4 dashboard server.
//!
//! Spawns an HTTP server (axum) on `127.0.0.1:<port>` (default 8420)
//! that serves the embedded React frontend + JSON API. When
//! `--no-browser` is omitted the dashboard tries to open the URL in
//! the user's default browser; the URL is always printed to stderr
//! as a fallback (works on WSL, headless Linux, BROWSER=foo
//! overrides).
//!
//! Phase 2.3 hardens this path:
//!
//! - **Headless detection**: when no display is available (CI, SSH,
//!   container), we skip the open() call cleanly and explain why
//!   instead of letting `webbrowser` hang or open something useless.
//! - **WSL handling**: webbrowser's `xdg-open` is unreliable on WSL;
//!   we route through `cmd.exe /c start` so the URL opens in the
//!   host Windows browser when WSL is detected.
//! - **Testable helpers**: the decision and detection logic is in
//!   pure functions that take an env shim so unit tests can drive
//!   them deterministically.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::SocketAddr;

pub async fn run(
    registry_path: String,
    port: u16,
    bind: String,
    no_browser: bool,
) -> Result<()> {
    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .with_context(|| format!("invalid bind/port: {bind}:{port}"))?;
    let url = format!("http://{bind}:{port}/");

    eprintln!("✦ NeuroGrim Dashboard");
    eprintln!("  Registry:  {}", registry_path);
    eprintln!("  Listening: {}", url);
    eprintln!();

    let env = current_env_view();
    match decide_browser_launch(no_browser, &env) {
        BrowserDecision::Skip { reason } => {
            eprintln!("  {reason} — navigate manually to the URL above.");
        }
        BrowserDecision::Try => match try_open_browser(&url, &env) {
            Ok(()) => eprintln!("  Opened in your default browser."),
            Err(e) => eprintln!(
                "  Could not open browser ({e}); navigate manually to the \
                 URL above."
            ),
        },
    }
    eprintln!();
    eprintln!("Press Ctrl+C to stop the server.");

    neurogrim_dashboard::serve(addr, registry_path).await
}

/// Outcome of the browser-launch decision. Distinct variants so the
/// CLI's stderr message can explain *why* a launch was skipped — an
/// operator running with `--no-browser` shouldn't see the same
/// message as one whose CI environment has no display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserDecision {
    /// Don't attempt to launch. The string is shown in the CLI
    /// output verbatim (terminal-friendly phrasing).
    Skip { reason: String },
    /// Attempt the launch.
    Try,
}

/// Pure decision: should we attempt to open the browser?
///
/// Inputs:
/// - `no_browser`: the explicit `--no-browser` flag.
/// - `env`: a snapshot of the relevant env vars (see
///   [`current_env_view`]).
///
/// Decision order:
/// 1. `--no-browser` always wins (operator intent).
/// 2. `CI=true` — running under CI, no display.
/// 3. SSH session (`SSH_CONNECTION` set) without DISPLAY — user
///    is remote, browser launch would either fail or surface in
///    an unintended X session.
/// 4. Linux with no DISPLAY/WAYLAND_DISPLAY (and not WSL) — no
///    graphical session.
/// 5. Otherwise — try.
pub fn decide_browser_launch(no_browser: bool, env: &EnvView) -> BrowserDecision {
    if no_browser {
        return BrowserDecision::Skip {
            reason: "--no-browser".to_string(),
        };
    }
    if env.ci {
        return BrowserDecision::Skip {
            reason: "CI environment detected".to_string(),
        };
    }
    if env.is_linux_like && !env.is_wsl {
        // Linux without a graphical session — can't open anything
        // useful. Note: WSL is handled separately because we route
        // through cmd.exe to reach the host browser regardless of
        // the WSL distro's DISPLAY state.
        if env.display.is_none() && env.wayland_display.is_none() {
            let detail = if env.ssh {
                "remote SSH session without DISPLAY"
            } else {
                "no graphical session (DISPLAY/WAYLAND_DISPLAY unset)"
            };
            return BrowserDecision::Skip {
                reason: detail.to_string(),
            };
        }
    }
    BrowserDecision::Try
}

/// Open `url` in the user's default browser, with a WSL-aware
/// fallback. Returns the underlying error message on failure so the
/// caller can surface it to the operator.
pub fn try_open_browser(url: &str, env: &EnvView) -> Result<(), String> {
    if env.is_wsl {
        // WSL: xdg-open inside the distro typically can't reach a
        // browser. Route through cmd.exe so the URL opens in the
        // Windows host's default browser.
        match std::process::Command::new("cmd.exe")
            .args(["/c", "start", url])
            .spawn()
        {
            Ok(mut child) => {
                // Don't wait — `start` returns immediately on Windows.
                // Detach so the dashboard isn't blocked by the child.
                let _ = child.wait();
                return Ok(());
            }
            Err(e) => return Err(format!("cmd.exe /c start failed: {e}")),
        }
    }
    webbrowser::open(url).map(|_| ()).map_err(|e| e.to_string())
}

/// The subset of process environment that affects browser-launch
/// behavior. Captured into a struct so tests can construct one
/// without touching real env vars.
#[derive(Debug, Clone)]
pub struct EnvView {
    pub ci: bool,
    pub ssh: bool,
    pub display: Option<String>,
    pub wayland_display: Option<String>,
    pub is_linux_like: bool,
    pub is_wsl: bool,
}

/// Read the relevant env vars at runtime. Tests build an `EnvView`
/// directly — they don't go through this function.
pub fn current_env_view() -> EnvView {
    let env: HashMap<String, String> = std::env::vars().collect();
    EnvView {
        ci: env_truthy(&env, "CI") || env_truthy(&env, "GITHUB_ACTIONS"),
        ssh: env.contains_key("SSH_CONNECTION") || env.contains_key("SSH_CLIENT"),
        display: env.get("DISPLAY").filter(|v| !v.is_empty()).cloned(),
        wayland_display: env
            .get("WAYLAND_DISPLAY")
            .filter(|v| !v.is_empty())
            .cloned(),
        is_linux_like: cfg!(target_os = "linux"),
        is_wsl: detect_wsl(),
    }
}

fn env_truthy(env: &HashMap<String, String>, key: &str) -> bool {
    match env.get(key) {
        None => false,
        Some(v) => {
            let v = v.trim().to_ascii_lowercase();
            !v.is_empty() && v != "0" && v != "false"
        }
    }
}

/// Detect WSL by inspecting `/proc/version`. Avoids a hard
/// dependency on the `WSL_DISTRO_NAME` env var (which only the
/// modern WSL2 path sets reliably).
fn detect_wsl() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    match std::fs::read_to_string("/proc/version") {
        Ok(s) => {
            let lower = s.to_lowercase();
            lower.contains("microsoft") || lower.contains("wsl")
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(
        ci: bool,
        ssh: bool,
        display: Option<&str>,
        wayland_display: Option<&str>,
        is_linux_like: bool,
        is_wsl: bool,
    ) -> EnvView {
        EnvView {
            ci,
            ssh,
            display: display.map(|s| s.to_string()),
            wayland_display: wayland_display.map(|s| s.to_string()),
            is_linux_like,
            is_wsl,
        }
    }

    #[test]
    fn no_browser_flag_skips_with_explicit_reason() {
        let e = env(false, false, Some(":0"), None, true, false);
        let d = decide_browser_launch(true, &e);
        match d {
            BrowserDecision::Skip { reason } => assert!(reason.contains("--no-browser")),
            BrowserDecision::Try => panic!("expected Skip"),
        }
    }

    #[test]
    fn ci_environment_skips() {
        let e = env(true, false, Some(":0"), None, true, false);
        match decide_browser_launch(false, &e) {
            BrowserDecision::Skip { reason } => assert!(reason.to_lowercase().contains("ci")),
            BrowserDecision::Try => panic!("expected Skip in CI"),
        }
    }

    #[test]
    fn linux_without_display_skips() {
        let e = env(false, false, None, None, true, false);
        match decide_browser_launch(false, &e) {
            BrowserDecision::Skip { reason } => assert!(reason.contains("no graphical session")),
            BrowserDecision::Try => panic!("expected Skip"),
        }
    }

    #[test]
    fn ssh_without_display_skips_with_ssh_reason() {
        let e = env(false, true, None, None, true, false);
        match decide_browser_launch(false, &e) {
            BrowserDecision::Skip { reason } => {
                assert!(reason.to_lowercase().contains("ssh"));
            }
            BrowserDecision::Try => panic!("expected Skip"),
        }
    }

    #[test]
    fn linux_with_display_tries() {
        let e = env(false, false, Some(":0"), None, true, false);
        assert_eq!(decide_browser_launch(false, &e), BrowserDecision::Try);
    }

    #[test]
    fn linux_with_wayland_display_tries() {
        let e = env(false, false, None, Some("wayland-0"), true, false);
        assert_eq!(decide_browser_launch(false, &e), BrowserDecision::Try);
    }

    #[test]
    fn wsl_tries_even_without_display() {
        // WSL distros often have no DISPLAY but we route through
        // cmd.exe — so the decision is always to try.
        let e = env(false, false, None, None, true, true);
        assert_eq!(decide_browser_launch(false, &e), BrowserDecision::Try);
    }

    #[test]
    fn macos_or_windows_always_tries_when_flag_unset() {
        // is_linux_like=false short-circuits the no-display check.
        let e = env(false, false, None, None, false, false);
        assert_eq!(decide_browser_launch(false, &e), BrowserDecision::Try);
    }

    #[test]
    fn no_browser_flag_wins_over_ci_detection() {
        // The explicit flag should produce the explicit message,
        // not the CI message — operators reading the output
        // shouldn't be misled into thinking CI was the trigger.
        let e = env(true, true, None, None, true, false);
        match decide_browser_launch(true, &e) {
            BrowserDecision::Skip { reason } => assert!(reason.contains("--no-browser")),
            BrowserDecision::Try => panic!("expected Skip"),
        }
    }

    #[test]
    fn env_truthy_recognizes_common_truthy_values() {
        let mut env = HashMap::new();
        env.insert("CI".into(), "true".into());
        assert!(env_truthy(&env, "CI"));

        env.insert("CI".into(), "1".into());
        assert!(env_truthy(&env, "CI"));

        env.insert("CI".into(), "yes".into());
        assert!(env_truthy(&env, "CI"));

        env.insert("CI".into(), "False".into());
        assert!(!env_truthy(&env, "CI"));

        env.insert("CI".into(), "0".into());
        assert!(!env_truthy(&env, "CI"));

        env.insert("CI".into(), "".into());
        assert!(!env_truthy(&env, "CI"));

        env.remove("CI");
        assert!(!env_truthy(&env, "CI"));
    }
}
