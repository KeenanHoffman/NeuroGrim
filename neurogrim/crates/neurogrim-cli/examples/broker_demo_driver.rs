//! End-to-end demo driver — spawns `neurogrim broker-serve` as a child
//! process + drives it via rmcp's MCP client over stdio.
//!
//! Used by the BROKER-HARNESS-DEMO.md procedure to exercise the harness
//! without needing a live Claude Code session (Claude Code IS the canonical
//! MCP client; this driver is a substitute for demo purposes).
//!
//! Run: `cargo run -p neurogrim-cli --example broker_demo_driver -- <cluster.toml>`

use anyhow::Result;
use rmcp::model::CallToolRequestParam;
use rmcp::service::ServiceExt;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use std::env;
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let cluster = args.get(1).map(String::as_str).unwrap_or(
        ".claude/brain/broker/cluster.toml",
    );

    println!("==> spawning broker-serve as child process");
    println!("    cluster: {}\n", cluster);

    // Find the neurogrim binary. Try release first, fall back to debug.
    let bin = locate_neurogrim_binary()?;
    println!("    binary: {}\n", bin);

    let cluster_arg = cluster.to_string();
    let transport = TokioChildProcess::new(Command::new(&bin).configure(|cmd| {
        cmd.args(["broker-serve", "--cluster", &cluster_arg]);
    }))?;

    // serve() spawns the rmcp client + does the initialize handshake
    let service = ().serve(transport).await?;
    let peer_info = service.peer_info();
    println!(
        "==> connected; server reports: {}\n",
        serde_json::to_string_pretty(&peer_info).unwrap_or_default()
    );

    // List tools
    let tools = service.list_tools(Default::default()).await?;
    println!("==> tools/list returned {} tool(s):", tools.tools.len());
    for t in &tools.tools {
        println!(
            "    - {} ({})",
            t.name,
            t.description
                .as_deref()
                .unwrap_or("(no description)")
                .chars()
                .take(80)
                .collect::<String>()
        );
    }
    println!();

    // Dispatch dispatch-work-unit for B-100 (expected: leaf-op fails because
    // the demo Work Broker has empty BacklogState — that's the success criterion
    // per BROKER-HARNESS-DEMO.md step 6: "round-trip completed + structured
    // outcome reported")
    println!("==> dispatching work-broker/dispatch-work-unit with work_unit_id=B-100");
    let result = service
        .call_tool(CallToolRequestParam {
            name: "dispatch_pipeline".into(),
            arguments: serde_json::json!({
                "broker_id": "work-broker",
                "pipeline_id": "work-broker/dispatch-work-unit",
                "params": {"work_unit_id": "B-100"}
            })
            .as_object()
            .cloned(),
        })
        .await?;
    println!("==> dispatch result:");
    println!("{}\n", serde_json::to_string_pretty(&result)?);

    // Dispatch arm-kill-switch (Surfaced governance pipeline)
    println!("==> dispatching work-broker/arm-kill-switch");
    let result = service
        .call_tool(CallToolRequestParam {
            name: "dispatch_pipeline".into(),
            arguments: serde_json::json!({
                "broker_id": "work-broker",
                "pipeline_id": "work-broker/arm-kill-switch",
                "params": {}
            })
            .as_object()
            .cloned(),
        })
        .await?;
    println!("==> arm-kill-switch result:");
    println!("{}\n", serde_json::to_string_pretty(&result)?);

    // After arming, retry dispatch-work-unit — should be refused with
    // GovernanceRefused (kill switch enforcement)
    println!("==> retry dispatch-work-unit after arming kill switch (should refuse)");
    let result = service
        .call_tool(CallToolRequestParam {
            name: "dispatch_pipeline".into(),
            arguments: serde_json::json!({
                "broker_id": "work-broker",
                "pipeline_id": "work-broker/dispatch-work-unit",
                "params": {"work_unit_id": "B-100"}
            })
            .as_object()
            .cloned(),
        })
        .await?;
    println!("==> dispatch-after-kill-switch result:");
    println!("{}\n", serde_json::to_string_pretty(&result)?);

    println!("==> demo complete; cancelling service");
    service.cancel().await?;
    Ok(())
}

fn locate_neurogrim_binary() -> Result<String> {
    // Try release first
    let release = "D:/Brains/NeuroGrim/neurogrim/target/release/neurogrim.exe";
    let debug = "D:/Brains/NeuroGrim/neurogrim/target/debug/neurogrim.exe";
    if std::path::Path::new(release).exists() {
        return Ok(release.to_string());
    }
    if std::path::Path::new(debug).exists() {
        return Ok(debug.to_string());
    }
    Err(anyhow::anyhow!(
        "neurogrim binary not found; run `cargo build --release -p neurogrim-cli`"
    ))
}
