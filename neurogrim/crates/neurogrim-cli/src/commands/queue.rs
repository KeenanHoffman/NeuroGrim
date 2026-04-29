//! `neurogrim queue {list, tail, publish, stats}` — v4.1 S13-B-7
//! (partial v1; `compact`, `migrate`, `inspect` are deferred until
//! the SQLite backend lands in S13-B-3).
//!
//! Thin CLI wrapper around `neurogrim_core::queue` with no
//! dashboard dependency — operators can inspect a Brain's bus state
//! without spinning up the HTTP server. Mirrors the convention from
//! `neurogrim test`, `neurogrim publish-gate`, etc.: the binary
//! resolves a `--project-root` and operates relative to its
//! `.claude/brain/queues/` directory.
//!
//! ## Sub-commands
//!
//! - `queue list` — every topic with a `*.jsonl` file on disk +
//!   per-topic stats (count, size, oldest/newest timestamps).
//! - `queue tail <topic> [-n N]` — print the last N messages on a
//!   topic as pretty JSON. Default N = 20.
//! - `queue publish <topic> <payload-as-json>` — manual produce
//!   (operator-driven flow; agents use the MCP `queue_publish`
//!   tool). Optional `--priority` and `--expires-in-ms`.
//! - `queue stats <topic>` — single-topic stats (same fields as
//!   `list` but for one topic, JSON-printed).

use anyhow::{anyhow, Context, Result};
use clap::{Args as ClapArgs, Subcommand};
use neurogrim_core::queue::{
    self, JsonlQueueReader, Priority, QueueMessage, Topic,
};
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

pub async fn run(args: Args) -> Result<()> {
    match args.command {
        QueueCommand::List(a) => list(a).await,
        QueueCommand::Tail(a) => tail(a).await,
        QueueCommand::Publish(a) => publish(a).await,
        QueueCommand::Stats(a) => stats(a).await,
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
}
