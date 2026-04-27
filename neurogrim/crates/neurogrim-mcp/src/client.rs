//! MCP client — discovers and invokes sensory tool servers.

use chrono::{DateTime, Utc};
use neurogrim_core::registry::SensoryServerConfig;
use neurogrim_core::scoring::CmdbData;
use rmcp::model::*;
use rmcp::transport::TokioChildProcess;
use rmcp::{ClientHandler, ServiceExt};
use std::collections::HashMap;
use tokio::process::Command;

/// MCP client handler for sensory invocations.
#[derive(Clone)]
pub struct SensoryClient;

impl ClientHandler for SensoryClient {}

/// Result of invoking a sensory server.
#[derive(Debug)]
pub struct SensoryResult {
    pub domain: String,
    pub cmdb_data: CmdbData,
    pub raw_json: serde_json::Value,
}

/// Invoke all configured sensory servers and collect CMDB data.
pub async fn invoke_sensory_servers(
    servers: &HashMap<String, SensoryServerConfig>,
    project_root: &str,
) -> Vec<SensoryResult> {
    let mut results = Vec::new();
    for (name, config) in servers {
        match invoke_single_server(name, config, project_root).await {
            Ok(mut r) => results.append(&mut r),
            Err(e) => tracing::warn!("Sensory server '{}' failed: {}", name, e),
        }
    }
    results
}

async fn invoke_single_server(
    name: &str,
    config: &SensoryServerConfig,
    project_root: &str,
) -> anyhow::Result<Vec<SensoryResult>> {
    let command = config
        .command
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("No command for sensory server '{}'", name))?;

    tracing::info!("Connecting to sensory server: {}", name);

    let parts: Vec<&str> = command.split_whitespace().collect();
    let (program, cmd_args) = parts
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("Empty command for '{}'", name))?;

    let mut cmd = Command::new(program);
    for a in cmd_args {
        cmd.arg(a);
    }
    for a in &config.args {
        cmd.arg(a);
    }
    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    let transport = TokioChildProcess::new(cmd)?;
    let client = SensoryClient;
    let service = client.serve(transport).await?;
    let peer = service.peer().clone();

    let tools_resp = peer.list_tools(None).await?;
    tracing::info!("Server '{}' offers {} tools", name, tools_resp.tools.len());

    let mut results = Vec::new();
    for tool in &tools_resp.tools {
        if !tool.name.starts_with("check_") {
            continue;
        }
        let domain = tool
            .name
            .strip_prefix("check_")
            .unwrap_or(&tool.name)
            .replace('_', "-");

        let call = CallToolRequestParam {
            name: tool.name.clone(),
            arguments: Some(
                serde_json::json!({"project_root": project_root})
                    .as_object()
                    .expect("json!({...}) with object literal always produces Value::Object")
                    .clone(),
            ),
        };

        match peer.call_tool(call).await {
            Ok(result) => {
                for content in &result.content {
                    if let Some(text) = content.as_text() {
                        if let Ok(cmdb) = parse_cmdb_response(&text.text) {
                            results.push(SensoryResult {
                                domain: domain.clone(),
                                cmdb_data: cmdb,
                                raw_json: serde_json::from_str(&text.text).unwrap_or_default(),
                            });
                        }
                    }
                }
            }
            Err(e) => tracing::warn!("Tool {} failed: {}", tool.name, e),
        }
    }

    Ok(results)
}

fn parse_cmdb_response(json_str: &str) -> anyhow::Result<CmdbData> {
    let cmdb: serde_json::Value = serde_json::from_str(json_str)?;
    let score = cmdb
        .get("score")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing score"))?;
    let ts_str = cmdb
        .get("updated_at")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing updated_at"))?;
    let ts: DateTime<Utc> = ts_str.parse()?;
    // Optional envelope-supplied confidence (E-B2-1, spec §3.8). When
    // present, takes precedence over age-decay; when absent, aggregator
    // falls back to exponential_decay(updated_at, ...).
    let confidence = cmdb
        .get("confidence")
        .and_then(|v| v.as_u64())
        .map(|n| n.min(100) as u8);
    Ok(CmdbData {
        score: score.min(100) as u8,
        updated_at: ts,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_cmdb() {
        let json = r#"{"meta":{"schema_version":"1","updated_at":"2026-04-11T10:00:00Z","updated_by":"test"},"score":85,"updated_at":"2026-04-11T10:00:00Z"}"#;
        let result = parse_cmdb_response(json).unwrap();
        assert_eq!(result.score, 85);
    }

    #[test]
    fn parse_cmdb_clamps_score() {
        let json = r#"{"score":150,"updated_at":"2026-04-11T10:00:00Z"}"#;
        assert_eq!(parse_cmdb_response(json).unwrap().score, 100);
    }

    #[test]
    fn parse_cmdb_missing_score_fails() {
        assert!(parse_cmdb_response(r#"{"updated_at":"2026-04-11T10:00:00Z"}"#).is_err());
    }
}
