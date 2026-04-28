//! Hat-calibrated narration output (templated, no LLM).
//!
//! v3.1 E-V31-C, C0–C3. Per the v3.1 charter §3 locked decision 1
//! (templated, no LLM): templates are deterministic TOML data
//! files bundled into the binary at compile time via `include_str!`.
//! Operators can author additional templates by extending the
//! bundle (or, in v3.x, via a runtime override path — deferred).
//!
//! ## Architecture
//!
//! 1. Each declared hat (see `.claude/skills/hats/SKILL.md` Per-Hat
//!    Communication Contract table) carries a [`NarrationTemplate`].
//! 2. The template is a vector of lines with `{{ placeholder }}`
//!    fields. The on-disk schema is documented in
//!    `data/narration-templates/visionary.toml`.
//! 3. The renderer interpolates [`AgentOutput`] fields and prunes
//!    blank-after-substitution lines, so optional sections drop
//!    cleanly when their data is absent.
//!
//! Templates are intentionally simple — string interpolation with
//! field names. No conditionals, no loops, no Turing-completeness;
//! that's the deterministic-templating posture.
//!
//! ## C0–C3 progression
//!
//! - **C0** (commit `cb2f443`): scaffold — `NarrationTemplate` struct,
//!   `for_hat()` lookup, `render()`, hardcoded visionary template,
//!   TOML schema reference. 5 unit tests.
//! - **C1** (this commit): replace hardcoded `visionary()` with
//!   compile-time TOML loading via `include_str!`. Adds the `toml`
//!   crate as a dep.
//! - **C2** (this commit): six new templates — adversary, architect,
//!   incident-commander, rubber-duck, security-auditor,
//!   supply-chain-auditor. `source-reader` is intentionally omitted
//!   (subagent-only per `hats/SKILL.md`).
//! - **C3** (this commit): CLI subcommand `neurogrim narrate
//!   --hat <hat>` and per-hat snapshot tests pinning structural
//!   shape (not verbatim wording — those would be brittle).

use neurogrim_core::agent_output::AgentOutput;
use neurogrim_core::types::TrajectoryClassification;
use serde::Deserialize;

// ── Bundled template data (compile-time include_str!) ─────────────────────────

const VISIONARY_TOML: &str = include_str!("../../data/narration-templates/visionary.toml");
const ADVERSARY_TOML: &str = include_str!("../../data/narration-templates/adversary.toml");
const ARCHITECT_TOML: &str = include_str!("../../data/narration-templates/architect.toml");
const INCIDENT_COMMANDER_TOML: &str =
    include_str!("../../data/narration-templates/incident-commander.toml");
const RUBBER_DUCK_TOML: &str = include_str!("../../data/narration-templates/rubber-duck.toml");
const SECURITY_AUDITOR_TOML: &str =
    include_str!("../../data/narration-templates/security-auditor.toml");
const SUPPLY_CHAIN_AUDITOR_TOML: &str =
    include_str!("../../data/narration-templates/supply-chain-auditor.toml");

/// The set of declared hats with narration templates. Used by callers
/// (e.g., the `narrate` CLI subcommand) to enumerate available hats
/// and by tests to iterate over all templates.
///
/// `source-reader` is intentionally absent — that hat is subagent-only
/// per `.claude/skills/hats/SKILL.md` and never produces human-facing
/// narration prose.
pub const SUPPORTED_HATS: &[&str] = &[
    "adversary",
    "architect",
    "incident-commander",
    "rubber-duck",
    "security-auditor",
    "supply-chain-auditor",
    "visionary",
];

// ── Public types ──────────────────────────────────────────────────────────────

/// A per-hat narration template parsed from a TOML data file.
///
/// Lines are rendered in order; blank lines (after substitution) are
/// pruned so optional sections drop cleanly. Placeholders use
/// `{{ name }}` form; supported names are documented at [`render`].
#[derive(Debug, Clone)]
pub struct NarrationTemplate {
    pub hat: String,
    pub schema_version: String,
    pub lines: Vec<String>,
}

// ── On-disk schema (TOML) ────────────────────────────────────────────────────

/// On-disk shape — the file form mirrored by every
/// `data/narration-templates/<hat>.toml`. Not exported; converted to
/// [`NarrationTemplate`] in `for_hat()`.
#[derive(Debug, Deserialize)]
struct TemplateFile {
    meta: TemplateMeta,
    content: TemplateContent,
}

#[derive(Debug, Deserialize)]
struct TemplateMeta {
    hat: String,
    schema_version: String,
}

#[derive(Debug, Deserialize)]
struct TemplateContent {
    lines: Vec<String>,
}

