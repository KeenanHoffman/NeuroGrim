//! `neurogrim skill new` — scaffold a project-specific SKILL.md skeleton
//! (v3.1.1 init automation Phase 3).
//!
//! Creates `.claude/skills/<name>/SKILL.md` with the standard frontmatter
//! (name, description, when_to_use) and a body skeleton matching the
//! `write-skill/SKILL.md` authoring standard. The operator fills in the
//! actual content.
//!
//! This standardizes Tier 3 skill authoring (project-specific skills like
//! `resume-prep-protocol`, `application-tracking`, `interview-prep` for
//! the job-hunt B'1 pilot) without forcing operators to copy-paste from
//! `write-skill`.

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args as ClapArgs, Subcommand};
use std::path::PathBuf;
use tokio::fs;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub subcommand: SkillCmd,
}

#[derive(Subcommand, Debug)]
pub enum SkillCmd {
    /// Scaffold a new SKILL.md skeleton for a project-specific skill.
    ///
    /// Creates `.claude/skills/<name>/SKILL.md` with the standard
    /// frontmatter (name, description, when_to_use) and a body skeleton.
    /// The operator fills in the body content following the `write-skill`
    /// authoring standard.
    New {
        /// Skill name (kebab-case). Must match `^[a-z][a-z0-9-]*$`.
        name: String,

        /// Project root containing `.claude/skills/`. Defaults to CWD.
        #[arg(long, default_value = ".")]
        directory: String,

        /// Overwrite an existing SKILL.md if present. Default refuses.
        #[arg(long)]
        force: bool,
    },
}

pub async fn run(args: Args) -> Result<()> {
    match args.subcommand {
        SkillCmd::New {
            name,
            directory,
            force,
        } => cmd_new(&name, &directory, force).await,
    }
}

/// Validate a skill name matches the kebab-case convention used by
/// every existing skill (e.g., `rubber-duck`, `imagination-mode`,
/// `resume-prep-protocol`).
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("skill name cannot be empty");
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_lowercase() {
        bail!(
            "skill name must start with a lowercase letter; got '{name}'. \
             Convention: kebab-case (e.g., 'resume-prep-protocol')."
        );
    }
    for c in name.chars() {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            bail!(
                "skill name must contain only lowercase letters, digits, and \
                 hyphens; got '{name}' (offending char: '{c}'). \
                 Convention: kebab-case."
            );
        }
    }
    if name.contains("--") {
        bail!("skill name must not contain consecutive hyphens; got '{name}'");
    }
    if name.ends_with('-') {
        bail!("skill name must not end with a hyphen; got '{name}'");
    }
    Ok(())
}

async fn cmd_new(name: &str, directory: &str, force: bool) -> Result<()> {
    validate_name(name).with_context(|| format!("invalid skill name '{name}'"))?;

    let root = PathBuf::from(directory);
    if !root.is_dir() {
        bail!(
            "directory '{directory}' is not a directory. Pass --directory \
             <project-root> or run from inside a project."
        );
    }

    let skill_dir = root.join(".claude").join("skills").join(name);
    let skill_md = skill_dir.join("SKILL.md");

    if skill_md.exists() && !force {
        bail!(
            "{} already exists. Pass --force to overwrite, or pick a \
             different name.",
            skill_md.display()
        );
    }

    fs::create_dir_all(&skill_dir)
        .await
        .with_context(|| format!("failed to mkdir {}", skill_dir.display()))?;

    let content = render_skill_skeleton(name);
    fs::write(&skill_md, content)
        .await
        .with_context(|| format!("failed to write {}", skill_md.display()))?;

    eprintln!("Wrote: {}", skill_md.display());
    eprintln!();
    eprintln!("Next steps:");
    eprintln!("  • Fill in the frontmatter (description + when_to_use are routing-critical)");
    eprintln!("  • Author the body following the `write-skill` authoring standard");
    eprintln!("  • The skill is discoverable to Claude Code automatically");
    Ok(())
}

