//! Scope tokens for A2A bearer authentication.
//!
//! Rust port of the Python `claude-proxy/claude_proxy/tokens.py` pattern.
//! Design invariants carried over 1:1:
//!
//! - Tokens are random 136-bit values, base32-encoded lowercase, prefixed
//!   `nb_sat_` ("NeuroBrain scope A2A Token"). Greppable in logs.
//! - Only the SHA-256 hash of the token is persisted. A full database
//!   compromise does NOT disclose previously-issued tokens.
//! - Validation uses `subtle::ConstantTimeEq` — timing attackers can't
//!   distinguish "unknown" from "revoked" from "expired."
//! - `token_id` is the first 16 chars of the hex digest. Stable
//!   identifier, safe for logs, used as the revocation key.
//!
//! The store is backed by a single SQLite file. Schema is created on
//! first open; the store is tolerant of being opened concurrently by
//! short-lived operator CLIs AND the long-running server.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base32::Alphabet;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

pub const TOKEN_PREFIX: &str = "nb_sat_";
const TOKEN_BODY_BYTES: usize = 17; // 136 bits; base32 → 28 chars (unpadded)

#[derive(Debug, Error)]
pub enum TokenStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRecord {
    pub token_id: String,
    pub label: String,
    pub profile: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub last_used_at: Option<i64>,
}

