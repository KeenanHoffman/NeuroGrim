//! v4.5 — Local time-series store for NeuroGrim's own observability.
//!
//! ## Why this exists
//!
//! NeuroGrim's value proposition is "continuous project health awareness +
//! trajectory intelligence". The bus topics (`_neurogrim/score-snapshots`
//! et al.) are event streams; the trajectory pipeline operates on
//! `score-history`. But before this module landed there was no first-class
//! way to ask:
//!
//! - "Per-domain score over the last 30 days" (only unified-score history)
//! - "Has this Brain's request latency regressed?"
//! - "Is the BrainContext cache actually helping?"
//! - "When did skill X usage start declining?"
//!
//! Those were each half-built in their own corner of the codebase, or not
//! built at all.
//!
//! ## Design boundary
//!
//! - **Metric** = a named series (`domain_score`, `request_duration`, etc.)
//!   with a bounded, pre-declared tag shape. We don't accept arbitrary
//!   labels Prometheus-style; the tag shape is the producer's commitment.
//! - **Storage** = SQLite, single file at
//!   `.claude/brain/queues/_neurogrim/metrics.sqlite`. WAL mode, consistent
//!   with the bus's `SqliteBackend`.
//! - **Queries** = a small typed surface; SQL is an implementation detail.
//! - **Ingest** = primarily via the bus (a separate `MetricExtractor`
//!   subscribes to topics and records points). Direct `record()` is used
//!   for hot-path instrumentation (`request_duration`) where going through
//!   the bus would add cost.
//!
//! See `docs/metrics.md` (TODO) for operator-facing schema docs.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ── Constants ───────────────────────────────────────────────────────────

/// Standard on-disk path for the metrics store.
/// `.claude/brain/queues/_neurogrim/metrics.sqlite` — colocated with the
/// queue topics' SQLite files so operators inspect one directory.
pub fn metrics_store_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("queues")
        .join("_neurogrim")
        .join("metrics.sqlite")
}

// ── Tags ────────────────────────────────────────────────────────────────

/// Pre-declared tag dimensions for a metric series. Sorted-key
/// `BTreeMap` so identical tag-sets serialize identically — important
/// for downstream aggregations that group by tag combinations.
///
/// We don't validate tag shapes at runtime; producers are expected to
/// emit consistent tags per metric. This is the LSP Brains methodology
/// applied — the schema is a producer commitment, not a runtime check.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tags(BTreeMap<String, String>);

impl Tags {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.0.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Canonical JSON encoding. Keys are sorted via `BTreeMap` so two
    /// `Tags` with identical content always produce identical strings.
    fn to_canonical_json(&self) -> String {
        // serde_json preserves BTreeMap insertion order which IS sorted-by-key.
        serde_json::to_string(&self.0).unwrap_or_else(|_| "{}".to_string())
    }

    fn from_canonical_json(s: &str) -> Self {
        serde_json::from_str::<BTreeMap<String, String>>(s)
            .map(Tags)
            .unwrap_or_default()
    }
}

// ── Data point ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataPoint {
    pub ts: DateTime<Utc>,
    pub value: f64,
    pub tags: Tags,
}

// ── Aggregations ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggregate {
    /// Mean of points in each bucket.
    Avg,
    /// Sum of values in each bucket.
    Sum,
    /// Minimum value in each bucket.
    Min,
    /// Maximum value in each bucket.
    Max,
    /// Number of points in each bucket.
    Count,
    /// Most-recent value in each bucket (last-write-wins).
    Last,
}

// ── Query ───────────────────────────────────────────────────────────────

/// A query against the store. Build with [`Query::new`] + builder methods.
#[derive(Debug, Clone)]
pub struct Query {
    pub name: String,
    /// Required-equals tag filters. Points missing any filtered tag, or
    /// having a different value, are excluded.
    pub tag_filters: BTreeMap<String, String>,
    pub since: DateTime<Utc>,
    pub until: Option<DateTime<Utc>>,
    /// `None` = return raw points; `Some` = bucketed aggregation.
    pub aggregate: Option<Aggregate>,
    /// Bucket size in milliseconds for aggregation. Required if
    /// `aggregate` is `Some`. Ignored otherwise.
    pub bucket_ms: Option<i64>,
    /// Hard cap on points returned. None = no cap (use cautiously —
    /// uncapped queries against year-long retention can be slow).
    pub limit: Option<usize>,
}

