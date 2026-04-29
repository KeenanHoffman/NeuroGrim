//! v3.5.0 — per-project random port assignment, persisted across runs.
//!
//! Addresses the recurring port-conflict pain in multi-Brain
//! ecosystems where 5+ services were manually pinned to
//! 8420 / 8421 / 8422 / ... and started colliding as adoption grew.
//! Each project picks two ports (dashboard + a2a) at first run from
//! the IANA dynamic range (49152-65535), persists the choice to
//! `<project>/.claude/brain/ports.json`, and reuses it on subsequent
//! runs. Existing users with bookmarks at `:8420` keep working with
//! `--port 8420` (CLI-explicit precedence wins; see precedence rule
//! at `crates/neurogrim-cli/src/commands/ui.rs::run`).
//!
//! # Schema
//!
//! `<project>/.claude/brain/ports.json`:
//!
//! ```json
//! {
//!   "schema_version": "1",
//!   "dashboard_port": 51234,
//!   "a2a_port": 51235,
//!   "created_at": "2026-04-29T14:22:31Z",
//!   "generated_by": "neurogrim/3.5.0"
//! }
//! ```
//!
//! # Allocation algorithm
//!
//! [`allocate`] reads `ports.json` if present (idempotent). Otherwise
//! picks two distinct ports from `49152..=65535` via random sampling,
//! retrying up to `max_attempts` times per pick. Each candidate is
//! checked for OS bind feasibility via [`try_bind`] (a synchronous
//! `TcpListener::bind` that immediately drops the listener). The
//! resulting `PortConfig` is persisted atomically (temp + rename) so
//! a concurrent reader sees either the old or new value, never a
//! partial write — same pattern as
//! `crates/neurogrim-dashboard/src/layout.rs::save_layout`.
//!
//! # Concurrency
//!
//! Two `neurogrim ui` processes started simultaneously can race the
//! allocator: both pick random ports, both `try_bind` succeed, both
//! write `ports.json` (last writer wins). Mitigation deferred to
//! v3.5.1 (file-lock on `ports.json.lock` during allocate). OS bind
//! feasibility is also checked at server-startup, so a lost race
//! surfaces as a startup `bind: address in use` error rather than
//! silent breakage.
//!
//! # Sync I/O
//!
//! Sync on purpose — keeps `neurogrim-core` free of the `tokio` IO
//! surface other modules avoid. Callers from `tokio::main` contexts
//! can wrap [`allocate`] in `tokio::task::spawn_blocking` if they
//! care about not blocking the runtime; in practice it's called
//! once at server startup before the runtime sees real load.

use chrono::{DateTime, Utc};
use rand::Rng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Schema version of `ports.json`. Bumped on incompatible changes.
pub const SCHEMA_VERSION: &str = "1";

/// IANA dynamic / private port range. Most operating systems pick
/// outbound ephemeral ports from this range, but only when no
/// listening socket already holds the port — squatting here for
/// long-lived servers is safe and avoids the well-known + registered
/// ranges where service ownership conventions exist.
pub const SAFE_PORT_LO: u16 = 49152;
pub const SAFE_PORT_HI: u16 = 65535;

/// Maximum random-pick attempts before giving up. 64 is enough to
/// be effectively exhaustive against the dynamic range when most
/// ports are free, while still bounding the loop on a hostile
/// system where every candidate is bound.
pub const DEFAULT_MAX_ATTEMPTS: u32 = 64;

/// Per-project port configuration, persisted at
/// `<project>/.claude/brain/ports.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortConfig {
    /// Wire-format schema version. Always `"1"` at v3.5.0; future
    /// shape changes bump this.
    pub schema_version: String,
    /// Port the dashboard server (`neurogrim ui`) binds on.
    pub dashboard_port: u16,
    /// Port the A2A peer server (`neurogrim a2a-serve`) binds on.
    pub a2a_port: u16,
    /// UTC timestamp of the first successful allocation. Never
    /// rewritten on subsequent reads — preserves provenance.
    pub created_at: DateTime<Utc>,
    /// Free-form software-version string, e.g. `"neurogrim/3.5.0"`.
    /// Surfaced for forensics if a future allocator rewrites the
    /// shape.
    pub generated_by: String,
}

