# Skill: CLI Mode — Use Bash Instead of MCP for NeuroGrim Tools

**When to read this:** You are running in a Claude Code session where
NeuroGrim is **NOT** registered as an MCP server in `.mcp.json`
(opt-in CLI mode). In this mode, the seven BrainServer tool schemas
(`get_health_score`, `get_trajectory`, etc.) are not in your context
— you must invoke the Brain via Bash subcommands instead of MCP tool
calls.

**When NOT to read this:** You are running in default MCP mode. Use
the `get_health_score`, `get_trajectory`, etc. MCP tools directly —
this skill is not applicable.

## TL;DR

```bash
# Full health (replaces get_health_score)
neurogrim agent --hat <optional> --human-persona <optional>

# Trajectory (replaces get_trajectory unified)
neurogrim trend --plain

# Recommendations (replaces get_recommendations)
neurogrim agent | jq '.top_recommendations'

# Refresh (replaces refresh_sensory; CLI is always fresh)
neurogrim score

# Validate registry (replaces validate_registry)
neurogrim validate

# Local awareness (replaces get_local_awareness)
neurogrim awareness
```

Full mapping + args + output shapes: `docs/cli-sensory-surface.md`.

## Why CLI Mode

From the 2026-04-22 benchmark
(`roadmap/data/b09-bench-2026-04-22.json`): registering NeuroGrim's
BrainServer in `.mcp.json` injects **~983 tokens** of tool-schema
documentation into every session's system prompt at startup. That's
modest, but for tight sessions or long conversations, it's pure
overhead if you already know the CLI surface. Opting out via CLI
mode eliminates it entirely (100% reduction on this axis).

## What's Lost

- **Typed schema validation.** MCP enforces argument types via the
  `rmcp` + `schemars` pipeline. Bash gets you Clap validation — less
  rich, error messages less uniform.
- **Uniform error shape.** MCP errors are typed JSON. Bash errors
  are stderr text + non-zero exit code. Always read both on failure.
- **Auto-discovery.** MCP's `list_tools()` lets the agent discover
  tools by description. CLI mode assumes the agent knows the surface
  — which is why you read this skill first.

## How to Invoke (the pattern)

For every Brain capability you'd normally call as an MCP tool:

1. Consult `docs/cli-sensory-surface.md` for the exact CLI subcommand.
2. Invoke via Bash.
3. Parse stdout (usually JSON from `neurogrim agent`).
4. On non-zero exit, read stderr for the diagnostic.

Example — "what's my current health score, with the adversary hat?":

```bash
# Instead of:
#   {"tool": "get_health_score", "arguments": {"hat": "adversary"}}
# Run:
neurogrim agent --hat adversary
```

Parse the result's `unified_score` (for the number) or full JSON
(for the breakdown).

## Gaps vs MCP Mode (document once, surface when hit)

1. **`get_trajectory(domain=X)`** — no per-domain CLI today. Parse
   `.domain_trajectories` from `neurogrim agent` output instead.
2. **`record_subagent_outcome`** — MCP-only. In CLI mode, either
   skip subagent-health tracking, or write directly to
   `.claude/brain/subagent-outcomes.jsonl` and let `neurogrim score`
   pick it up on next run. See surface doc §7 for the JSONL shape.

## Related

- `docs/cli-mode.md` — when to choose MCP vs CLI, `.mcp.json`
  examples, benchmark pointer.
- `docs/cli-sensory-surface.md` — full MCP↔CLI mapping with args,
  output shapes, and gaps.
- `README.md` § Command aliases — grimoire-themed aliases for
  every primary command.

## Methodology note

This skill exists because of B-09 (CLI-mode sensory access,
2026-04-22). It is NOT the default posture — MCP mode stays the
recommended mode for newcomers and for sessions where discovery
matters more than token savings.
