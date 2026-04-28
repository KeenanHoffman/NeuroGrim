//! Init template scaffolding (v3.1.1 init automation; companion to
//! `commands/init.rs`).
//!
//! When `neurogrim init --template <name>` is invoked, this module:
//!
//! 1. Loads the bundled template manifest (compile-time include_str!).
//! 2. Constructs the domain set (manifest defaults + --domains overrides).
//! 3. Materializes all Tier-1+2 artifacts:
//!    - `.claude/culture.yaml` (byte-identical canonical)
//!    - `.claude/<domain>-cmdb.json` stub per declared domain
//!    - `.claude/skills/<each>/` from bundled general-purpose skills
//!    - `.claude/settings.local.json` (gitignored hook config)
//!    - `scripts/record-skill-invocation.sh`
//!    - `CLAUDE.md` from template (with placeholder substitution)
//!    - `.gitignore` extension (idempotent via marker comment)
//!
//! The companion `init.rs::run` handles the existing registry generation;
//! this module's `scaffold_full` is called AFTER the registry write to
//! materialize the rest.
//!
//! All bundled content is compile-time `include_str!`. No runtime
//! filesystem dependencies — works from a single binary distribution.

use anyhow::{anyhow, bail, Context, Result};
// chrono no longer used here directly — stub_cmdb_json delegates to
// neurogrim_mcp::domain::stub_cmdb_json which holds the timestamp logic.
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use tokio::fs;

// ── Bundled template manifests (TOML) ─────────────────────────────────────────

const ABSTRACT_PROJECT_MANIFEST: &str =
    include_str!("../../data/init-templates/abstract-project/manifest.toml");
const ABSTRACT_PROJECT_CLAUDE_TMPL: &str =
    include_str!("../../data/init-templates/abstract-project/CLAUDE.md.tmpl");
const ABSTRACT_PROJECT_README: &str =
    include_str!("../../data/init-templates/abstract-project/README-snippet.md");
const ABSTRACT_PROJECT_GITIGNORE: &str =
    include_str!("../../data/init-templates/abstract-project/gitignore-snippet");

const CODE_PROJECT_MANIFEST: &str =
    include_str!("../../data/init-templates/code-project/manifest.toml");
const CODE_PROJECT_CLAUDE_TMPL: &str =
    include_str!("../../data/init-templates/code-project/CLAUDE.md.tmpl");
const CODE_PROJECT_README: &str =
    include_str!("../../data/init-templates/code-project/README-snippet.md");
const CODE_PROJECT_GITIGNORE: &str =
    include_str!("../../data/init-templates/code-project/gitignore-snippet");

const MIXED_MANIFEST: &str = include_str!("../../data/init-templates/mixed/manifest.toml");
const MIXED_CLAUDE_TMPL: &str = include_str!("../../data/init-templates/mixed/CLAUDE.md.tmpl");
const MIXED_README: &str = include_str!("../../data/init-templates/mixed/README-snippet.md");
const MIXED_GITIGNORE: &str = include_str!("../../data/init-templates/mixed/gitignore-snippet");

// ── Bundled universal artifacts ───────────────────────────────────────────────

const BUNDLED_CULTURE_YAML: &str = include_str!("../../data/init-culture.yaml");
const BUNDLED_HOOK_SCRIPT: &str = include_str!("../../data/init-hook-script.sh");

// ── Bundled skills (general-purpose; copied verbatim) ─────────────────────────

const HATS_SKILL: &str = include_str!("../../data/init-skills/hats/SKILL.md");
const HATS_ADVERSARY: &str = include_str!("../../data/init-skills/hats/adversary.md");
const HATS_ARCHITECT: &str = include_str!("../../data/init-skills/hats/architect.md");
const HATS_INCIDENT_COMMANDER: &str =
    include_str!("../../data/init-skills/hats/incident-commander.md");
