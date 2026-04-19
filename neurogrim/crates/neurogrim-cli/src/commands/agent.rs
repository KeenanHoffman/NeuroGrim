use super::context::BrainContext;
use crate::output::json;
use anyhow::Result;

pub async fn run(
    registry_path: &str,
    hat: Option<String>,
    human_persona: Option<String>,
) -> Result<()> {
    let ctx = BrainContext::load(registry_path, hat, human_persona).await?;
    json::display_agent_json(&ctx.agent_output);
    Ok(())
}
