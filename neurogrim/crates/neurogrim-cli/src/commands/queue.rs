//! `neurogrim queue {list, tail, publish, stats, compact, migrate,
//! inspect}` — v4.1 S13-B-7 + S13-B-3 v2 wiring.
//!
//! Thin CLI wrapper around `neurogrim_core::queue` +
//! `queue_backend` with no dashboard dependency — operators can
//! inspect a Brain's bus state without spinning up the HTTP server.
//! Mirrors the convention from `neurogrim test`, `neurogrim
//! publish-gate`, etc.: the binary resolves a `--project-root` and
//! operates relative to its `.claude/brain/queues/` directory.
//!
//! ## Sub-commands
//!
//! - `queue list` — every topic with a `.jsonl` OR `.sqlite` file
//!   on disk + per-topic stats (count, size, oldest/newest
//!   timestamps).
//! - `queue tail <topic> [-n N]` — print the last N messages on a
//!   topic as pretty JSON. Default N = 20. Honors per-topic backend
//!   choice from `queue-config.yaml` when present.
//! - `queue publish <topic> <payload-as-json>` — manual produce
//!   (operator-driven flow; agents use the MCP `queue_publish`
//!   tool). Optional `--priority` and `--expires-in-ms`.
//! - `queue stats <topic>` — single-topic stats (same fields as
//!   `list` but for one topic, JSON-printed).
//! - `queue compact` — apply retention to a JSONL topic.
//! - `queue migrate <topic> <from> <to>` — convert a topic's
//!   on-disk format. Reads every message from `from`, writes to
//!   `to`, leaves the source file in place (operator deletes after
//!   verifying). v2 of S13-B-3.
//! - `queue inspect <topic>` — read all messages from whichever
//!   backend is on disk + emit them as JSONL on stdout. Same shape
//!   regardless of backend, so `cat` / `tail -f` workflows work
//!   for SQLite topics too. v2 of S13-B-3.

use anyhow::{anyhow, Context, Result};
use clap::{Args as ClapArgs, Subcommand};
use neurogrim_core::queue::{
    self, JsonlQueueReader, Priority, QueueMessage, RetentionPolicy, Topic,
};
use neurogrim_core::queue_backend::{
    built_in_factories, QueueBackend, QueueBackendRegistry,
};
// V5-MOD-3 Phase 3 (2026-05-02): JsonlBackend + SqliteBackend are
// now reached through the registry's factories. The direct
// `SqliteBackend::open` import below is test-only — the integration
// tests at the bottom of this file probe SQLite state directly to
// verify migration outcomes.
#[cfg(test)]
use neurogrim_core::queue_backend::SqliteBackend;
use std::path::{Path, PathBuf};

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub command: QueueCommand,
}

#[derive(Subcommand, Debug)]
pub enum QueueCommand {
    /// List every topic on disk with per-topic stats.
    List(ListArgs),
    /// Print the most recent N messages on a topic.
    Tail(TailArgs),
    /// Manually publish a message to a topic.
    Publish(PublishArgs),
    /// Print per-topic stats as JSON.
    Stats(StatsArgs),
    /// v4.1 S13-B-7 expansion — rotate older entries to the
    /// archive file, drop expired messages. JSONL backend only in
    /// v1; SQLite topics will gain an analogous compact path with
    /// B-3.
    Compact(CompactArgs),
    /// v4.1 S13-B-3 v2 — convert a topic's on-disk format
    /// between JSONL and SQLite. Reads every message from `--from`,
    /// writes to `--to`. Source file is left in place; operator
    /// deletes after verifying.
    Migrate(MigrateArgs),
    /// v4.1 S13-B-3 v2 — read every message from a topic
    /// (whichever backend is on disk) + emit as JSONL on stdout.
    /// Same shape regardless of backend, restoring `cat` / `tail -f`
    /// inspectability for SQLite topics.
    Inspect(InspectArgs),
}

#[derive(ClapArgs, Debug)]
pub struct ListArgs {
    /// Project root containing `.claude/brain/queues/`.
    #[arg(long, default_value = ".")]
    pub project_root: String,
    /// Emit JSON instead of a human-formatted table.
    #[arg(long)]
    pub json: bool,
}

