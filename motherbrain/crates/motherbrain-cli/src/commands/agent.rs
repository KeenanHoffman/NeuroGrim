use anyhow::Result;
use super::context::BrainContext;
use crate::output::json;

pub async fn run(registry_path: &str, hat: Option<String>, persona: Option<String>) -> Result<()> {
    let ctx = BrainContext::load(registry_path, hat, persona).await?;
    json::display_agent_json(&ctx.agent_output);
    Ok(())
}