impl NarrationTemplate {
    /// Look up the bundled template for a given hat name. Returns
    /// `None` if the hat isn't recognized — callers SHOULD fall
    /// back to the existing `super::display::display_health` path.
    ///
    /// Templates are bundled at compile time via `include_str!`. A
    /// missing TOML file fails the build; a malformed TOML fails this
    /// function and returns `None` (logged via `log::warn` if a
    /// future revision wires logging in).
    pub fn for_hat(hat: &str) -> Option<Self> {
        let toml_str = match hat {
            "visionary" => VISIONARY_TOML,
            "adversary" => ADVERSARY_TOML,
            "architect" => ARCHITECT_TOML,
            "incident-commander" => INCIDENT_COMMANDER_TOML,
            "rubber-duck" => RUBBER_DUCK_TOML,
            "security-auditor" => SECURITY_AUDITOR_TOML,
            "supply-chain-auditor" => SUPPLY_CHAIN_AUDITOR_TOML,
            _ => return None,
        };
        let parsed: TemplateFile = toml::from_str(toml_str).ok()?;
        // Defense-in-depth: refuse to return a template whose declared
        // `meta.hat` doesn't match the lookup key. Catches authoring
        // mistakes in the bundled TOML (a copy-paste from one template
        // to another that forgot to update the meta block).
        if parsed.meta.hat != hat {
            return None;
        }
        Some(NarrationTemplate {
            hat: parsed.meta.hat,
            schema_version: parsed.meta.schema_version,
            lines: parsed.content.lines,
        })
    }
}

/// Render a template against an [`AgentOutput`], returning the
/// narration as a vector of non-empty lines (empty-after-substitution
/// lines are pruned).
///
/// Supported placeholders:
///
/// - `{{ score }}` — unified score (0-100)
/// - `{{ trajectory }}` — trajectory classification verbiage
///   (e.g., "stable", "degrading"); "unknown" if absent
/// - `{{ top_domain.name }}` — lowest-effective-score weighted domain
///   (advisory domains excluded); "(no weighted domain)" if none
/// - `{{ top_domain.score }}` — that domain's effective score; "—" if none
/// - `{{ correlations_fired }}` — count of fired correlations
pub fn render(template: &NarrationTemplate, output: &AgentOutput) -> Vec<String> {
    let trajectory_str = match output.trajectory.as_ref().map(|t| &t.classification) {
        Some(TrajectoryClassification::Improving) => "improving",
        Some(TrajectoryClassification::Degrading) => "degrading",
        Some(TrajectoryClassification::Stable) => "stable",
        Some(TrajectoryClassification::Volatile) => "volatile",
        Some(TrajectoryClassification::NoData) => "insufficient data",
        None => "unknown",
    };

    let top_domain = output
        .domains
        .iter()
        .filter(|(_, d)| d.weight > 0.0)
        .min_by_key(|(_, d)| d.effective_score);

    let (top_name, top_score) = match top_domain {
        Some((name, d)) => (name.as_str(), d.effective_score.to_string()),
        None => ("(no weighted domain)", "—".to_string()),
    };

    let correlations_fired = output.correlations_fired.len();
    let score_str = output.score.to_string();
    let correlations_str = correlations_fired.to_string();

    template
        .lines
        .iter()
        .map(|line| {
            interpolate(
                line,
                &score_str,
                trajectory_str,
                top_name,
                &top_score,
                &correlations_str,
            )
        })
        .filter(|line| !line.trim().is_empty())
        .collect()
}