impl Query {
    pub fn new(name: impl Into<String>, since: DateTime<Utc>) -> Self {
        Self {
            name: name.into(),
            tag_filters: BTreeMap::new(),
            since,
            until: None,
            aggregate: None,
            bucket_ms: None,
            limit: None,
        }
    }

    pub fn until(mut self, until: DateTime<Utc>) -> Self {
        self.until = Some(until);
        self
    }

    pub fn filter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tag_filters.insert(key.into(), value.into());
        self
    }

    pub fn aggregate(mut self, agg: Aggregate, bucket: chrono::Duration) -> Self {
        self.aggregate = Some(agg);
        self.bucket_ms = Some(bucket.num_milliseconds().max(1));
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
}

// ── Series info (for listing) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesInfo {
    pub name: String,
    pub point_count: u64,
    /// Distinct tag-set count (cardinality). High cardinality is the
    /// classic TSDB footgun — surfacing it in listings lets operators
    /// catch a runaway producer early.
    pub cardinality: u64,
    pub earliest_ts: Option<DateTime<Utc>>,
    pub latest_ts: Option<DateTime<Utc>>,
}

// ── Store ───────────────────────────────────────────────────────────────

/// Local time-series store backed by SQLite (WAL mode).
///
/// Open one per project root. Cheap to clone via `Arc<Mutex<MetricsStore>>`
/// at the dashboard layer (rusqlite's `Connection` is `!Sync`).
pub struct MetricsStore {
    conn: Connection,
    path: PathBuf,
}

