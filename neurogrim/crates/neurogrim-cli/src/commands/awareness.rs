//! `neurogrim awareness` — manage local machine-specific facts and notes.
//!
//! The awareness store (`.claude/brain/local-awareness.json`) is gitignored and
//! machine-local. It persists facts that agents discover about the local environment:
//! tool paths not on PATH, OS quirks, known behavioral patterns.
//!
//! # Examples
//!
//! ```bash
//! # Add a fact
//! neurogrim awareness add --key tool_paths.cargo \
//!   --value "C:\Users\...\.rustup\toolchains\stable\bin\cargo.exe" \
//!   --category tool_paths \
//!   --note "Not on PATH in bash; invoke via full path"
//!
//! # Add a free-form note
//! neurogrim awareness note "cargo test exits 1 on Windows due to file lock warning — tests actually pass" \
//!   --category patterns
//!
//! # List everything
//! neurogrim awareness
//!
//! # Get a specific fact
//! neurogrim awareness get tool_paths.cargo
//! ```

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use neurogrim_core::awareness::{AwarenessCategory, LocalAwareness};
use std::path::Path;
use std::str::FromStr;

/// Sub-commands for `neurogrim awareness`.
#[derive(Subcommand, Debug)]
pub enum AwarenessCmd {
    /// Add or update a key-value fact
    Add {
        /// Dot-namespaced key, e.g. tool_paths.cargo
        #[arg(long)]
        key: String,
        /// The fact's value
        #[arg(long)]
        value: String,
        /// Category: tool_paths, environment, patterns, constraints, general
        #[arg(long)]
        category: Option<String>,
        /// Optional human-readable context explaining why this fact matters
        #[arg(long)]
        note: Option<String>,
    },
    /// Add a free-form note about a local pattern or quirk
    Note {
        /// Note content
        content: String,
        /// Category: tool_paths, environment, patterns, constraints, general
        #[arg(long)]
        category: Option<String>,
    },
    /// Get a specific fact by key
    Get {
        /// The fact key to retrieve
        key: String,
    },
}

/// Path to the awareness file relative to project root.
pub const AWARENESS_FILE: &str = ".claude/brain/local-awareness.json";

/// Load the awareness store from disk, or return empty if the file doesn't exist yet.
async fn load(project_root: &Path) -> Result<LocalAwareness> {
    let path = project_root.join(AWARENESS_FILE);
    if !path.exists() {
        return Ok(LocalAwareness::empty());
    }
    let s = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&s).with_context(|| format!("Failed to parse {}", path.display()))
}

/// Save the awareness store back to disk.
async fn save(project_root: &Path, awareness: &LocalAwareness) -> Result<()> {
    let path = project_root.join(AWARENESS_FILE);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(awareness)?;
    tokio::fs::write(&path, json).await?;
    Ok(())
}

/// Parse a category string, defaulting to General if unrecognised.
fn parse_category(s: Option<&str>) -> AwarenessCategory {
    s.and_then(|v| AwarenessCategory::from_str(v).ok())
        .unwrap_or_default()
}

/// Entry point called from `main.rs`.
pub async fn run(project_root: &str, subcommand: Option<AwarenessCmd>) -> Result<()> {
    let root = std::path::PathBuf::from(project_root);
    match subcommand {
        None => cmd_list(&root).await,
        Some(AwarenessCmd::Add {
            key,
            value,
            category,
            note,
        }) => {
            cmd_add(
                &root,
                &key,
                &value,
                parse_category(category.as_deref()),
                note.as_deref(),
            )
            .await
        }
        Some(AwarenessCmd::Note { content, category }) => {
            cmd_note(&root, &content, parse_category(category.as_deref())).await
        }
        Some(AwarenessCmd::Get { key }) => cmd_get(&root, &key).await,
    }
}

async fn cmd_list(root: &Path) -> Result<()> {
    let a = load(root).await?;

    if a.is_empty() {
        println!("{}", "No awareness data yet.".dimmed());
        println!();
        println!("Add facts:  neurogrim awareness add --key KEY --value VALUE");
        println!("Add notes:  neurogrim awareness note \"text\"");
        return Ok(());
    }

    if !a.facts.is_empty() {
        println!("{}", "Facts".bold().underline());
        for f in &a.facts {
            let cat = format!("[{}]", f.category).dimmed();
            println!("  {} {} = {}", cat, f.key.cyan(), f.value);
            if let Some(ref n) = f.note {
                println!("    {}", n.dimmed());
            }
        }
        println!();
    }

    if !a.notes.is_empty() {
        println!("{}", "Notes".bold().underline());
        for n in &a.notes {
            let cat = format!("[{}]", n.category).dimmed();
            let ts = n.discovered_at.format("%Y-%m-%d").to_string().dimmed();
            println!("  {} {} {}", cat, ts, n.content);
        }
        println!();
    }

    if let Some(ts) = a.updated_at {
        println!(
            "{} {}",
            "Last updated:".dimmed(),
            ts.format("%Y-%m-%dT%H:%M:%SZ").to_string().dimmed()
        );
    }

    Ok(())
}

