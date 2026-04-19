//! Agent-mode JSON output.

use neurogrim_core::agent_output::AgentOutput;

/// Print the full agent output as pretty JSON.
pub fn display_agent_json(output: &AgentOutput) {
    match serde_json::to_string_pretty(output) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("JSON serialization error: {}", e),
    }
}