const HATS_RUBBER_DUCK_FILE: &str = include_str!("../../data/init-skills/hats/rubber-duck.md");
const HATS_SECURITY_AUDITOR: &str = include_str!("../../data/init-skills/hats/security-auditor.md");
const HATS_SOURCE_READER: &str = include_str!("../../data/init-skills/hats/source-reader.md");
const HATS_SUPPLY_CHAIN_AUDITOR: &str =
    include_str!("../../data/init-skills/hats/supply-chain-auditor.md");
const HATS_VISIONARY: &str = include_str!("../../data/init-skills/hats/visionary.md");

const IMAGINATION_MODE_SKILL: &str =
    include_str!("../../data/init-skills/imagination-mode/SKILL.md");
const NORTH_STAR_SKILL: &str = include_str!("../../data/init-skills/north-star/SKILL.md");
const RUBBER_DUCK_SKILL: &str = include_str!("../../data/init-skills/rubber-duck/SKILL.md");
const HUMAN_COMMS_SKILL: &str = include_str!("../../data/init-skills/human-comms/SKILL.md");
const WRITE_SKILL_SKILL: &str = include_str!("../../data/init-skills/write-skill/SKILL.md");
const NEUROGRIM_ONBOARDING_SKILL: &str =
    include_str!("../../data/init-skills/neurogrim-onboarding/SKILL.md");
const CLI_MODE_SKILL: &str = include_str!("../../data/init-skills/cli-mode/SKILL.md");

/// Bundled file: relative path within `.claude/skills/<skill-name>/` and
/// its content. Used by `materialize_skills` to write out all files for
/// a copied skill.
struct BundledSkillFile {
    relative_path: &'static str,
    content: &'static str,
}

/// Lookup table: skill name → list of bundled files for that skill.
/// Multi-file skills (hats) carry their full file set.
fn bundled_skill_files(name: &str) -> Option<&'static [BundledSkillFile]> {
    static HATS: &[BundledSkillFile] = &[
        BundledSkillFile { relative_path: "SKILL.md", content: HATS_SKILL },
        BundledSkillFile { relative_path: "adversary.md", content: HATS_ADVERSARY },
        BundledSkillFile { relative_path: "architect.md", content: HATS_ARCHITECT },
        BundledSkillFile { relative_path: "incident-commander.md", content: HATS_INCIDENT_COMMANDER },
        BundledSkillFile { relative_path: "rubber-duck.md", content: HATS_RUBBER_DUCK_FILE },
        BundledSkillFile { relative_path: "security-auditor.md", content: HATS_SECURITY_AUDITOR },
        BundledSkillFile { relative_path: "source-reader.md", content: HATS_SOURCE_READER },
        BundledSkillFile { relative_path: "supply-chain-auditor.md", content: HATS_SUPPLY_CHAIN_AUDITOR },
        BundledSkillFile { relative_path: "visionary.md", content: HATS_VISIONARY },
    ];
    static IMAGINATION_MODE: &[BundledSkillFile] = &[
        BundledSkillFile { relative_path: "SKILL.md", content: IMAGINATION_MODE_SKILL },
    ];
    static NORTH_STAR: &[BundledSkillFile] = &[
        BundledSkillFile { relative_path: "SKILL.md", content: NORTH_STAR_SKILL },
    ];
    static RUBBER_DUCK: &[BundledSkillFile] = &[
        BundledSkillFile { relative_path: "SKILL.md", content: RUBBER_DUCK_SKILL },
    ];
    static HUMAN_COMMS: &[BundledSkillFile] = &[
        BundledSkillFile { relative_path: "SKILL.md", content: HUMAN_COMMS_SKILL },
    ];
    static WRITE_SKILL: &[BundledSkillFile] = &[
        BundledSkillFile { relative_path: "SKILL.md", content: WRITE_SKILL_SKILL },
    ];
    static NEUROGRIM_ONBOARDING: &[BundledSkillFile] = &[
        BundledSkillFile { relative_path: "SKILL.md", content: NEUROGRIM_ONBOARDING_SKILL },
    ];
    static CLI_MODE: &[BundledSkillFile] = &[
        BundledSkillFile { relative_path: "SKILL.md", content: CLI_MODE_SKILL },
    ];
    match name {
        "hats" => Some(HATS),
        "imagination-mode" => Some(IMAGINATION_MODE),
        "north-star" => Some(NORTH_STAR),
        "rubber-duck" => Some(RUBBER_DUCK),
        "human-comms" => Some(HUMAN_COMMS),
        "write-skill" => Some(WRITE_SKILL),
        "neurogrim-onboarding" => Some(NEUROGRIM_ONBOARDING),
        "cli-mode" => Some(CLI_MODE),
        _ => None,
    }
}

