//! `neurogrim narrate` — hat-calibrated narration of Brain output
//! (v3.1 E-V31-C C3).
//!
//! Loads the AgentOutput via the standard `BrainContext` flow,
//! looks up the requested hat's narration template, renders, and
//! prints. Templates are deterministic, no LLM dependency
//! (charter §3 locked decision 1).
//!
//! Per the per-hat communication contract documented in
//! `.claude/skills/hats/SKILL.md`, each hat narrates the same
//! AgentOutput data through a different lens: adversary emphasizes
//! risk, architect emphasizes decision points, incident-commander
//! emphasizes blast radius, etc. The data is the same; the
//! framing differs.

use super::context::BrainContext;
use crate::output::narration::{render, NarrationTemplate, SUPPORTED_HATS};
use anyhow::{anyhow, Result};

pub async fn run(registry_path: &str, hat: String) -> Result<()> {
    // Load the template before loading the Brain — fail fast on
    // unknown hats so operators don't pay the score-computation
    // cost for a typo'd `--hat`.
    let template = NarrationTemplate::for_hat(&hat).ok_or_else(|| {
        anyhow!(
            "no narration template for hat `{hat}`. Supported hats: {SUPPORTED_HATS:?}. \
             (`source-reader` is intentionally subagent-only; pilot agents don't narrate \
             through it.)"
        )
    })?;

    // Load AgentOutput via the standard registry flow. The hat is
    // also passed to BrainContext so domain-emphasis (registry hat
    // §5.4) takes effect during scoring — narrating with the
    // adversary hat gets adversary-weighted scores.
    let ctx = BrainContext::load(registry_path, Some(hat.clone()), None).await?;

    // Render and print. `render` returns Vec<String>; one line
    // per non-empty rendered template line.
    let lines = render(&template, &ctx.agent_output);
    for line in lines {
        println!("{line}");
    }

    Ok(())
}