/// Render the SKILL.md skeleton for a new project-specific skill.
/// Mirrors the format used across NeuroGrim's `.claude/skills/`: YAML
/// frontmatter (name + description + when_to_use), then a markdown body
/// with section placeholders the operator fills in.
fn render_skill_skeleton(name: &str) -> String {
    // Humanize the kebab-case name for the H1: "resume-prep-protocol"
    // → "Resume Prep Protocol".
    let display = name
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(ch) => ch.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        r#"---
name: {name}
description: TODO — one-line routing-critical description. State WHAT this skill is for and WHEN to invoke it. Should fit in ~200 chars and contain natural trigger phrases an agent might encounter (e.g., "you are about to X", "you need to Y", "the user is Z").
when_to_use: TODO — natural-language sentences describing the conditions under which an agent should invoke this skill. Include trigger phrases inline so the routing index can match. Pair with `description` — together they're the routing contract.
---

# Skill: {display}

**When to use this skill:** TODO — restate the conditions in narrative form.
Echo the `when_to_use` field's content; the body section is the agent-facing
description, the frontmatter is the index entry.

## The pattern

TODO — describe the methodology this skill captures. What's the discipline?
What's the canonical structure? What does following this skill produce that
ad-hoc work doesn't?

## The discipline

TODO — concrete steps, decision points, conventions. Make it actionable.
A skill that's all philosophy and no procedure isn't a skill.

## What this skill does NOT do

TODO — negative scope. What is this skill NOT for? Helps disambiguate from
adjacent skills. The agent should know when NOT to invoke this.

## Cultural substrate

TODO — how do the five cultural invariants (positivity, integrity, honesty,
critical_but_kind, respect) apply within this skill's operations? Specific
examples beat abstract claims.

## See also

- TODO: link to paired Brain domain (e.g., `.claude/brain-registry.json`
  domain that this skill's discipline pairs with)
- TODO: link to related skills
- TODO: link to relevant LSP Brains spec sections
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn validate_name_accepts_canonical_forms() {
        for n in ["foo", "foo-bar", "foo-bar-baz", "x123", "x-y-z"] {
            validate_name(n).unwrap_or_else(|e| panic!("'{n}' should be valid; got: {e}"));
        }
    }

    #[test]
    fn validate_name_rejects_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn validate_name_rejects_uppercase() {
        assert!(validate_name("Foo").is_err());
        assert!(validate_name("foo-Bar").is_err());
    }

    #[test]
    fn validate_name_rejects_underscores() {
        assert!(validate_name("foo_bar").is_err());
    }

    #[test]
    fn validate_name_rejects_leading_digit() {
        assert!(validate_name("1foo").is_err());
    }

    #[test]
    fn validate_name_rejects_consecutive_hyphens() {
        assert!(validate_name("foo--bar").is_err());
    }

    #[test]
    fn validate_name_rejects_trailing_hyphen() {
        assert!(validate_name("foo-").is_err());
    }

    #[test]
    fn render_skill_skeleton_has_required_frontmatter() {
        let s = render_skill_skeleton("test-skill");
        assert!(s.starts_with("---\n"));
        assert!(s.contains("name: test-skill"));
        assert!(s.contains("description:"));
        assert!(s.contains("when_to_use:"));
        // H1 humanized correctly.
        assert!(s.contains("# Skill: Test Skill"));
        // Has the standard sections.
        assert!(s.contains("## The pattern"));
        assert!(s.contains("## The discipline"));
        assert!(s.contains("## What this skill does NOT do"));
        assert!(s.contains("## Cultural substrate"));
        assert!(s.contains("## See also"));
    }

    #[tokio::test]
    async fn cmd_new_writes_skill_md() {
        let tmp = TempDir::new().unwrap();
        cmd_new("test-skill", tmp.path().to_str().unwrap(), false)
            .await
            .unwrap();
        let path = tmp.path().join(".claude/skills/test-skill/SKILL.md");
        assert!(path.is_file());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("name: test-skill"));
    }

    #[tokio::test]
    async fn cmd_new_refuses_existing_without_force() {
        let tmp = TempDir::new().unwrap();
        cmd_new("test-skill", tmp.path().to_str().unwrap(), false)
            .await
            .unwrap();
        let err = cmd_new("test-skill", tmp.path().to_str().unwrap(), false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn cmd_new_overwrites_with_force() {
        let tmp = TempDir::new().unwrap();
        cmd_new("test-skill", tmp.path().to_str().unwrap(), false)
            .await
            .unwrap();
        // Mutate the file so we can verify --force overwrites.
        let path = tmp.path().join(".claude/skills/test-skill/SKILL.md");
        std::fs::write(&path, "MUTATED").unwrap();
        cmd_new("test-skill", tmp.path().to_str().unwrap(), true)
            .await
            .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("MUTATED"));
        assert!(content.contains("name: test-skill"));
    }
}