// ── Manifest schema (deserialized from bundled TOML) ──────────────────────────

#[derive(Debug, Deserialize)]
pub struct TemplateManifest {
    pub meta: ManifestMeta,
    pub domains: ManifestDomains,
    pub skills: ManifestSkills,
}

#[derive(Debug, Deserialize)]
pub struct ManifestMeta {
    pub template_name: String,
    pub schema_version: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct ManifestDomains {
    pub default: Vec<String>,
    #[serde(default)]
    pub abstract_examples: Vec<String>,
    #[serde(default)]
    pub advisory_defaults: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestSkills {
    pub copy: Vec<String>,
}

/// Load the bundled manifest for a named template.
pub fn load_template(name: &str) -> Result<TemplateManifest> {
    let toml_str = match name {
        "abstract-project" => ABSTRACT_PROJECT_MANIFEST,
        "code-project" => CODE_PROJECT_MANIFEST,
        "mixed" => MIXED_MANIFEST,
        _ => bail!(
            "Unknown template '{name}'. Supported: abstract-project, code-project, mixed."
        ),
    };
    toml::from_str::<TemplateManifest>(toml_str)
        .with_context(|| format!("failed to parse bundled template manifest for '{name}'"))
}

/// Look up the CLAUDE.md template body for a named template.
fn claude_template(name: &str) -> Result<&'static str> {
    Ok(match name {
        "abstract-project" => ABSTRACT_PROJECT_CLAUDE_TMPL,
        "code-project" => CODE_PROJECT_CLAUDE_TMPL,
        "mixed" => MIXED_CLAUDE_TMPL,
        _ => return Err(anyhow!("no CLAUDE.md template bundled for '{name}'")),
    })
}

/// Look up the README append snippet for a named template.
#[allow(dead_code)]
fn readme_snippet(name: &str) -> Result<&'static str> {
    Ok(match name {
        "abstract-project" => ABSTRACT_PROJECT_README,
        "code-project" => CODE_PROJECT_README,
        "mixed" => MIXED_README,
        _ => return Err(anyhow!("no README snippet bundled for '{name}'")),
    })
}

/// Look up the .gitignore append snippet for a named template.
fn gitignore_snippet(name: &str) -> Result<&'static str> {
    Ok(match name {
        "abstract-project" => ABSTRACT_PROJECT_GITIGNORE,
        "code-project" => CODE_PROJECT_GITIGNORE,
        "mixed" => MIXED_GITIGNORE,
        _ => return Err(anyhow!("no gitignore snippet bundled for '{name}'")),
    })
}

// ── Scaffolding API ──────────────────────────────────────────────────────────