pub struct TokenStore {
    conn: Connection,
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl TokenStore {
    /// Open or create the token store at `db_path`. The parent directory
    /// is created if it does not exist.
    pub fn open<P: AsRef<Path>>(db_path: P) -> Result<Self, TokenStoreError> {
        let path = db_path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init_schema(&conn)?;
        Ok(Self { conn, db_path: path })
    }

    /// Open an in-memory store. Useful for tests; not for production.
    pub fn open_in_memory() -> Result<Self, TokenStoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn,
            db_path: PathBuf::new(),
        })
    }

    fn init_schema(conn: &Connection) -> Result<(), TokenStoreError> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tokens (
                token_hash    TEXT PRIMARY KEY,
                token_id      TEXT NOT NULL UNIQUE,
                label         TEXT NOT NULL,
                profile       TEXT NOT NULL DEFAULT 'default',
                created_at    INTEGER NOT NULL,
                expires_at    INTEGER,
                revoked_at    INTEGER,
                last_used_at  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_tokens_label ON tokens(label);
            "#,
        )?;
        Ok(())
    }

    /// Issue a new token. Returns `(raw_token, record)`. The raw token
    /// is returned ONLY here — it is never persisted. Only its hash is.
    /// Callers must display the raw token to the operator and then
    /// forget it; there is no recovery path for a lost token (revoke +
    /// reissue).
    pub fn issue(
        &self,
        label: &str,
        profile: &str,
        expires_in_seconds: Option<i64>,
    ) -> Result<(String, TokenRecord), TokenStoreError> {
        let raw = new_token();
        let hash = hash_token(&raw);
        let token_id = token_id_from_hash(&hash);
        let now = now_unix();
        let expires_at = expires_in_seconds.map(|d| now + d);
        self.conn.execute(
            "INSERT INTO tokens
                (token_hash, token_id, label, profile, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![hash, token_id, label, profile, now, expires_at],
        )?;
        Ok((
            raw,
            TokenRecord {
                token_id,
                label: label.to_string(),
                profile: profile.to_string(),
                created_at: now,
                expires_at,
                revoked_at: None,
                last_used_at: None,
            },
        ))
    }

    /// Revoke by `token_id`. Returns true if a row was updated.
    pub fn revoke(&self, token_id: &str) -> Result<bool, TokenStoreError> {
        let now = now_unix();
        let rows = self.conn.execute(
            "UPDATE tokens SET revoked_at = ?1
             WHERE token_id = ?2 AND revoked_at IS NULL",
            params![now, token_id],
        )?;
        Ok(rows > 0)
    }

    /// Validate a raw token. Returns `Some(record)` on success; `None`
    /// when the token is unknown, revoked, or expired.
    ///
    /// Constant-time hash comparison: even when the token is unknown,
    /// we perform a dummy comparison to keep the timing profile stable.
    pub fn validate(&self, raw_token: &str) -> Result<Option<TokenRecord>, TokenStoreError> {
        let presented_hash = hash_token(raw_token);

        // Look up by primary key (also a hash). `presented_hash == stored`
        // by construction when we find a row, but we run a constant-time
        // comparison anyway so the "not found" and "found-but-hash-mismatch"
        // (shouldn't happen) paths take the same time as "found-and-matches".
        let row: Option<(String, String, String, String, i64, Option<i64>, Option<i64>, Option<i64>)> =
            self.conn
                .query_row(
                    "SELECT token_hash, token_id, label, profile, created_at,
                            expires_at, revoked_at, last_used_at
                     FROM tokens WHERE token_hash = ?1",
                    params![presented_hash],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                            r.get(7)?,
                        ))
                    },
                )
                .optional()?;

        // Constant-time "is there a real match" check. If row is None,
        // compare against a dummy all-zeros hash of the same length so
        // the path through the function looks identical in duration.
        let dummy = "0".repeat(64);
        let (stored_hash, token_id, label, profile, created_at, expires_at, revoked_at, _last_used_at) =
            match row {
                Some(r) => r,
                None => {
                    let _: bool = presented_hash.as_bytes().ct_eq(dummy.as_bytes()).into();
                    return Ok(None);
                }
            };

        let match_ok: bool = presented_hash
            .as_bytes()
            .ct_eq(stored_hash.as_bytes())
            .into();
        if !match_ok {
            return Ok(None);
        }

        let now = now_unix();
        if revoked_at.is_some() {
            return Ok(None);
        }
        if let Some(exp) = expires_at {
            if now >= exp {
                return Ok(None);
            }
        }

        // Stamp last_used. Ignore errors — a failure here must not break
        // auth (e.g. read-only FS would break everything else first).
        let _ = self.conn.execute(
            "UPDATE tokens SET last_used_at = ?1 WHERE token_id = ?2",
            params![now, token_id],
        );

        Ok(Some(TokenRecord {
            token_id,
            label,
            profile,
            created_at,
            expires_at,
            revoked_at,
            last_used_at: Some(now),
        }))
    }

    /// List all tokens, newest-first by creation time.
    pub fn list_all(&self) -> Result<Vec<TokenRecord>, TokenStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT token_id, label, profile, created_at, expires_at,
                    revoked_at, last_used_at
             FROM tokens ORDER BY created_at DESC, token_id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TokenRecord {
                token_id: r.get(0)?,
                label: r.get(1)?,
                profile: r.get(2)?,
                created_at: r.get(3)?,
                expires_at: r.get(4)?,
                revoked_at: r.get(5)?,
                last_used_at: r.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

// ---------- helpers ----------

fn new_token() -> String {
    let mut bytes = [0u8; TOKEN_BODY_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    let encoded = base32::encode(Alphabet::Rfc4648 { padding: false }, &bytes);
    format!("{TOKEN_PREFIX}{}", encoded.to_lowercase())
}

fn hash_token(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn token_id_from_hash(hash: &str) -> String {
    hash[..16].to_string()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Return a loggable short form of a token_id (first 8 chars).
pub fn token_id_prefix(token_id: &str) -> &str {
    if token_id.len() >= 8 {
        &token_id[..8]
    } else {
        token_id
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_returns_prefixed_token() {
        let store = TokenStore::open_in_memory().unwrap();
        let (raw, rec) = store.issue("test-label", "default", None).unwrap();
        assert!(raw.starts_with(TOKEN_PREFIX));
        assert!(raw.len() > TOKEN_PREFIX.len() + 20);
        assert_eq!(rec.label, "test-label");
        assert_eq!(rec.profile, "default");
        assert!(rec.revoked_at.is_none());
    }

    #[test]
    fn validate_accepts_fresh_token() {
        let store = TokenStore::open_in_memory().unwrap();
        let (raw, rec) = store.issue("x", "default", None).unwrap();
        let v = store.validate(&raw).unwrap();
        assert!(v.is_some());
        assert_eq!(v.unwrap().token_id, rec.token_id);
    }

    #[test]
    fn validate_rejects_unknown_token() {
        let store = TokenStore::open_in_memory().unwrap();
        let v = store.validate("nb_sat_definitelynotreal").unwrap();
        assert!(v.is_none());
    }

    #[test]
    fn validate_rejects_revoked_token() {
        let store = TokenStore::open_in_memory().unwrap();
        let (raw, rec) = store.issue("x", "default", None).unwrap();
        assert!(store.revoke(&rec.token_id).unwrap());
        assert!(store.validate(&raw).unwrap().is_none());
    }

    #[test]
    fn validate_rejects_expired_token() {
        let store = TokenStore::open_in_memory().unwrap();
        let (raw, _) = store.issue("x", "default", Some(-1)).unwrap();
        assert!(store.validate(&raw).unwrap().is_none());
    }

    #[test]
    fn raw_token_not_persisted_anywhere() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("tokens.sqlite");
        let raw = {
            let store = TokenStore::open(&db_path).unwrap();
            let (raw, _) = store.issue("x", "default", None).unwrap();
            raw
        };
        // Re-open, scan every row as text, assert the raw token never appears.
        let conn = Connection::open(&db_path).unwrap();
        let mut stmt = conn.prepare("SELECT * FROM tokens").unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..row.as_ref().column_count() {
                if let Ok(s) = row.get::<_, String>(i) {
                    assert!(
                        !s.contains(&raw),
                        "raw token leaked in database column {i}: {s}"
                    );
                }
            }
        }
    }

    #[test]
    fn list_returns_all_issued_tokens() {
        let store = TokenStore::open_in_memory().unwrap();
        store.issue("first", "default", None).unwrap();
        store.issue("second", "default", None).unwrap();
        let labels: std::collections::HashSet<_> = store
            .list_all()
            .unwrap()
            .into_iter()
            .map(|t| t.label)
            .collect();
        assert_eq!(labels, ["first".to_string(), "second".to_string()].into_iter().collect());
    }

    #[test]
    fn revoke_unknown_returns_false() {
        let store = TokenStore::open_in_memory().unwrap();
        assert!(!store.revoke("0000deadbeef0000").unwrap());
    }

    #[test]
    fn token_id_prefix_truncates() {
        assert_eq!(token_id_prefix("deadbeefcafebabe01"), "deadbeef");
        assert_eq!(token_id_prefix("short"), "short");
    }
}
