//! `neurogrim agent --prose` — agent-friendly orientation output (v3.2 Phase A.1).
//!
//! v3.2.1: this module is now a thin wrapper around
//! `neurogrim_mcp::prose::render_prose`, which holds the canonical
//! renderer. The MCP `orient` tool uses the same renderer — single
//! source of truth.

use neurogrim_mcp::context::BrainContext;

/// Render the prose orientation and write it to stdout. `plain=true`
/// suppresses ANSI color escapes (required when stdout is piped).
/// v3.3 F4: `all_domains=true` lists every declared domain instead
/// of capping at the top 3.
pub fn display_prose(ctx: &BrainContext, plain: bool, all_domains: bool) {
    let rendered = neurogrim_mcp::prose::render_prose(
        &ctx.registry,
        &ctx.project_root,
        &ctx.agent_output,
        plain,
        all_domains,
    );
    print!("{}", rendered);
}