impl MetricsStore {
    /// Open or create the metrics store. Creates parent directory if
    /// absent. Initializes schema on first open.
    pub fn open(project_root: &Path) -> Result<Self> {
        let path = metrics_store_path(project_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("metrics: create dir {:?}", parent))?;
        }
        let conn = Connection::open(&path)
            .with_context(|| format!("metrics: open {:?}", path))?;
        // WAL mode for concurrent reads + atomic writes (mirrors
        // SqliteBackend).
        conn.pragma_update(None, "journal_mode", "WAL")
            .context("metrics: enable WAL")?;
        Self::ensure_schema(&conn)?;
        Ok(Self { conn, path })
    }

    fn ensure_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS metric_points (
              id          INTEGER PRIMARY KEY AUTOINCREMENT,
              metric_name TEXT    NOT NULL,
              ts_ms       INTEGER NOT NULL,
              tags_json   TEXT    NOT NULL,
              value       REAL    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mp_name_ts
              ON metric_points(metric_name, ts_ms);
            CREATE INDEX IF NOT EXISTS idx_mp_ts
              ON metric_points(ts_ms);

            CREATE TABLE IF NOT EXISTS metric_schema_version (
              version INTEGER PRIMARY KEY
            );
            INSERT OR IGNORE INTO metric_schema_version (version) VALUES (1);
            "#,
        )
        .context("metrics: ensure schema")?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Record a single data point at `Utc::now()`.
    pub fn record(&mut self, name: &str, tags: &Tags, value: f64) -> Result<()> {
        self.record_at(name, tags, value, Utc::now())
    }

    /// Record a single data point at the given timestamp. Use for
    /// historical backfills + tests; production code prefers `record`.
    pub fn record_at(
        &mut self,
        name: &str,
        tags: &Tags,
        value: f64,
        ts: DateTime<Utc>,
    ) -> Result<()> {
        let tags_json = tags.to_canonical_json();
        self.conn
            .execute(
                r#"INSERT INTO metric_points (metric_name, ts_ms, tags_json, value)
                   VALUES (?1, ?2, ?3, ?4)"#,
                params![name, ts.timestamp_millis(), tags_json, value],
            )
            .context("metrics: record_at insert")?;
        Ok(())
    }

    /// Run a query. Returns raw points if `aggregate` is `None`, or
    /// bucketed aggregations if `Some`.
    pub fn query(&self, q: &Query) -> Result<Vec<DataPoint>> {
        match q.aggregate {
            None => self.query_raw(q),
            Some(agg) => self.query_aggregated(q, agg),
        }
    }

    fn query_raw(&self, q: &Query) -> Result<Vec<DataPoint>> {
        let mut sql = String::from(
            "SELECT ts_ms, value, tags_json FROM metric_points \
             WHERE metric_name = ?1 AND ts_ms >= ?2",
        );
        if q.until.is_some() {
            sql.push_str(" AND ts_ms <= ?3");
        }
        sql.push_str(" ORDER BY ts_ms ASC");
        if let Some(n) = q.limit {
            sql.push_str(&format!(" LIMIT {}", n.max(1)));
        }

        let mut stmt = self.conn.prepare(&sql).context("metrics: prepare raw")?;
        let since_ms = q.since.timestamp_millis();
        let until_ms = q.until.map(|u| u.timestamp_millis());

        let rows = if let Some(u) = until_ms {
            stmt.query_map(params![&q.name, since_ms, u], Self::row_to_point)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(params![&q.name, since_ms], Self::row_to_point)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };

        // Tag-filter in Rust to keep the SQL simple. JSON predicate
        // pushdown could be added if profiling shows this matters.
        let filtered: Vec<DataPoint> = rows
            .into_iter()
            .filter(|p| q.tag_filters.iter().all(|(k, v)| p.tags.get(k) == Some(v)))
            .collect();

        Ok(filtered)
    }

    fn query_aggregated(&self, q: &Query, agg: Aggregate) -> Result<Vec<DataPoint>> {
        let bucket_ms = q
            .bucket_ms
            .ok_or_else(|| anyhow::anyhow!("metrics: aggregate query missing bucket_ms"))?;
        // Pull raw points then aggregate in Rust. SQL-side aggregation
        // is faster but requires per-aggregator SQL — punt that to v2.
        let raw = self.query_raw(q)?;
        if raw.is_empty() {
            return Ok(vec![]);
        }
        let mut buckets: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
        for p in raw {
            let bucket = (p.ts.timestamp_millis() / bucket_ms) * bucket_ms;
            buckets.entry(bucket).or_default().push(p.value);
        }
        let points = buckets
            .into_iter()
            .map(|(bucket_start_ms, values)| {
                let value = match agg {
                    Aggregate::Avg => values.iter().sum::<f64>() / values.len() as f64,
                    Aggregate::Sum => values.iter().sum::<f64>(),
                    Aggregate::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
                    Aggregate::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    Aggregate::Count => values.len() as f64,
                    Aggregate::Last => *values.last().expect("non-empty by construction"),
                };
                DataPoint {
                    ts: DateTime::from_timestamp_millis(bucket_start_ms).unwrap_or_else(Utc::now),
                    value,
                    tags: Tags::default(),
                }
            })
            .collect();
        Ok(points)
    }

    /// List every recorded series with summary stats. Cheap query
    /// powered by an index scan; suitable for the Plumbing page.
    pub fn list_series(&self) -> Result<Vec<SeriesInfo>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT metric_name,
                      COUNT(*),
                      COUNT(DISTINCT tags_json),
                      MIN(ts_ms),
                      MAX(ts_ms)
               FROM metric_points
               GROUP BY metric_name
               ORDER BY metric_name ASC"#,
        )?;
        let rows = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let point_count: i64 = row.get(1)?;
                let cardinality: i64 = row.get(2)?;
                let earliest_ms: Option<i64> = row.get(3)?;
                let latest_ms: Option<i64> = row.get(4)?;
                Ok(SeriesInfo {
                    name,
                    point_count: point_count.max(0) as u64,
                    cardinality: cardinality.max(0) as u64,
                    earliest_ts: earliest_ms.and_then(DateTime::from_timestamp_millis),
                    latest_ts: latest_ms.and_then(DateTime::from_timestamp_millis),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Total point count across all series. Used by the Plumbing page's
    /// substrate-overview header.
    pub fn total_points(&self) -> Result<u64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM metric_points", [], |row| row.get(0))?;
        Ok(n.max(0) as u64)
    }

    /// File size on disk in bytes. Useful for the Plumbing Storage tab.
    pub fn size_bytes(&self) -> Result<u64> {
        Ok(std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0))
    }

    /// Delete points older than the cutoff. Returns the number of rows
    /// removed. Operators call this from the Plumbing actions; an
    /// automated retention worker is iteration 3 work.
    pub fn delete_before(&mut self, cutoff: DateTime<Utc>) -> Result<u64> {
        let n = self
            .conn
            .execute(
                "DELETE FROM metric_points WHERE ts_ms < ?1",
                params![cutoff.timestamp_millis()],
            )
            .context("metrics: delete_before")?;
        Ok(n as u64)
    }

    fn row_to_point(row: &rusqlite::Row<'_>) -> rusqlite::Result<DataPoint> {
        let ts_ms: i64 = row.get(0)?;
        let value: f64 = row.get(1)?;
        let tags_json: String = row.get(2)?;
        Ok(DataPoint {
            ts: DateTime::from_timestamp_millis(ts_ms).unwrap_or_else(Utc::now),
            value,
            tags: Tags::from_canonical_json(&tags_json),
        })
    }
}