/// Configuration passed from `init::run` to drive the scaffolding pass.
pub struct ScaffoldConfig {
    /// Project root (the directory containing `.claude/`, `scripts/`, etc.).
    pub project_root: std::path::PathBuf,
    /// Project name (substituted into CLAUDE.md placeholders).
    pub project_name: String,
    /// Template name (looked up via `load_template`).
    pub template_name: String,
    /// Final domain set (manifest defaults + operator additions). One stub
    /// CMDB is created per name.
    pub domains: Vec<String>,
    /// Skills to copy from the bundled set. Names must exist in
    /// `bundled_skill_files`.
    pub skills: Vec<String>,
    /// When `false`, skip culture.yaml, hook script, and settings.local.json.
    /// Useful for standalone-no-federation use cases.
    pub include_culture: bool,
    /// When `false`, skip skills directory entirely.
    pub include_skills: bool,
    /// When `false`, skip hook script + settings.local.json.
    pub include_hooks: bool,
}

/// Materialize all template-driven artifacts. Called by `init::run`
/// AFTER the registry has been written.
pub async fn scaffold_full(cfg: &ScaffoldConfig) -> Result<()> {
    let template = load_template(&cfg.template_name)?;
    let claude_dir = cfg.project_root.join(".claude");
    fs::create_dir_all(&claude_dir).await?;

    // 1. culture.yaml
    if cfg.include_culture {
        let culture_path = claude_dir.join("culture.yaml");
        write_idempotent(&culture_path, BUNDLED_CULTURE_YAML).await?;
        eprintln!("Wrote: .claude/culture.yaml");
    }

    // 2. Stub CMDBs (one per declared domain)
    for domain in &cfg.domains {
        let cmdb_path = claude_dir.join(format!("{domain}-cmdb.json"));
        let cmdb_content = stub_cmdb_json(domain)?;
        write_idempotent(&cmdb_path, &cmdb_content).await?;
        eprintln!("Wrote: .claude/{domain}-cmdb.json");
    }

    // 3. Skills
    if cfg.include_skills {
        let skills_dir = claude_dir.join("skills");
        for skill in &cfg.skills {
            let bundled = bundled_skill_files(skill).ok_or_else(|| {
                anyhow!(
                    "skill '{skill}' is not in the bundled set. \
                     Bundled skills: hats, imagination-mode, north-star, \
                     rubber-duck, human-comms, write-skill, neurogrim-onboarding, \
                     cli-mode."
                )
            })?;
            let skill_dir = skills_dir.join(skill);
            fs::create_dir_all(&skill_dir).await?;
            for file in bundled {
                let target = skill_dir.join(file.relative_path);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).await?;
                }
                write_idempotent(&target, file.content).await?;
            }
            eprintln!("Wrote: .claude/skills/{skill}/");
        }
    }

    // 4. Hook script + settings.local.json
    if cfg.include_hooks {
        let scripts_dir = cfg.project_root.join("scripts");
        fs::create_dir_all(&scripts_dir).await?;
        let hook_path = scripts_dir.join("record-skill-invocation.sh");
        write_idempotent(&hook_path, BUNDLED_HOOK_SCRIPT).await?;
        // Best-effort: make executable on Unix. On Windows the bit
        // doesn't apply; on Unix the operator can still chmod manually
        // if this set_permissions call fails.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&hook_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&hook_path, perms);
            }
        }
        eprintln!("Wrote: scripts/record-skill-invocation.sh");

        let settings_path = claude_dir.join("settings.local.json");
        let settings_content = settings_local_json();
        write_idempotent(&settings_path, &settings_content).await?;
        eprintln!("Wrote: .claude/settings.local.json (gitignored)");
    }

    // 5. CLAUDE.md
    let claude_path = cfg.project_root.join("CLAUDE.md");
    let claude_content = render_claude_md(&template, cfg)?;
    write_idempotent(&claude_path, &claude_content).await?;
    eprintln!("Wrote: CLAUDE.md");

    // 6. .gitignore extension (idempotent via marker comment)
    let gitignore_path = cfg.project_root.join(".gitignore");
    append_gitignore_idempotent(&gitignore_path, &cfg.template_name).await?;
    eprintln!("Updated: .gitignore (LSP Brains runtime artifacts)");

    Ok(())
}

