use neurogrim_mcp::context::{append_proposal_ledger, append_score_history, BrainContext};
use crate::output::display;
use anyhow::Result;

pub async fn run(
    registry_path: &str,
    plain: bool,
    hat: Option<String>,
    human_persona: Option<String>,
) -> Result<()> {
    let ctx = BrainContext::load(registry_path, hat, human_persona).await?;

    if let Some(ref p) = ctx.agent_output.current_human_persona {
        crate::output::human_persona::display_human_persona(&ctx.agent_output, p, plain);
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

    // Record recommendations in the proposal ledger — closes the
    // learning loop (principle #4). Linked-list pre_score ties this
    // entry to the previous one's post_score so compute_all_effectiveness
    // can credit last round's recommendations with this round's delta.
    append_proposal_ledger(&ctx.project_root, &ctx.agent_output).await;

    Ok(())
}