// ── Convenience: thread-safe handle ─────────────────────────────────────

use std::sync::{Arc, Mutex};

/// Thread-safe wrapper around `MetricsStore`. The dashboard's `AppState`
/// holds one of these so handlers can record from any task without
/// coordinating connection ownership. Mutex is fine — record/query
/// operations are sub-millisecond and contention is low.
#[derive(Clone)]
pub struct MetricsHandle(Arc<Mutex<MetricsStore>>);

impl MetricsHandle {
    pub fn new(store: MetricsStore) -> Self {
        Self(Arc::new(Mutex::new(store)))
    }

    pub fn open(project_root: &Path) -> Result<Self> {
        Ok(Self::new(MetricsStore::open(project_root)?))
    }

    pub fn record(&self, name: &str, tags: &Tags, value: f64) {
        if let Ok(mut s) = self.0.lock() {
            if let Err(e) = s.record(name, tags, value) {
                tracing::warn!("metrics: record({name}) failed: {e}");
            }
        }
    }

    pub fn record_at(
        &self,
        name: &str,
        tags: &Tags,
        value: f64,
        ts: DateTime<Utc>,
    ) {
        if let Ok(mut s) = self.0.lock() {
            if let Err(e) = s.record_at(name, tags, value, ts) {
                tracing::warn!("metrics: record_at({name}) failed: {e}");
            }
        }
    }

    pub fn query(&self, q: &Query) -> Result<Vec<DataPoint>> {
        let s = self
            .0
            .lock()
            .map_err(|e| anyhow::anyhow!("metrics: lock poisoned: {e}"))?;
        s.query(q)
    }

    pub fn list_series(&self) -> Result<Vec<SeriesInfo>> {
        let s = self
            .0
            .lock()
            .map_err(|e| anyhow::anyhow!("metrics: lock poisoned: {e}"))?;
        s.list_series()
    }

    pub fn total_points(&self) -> Result<u64> {
        let s = self
            .0
            .lock()
            .map_err(|e| anyhow::anyhow!("metrics: lock poisoned: {e}"))?;
        s.total_points()
    }

    pub fn size_bytes(&self) -> Result<u64> {
        let s = self
            .0
            .lock()
            .map_err(|e| anyhow::anyhow!("metrics: lock poisoned: {e}"))?;
        s.size_bytes()
    }

