use super::context::BrainContext;
use crate::output::{json, prose};
use anyhow::Result;

/// Run the `agent` command.
///
/// v3.2 Phase A.1: when `prose=true`, render an agent-friendly
/// orientation summary instead of the JSON `AgentOutput` envelope. Same
/// upstream data either way; only the rendering target differs. The
/// JSON path remains the canonical machine-readable contract for A2A
/// peers and `neurogrim-ecosystem` aggregation.
pub async fn run(
    registry_path: &str,
    hat: Option<String>,
    human_persona: Option<String>,
    prose_mode: bool,
    plain: bool,
    all_domains: bool,
) -> Result<()> {
    let ctx = BrainContext::load(registry_path, hat, human_persona).await?;
    if prose_mode {
        prose::display_prose(&ctx, plain, all_domains);
    } else {
        json::display_agent_json(&ctx.agent_output);
    }
    Ok(())
}
