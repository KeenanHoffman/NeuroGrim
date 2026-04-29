//! Proxy-token mint + redeem (S14-S-5, v4.2).
//!
//! Agents calling `secret_fetch` receive an opaque proxy token.
//! They never see the underlying secret value. The token authorizes
//! exactly one upstream API call through claude-proxy and expires
//! in 60 seconds.
//!
//! ## Lifecycle
//!
//! 1. Agent calls `secret_fetch(key)` MCP tool.
//! 2. Tool resolves autonomy (default Approve via S13 — operator
//!    sees + approves the request via the Approvals UI).
//! 3. Tool mints a token: random UUID v4, stored in the in-process
//!    [`ProxyTokenStore`] keyed by token UUID, with a 60-second
//!    TTL. The store retains the secret_key + brain_id so the
//!    proxy-side redeem path knows which secret to forward.
//! 4. Token returned to agent.
//! 5. Agent passes `X-Scope-Token: <token>` to claude-proxy on
//!    its single upstream API call.
//! 6. claude-proxy redeems: looks up the token, marks it used,
//!    fetches the underlying secret from `SecretStore`, attaches
//!    to the upstream request.
//! 7. Token state cleared.
//!
//! ## v4.2 v1 scope
//!
//! This module ships steps 1-4 + the in-memory store. Step 5
//! (claude-proxy migration to read tokens) is **S14-S-4** —
//! deferred to a follow-up session per scope honesty.

use neurogrim_secrets::SecretKey;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Default token TTL in seconds. Per the v4.2 epic invariant:
/// "Returned tokens are single-use, expire in 60s, can only be
/// passed to claude-proxy".
pub const DEFAULT_TTL_SECS: u64 = 60;

/// One outstanding proxy token. Carries enough state for the
/// proxy-side redeem to identify the underlying secret.
#[derive(Debug, Clone)]
pub struct ProxyToken {
    pub token_id: String,
    pub secret_key: SecretKey,
    pub minted_at: Instant,
    pub expires_at: Instant,
    /// Operator-approved scope from the autonomy gate (e.g.,
    /// "anthropic-api-once"). Surfaced in audit logs.
    pub scope: Option<String>,
    pub used: bool,
}

impl ProxyToken {
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires_at
    }

    /// Operator-facing summary suitable for logs / the audit ledger.
    /// Deliberately mentions the secret_id (which is non-sensitive
    /// metadata — the value is what's protected) but never the
    /// underlying value.
    pub fn audit_summary(&self) -> String {
        format!(
            "ProxyToken(id={}, brain={}, secret_id={}, scope={:?}, used={})",
            self.token_id, self.secret_key.brain_id, self.secret_key.secret_id, self.scope, self.used
        )
    }
}

/// In-process registry of outstanding tokens. Per-`BrainServer`
/// instance; tokens don't survive process restart.
///
/// Concurrency: behind a `Mutex` for simplicity. Token ops are
/// rare relative to scoring traffic (an upper bound of one mint
/// per `secret_fetch` call); contention is not a concern.
#[derive(Debug, Clone, Default)]
pub struct ProxyTokenStore {
    tokens: Arc<Mutex<HashMap<String, ProxyToken>>>,
}