/// Idempotent file write — only writes if the file doesn't exist OR has
/// matching content. Refuses (returns Err) when content differs;
/// `init::run` already enforces `--force` at the top level.
async fn write_idempotent(path: &Path, content: &str) -> Result<()> {
    if let Ok(existing) = fs::read_to_string(path).await {
        if existing == content {
            return Ok(());
        }
        bail!(
            "refusing to overwrite {} (content differs from bundled). \
             Re-run with --force to overwrite, or remove the file manually.",
            path.display()
        );
    }
    fs::write(path, content).await.with_context(|| {
        format!("failed to write {}", path.display())
    })?;
    Ok(())
}

/// Build a template-aware brain-registry.json content for the given
/// project name + template + final domain set. All domains ship advisory
/// weight 0.0 with `domain_definitions` pointing at the stub CMDB paths.
///
/// This replaces the legacy `init::generate_registry` call when
/// `--template` is passed — the legacy generator hardcodes code-quality
/// + test-health + deploy-readiness weighted, which doesn't match
/// abstract-project / mixed templates.
pub fn template_registry_json(
    project_name: &str,
    template_name: &str,
    domains: &[String],
    description_override: Option<&str>,
    domain_descriptions: &std::collections::HashMap<String, String>,
) -> Result<String> {
    let mut domain_weights = serde_json::Map::new();
    let mut advisory_domains = Vec::new();
    let mut principle_map = serde_json::Map::new();
    let mut domain_definitions = serde_json::Map::new();

    for d in domains {
        domain_weights.insert(d.clone(), json!(0.0));
        advisory_domains.push(json!(d.clone()));
        // Generate a humanized display name for the principle map.
        let display = d
            .split('-')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        principle_map.insert(d.clone(), json!(display));

        // v3.3 F10: include _todo_<domain> with the operator-supplied
        // description string when present. The `_<key>` underscore-prefix
        // convention is filtered out of `domain_definitions` deserialization
        // (registry.rs), so it stays as documentation without affecting
        // scoring or schema validation.
        let mut def = serde_json::Map::new();
        def.insert(
            "scoring_source".to_string(),
            json!({
                "type": "cmdb",
                "path": format!(".claude/{d}-cmdb.json")
            }),
        );
        if let Some(desc) = domain_descriptions.get(d) {
            def.insert(format!("_todo_{d}"), json!(desc));
        }
        domain_definitions.insert(d.clone(), Value::Object(def));
    }

    // v3.3 F8: prefer operator-supplied --description; fall back to the
    // generic init-template framing when absent.
    let meta_description = match description_override {
        Some(d) if !d.trim().is_empty() => d.to_string(),
        _ => format!(
            "{project_name} Brain — initialized via `neurogrim init --template {template_name}` (v3.1.1+). All declared domains advisory weight 0.0; sensors deferred. CMDBs at score 50 are honest 'unknown' per spec principle #2."
        ),
    };

    let registry = json!({
        "meta": {
            "schema_version": "2.1",
            "description": meta_description,
            "updated_by": "neurogrim-init",
            "project": project_name,
            "note": "Self-contained: works standalone without an LSP Brains ecosystem adjacent."
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": serde_json::Value::Object(domain_weights),
            "advisory_domains": advisory_domains,
            "principle_map": serde_json::Value::Object(principle_map),
            "domain_definitions": serde_json::Value::Object(domain_definitions),
            "scoring": {
                "model": "multiplier",
                "floor_confidence_threshold": 30,
                "floor_score_ceiling": 30
            },
            "gate_tiers": {
                "advisory": {
                    "scoring_weight": 1.0,
                    "priority_weight": 1.0,
                    "description": "All gates advisory at v1; templates may add tiers as projects mature."
                }
            },
            "confidence_thresholds": {
                "cmdb_fresh_days": 1.0,
                "cmdb_stale_days": 7.0,
                "cmdb_very_stale_days": 30.0
            },
            "autonomy": {
                "levels": {
                    "auto":    { "requires_approval": false, "description": "Execute without human approval." },
                    "notify":  { "requires_approval": false, "description": "Execute and notify." },
                    "approve": { "requires_approval": true,  "description": "Require explicit human approval." },
                    "blocked": { "requires_approval": true,  "description": "Hard-blocked; safety invariant." }
                },
                "action_types": {},
                "safety_invariants": []
            },
            "hats": {},
            "correlations": [],
            "incident_patterns": [],
            "sensory_servers": {},
            "children": {}
        }
    });
    Ok(serde_json::to_string_pretty(&registry)? + "\n")
}

