//! `neurogrim a2a-token` — manage scope tokens for A2A bearer auth.
//!
//! Mirrors the shape of `claude-proxy`'s `proxy-cli` so operators learn one
//! pattern. The CLI talks directly to the sqlite store at the path given by
//! `--store`; there is no admin HTTP API. On a single-host deployment this
//! is the intended posture — filesystem access is sufficient privilege.
//!
//! Subcommands:
//!
//! - `issue --label <L> [--profile <P>] [--expires <DUR>]` — print the raw
//!   token on stdout; issue metadata on stderr. The raw token is shown ONCE
//!   and never again.
//! - `list` — human-readable table of every token (active / revoked / expired).
//! - `revoke <token_id>` — mark a token revoked. `token_id` is the 16-char
//!   hex prefix shown in `list`.
//!
//! Duration syntax for `--expires` accepts `<n>[smhd]` (seconds / minutes /
//! hours / days). A bare integer is seconds. Empty / omitted = no expiry.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Subcommand;
use neurogrim_a2a::token_store::TokenStore;

#[derive(Subcommand, Debug)]
pub enum A2aTokenCmd {
    /// Issue a new scope token.
    Issue {
        /// Human-readable tag (e.g. `ceo-alice`). Appears in audit logs.
        #[arg(long)]
        label: String,
        /// Rate-limit / policy profile name. Informational in v1.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Expiration. Accepts `30d`, `12h`, `90m`, `3600s`, or a bare
        /// integer (seconds). Omit for no expiry.
        #[arg(long)]
        expires: Option<String>,
    },
    /// List all tokens with status.
    List,
    /// Revoke a token by its `token_id` (the 16-char hex shown in `list`).
    Revoke {
        /// Token ID (16-char hex prefix).
        token_id: String,
    },
}

pub async fn run(store_path: String, cmd: A2aTokenCmd) -> Result<()> {
    let store = TokenStore::open(&store_path)
        .with_context(|| format!("failed to open token store at {store_path}"))?;

    match cmd {
        A2aTokenCmd::Issue {
            label,
            profile,
            expires,
        } => cmd_issue(&store, &label, &profile, expires.as_deref()),
        A2aTokenCmd::List => cmd_list(&store),
        A2aTokenCmd::Revoke { token_id } => cmd_revoke(&store, &token_id),
    }
}

fn cmd_issue(
    store: &TokenStore,
    label: &str,
    profile: &str,
    expires: Option<&str>,
) -> Result<()> {
    let expires_in = match expires {
        Some(s) => Some(parse_expires(s)?),
        None => None,
    };
    let (raw, record) = store
        .issue(label, profile, expires_in)
        .context("failed to issue token")?;

    // The raw token goes to stdout — single line, easy to pipe / capture.
    println!("{raw}");
    // Metadata + the "shown-once" warning goes to stderr so it doesn't
    // pollute the captured token.
    eprintln!("  label:    {}", record.label);
    eprintln!("  token_id: {}", record.token_id);
    eprintln!("  profile:  {}", record.profile);
    eprintln!("  expires:  {}", fmt_ts(record.expires_at));
    eprintln!();
    eprintln!("Copy the token above NOW — it will not be shown again.");
    Ok(())
}

fn cmd_list(store: &TokenStore) -> Result<()> {
    let records = store.list_all().context("failed to list tokens")?;
    if records.is_empty() {
        println!("no tokens issued");
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp();
    println!(
        "{:<18} {:<24} {:<10} {:<18} {:<18} {:<18} STATUS",
        "TOKEN_ID", "LABEL", "PROFILE", "CREATED", "EXPIRES", "LAST_USED"
    );
    for r in records {
        let status = if r.revoked_at.is_some() {
            "revoked"
        } else if r.expires_at.map(|e| now >= e).unwrap_or(false) {
            "expired"
        } else {
            "active"
        };
        let label = truncate(&r.label, 24);
        println!(
            "{:<18} {:<24} {:<10} {:<18} {:<18} {:<18} {}",
            r.token_id,
            label,
            r.profile,
            fmt_ts(Some(r.created_at)),
            fmt_ts(r.expires_at),
            fmt_ts(r.last_used_at),
            status
        );
    }
    Ok(())
}

fn cmd_revoke(store: &TokenStore, token_id: &str) -> Result<()> {
    let revoked = store
        .revoke(token_id)
        .with_context(|| format!("failed to revoke {token_id}"))?;
    if revoked {
        println!("revoked: {token_id}");
        Ok(())
    } else {
        // Non-zero exit so scripts can distinguish no-op from success.
        anyhow::bail!("not found or already revoked: {token_id}")
    }
}

fn fmt_ts(ts: Option<i64>) -> String {
    match ts {
        None => "—".to_string(),
        Some(secs) => match DateTime::<Utc>::from_timestamp(secs, 0) {
            Some(dt) => dt.format("%Y-%m-%d %H:%MZ").to_string(),
            None => "?".to_string(),
        },
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect()
    }
}

/// Parse a duration string: `30d`, `12h`, `90m`, `45s`, or a bare integer
/// (treated as seconds). Returns the equivalent in seconds.
fn parse_expires(s: &str) -> Result<i64> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("empty --expires value");
    }
    let last = s.chars().last().unwrap();
    let (n_str, unit) = match last {
        's' | 'm' | 'h' | 'd' => (&s[..s.len() - 1], Some(last)),
        _ => (s, None),
    };
    let n: i64 = n_str
        .parse()
        .with_context(|| format!("invalid duration {s:?} — expected e.g. 30d, 12h, 90m, 3600s"))?;
    let mul = match unit {
        None | Some('s') => 1,
        Some('m') => 60,
        Some('h') => 3600,
        Some('d') => 86400,
        _ => unreachable!(),
    };
    Ok(n * mul)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_expires_accepts_units() {
        assert_eq!(parse_expires("30s").unwrap(), 30);
        assert_eq!(parse_expires("2m").unwrap(), 120);
        assert_eq!(parse_expires("3h").unwrap(), 10800);
        assert_eq!(parse_expires("7d").unwrap(), 604800);
    }

    #[test]
    fn parse_expires_bare_integer_is_seconds() {
        assert_eq!(parse_expires("45").unwrap(), 45);
    }

    #[test]
    fn parse_expires_rejects_garbage() {
        assert!(parse_expires("garbage").is_err());
        assert!(parse_expires("").is_err());
        assert!(parse_expires("3w").is_err());
    }

    #[test]
    fn truncate_handles_short_and_long() {
        assert_eq!(truncate("hi", 10), "hi");
        assert_eq!(truncate("abcdefghij", 5), "abcde");
    }

    #[test]
    fn fmt_ts_handles_none() {
        assert_eq!(fmt_ts(None), "—");
    }

    #[test]
    fn cmd_issue_persists_record() {
        let store = TokenStore::open_in_memory().unwrap();
        let (_raw, rec) = store.issue("test", "default", None).unwrap();
        let list = store.list_all().unwrap();
        assert!(list.iter().any(|r| r.token_id == rec.token_id));
    }
}