fn interpolate(
    line: &str,
    score: &str,
    trajectory: &str,
    top_name: &str,
    top_score: &str,
    correlations_fired: &str,
) -> String {
    line.replace("{{ score }}", score)
        .replace("{{ trajectory }}", trajectory)
        .replace("{{ top_domain.name }}", top_name)
        .replace("{{ top_domain.score }}", top_score)
        .replace("{{ correlations_fired }}", correlations_fired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_supported_hats_load() {
        for hat in SUPPORTED_HATS {
            let t = NarrationTemplate::for_hat(hat)
                .unwrap_or_else(|| panic!("template for hat `{hat}` failed to load"));
            assert_eq!(&t.hat, hat, "template's meta.hat must match lookup key");
            assert_eq!(t.schema_version, "1", "v1 schema for all hats");
            assert!(!t.lines.is_empty(), "template `{hat}` has no lines");
        }
    }

    #[test]
    fn unknown_hat_returns_none() {
        assert!(NarrationTemplate::for_hat("nonexistent").is_none());
        // source-reader is subagent-only — intentionally excluded
        // from SUPPORTED_HATS and not loadable.
        assert!(NarrationTemplate::for_hat("source-reader").is_none());
    }

    #[test]
    fn interpolate_substitutes_all_placeholders() {
        let result = interpolate(
            "Brain at {{ score }}/100, trajectory {{ trajectory }}.",
            "78",
            "stable",
            "test-health",
            "92",
            "1",
        );
        assert_eq!(result, "Brain at 78/100, trajectory stable.");
    }

    #[test]
    fn interpolate_handles_top_domain_fields() {
        let result = interpolate(
            "Leverage: {{ top_domain.name }} ({{ top_domain.score }}/100) — {{ correlations_fired }} fired.",
            "0",
            "",
            "supply-chain-vigilance",
            "62",
            "3",
        );
        assert_eq!(
            result,
            "Leverage: supply-chain-vigilance (62/100) — 3 fired."
        );
    }

    #[test]
    fn interpolate_leaves_unknown_placeholders_intact() {
        // Placeholders not in the supported set pass through unchanged
        // — they're a signal that the template uses an unimplemented
        // field. Future revisions may expand the supported set.
        let result = interpolate("{{ unsupported_field }}", "0", "", "", "", "");
        assert_eq!(result, "{{ unsupported_field }}");
    }

    // ── Per-hat structural snapshot tests (C3) ───────────────────────────────
    //
    // Each snapshot pins the STRUCTURAL shape of a hat's narration —
    // verifying that the hat's distinguishing language appears in its
    // template. These tests catch template-content drift; they do NOT
    // pin verbatim wording (which would break on every micro-edit).

    #[test]
    fn adversary_template_emphasizes_risk() {
        let t = NarrationTemplate::for_hat("adversary").expect("adversary template");
        let text = t.lines.join("\n").to_lowercase();
        assert!(text.contains("risk"), "adversary template must mention risk");
    }

    #[test]
    fn architect_template_mentions_decision_or_tradeoff() {
        let t = NarrationTemplate::for_hat("architect").expect("architect template");
        let text = t.lines.join("\n").to_lowercase();
        assert!(
            text.contains("decision") || text.contains("tradeoff"),
            "architect template must mention decision or tradeoff"
        );
    }

    #[test]
    fn incident_commander_template_mentions_blast_radius_or_stabilize() {
        let t = NarrationTemplate::for_hat("incident-commander")
            .expect("incident-commander template");
        let text = t.lines.join("\n").to_lowercase();
        assert!(
            text.contains("blast radius") || text.contains("stabilize"),
            "incident-commander template must mention blast radius or stabilize"
        );
    }

    #[test]
    fn rubber_duck_template_asks_a_question() {
        let t = NarrationTemplate::for_hat("rubber-duck").expect("rubber-duck template");
        let text = t.lines.join("\n");
        assert!(
            text.contains('?'),
            "rubber-duck template must ask the operator a question"
        );
    }

    #[test]
    fn security_auditor_template_uses_paranoid_framing() {
        let t = NarrationTemplate::for_hat("security-auditor").expect("security-auditor template");
        let text = t.lines.join("\n").to_lowercase();
        assert!(
            text.contains("paranoid") || text.contains("surface area"),
            "security-auditor template must use paranoid framing or surface-area emphasis"
        );
    }

    #[test]
    fn supply_chain_auditor_template_mentions_provenance_or_pin() {
        let t = NarrationTemplate::for_hat("supply-chain-auditor")
            .expect("supply-chain-auditor template");
        let text = t.lines.join("\n").to_lowercase();
        assert!(
            text.contains("provenance") || text.contains("pin"),
            "supply-chain-auditor template must mention provenance or pin-to-last-good"
        );
    }

    #[test]
    fn visionary_template_mentions_direction_or_options() {
        let t = NarrationTemplate::for_hat("visionary").expect("visionary template");
        let text = t.lines.join("\n").to_lowercase();
        assert!(
            text.contains("direction") || text.contains("explore") || text.contains("leverage"),
            "visionary template must mention direction or exploration"
        );
    }

    #[test]
    fn all_templates_use_the_supported_placeholder_set() {
        // Every template should use ONLY placeholders the renderer
        // supports. If a template introduces a new placeholder, this
        // test fails — forcing a coordinated update of `render()` +
        // its rustdoc + this test's whitelist.
        let supported = [
            "{{ score }}",
            "{{ trajectory }}",
            "{{ top_domain.name }}",
            "{{ top_domain.score }}",
            "{{ correlations_fired }}",
        ];
        let pattern = regex_lite_braces();
        for hat in SUPPORTED_HATS {
            let t = NarrationTemplate::for_hat(hat).expect("template loads");
            for line in &t.lines {
                let mut idx = 0;
                while let Some(start) = line[idx..].find("{{") {
                    let abs_start = idx + start;
                    if let Some(end) = line[abs_start..].find("}}") {
                        let abs_end = abs_start + end + 2;
                        let placeholder = &line[abs_start..abs_end];
                        assert!(
                            supported.contains(&placeholder),
                            "hat `{hat}` uses unsupported placeholder `{placeholder}` in line: {line}"
                        );
                        idx = abs_end;
                    } else {
                        // Unbalanced — also a failure.
                        panic!(
                            "hat `{hat}` has unbalanced placeholder braces in line: {line}"
                        );
                    }
                }
                // Use `pattern` so it doesn't get optimized out by the
                // compiler — the regex is a documentation anchor.
                let _ = pattern;
            }
        }
    }

    /// Documentation anchor — the placeholder regex shape this test
    /// approximates with manual scanning. Kept as a function so the
    /// constant doesn't need to depend on a regex crate.
    fn regex_lite_braces() -> &'static str {
        r"\{\{\s*[a-zA-Z._]+\s*\}\}"
    }
}