async fn cmd_add(
    root: &Path,
    key: &str,
    value: &str,
    category: AwarenessCategory,
    note: Option<&str>,
) -> Result<()> {
    let mut a = load(root).await?;
    let is_update = a.get_fact(key).is_some();
    a.upsert_fact(key, value, category, note);
    save(root, &a).await?;

    if is_update {
        println!("{} {}", "Updated:".green().bold(), key);
    } else {
        println!("{} {}", "Added:".green().bold(), key);
    }
    println!("  value: {}", value.cyan());
    if let Some(n) = note {
        println!("  note:  {}", n.dimmed());
    }
    Ok(())
}

async fn cmd_note(root: &Path, content: &str, category: AwarenessCategory) -> Result<()> {
    let mut a = load(root).await?;
    a.add_note(content, category);
    save(root, &a).await?;
    println!("{} {}", "Note added:".green().bold(), content.dimmed());
    Ok(())
}

async fn cmd_get(root: &Path, key: &str) -> Result<()> {
    let a = load(root).await?;
    match a.get_fact(key) {
        Some(f) => {
            println!("{}", f.value);
            if let Some(ref n) = f.note {
                eprintln!("# {}", n);
            }
        }
        None => {
            eprintln!("No fact found for key '{}'", key);
            std::process::exit(1);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn temp_root() -> TempDir {
        let dir = TempDir::new().unwrap();
        tokio::fs::create_dir_all(dir.path().join(".claude/brain"))
            .await
            .unwrap();
        dir
    }

    #[tokio::test]
    async fn add_creates_file_if_missing() {
        let dir = TempDir::new().unwrap();
        run(
            dir.path().to_str().unwrap(),
            Some(AwarenessCmd::Add {
                key: "tool_paths.cargo".into(),
                value: "/usr/bin/cargo".into(),
                category: None,
                note: None,
            }),
        )
        .await
        .unwrap();
        let path = dir.path().join(AWARENESS_FILE);
        assert!(path.exists(), "awareness file should be created");
        let a: LocalAwareness =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(a.facts[0].key, "tool_paths.cargo");
    }

    #[tokio::test]
    async fn add_appends_multiple_facts() {
        let dir = TempDir::new().unwrap();
        for (k, v) in [("key_a", "val_a"), ("key_b", "val_b")] {
            run(
                dir.path().to_str().unwrap(),
                Some(AwarenessCmd::Add {
                    key: k.into(),
                    value: v.into(),
                    category: None,
                    note: None,
                }),
            )
            .await
            .unwrap();
        }
        let a = load(dir.path()).await.unwrap();
        assert_eq!(a.facts.len(), 2);
    }

    #[tokio::test]
    async fn add_updates_existing_fact() {
        let dir = TempDir::new().unwrap();
        run(
            dir.path().to_str().unwrap(),
            Some(AwarenessCmd::Add {
                key: "cargo_path".into(),
                value: "/old".into(),
                category: None,
                note: None,
            }),
        )
        .await
        .unwrap();
        run(
            dir.path().to_str().unwrap(),
            Some(AwarenessCmd::Add {
                key: "cargo_path".into(),
                value: "/new".into(),
                category: None,
                note: None,
            }),
        )
        .await
        .unwrap();
        let a = load(dir.path()).await.unwrap();
        assert_eq!(a.facts.len(), 1, "should not duplicate");
        assert_eq!(a.facts[0].value, "/new");
    }

    #[tokio::test]
    async fn get_returns_correct_value() {
        let dir = TempDir::new().unwrap();
        run(
            dir.path().to_str().unwrap(),
            Some(AwarenessCmd::Add {
                key: "my_key".into(),
                value: "my_value".into(),
                category: None,
                note: None,
            }),
        )
        .await
        .unwrap();
        // Get internally: just check the stored value
        let a = load(dir.path()).await.unwrap();
        assert_eq!(a.get_fact("my_key").unwrap().value, "my_value");
    }

    #[tokio::test]
    async fn note_adds_to_notes_not_facts() {
        let dir = temp_root().await;
        run(
            dir.path().to_str().unwrap(),
            Some(AwarenessCmd::Note {
                content: "cargo exits 1 on windows".into(),
                category: Some("patterns".into()),
            }),
        )
        .await
        .unwrap();
        let a = load(dir.path()).await.unwrap();
        assert!(a.facts.is_empty());
        assert_eq!(a.notes.len(), 1);
        assert_eq!(
            a.notes[0].category,
            neurogrim_core::awareness::AwarenessCategory::Patterns
        );
    }

    #[tokio::test]
    async fn list_on_empty_store_does_not_error() {
        let dir = TempDir::new().unwrap();
        run(dir.path().to_str().unwrap(), None).await.unwrap();
    }
}
