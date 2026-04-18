use super::context::{append_score_history, BrainContext};
use crate::output::display;
use anyhow::Result;

pub async fn run(
    registry_path: &str,
    plain: bool,
    hat: Option<String>,
    persona: Option<String>,
) -> Result<()> {
    let ctx = BrainContext::load(registry_path, hat, persona).await?;

    if let Some(ref p) = ctx.agent_output.current_persona {
        crate::output::persona::display_persona(&ctx.agent_output, p, plain);
    } else {
        display::display_score(&ctx.agent_output, plain);
    }

    // Record this invocation in the score-history ledger — feeds
    // trajectory intelligence (spec §7, principle #12). Best-effort;
    // a history-write failure must not break `score`.
    append_score_history(
        &ctx.project_root,
        &ctx.agent_output,
        ctx.registry.config.trajectory.retention_days,
    )
    .await;

    Ok(())
}