/// Tunables for the allocator. Most callers pass `Default::default()`;
/// tests override `rng_seed` to make the random pick deterministic.
#[derive(Debug, Clone)]
pub struct PortAllocator {
    /// When `Some(n)`, the allocator uses a deterministic seeded RNG
    /// instead of `from_entropy()`. Test-only escape hatch.
    pub rng_seed: Option<u64>,
    /// Max attempts before giving up on a port pick.
    pub max_attempts: u32,
    /// Inclusive range to pick from. Defaults to 49152..=65535.
    pub range: std::ops::RangeInclusive<u16>,
}

impl Default for PortAllocator {
    fn default() -> Self {
        Self {
            rng_seed: None,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            range: SAFE_PORT_LO..=SAFE_PORT_HI,
        }
    }
}

/// Path to a project's `ports.json`. Returns the path even when the
/// file doesn't exist (caller decides whether to read or generate).
pub fn ports_file_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("ports.json")
}

/// Read `ports.json` if present.
///
/// Returns `Some(parsed)` on success, `None` when the file is
/// missing OR malformed. Malformed files are logged via
/// `tracing::warn` so the operator sees the problem; the caller
/// falls back to allocation rather than blowing up.
pub fn read_ports(project_root: &Path) -> Option<PortConfig> {
    let path = ports_file_path(project_root);
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<PortConfig>(trimmed) {
        Ok(parsed) => Some(parsed),
        Err(e) => {
            tracing::warn!(
                "ports.json at {:?} could not be parsed ({e}); will allocate fresh",
                path
            );
            None
        }
    }
}