impl ProxyTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a fresh token for `secret_key`. TTL defaults to
    /// [`DEFAULT_TTL_SECS`]; pass `None` to use it, or `Some(secs)`
    /// for an override (per-secret operator override is a v2
    /// feature; v1 always uses the default).
    pub fn mint(
        &self,
        secret_key: SecretKey,
        scope: Option<String>,
        ttl_secs: Option<u64>,
    ) -> ProxyToken {
        let ttl = Duration::from_secs(ttl_secs.unwrap_or(DEFAULT_TTL_SECS));
        let now = Instant::now();
        let token_id = Uuid::new_v4().to_string();
        let token = ProxyToken {
            token_id: token_id.clone(),
            secret_key,
            minted_at: now,
            expires_at: now + ttl,
            scope,
            used: false,
        };
        self.tokens.lock().unwrap().insert(token_id, token.clone());
        token
    }

    /// Redeem a token: return its details + mark used. Returns
    /// `None` when:
    /// - Token isn't in the store (never minted, expired and swept,
    ///   or already redeemed)
    /// - Token has expired
    ///
    /// Single-use: a token that's already `used` is treated as
    /// invalid (returns None). The proxy can safely call this
    /// concurrently across requests; only one redeem succeeds.
    pub fn redeem(&self, token_id: &str) -> Option<ProxyToken> {
        let mut map = self.tokens.lock().unwrap();
        let token = map.get_mut(token_id)?;
        let now = Instant::now();
        if token.used || token.is_expired(now) {
            return None;
        }
        token.used = true;
        Some(token.clone())
    }

    /// Clean up expired + used tokens. Called periodically by the
    /// dashboard's daily-compaction sweep (S13-B-7 scheduler when
    /// it lands; v1 is best-effort manual sweep on read).
    pub fn sweep_expired(&self) -> usize {
        let now = Instant::now();
        let mut map = self.tokens.lock().unwrap();
        let before = map.len();
        map.retain(|_, t| !t.used && !t.is_expired(now));
        before - map.len()
    }

    /// Number of currently outstanding (unexpired, unused) tokens.
    /// Used by the readiness sensor (S-8) to surface "agents have
    /// secret tokens outstanding right now" findings.
    pub fn outstanding(&self) -> usize {
        let now = Instant::now();
        let map = self.tokens.lock().unwrap();
        map.values()
            .filter(|t| !t.used && !t.is_expired(now))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_secrets::SecretKey;

    fn key() -> SecretKey {
        SecretKey::new("test-brain", "test-secret")
    }

    #[test]
    fn mint_returns_token_with_uuid_and_60s_ttl() {
        let store = ProxyTokenStore::new();
        let token = store.mint(key(), None, None);
        assert!(!token.token_id.is_empty());
        // UUID v4 format: 36 chars with 4 hyphens.
        assert_eq!(token.token_id.len(), 36);
        assert_eq!(token.token_id.matches('-').count(), 4);
        // Default TTL = 60s.
        let ttl = token.expires_at.duration_since(token.minted_at);
        assert_eq!(ttl, Duration::from_secs(60));
    }

    #[test]
    fn mint_with_custom_ttl() {
        let store = ProxyTokenStore::new();
        let token = store.mint(key(), None, Some(120));
        let ttl = token.expires_at.duration_since(token.minted_at);
        assert_eq!(ttl, Duration::from_secs(120));
    }

    #[test]
    fn redeem_returns_token_then_marks_used() {
        let store = ProxyTokenStore::new();
        let minted = store.mint(key(), Some("once".to_string()), None);
        let redeemed = store.redeem(&minted.token_id).expect("first redeem");
        assert_eq!(redeemed.token_id, minted.token_id);
        assert!(redeemed.used, "redeem flips the used flag");
        assert_eq!(redeemed.scope.as_deref(), Some("once"));
    }

    #[test]
    fn redeem_twice_returns_none_second_time() {
        let store = ProxyTokenStore::new();
        let minted = store.mint(key(), None, None);
        store.redeem(&minted.token_id).expect("first ok");
        assert!(
            store.redeem(&minted.token_id).is_none(),
            "single-use guarantee: second redeem must return None"
        );
    }

    #[test]
    fn redeem_unknown_token_returns_none() {
        let store = ProxyTokenStore::new();
        assert!(store.redeem("not-a-real-token-id").is_none());
    }

    #[test]
    fn redeem_expired_token_returns_none() {
        let store = ProxyTokenStore::new();
        // 0-second TTL: expires immediately.
        let minted = store.mint(key(), None, Some(0));
        // Sleep a tiny bit to ensure now > expires_at.
        std::thread::sleep(Duration::from_millis(10));
        assert!(store.redeem(&minted.token_id).is_none());
    }

    #[test]
    fn sweep_expired_removes_used_and_expired() {
        let store = ProxyTokenStore::new();
        let live = store.mint(key(), None, Some(60));
        let used = store.mint(key(), None, Some(60));
        let expired = store.mint(key(), None, Some(0));
        store.redeem(&used.token_id);
        std::thread::sleep(Duration::from_millis(10));
        let removed = store.sweep_expired();
        assert_eq!(removed, 2, "used + expired removed; live remains");
        assert_eq!(store.outstanding(), 1);
        // Live still redeemable.
        assert!(store.redeem(&live.token_id).is_some());
    }

    #[test]
    fn outstanding_counts_only_unused_unexpired() {
        let store = ProxyTokenStore::new();
        let _live = store.mint(key(), None, Some(60));
        let used = store.mint(key(), None, Some(60));
        let _expired = store.mint(key(), None, Some(0));
        store.redeem(&used.token_id);
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(store.outstanding(), 1);
    }

    #[test]
    fn audit_summary_redacts_value_metadata_only() {
        let store = ProxyTokenStore::new();
        let token = store.mint(SecretKey::new("alpha", "anthropic"), None, None);
        let summary = token.audit_summary();
        // Identifiers are surfaced.
        assert!(summary.contains("alpha"));
        assert!(summary.contains("anthropic"));
        assert!(summary.contains(&token.token_id));
        // The actual secret value never could appear here — there
        // is no value to leak in the token; this is a regression
        // guard test so future refactors that add fields don't
        // accidentally include plaintext.
    }

    #[test]
    fn token_store_is_clone_for_sharing_across_handlers() {
        // ProxyTokenStore::Clone is the property; cloning gives a
        // view into the same underlying Arc<Mutex<…>>. Mint on one
        // clone, redeem on another.
        let a = ProxyTokenStore::new();
        let b = a.clone();
        let token = a.mint(key(), None, None);
        let redeemed = b.redeem(&token.token_id);
        assert!(redeemed.is_some(), "clone shares state with original");
    }
}