#[derive(ClapArgs, Debug)]
pub struct TailArgs {
    pub topic: String,
    /// Number of messages to print from the tail. Default: 20.
    #[arg(long, short = 'n', default_value_t = 20)]
    pub count: usize,
    #[arg(long, default_value = ".")]
    pub project_root: String,
    /// Print as pretty-printed JSON (one object per line).
    #[arg(long, default_value_t = true)]
    pub json: bool,
}

#[derive(ClapArgs, Debug)]
pub struct PublishArgs {
    pub topic: String,
    /// JSON payload. Use shell quoting; e.g.,
    /// `'{"action": "scan"}'`.
    pub payload: String,
    /// "low" | "normal" | "high". Default: normal.
    #[arg(long)]
    pub priority: Option<String>,
    /// Time-to-live in milliseconds.
    #[arg(long)]
    pub expires_in_ms: Option<u64>,
    #[arg(long, default_value = ".")]
    pub project_root: String,
}

#[derive(ClapArgs, Debug)]
pub struct StatsArgs {
    pub topic: String,
    #[arg(long, default_value = ".")]
    pub project_root: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum BackendChoice {
    Jsonl,
    Sqlite,
}

#[derive(ClapArgs, Debug)]
pub struct MigrateArgs {
    /// Topic to migrate. Both source + destination paths are
    /// derived from `<project>/.claude/brain/queues/<topic>`.
    pub topic: String,
    /// Source backend.
    #[arg(long, value_enum)]
    pub from: BackendChoice,
    /// Destination backend.
    #[arg(long, value_enum)]
    pub to: BackendChoice,
    #[arg(long, default_value = ".")]
    pub project_root: String,
}

#[derive(ClapArgs, Debug)]
pub struct InspectArgs {
    /// Topic to inspect.
    pub topic: String,
    /// Which backend to read from. Default: auto-detect (prefers
    /// SQLite when both files exist; falls back to JSONL).
    #[arg(long, value_enum)]
    pub backend: Option<BackendChoice>,
    /// Max messages to emit. Default: all.
    #[arg(long)]
    pub limit: Option<usize>,
    #[arg(long, default_value = ".")]
    pub project_root: String,
}

#[derive(ClapArgs, Debug)]
pub struct CompactArgs {
    pub topic: String,
    /// Keep messages newer than N days; older entries move to the
    /// archive. Default 30 (the v4.1 epic's refinement #7).
    #[arg(long, default_value_t = 30)]
    pub max_days: u32,
    /// Cap live-file message count at N; oldest excess moves to
    /// the archive. Default 10000 (refinement #7).
    #[arg(long, default_value_t = 10000)]
    pub max_messages: u32,
    /// Disable the days-based retention (keep messages indefinitely
    /// — only the message-count cap fires).
    #[arg(long)]
    pub no_max_days: bool,
    /// Disable the count-based retention (only the days cap fires).
    #[arg(long)]
    pub no_max_messages: bool,
    #[arg(long, default_value = ".")]
    pub project_root: String,
}

pub async fn run(args: Args) -> Result<()> {
    match args.command {
        QueueCommand::List(a) => list(a).await,
        QueueCommand::Tail(a) => tail(a).await,
        QueueCommand::Publish(a) => publish(a).await,
        QueueCommand::Stats(a) => stats(a).await,
        QueueCommand::Compact(a) => compact(a).await,
        QueueCommand::Migrate(a) => migrate(a).await,
        QueueCommand::Inspect(a) => inspect(a).await,
    }
}

// ── Sub-command implementations ───────────────────────────────────────

async fn list(args: ListArgs) -> Result<()> {
    let root = Path::new(&args.project_root);
    let topics = list_topics(root);
    if topics.is_empty() {
        if args.json {
            println!("{}", serde_json::json!({"topics": []}));
        } else {
            eprintln!(
                "✦ no topics on disk under {}",
                root.join(".claude/brain/queues").display()
            );
        }
        return Ok(());
    }
    let stats: Vec<serde_json::Value> = topics
        .iter()
        .map(|t| {
            let path = topic_path(root, t);
            let s = stats_for(t, &path);
            serde_json::to_value(s).unwrap_or(serde_json::Value::Null)
        })
        .collect();
    if args.json {
        println!("{}", serde_json::json!({"topics": stats}));
    } else {
        eprintln!("✦ {} topic(s):", stats.len());
        for s in &stats {
            let topic = s["topic"].as_str().unwrap_or("?");
            let count = s["message_count"].as_u64().unwrap_or(0);
            let bytes = s["size_bytes"].as_u64().unwrap_or(0);
            eprintln!("  {topic:<40}  {count:>6} msgs  {bytes:>10} B");
        }
    }
    Ok(())
}

async fn tail(args: TailArgs) -> Result<()> {
    if !Topic::is_valid(&args.topic) {
        return Err(anyhow!(
            "invalid topic name '{}'; use kebab-case `<scope>/<name>`",
            args.topic
        ));
    }
    let root = Path::new(&args.project_root);
    let path = topic_path(root, &args.topic);
    let reader = JsonlQueueReader::open(&path).with_context(|| {
        format!("failed to read topic at {}", path.display())
    })?;
    if reader.is_empty() {
        eprintln!("✦ topic '{}' is empty (or doesn't exist)", args.topic);
        return Ok(());
    }
    let tail_msgs = reader.tail(args.count);
    for m in tail_msgs {
        if args.json {
            let line = serde_json::to_string(m).unwrap_or_default();
            println!("{line}");
        } else {
            println!(
                "[{}] {} ({:?}) → {}",
                m.produced_at.to_rfc3339(),
                m.id,
                m.priority,
                m.payload
            );
        }
    }
    Ok(())
}

async fn publish(args: PublishArgs) -> Result<()> {
    if !Topic::is_valid(&args.topic) {
        return Err(anyhow!(
            "invalid topic name '{}'; use kebab-case `<scope>/<name>` or system `_neurogrim/<name>`",
            args.topic
        ));
    }
    let payload: serde_json::Value =
        serde_json::from_str(&args.payload).with_context(|| {
            format!(
                "payload is not valid JSON: {}\nshell-quote it like '{{\"k\":\"v\"}}'",
                args.payload
            )
        })?;
    let mut msg = QueueMessage::new(args.topic.clone(), payload);
    if let Some(p) = args.priority {
        match p.as_str() {
            "low" => msg.priority = Priority::Low,
            "normal" => msg.priority = Priority::Normal,
            "high" => msg.priority = Priority::High,
            other => {
                return Err(anyhow!(
                    "invalid priority '{other}'; expected one of low|normal|high"
                ));
            }
        }
    }
    if let Some(ttl_ms) = args.expires_in_ms {
        let when = msg.produced_at + chrono::Duration::milliseconds(ttl_ms as i64);
        msg = msg.with_expires_at(when);
    }
    let root = Path::new(&args.project_root);
    let path = topic_path(root, &args.topic);
    queue::append(&path, &msg)?;
    println!(
        "{}",
        serde_json::json!({
            "id": msg.id.to_string(),
            "topic": msg.topic,
            "produced_at": msg.produced_at.to_rfc3339(),
            "path": path.display().to_string(),
        })
    );
    Ok(())
}

async fn compact(args: CompactArgs) -> Result<()> {
    if !Topic::is_valid(&args.topic) {
        return Err(anyhow!("invalid topic name '{}'", args.topic));
    }
    let root = Path::new(&args.project_root);
    let path = topic_path(root, &args.topic);
    let policy = RetentionPolicy {
        max_days: if args.no_max_days {
            None
        } else {
            Some(args.max_days)
        },
        max_messages: if args.no_max_messages {
            None
        } else {
            Some(args.max_messages)
        },
    };
    let report = queue::compact(&path, policy)
        .with_context(|| format!("compact failed for topic '{}'", args.topic))?;
    println!(
        "{}",
        serde_json::json!({
            "topic": args.topic,
            "topic_path": report.topic_path.display().to_string(),
            "archive_path": report.archive_path.display().to_string(),
            "kept": report.kept,
            "archived": report.archived,
            "dropped_expired": report.dropped_expired,
            "policy": {
                "max_days": policy.max_days,
                "max_messages": policy.max_messages,
            },
        })
    );
    Ok(())
}

async fn stats(args: StatsArgs) -> Result<()> {
    if !Topic::is_valid(&args.topic) {
        return Err(anyhow!(
            "invalid topic name '{}'",
            args.topic
        ));
    }
    let root = Path::new(&args.project_root);
    let path = topic_path(root, &args.topic);
    let s = stats_for(&args.topic, &path);
    println!(
        "{}",
        serde_json::to_string_pretty(&s).unwrap_or_default()
    );
    Ok(())
}

// ── S13-B-3 v2: migrate + inspect ─────────────────────────────────────

async fn migrate(args: MigrateArgs) -> Result<()> {
    if !Topic::is_valid(&args.topic) {
        return Err(anyhow!("invalid topic name '{}'", args.topic));
    }
    if args.from == args.to {
        return Err(anyhow!(
            "migrate: --from and --to are the same backend ({:?}); \
             nothing to do",
            args.from
        ));
    }
    let root = Path::new(&args.project_root);

    // Step 1: open source. Read every message into memory. Topics
    // are bounded by retention so loading the full set is OK; if
    // an operator has 500k+ messages they'll want to compact first.
    let source: std::sync::Arc<dyn QueueBackend> = open_backend(&args.from, root, &args.topic)?;
    let total = source.len()?;
    let messages = source.read_from(0, total as usize)?;
    drop(source); // release any open handles before opening dest.

    // Step 2: open destination + replay. We append rather than
    // batch-insert so the message ids + produced_at timestamps are
    // preserved exactly.
    let dest: std::sync::Arc<dyn QueueBackend> = open_backend(&args.to, root, &args.topic)?;
    // Refuse migration if dest already has data — we don't want to
    // mix topics. The operator should `rm` the dest first if they
    // really mean to overwrite (loud failure beats silent merge).
    let dest_existing = dest.len()?;
    if dest_existing > 0 {
        return Err(anyhow!(
            "migrate: destination ({:?}) already has {} messages; \
             refusing to overwrite — delete the destination file \
             first if this is intentional",
            args.to,
            dest_existing
        ));
    }
    for sm in &messages {
        dest.append(&sm.message)?;
    }

    let source_path = topic_path_for(&args.from, root, &args.topic);
    let dest_path = topic_path_for(&args.to, root, &args.topic);
    println!(
        "{}",
        serde_json::json!({
            "topic": args.topic,
            "migrated": messages.len(),
            "from": format!("{:?}", args.from).to_lowercase(),
            "from_path": source_path.display().to_string(),
            "to": format!("{:?}", args.to).to_lowercase(),
            "to_path": dest_path.display().to_string(),
            "note": "source file left in place; remove after verifying the destination",
        })
    );
    Ok(())
}

async fn inspect(args: InspectArgs) -> Result<()> {
    if !Topic::is_valid(&args.topic) {
        return Err(anyhow!("invalid topic name '{}'", args.topic));
    }
    let root = Path::new(&args.project_root);
    let backend = match args.backend {
        Some(b) => b,
        None => detect_backend(root, &args.topic).ok_or_else(|| {
            anyhow!(
                "queue inspect: no `.jsonl` or `.sqlite` file found for \
                 topic '{}' under {}/.claude/brain/queues/",
                args.topic,
                root.display()
            )
        })?,
    };
    let be = open_backend(&backend, root, &args.topic)?;
    let total = be.len()?;
    let limit = args.limit.unwrap_or(total as usize);
    let messages = be.read_from(0, limit)?;
    for sm in messages {
        let line = serde_json::to_string(&sm.message)
            .context("inspect: serialize message")?;
        println!("{line}");
    }
    Ok(())
}

fn detect_backend(root: &Path, topic: &str) -> Option<BackendChoice> {
    let sqlite = topic_path_for(&BackendChoice::Sqlite, root, topic);
    let jsonl = topic_path_for(&BackendChoice::Jsonl, root, topic);
    if sqlite.exists() {
        Some(BackendChoice::Sqlite)
    } else if jsonl.exists() {
        Some(BackendChoice::Jsonl)
    } else {
        None
    }
}

fn topic_path_for(backend: &BackendChoice, root: &Path, topic: &str) -> PathBuf {
    let mut p = root.join(".claude").join("brain").join("queues");
    for seg in topic.split('/') {
        if !seg.is_empty() {
            p.push(seg);
        }
    }
    let ext = match backend {
        BackendChoice::Jsonl => "jsonl",
        BackendChoice::Sqlite => "sqlite",
    };
    p.set_extension(ext);
    p
}

fn open_backend(
    backend: &BackendChoice,
    root: &Path,
    topic: &str,
) -> Result<std::sync::Arc<dyn QueueBackend>> {
    // V5-MOD-3 Phase 3 (2026-05-02): route through QueueBackendRegistry.
    // The CLI exposes only the closed-set BackendChoice (jsonl|sqlite)
    // because it's the user-facing arg of `neurogrim queue migrate`;
    // third-party backend support at the CLI surface is a v5.5
    // follow-up.
    let mut registry = QueueBackendRegistry::new();
    registry.register_all(built_in_factories());
    let name = match backend {
        BackendChoice::Jsonl => "jsonl",
        BackendChoice::Sqlite => "sqlite",
    };
    let queue_root = root.join(".claude").join("brain").join("queues");
    registry
        .build(name, &queue_root, topic)
        .ok_or_else(|| anyhow::anyhow!("queue: backend {name:?} factory not registered"))?
        .with_context(|| format!("queue: open {name} backend for topic {topic:?}"))
}

// ── Local helpers (pure functions, no I/O dependency on the dashboard) ─

#[derive(Debug, Clone, serde::Serialize)]
struct Stats {
    topic: String,
    message_count: usize,
    size_bytes: u64,
    oldest: Option<String>,
    newest: Option<String>,
    path: String,
}

fn topic_path(project_root: &Path, topic: &str) -> PathBuf {
    let mut p = project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    for seg in topic.split('/') {
        if !seg.is_empty() {
            p.push(seg);
        }
    }
    p.set_extension("jsonl");
    p
}

fn list_topics(project_root: &Path) -> Vec<String> {
    let queues_root = project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    if !queues_root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk(&queues_root, &queues_root, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            let with_ext = rel.to_string_lossy().replace('\\', "/");
            let topic = with_ext.trim_end_matches(".jsonl").to_string();
            if !topic.is_empty() {
                out.push(topic);
            }
        }
    }
}