/// Atomically write `ports.json` to disk. Creates
/// `<project>/.claude/brain/` if missing.
///
/// Atomicity: serializes to `ports.json.tmp` in the same directory,
/// then `rename`s onto the final path. Same-directory rename is
/// atomic on every supported OS — a concurrent reader never sees a
/// half-written file. Mirrors the dashboard layout writer's pattern
/// (`crates/neurogrim-dashboard/src/layout.rs::save_layout`).
pub fn save_ports(project_root: &Path, cfg: &PortConfig) -> std::io::Result<()> {
    use std::io::Write;
    let dir = project_root.join(".claude").join("brain");
    std::fs::create_dir_all(&dir)?;
    let final_path = dir.join("ports.json");
    let tmp_path = dir.join("ports.json.tmp");

    let serialized = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(serialized.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

/// Probe whether a port is currently bindable on `127.0.0.1`. The
/// `TcpListener` returned by a successful `bind` is dropped
/// immediately, releasing the port for the actual server.
///
/// Sync on purpose — keeps `neurogrim-core` free of the `tokio` IO
/// surface other modules avoid. There's a TOCTOU window between
/// this probe and the caller's actual bind: another process could
/// grab the port in the interim. Callers should still surface the
/// real bind failure if it happens; this probe is a fast filter,
/// not a guarantee.
pub fn try_bind(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Read existing `ports.json` or allocate a fresh one.
///
/// Returns `(cfg, was_freshly_generated)`:
/// - When `ports.json` already exists and parses, returns
///   `(parsed, false)`.
/// - Otherwise picks two distinct ports, persists the result, and
///   returns `(cfg, true)`. Callers use the bool to decide whether
///   to print a one-time "Port auto-allocated…" message.
///
/// Errors when the random pick fails after `alloc.max_attempts`
/// (every candidate already bound) or when the persistence write
/// fails (permissions, disk full).
pub fn allocate(
    project_root: &Path,
    alloc: &PortAllocator,
) -> Result<(PortConfig, bool), AllocateError> {
    if let Some(existing) = read_ports(project_root) {
        return Ok((existing, false));
    }
    let mut chosen = HashSet::new();
    let dashboard_port = pick_one(alloc, &chosen)?;
    chosen.insert(dashboard_port);
    let a2a_port = pick_one(alloc, &chosen)?;
    let cfg = PortConfig {
        schema_version: SCHEMA_VERSION.to_string(),
        dashboard_port,
        a2a_port,
        created_at: Utc::now(),
        generated_by: format!("neurogrim/{}", env!("CARGO_PKG_VERSION")),
    };
    save_ports(project_root, &cfg).map_err(AllocateError::Persist)?;
    Ok((cfg, true))
}

/// Errors from [`allocate`]. Distinct variants so callers can
/// surface a precise message per failure mode.
#[derive(Debug, thiserror::Error)]
pub enum AllocateError {
    /// Couldn't find a free port in `max_attempts` random tries.
    #[error(
        "no free port found in {attempts} attempts within range {range_lo}-{range_hi} \
         — every candidate is already bound"
    )]
    Exhausted {
        attempts: u32,
        range_lo: u16,
        range_hi: u16,
    },
    /// Filesystem write failed (permissions, disk full, etc.).
    #[error("failed to persist ports.json: {0}")]
    Persist(std::io::Error),
}

/// Pick a single port from the allocator's range that isn't in
/// `exclude` AND isn't currently bound on the OS. Returns
/// [`AllocateError::Exhausted`] after `alloc.max_attempts` tries.
fn pick_one(alloc: &PortAllocator, exclude: &HashSet<u16>) -> Result<u16, AllocateError> {
    let lo = *alloc.range.start();
    let hi = *alloc.range.end();
    let mut rng: rand::rngs::StdRng = match alloc.rng_seed {
        Some(seed) => rand::rngs::StdRng::seed_from_u64(seed),
        None => rand::rngs::StdRng::from_entropy(),
    };
    for _ in 0..alloc.max_attempts {
        let candidate: u16 = rng.gen_range(lo..=hi);
        if exclude.contains(&candidate) {
            continue;
        }
        if try_bind(candidate) {
            return Ok(candidate);
        }
    }
    Err(AllocateError::Exhausted {
        attempts: alloc.max_attempts,
        range_lo: lo,
        range_hi: hi,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use tempfile::TempDir;

    fn fixture_root() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn allocate_writes_ports_json_when_missing() {
        let tmp = fixture_root();
        let alloc = PortAllocator::default();
        let (cfg, fresh) = allocate(tmp.path(), &alloc).expect("allocate");
        assert!(fresh, "first allocation must be fresh");

        let path = ports_file_path(tmp.path());
        assert!(path.exists(), "ports.json must be written");
        assert!((SAFE_PORT_LO..=SAFE_PORT_HI).contains(&cfg.dashboard_port));
        assert!((SAFE_PORT_LO..=SAFE_PORT_HI).contains(&cfg.a2a_port));
        assert_ne!(
            cfg.dashboard_port, cfg.a2a_port,
            "dashboard and a2a ports must differ"
        );
        assert_eq!(cfg.schema_version, "1");
        assert!(cfg.generated_by.starts_with("neurogrim/"));
    }

    #[test]
    fn allocate_idempotent_when_file_exists() {
        let tmp = fixture_root();
        let alloc = PortAllocator::default();
        let (first, fresh1) = allocate(tmp.path(), &alloc).unwrap();
        let (second, fresh2) = allocate(tmp.path(), &alloc).unwrap();
        assert!(fresh1);
        assert!(!fresh2, "second allocation must read from disk");
        assert_eq!(first, second, "second call must return identical config");
    }

    #[test]
    fn read_ports_returns_none_on_malformed() {
        let tmp = fixture_root();
        std::fs::create_dir_all(tmp.path().join(".claude/brain")).unwrap();
        std::fs::write(ports_file_path(tmp.path()), "{ not json").unwrap();
        // Malformed file → None (warn-logged, but doesn't panic).
        assert!(read_ports(tmp.path()).is_none());
    }

    #[test]
    fn read_ports_returns_none_when_file_missing() {
        let tmp = fixture_root();
        assert!(read_ports(tmp.path()).is_none());
    }

    #[test]
    fn save_ports_atomic_no_temp_file_left() {
        // Save twice; second save replaces the first. The atomic-rename
        // temp file should not remain on disk.
        let tmp = fixture_root();
        let cfg1 = PortConfig {
            schema_version: "1".into(),
            dashboard_port: 50001,
            a2a_port: 50002,
            created_at: Utc::now(),
            generated_by: "test".into(),
        };
        save_ports(tmp.path(), &cfg1).unwrap();
        let cfg2 = PortConfig {
            schema_version: "1".into(),
            dashboard_port: 50003,
            a2a_port: 50004,
            created_at: Utc::now(),
            generated_by: "test".into(),
        };
        save_ports(tmp.path(), &cfg2).unwrap();
        assert!(!tmp.path().join(".claude/brain/ports.json.tmp").exists());
        let read = read_ports(tmp.path()).unwrap();
        assert_eq!(read.dashboard_port, 50003, "second save must win");
    }

    #[test]
    fn save_then_read_round_trip_preserves_all_fields() {
        let tmp = fixture_root();
        let now = Utc::now();
        let cfg = PortConfig {
            schema_version: "1".into(),
            dashboard_port: 50050,
            a2a_port: 50051,
            created_at: now,
            generated_by: "neurogrim/3.5.0".into(),
        };
        save_ports(tmp.path(), &cfg).unwrap();
        let read = read_ports(tmp.path()).unwrap();
        // chrono roundtrip preserves the instant; comparing structs is fine.
        assert_eq!(read, cfg);
    }

    #[test]
    fn pick_one_skips_bound_ports() {
        // Bind a port, then ask the allocator for one in a tight
        // range that includes it. If the allocator returns
        // successfully, the result must NOT be the bound port.
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let bound = listener.local_addr().unwrap().port();
        let alloc = PortAllocator {
            rng_seed: Some(42),
            max_attempts: 64,
            range: bound..=bound.saturating_add(5),
        };
        let exclude = HashSet::new();
        match pick_one(&alloc, &exclude) {
            Ok(port) => assert_ne!(port, bound, "picker must skip bound ports"),
            // If every port in the tiny range happens to be bound by
            // unrelated tests or services, the picker is allowed to
            // fail. The contract is "if it succeeds, it's not bound."
            Err(AllocateError::Exhausted { .. }) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn pick_one_fails_when_attempts_exhausted_in_tight_range() {
        // Bind the only port in a 1-port range so the allocator can
        // never succeed. Verify it gives up cleanly with the
        // documented error.
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let bound = listener.local_addr().unwrap().port();
        let alloc = PortAllocator {
            rng_seed: Some(0),
            max_attempts: 8,
            range: bound..=bound,
        };
        let exclude = HashSet::new();
        let result = pick_one(&alloc, &exclude);
        match result {
            Err(AllocateError::Exhausted {
                attempts,
                range_lo,
                range_hi,
            }) => {
                assert_eq!(attempts, 8);
                assert_eq!(range_lo, bound);
                assert_eq!(range_hi, bound);
            }
            other => panic!("expected Exhausted, got {other:?}"),
        }
    }

    #[test]
    fn pick_one_skips_excluded_ports() {
        // First pick lands somewhere; exclude that exact value;
        // second pick must land elsewhere. Seeded so the test is
        // deterministic.
        let alloc = PortAllocator {
            rng_seed: Some(123),
            max_attempts: 32,
            range: SAFE_PORT_LO..=SAFE_PORT_HI,
        };
        let first = pick_one(&alloc, &HashSet::new()).unwrap();
        let mut excluded = HashSet::new();
        excluded.insert(first);
        let second = pick_one(&alloc, &excluded).unwrap();
        assert_ne!(first, second, "exclude set must steer the picker away");
    }

    #[test]
    fn ports_file_path_is_under_claude_brain() {
        let p = ports_file_path(Path::new("/proj"));
        assert!(
            p.ends_with(Path::new(".claude/brain/ports.json"))
                || p.ends_with(Path::new(".claude\\brain\\ports.json")),
            "unexpected path shape: {p:?}"
        );
    }

    #[test]
    fn allocate_picks_distinct_ports_for_dashboard_and_a2a() {
        // Property: two ports allocated together are never equal
        // (distinct by construction in `allocate`).
        for _ in 0..16 {
            let tmp = fixture_root();
            let alloc = PortAllocator::default();
            let (cfg, _) = allocate(tmp.path(), &alloc).unwrap();
            assert_ne!(cfg.dashboard_port, cfg.a2a_port);
        }
    }
}
