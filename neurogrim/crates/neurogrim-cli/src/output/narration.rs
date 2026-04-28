//! Hat-calibrated narration output (templated, no LLM).
//!
//! v3.1 C0 SCAFFOLD — the design surface and one proof-point template
//! (visionary) are in place; full per-hat template suite + TOML
//! loading + CLI subcommand wiring are tracked under v3.1 C1-C3.
//!
//! ## Architecture
//!
//! 1. Each declared hat (see `.claude/skills/hats/SKILL.md` Per-Hat
//!    Communication Contract table) carries a [`NarrationTemplate`].
//! 2. The template is a vector of lines with `{{ placeholder }}`
//!    fields.
//! 3. The renderer interpolates [`AgentOutput`] fields and prunes
//!    blank-after-substitution lines, so optional sections drop
//!    cleanly when their data is absent.
//!
//! Templates are intentionally simple — string interpolation with
//! field names. No conditionals, no loops, no Turing-completeness;
//! that's the deterministic-templating posture (no LLM in the Brain
//! runtime — see v3.1 charter §3 locked decision 1).
//!
//! ## What v3.1 C0 ships
//!
//! - The [`NarrationTemplate`] struct and [`NarrationTemplate::for_hat`]
//!   lookup.
//! - The [`render`] function, deterministic and side-effect-free.
//! - One proof-point template ([`NarrationTemplate::visionary`]).
//! - The TOML reference at `data/narration-templates/visionary.toml`
//!   shows the future on-disk form. v3.1 C1 will load TOML at runtime
//!   instead of hardcoding here.
//!
//! ## What v3.1 C1-C3 will add
//!
//! - C1: TOML loader (replaces hardcoded `visionary()` with
//!   `from_toml_file()`); adds the `toml` crate to dependencies.
//! - C2: Six more templates (adversary, architect, incident-commander,
//!   rubber-duck, security-auditor, supply-chain-auditor) plus
//!   `source-reader` (subagent-only — likely an explicit no-op).
//! - C3: CLI subcommand `neurogrim narrate --hat <hat>` that pipes
//!   AgentOutput through `render()` and prints. Per-hat snapshot
//!   tests pin structural shape, not verbatim wording.

use neurogrim_core::agent_output::AgentOutput;
use neurogrim_core::types::TrajectoryClassification;

/// A per-hat narration template.
///
/// Lines are rendered in order; blank lines (after substitution) are
/// pruned so optional sections drop cleanly. Placeholders use
/// `{{ name }}` form; supported names are documented at [`render`].
#[derive(Debug, Clone)]
pub struct NarrationTemplate {
    pub hat: &'static str,
    pub schema_version: &'static str,
    pub lines: Vec<&'static str>,
}

impl NarrationTemplate {
    /// Look up the template for a given hat.
    ///
    /// v3.1 C0 ships only the `visionary` template as proof-point.
    /// C2 will populate the remaining seven hats. Returns `None` if
    /// the hat isn't recognized; callers SHOULD fall back to the
    /// existing `super::display::display_health` path.
    pub fn for_hat(hat: &str) -> Option<Self> {
        match hat {
            "visionary" => Some(Self::visionary()),
            // v3.1 C2: adversary, architect, incident-commander,
            // rubber-duck, security-auditor, supply-chain-auditor.
            // source-reader is subagent-only per hats/SKILL.md and
            // likely returns a no-op placeholder rather than prose.
            _ => None,
        }
    }

    /// The visionary-hat narration template (v3.1 C0 proof-point).
    ///
    /// Mirrors the hat's communication contract from
    /// `.claude/skills/hats/SKILL.md` ("Options named. '3 approaches
    /// explored, recommend A.'"), reframed as "options surfaced
    /// from Brain state." Three lines:
    ///
    /// 1. Establishes overall posture (score + trajectory).
    /// 2. Names the highest-leverage signals.
    /// 3. Recommends a direction without prescribing implementation.
    fn visionary() -> Self {
        Self {
            hat: "visionary",
            schema_version: "1",
            lines: vec![
                "Brain at {{ score }}/100, trajectory {{ trajectory }}.",
                "Leverage: {{ top_domain.name }} ({{ top_domain.score }}/100) is the largest drift surface; {{ correlations_fired }} correlation(s) fired.",
                "Direction: explore {{ top_domain.name }} first — broader surface narrows after correlation review.",
            ],
        }
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
    fn visionary_template_loads() {
        let t = NarrationTemplate::for_hat("visionary")
            .expect("visionary template should be available in v3.1 C0");
        assert_eq!(t.hat, "visionary");
        assert_eq!(t.schema_version, "1");
        assert_eq!(t.lines.len(), 3);
    }

    #[test]
    fn unknown_hat_returns_none() {
        assert!(NarrationTemplate::for_hat("nonexistent").is_none());
        // C2-deferred hats also currently return None; once C2 lands,
        // this assertion will need to flip per-hat.
        assert!(NarrationTemplate::for_hat("adversary").is_none());
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
        // field. C2 will expand the supported set per v3.1 charter.
        let result = interpolate("{{ unsupported_field }}", "0", "", "", "", "");
        assert_eq!(result, "{{ unsupported_field }}");
    }
}