fn stats_for(topic: &str, path: &Path) -> Stats {
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let reader = JsonlQueueReader::open(path).ok();
    let messages = reader.map(|r| r.into_messages()).unwrap_or_default();
    let oldest = messages.first().map(|m| m.produced_at.to_rfc3339());
    let newest = messages.last().map(|m| m.produced_at.to_rfc3339());
    Stats {
        topic: topic.to_string(),
        message_count: messages.len(),
        size_bytes,
        oldest,
        newest,
        path: path.display().to_string(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn topic_path_resolves_subdirs() {
        let r = Path::new("/proj");
        assert_eq!(
            topic_path(r, "_neurogrim/approvals"),
            Path::new("/proj/.claude/brain/queues/_neurogrim/approvals.jsonl")
        );
    }

    #[test]
    fn list_topics_walks_subdirs() {
        let dir = TempDir::new().unwrap();
        let queues = dir.path().join(".claude/brain/queues");
        std::fs::create_dir_all(queues.join("ng")).unwrap();
        std::fs::write(queues.join("ng/a.jsonl"), "").unwrap();
        std::fs::write(queues.join("ng/b.jsonl"), "").unwrap();
        std::fs::write(queues.join("scratch.jsonl"), "").unwrap();
        let mut topics = list_topics(dir.path());
        topics.sort();
        assert_eq!(
            topics,
            vec![
                "ng/a".to_string(),
                "ng/b".to_string(),
                "scratch".to_string()
            ]
        );
    }

    #[test]
    fn stats_for_empty_topic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("scratch.jsonl");
        std::fs::write(&path, "").unwrap();
        let s = stats_for("scratch", &path);
        assert_eq!(s.message_count, 0);
        assert_eq!(s.oldest, None);
    }

    #[test]
    fn stats_for_populated_topic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("scratch.jsonl");
        for i in 0..3 {
            queue::append(&path, &QueueMessage::new("scratch", json!({"i": i}))).unwrap();
        }
        let s = stats_for("scratch", &path);
        assert_eq!(s.message_count, 3);
        assert!(s.oldest.is_some());
        assert!(s.newest.is_some());
    }

    #[tokio::test]
    async fn publish_writes_to_disk() {
        let dir = TempDir::new().unwrap();
        let args = PublishArgs {
            topic: "ng/test".into(),
            payload: r#"{"k":"v"}"#.into(),
            priority: Some("high".into()),
            expires_in_ms: Some(60_000),
            project_root: dir.path().display().to_string(),
        };
        publish(args).await.unwrap();
        let path = topic_path(dir.path(), "ng/test");
        let r = JsonlQueueReader::open(&path).unwrap();
        assert_eq!(r.len(), 1);
        let messages = r.into_messages();
        assert_eq!(messages[0].priority, Priority::High);
        assert!(messages[0].expires_at.is_some());
    }

    #[tokio::test]
    async fn publish_rejects_bad_topic() {
        let dir = TempDir::new().unwrap();
        let args = PublishArgs {
            topic: "Bad/Name".into(),
            payload: "{}".into(),
            priority: None,
            expires_in_ms: None,
            project_root: dir.path().display().to_string(),
        };
        let res = publish(args).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("invalid topic"));
    }

    #[tokio::test]
    async fn publish_rejects_bad_payload_json() {
        let dir = TempDir::new().unwrap();
        let args = PublishArgs {
            topic: "ng/test".into(),
            payload: "not json".into(),
            priority: None,
            expires_in_ms: None,
            project_root: dir.path().display().to_string(),
        };
        let res = publish(args).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("not valid JSON"));
    }

    #[tokio::test]
    async fn publish_rejects_bad_priority() {
        let dir = TempDir::new().unwrap();
        let args = PublishArgs {
            topic: "ng/test".into(),
            payload: "{}".into(),
            priority: Some("urgent".into()),
            expires_in_ms: None,
            project_root: dir.path().display().to_string(),
        };
        let res = publish(args).await;
        assert!(res.is_err());
    }

    // ── S13-B-3 v2: migrate + inspect tests ────────────────────────

    fn seed_jsonl_topic(root: &Path, topic: &str, count: usize) {
        let path = topic_path_for(&BackendChoice::Jsonl, root, topic);
        for i in 0..count {
            queue::append(
                &path,
                &QueueMessage::new(topic, json!({"i": i})),
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn migrate_jsonl_to_sqlite_round_trip() {
        let dir = TempDir::new().unwrap();
        seed_jsonl_topic(dir.path(), "scratch", 3);
        let args = MigrateArgs {
            topic: "scratch".into(),
            from: BackendChoice::Jsonl,
            to: BackendChoice::Sqlite,
            project_root: dir.path().display().to_string(),
        };
        migrate(args).await.unwrap();
        // Source remains, destination has the data.
        let jsonl = topic_path_for(&BackendChoice::Jsonl, dir.path(), "scratch");
        let sqlite = topic_path_for(&BackendChoice::Sqlite, dir.path(), "scratch");
        assert!(jsonl.exists(), "source should remain after migrate");
        assert!(sqlite.exists(), "dest should be created");
        // SQLite has 3 messages.
        let be = SqliteBackend::open(&sqlite).unwrap();
        assert_eq!(be.len().unwrap(), 3);
    }

    #[tokio::test]
    async fn migrate_sqlite_to_jsonl_round_trip() {
        let dir = TempDir::new().unwrap();
        // Seed via SQLite directly.
        let sqlite = topic_path_for(&BackendChoice::Sqlite, dir.path(), "scratch");
        std::fs::create_dir_all(sqlite.parent().unwrap()).unwrap();
        {
            let be = SqliteBackend::open(&sqlite).unwrap();
            for i in 0..2 {
                be.append(&QueueMessage::new("scratch", json!({"i": i})))
                    .unwrap();
            }
        }
        let args = MigrateArgs {
            topic: "scratch".into(),
            from: BackendChoice::Sqlite,
            to: BackendChoice::Jsonl,
            project_root: dir.path().display().to_string(),
        };
        migrate(args).await.unwrap();
        let jsonl = topic_path_for(&BackendChoice::Jsonl, dir.path(), "scratch");
        let r = JsonlQueueReader::open(&jsonl).unwrap();
        assert_eq!(r.len(), 2);
    }

    #[tokio::test]
    async fn migrate_rejects_same_backend() {
        let dir = TempDir::new().unwrap();
        let args = MigrateArgs {
            topic: "scratch".into(),
            from: BackendChoice::Jsonl,
            to: BackendChoice::Jsonl,
            project_root: dir.path().display().to_string(),
        };
        let err = migrate(args).await.unwrap_err().to_string();
        assert!(err.contains("same backend"));
    }

    #[tokio::test]
    async fn migrate_refuses_to_overwrite_populated_dest() {
        let dir = TempDir::new().unwrap();
        seed_jsonl_topic(dir.path(), "scratch", 2);
        // Pre-populate the destination so the migration would mix.
        let sqlite = topic_path_for(&BackendChoice::Sqlite, dir.path(), "scratch");
        std::fs::create_dir_all(sqlite.parent().unwrap()).unwrap();
        {
            let be = SqliteBackend::open(&sqlite).unwrap();
            be.append(&QueueMessage::new("scratch", json!({"existing": true})))
                .unwrap();
        }
        let args = MigrateArgs {
            topic: "scratch".into(),
            from: BackendChoice::Jsonl,
            to: BackendChoice::Sqlite,
            project_root: dir.path().display().to_string(),
        };
        let err = migrate(args).await.unwrap_err().to_string();
        assert!(err.contains("already has"));
    }

    #[tokio::test]
    async fn migrate_rejects_invalid_topic() {
        let dir = TempDir::new().unwrap();
        let args = MigrateArgs {
            topic: "Bad/Name".into(),
            from: BackendChoice::Jsonl,
            to: BackendChoice::Sqlite,
            project_root: dir.path().display().to_string(),
        };
        let err = migrate(args).await.unwrap_err().to_string();
        assert!(err.contains("invalid topic"));
    }

    #[test]
    fn detect_backend_prefers_sqlite_over_jsonl() {
        let dir = TempDir::new().unwrap();
        let queues = dir.path().join(".claude/brain/queues");
        std::fs::create_dir_all(&queues).unwrap();
        std::fs::write(queues.join("scratch.jsonl"), "").unwrap();
        std::fs::write(queues.join("scratch.sqlite"), b"").unwrap();
        assert_eq!(detect_backend(dir.path(), "scratch"), Some(BackendChoice::Sqlite));
    }

    #[test]
    fn detect_backend_falls_back_to_jsonl() {
        let dir = TempDir::new().unwrap();
        let queues = dir.path().join(".claude/brain/queues");
        std::fs::create_dir_all(&queues).unwrap();
        std::fs::write(queues.join("scratch.jsonl"), "").unwrap();
        assert_eq!(detect_backend(dir.path(), "scratch"), Some(BackendChoice::Jsonl));
    }

    #[test]
    fn detect_backend_returns_none_when_no_file() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_backend(dir.path(), "absent"), None);
    }

    #[tokio::test]
    async fn inspect_rejects_invalid_topic() {
        let dir = TempDir::new().unwrap();
        let args = InspectArgs {
            topic: "Bad/Name".into(),
            backend: None,
            limit: None,
            project_root: dir.path().display().to_string(),
        };
        let err = inspect(args).await.unwrap_err().to_string();
        assert!(err.contains("invalid topic"));
    }

    #[tokio::test]
    async fn inspect_errors_when_no_backend_file_exists() {
        let dir = TempDir::new().unwrap();
        let args = InspectArgs {
            topic: "absent".into(),
            backend: None,
            limit: None,
            project_root: dir.path().display().to_string(),
        };
        let err = inspect(args).await.unwrap_err().to_string();
        assert!(err.contains("no `.jsonl`"));
    }
}
