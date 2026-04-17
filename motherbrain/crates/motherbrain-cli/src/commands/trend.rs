use anyhow::Result;
use super::context::BrainContext;
use crate::output::display;

pub async fn run(registry_path: &str, plain: bool) -> Result<()> {
    let ctx = BrainContext::load(registry_path, None, None).await?;
    display::display_trend(&ctx.agent_output, plain);
    Ok(())
}
