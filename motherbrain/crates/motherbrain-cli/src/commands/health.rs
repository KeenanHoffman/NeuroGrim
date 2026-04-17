use super::context::BrainContext;
use crate::output::display;
use anyhow::Result;

pub async fn run(
    registry_path: &str,
    hat: Option<String>,
    persona: Option<String>,
    plain: bool,
) -> Result<()> {
    let ctx = BrainContext::load(registry_path, hat, persona).await?;

    if let Some(ref p) = ctx.agent_output.current_persona {
        crate::output::persona::display_persona(&ctx.agent_output, p, plain);
    } else {
        display::display_health(&ctx.agent_output, plain);
    }

    Ok(())
}
