use anyhow::Result;
use neurogrim_core::registry::BrainRegistry;
use neurogrim_mcp::BrainServer;
use rmcp::ServiceExt;
use std::path::Path;

pub async fn run(registry_path: &str) -> Result<()> {
    let json = std::fs::read_to_string(registry_path)?;
    let registry = BrainRegistry::from_json(&json)?;
    registry.validate()?;

    let registry_dir = Path::new(registry_path).parent().unwrap_or(Path::new("."));
    let project_root = registry_dir
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    let server = BrainServer::new(registry, project_root);
    eprintln!("NeuroGrim MCP server starting on stdio...");

    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