    pub fn delete_before(&self, cutoff: DateTime<Utc>) -> Result<u64> {
        let mut s = self
            .0
            .lock()
            .map_err(|e| anyhow::anyhow!("metrics: lock poisoned: {e}"))?;
        s.delete_before(cutoff)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    fn open_store(tmp: &TempDir) -> MetricsStore {
        MetricsStore::open(tmp.path()).expect("open store")
    }

    #[test]
    fn open_creates_directory_and_file() {
        let tmp = TempDir::new().unwrap();
        let _store = open_store(&tmp);
        let path = metrics_store_path(tmp.path());
        assert!(path.exists(), "metrics.sqlite should be created");
        assert!(path.parent().unwrap().exists(), "parent dir should exist");
    }

    #[test]
    fn record_then_raw_query_returns_points() {
        let tmp = TempDir::new().unwrap();
        let mut store = open_store(&tmp);
        let tags = Tags::new().with("domain", "test-health");
        store.record("domain_score", &tags, 78.0).unwrap();
        store.record("domain_score", &tags, 80.0).unwrap();

        let since = Utc::now() - Duration::seconds(60);
        let q = Query::new("domain_score", since);
        let pts = store.query(&q).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].value, 78.0);
        assert_eq!(pts[0].tags.get("domain"), Some("test-health"));
        assert_eq!(pts[1].value, 80.0);
    }

    #[test]
    fn tag_filter_excludes_non_matching_points() {
        let tmp = TempDir::new().unwrap();
        let mut store = open_store(&tmp);
        store
            .record(
                "domain_score",
                &Tags::new().with("domain", "test-health"),
                78.0,
            )
            .unwrap();
        store
            .record(
                "domain_score",
                &Tags::new().with("domain", "code-quality"),
                65.0,
            )
            .unwrap();

        let since = Utc::now() - Duration::seconds(60);
        let q = Query::new("domain_score", since).filter("domain", "test-health");
        let pts = store.query(&q).unwrap();
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].value, 78.0);
    }

    #[test]
    fn aggregation_buckets_and_averages() {
        let tmp = TempDir::new().unwrap();
        let mut store = open_store(&tmp);
        let tags = Tags::new().with("domain", "test-health");
        // Anchor at an hour boundary so points fall predictably into
        // 1-hour buckets. Using `Utc::now()` made this flaky — points
        // could span 3 buckets if `now` happened to fall mid-hour.
        // 1_714_500_000 = 2024-04-30 14:00:00 UTC (top of an hour).
        let base = DateTime::from_timestamp(1_714_500_000, 0).unwrap();
        // 3 points in hour-1 (offsets 0, 20, 40 min), 2 in hour-2 (70, 100 min)
        store.record_at("domain_score", &tags, 60.0, base).unwrap();
        store
            .record_at("domain_score", &tags, 70.0, base + Duration::minutes(20))
            .unwrap();
        store
            .record_at("domain_score", &tags, 80.0, base + Duration::minutes(40))
            .unwrap();
        store
            .record_at("domain_score", &tags, 90.0, base + Duration::minutes(70))
            .unwrap();
        store
            .record_at("domain_score", &tags, 100.0, base + Duration::minutes(100))
            .unwrap();

        let q = Query::new("domain_score", base - Duration::minutes(1))
            .until(base + Duration::hours(3))
            .aggregate(Aggregate::Avg, Duration::hours(1));
        let pts = store.query(&q).unwrap();
        // Two buckets: hour-1 avg = 70 (60+70+80)/3, hour-2 avg = 95 (90+100)/2
        assert_eq!(pts.len(), 2);
        assert!((pts[0].value - 70.0).abs() < 1e-9);
        assert!((pts[1].value - 95.0).abs() < 1e-9);
    }

    #[test]
    fn aggregation_count_returns_point_counts() {
        let tmp = TempDir::new().unwrap();
        let mut store = open_store(&tmp);
        let tags = Tags::new();
        // Anchor at a deterministic minute boundary so 5 points spaced
        // 10s apart all fall in the same 1-minute bucket. Using
        // `Utc::now()` made this flaky — points sometimes straddled
        // a minute boundary depending on when the test ran.
        let base = DateTime::from_timestamp(1_714_500_000, 0).unwrap();
        for i in 0..5 {
            store
                .record_at("evt", &tags, 1.0, base + Duration::seconds(i * 10))
                .unwrap();
        }
        let q = Query::new("evt", base - Duration::seconds(1))
            .until(base + Duration::minutes(2))
            .aggregate(Aggregate::Count, Duration::minutes(1));
        let pts = store.query(&q).unwrap();
        // 5 points within a 40-second span starting at a minute
        // boundary, with 1-minute buckets — single bucket of 5.
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].value, 5.0);
    }

    #[test]
    fn list_series_returns_summary_per_metric() {
        let tmp = TempDir::new().unwrap();
        let mut store = open_store(&tmp);
        store
            .record("a", &Tags::new().with("k", "1"), 1.0)
            .unwrap();
        store
            .record("a", &Tags::new().with("k", "2"), 2.0)
            .unwrap();
        store.record("b", &Tags::new(), 3.0).unwrap();

        let mut series = store.list_series().unwrap();
        series.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].name, "a");
        assert_eq!(series[0].point_count, 2);
        assert_eq!(series[0].cardinality, 2);
        assert_eq!(series[1].name, "b");
        assert_eq!(series[1].point_count, 1);
    }

    #[test]
    fn delete_before_removes_old_points() {
        let tmp = TempDir::new().unwrap();
        let mut store = open_store(&tmp);
        let tags = Tags::new();
        let old = Utc::now() - Duration::days(100);
        let new = Utc::now() - Duration::days(1);
        store.record_at("evt", &tags, 1.0, old).unwrap();
        store.record_at("evt", &tags, 2.0, new).unwrap();

        let cutoff = Utc::now() - Duration::days(30);
        let removed = store.delete_before(cutoff).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.total_points().unwrap(), 1);
    }

    #[test]
    fn handle_is_clone_and_thread_safe() {
        let tmp = TempDir::new().unwrap();
        let store = open_store(&tmp);
        let h1 = MetricsHandle::new(store);
        let h2 = h1.clone();
        h1.record("evt", &Tags::new(), 1.0);
        h2.record("evt", &Tags::new(), 2.0);
        assert_eq!(h1.total_points().unwrap(), 2);
    }

    #[test]
    fn canonical_tags_serialize_deterministically() {
        let t1 = Tags::new().with("b", "2").with("a", "1");
        let t2 = Tags::new().with("a", "1").with("b", "2");
        assert_eq!(t1.to_canonical_json(), t2.to_canonical_json());
    }
}