/// Build a stub CMDB JSON for a domain. v3.2.1 — re-exported from
/// `neurogrim_mcp::domain` so the same renderer is used by both
/// `neurogrim init --template` and `neurogrim domain new`.
pub(crate) fn stub_cmdb_json(domain: &str) -> Result<String> {
    neurogrim_mcp::domain::stub_cmdb_json(domain)
}

/// Render `settings.local.json` content. Hook config is identical
/// regardless of template — points at the bundled-and-copied script.
fn settings_local_json() -> String {
    serde_json::to_string_pretty(&json!({
        "hooks": {
            "PostToolUse": [{
                "matcher": "Skill",
                "hooks": [{
                    "type": "command",
                    "command": "bash \"$CLAUDE_PROJECT_DIR/scripts/record-skill-invocation.sh\""
                }]
            }]
        }
    }))
    .unwrap_or_default()
        + "\n"
}

/// Render the CLAUDE.md template with placeholder substitution.
fn render_claude_md(_template: &TemplateManifest, cfg: &ScaffoldConfig) -> Result<String> {
    let body = claude_template(&cfg.template_name)?;
    let domain_count = cfg.domains.len();
    let domain_list = cfg
        .domains
        .iter()
        .map(|d| format!("- `{d}`"))
        .collect::<Vec<_>>()
        .join("\n");
    let skills_count = cfg.skills.len();
    let skills_list = cfg
        .skills
        .iter()
        .map(|s| format!("- `{s}/`"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(body
        .replace("{{ project_name }}", &cfg.project_name)
        .replace("{{ domain_count }}", &domain_count.to_string())
        .replace("{{ domain_list }}", &domain_list)
        .replace("{{ skills_count }}", &skills_count.to_string())
        .replace("{{ skills_list }}", &skills_list))
}

/// Append the template's gitignore-snippet to `.gitignore`, skipping if
/// the marker is already present (idempotency).
async fn append_gitignore_idempotent(path: &Path, template_name: &str) -> Result<()> {
    let snippet = gitignore_snippet(template_name)?;
    let existing = fs::read_to_string(path).await.unwrap_or_default();
    let marker = "# LSP Brains runtime artifacts (added by neurogrim init)";
    if existing.contains(marker) {
        return Ok(());
    }
    let mut combined = existing;
    if !combined.ends_with('\n') && !combined.is_empty() {
        combined.push('\n');
    }
    combined.push('\n');
    combined.push_str(snippet);
    fs::write(path, combined).await?;
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn all_three_templates_load() {
        for name in ["abstract-project", "code-project", "mixed"] {
            let m = load_template(name)
                .unwrap_or_else(|e| panic!("load_template({name}) failed: {e}"));
            assert_eq!(m.meta.template_name, name);
            assert_eq!(m.meta.schema_version, "1");
            assert!(!m.skills.copy.is_empty());
        }
    }

    #[test]
    fn unknown_template_errors_clearly() {
        let err = load_template("nonexistent").unwrap_err();
        assert!(err.to_string().contains("Unknown template"));
    }

    #[test]
    fn all_bundled_skills_resolve() {
        for name in ["hats", "imagination-mode", "north-star", "rubber-duck", "human-comms", "write-skill", "neurogrim-onboarding", "cli-mode"] {
            let files = bundled_skill_files(name)
                .unwrap_or_else(|| panic!("bundled_skill_files({name}) returned None"));
            assert!(!files.is_empty(), "skill '{name}' has no bundled files");
            // Every skill must have a SKILL.md as the index file.
            assert!(
                files.iter().any(|f| f.relative_path == "SKILL.md"),
                "skill '{name}' has no SKILL.md"
            );
        }
    }

    #[test]
    fn stub_cmdb_has_required_envelope_fields() {
        let json = stub_cmdb_json("test-domain").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["score"], 50);
        assert!(parsed["meta"]["updated_at"].is_string());
        assert!(parsed["findings"].is_array());
        assert_eq!(parsed["findings"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["findings"][0]["name"], "test-domain:stub");
        assert_eq!(parsed["exported_variables"]["test-domain:low_confidence"], true);
    }

    #[test]
    fn settings_local_json_has_post_tool_use_hook() {
        let s = settings_local_json();
        assert!(s.contains("PostToolUse"));
        assert!(s.contains("record-skill-invocation.sh"));
        assert!(s.contains("CLAUDE_PROJECT_DIR"));
    }

    #[test]
    fn render_claude_md_substitutes_placeholders() {
        let m = load_template("abstract-project").unwrap();
        let cfg = ScaffoldConfig {
            project_root: std::path::PathBuf::from("/tmp/test"),
            project_name: "test-project".into(),
            template_name: "abstract-project".into(),
            domains: vec!["domain-a".into(), "domain-b".into()],
            skills: vec!["hats".into(), "rubber-duck".into()],
            include_culture: true,
            include_skills: true,
            include_hooks: true,
        };
        let rendered = render_claude_md(&m, &cfg).unwrap();
        assert!(rendered.contains("test-project"));
        assert!(rendered.contains("domain-a"));
        assert!(rendered.contains("domain-b"));
        assert!(rendered.contains("`hats/`"));
        assert!(rendered.contains("`rubber-duck/`"));
        // No unsubstituted placeholders remain.
        assert!(!rendered.contains("{{ project_name }}"));
        assert!(!rendered.contains("{{ domain_list }}"));
    }

    #[tokio::test]
    async fn scaffold_full_writes_expected_files() {
        let tmp = TempDir::new().unwrap();
        let cfg = ScaffoldConfig {
            project_root: tmp.path().to_path_buf(),
            project_name: "smoketest".into(),
            template_name: "abstract-project".into(),
            domains: vec!["test-domain".into()],
            skills: vec!["hats".into()],
            include_culture: true,
            include_skills: true,
            include_hooks: true,
        };
        scaffold_full(&cfg).await.unwrap();

        let files = [
            ".claude/culture.yaml",
            ".claude/test-domain-cmdb.json",
            ".claude/skills/hats/SKILL.md",
            ".claude/skills/hats/visionary.md",
            ".claude/settings.local.json",
            "scripts/record-skill-invocation.sh",
            "CLAUDE.md",
            ".gitignore",
        ];
        for f in files {
            assert!(
                tmp.path().join(f).is_file(),
                "expected file {f} to exist after scaffold_full"
            );
        }

        // Idempotency: re-running should succeed (matching content).
        scaffold_full(&cfg).await.unwrap();
    }

    #[tokio::test]
    async fn scaffold_full_refuses_to_overwrite_existing_different_content() {
        let tmp = TempDir::new().unwrap();
        // Pre-create CLAUDE.md with different content.
        std::fs::write(tmp.path().join("CLAUDE.md"), "pre-existing different content").unwrap();
        let cfg = ScaffoldConfig {
            project_root: tmp.path().to_path_buf(),
            project_name: "smoketest".into(),
            template_name: "abstract-project".into(),
            domains: vec!["d1".into()],
            skills: vec![],
            include_culture: false,
            include_skills: false,
            include_hooks: false,
        };
        let err = scaffold_full(&cfg).await.unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
    }
}
