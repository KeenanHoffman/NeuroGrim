# CLI Mode — Power-User Alternative to MCP

## What This Is

An opt-in way to use NeuroGrim from a Claude Code session **without**
registering NeuroGrim as an MCP server. The agent invokes the Brain
via Bash calls to the existing `neurogrim` CLI subcommands
(`score`, `trend`, `health`, `validate`, `sensory`, `awareness`)
instead of through MCP tool calls.

**Why:** each MCP server registered in a Claude Code session injects
its tool schemas into the model's system prompt at session start.
For NeuroGrim's `BrainServer` (seven scoring tools), this typically
consumes thousands of tokens of context before any real work happens.
For small local sessions where the operator already knows the tool
surface, that overhead is pure cost.

**This is a deliberate opt-in for power users.** The default is MCP
because MCP provides uniform tool discovery, schema validation, and
LLM-friendly error shapes. Read `docs/cli-sensory-surface.md` before
switching — if you haven't, CLI mode gives you fewer guardrails.

## What You Trade

| Property | MCP mode (default) | CLI mode |
|---|---|---|
| Tool discovery | Auto via `list_tools()` | You memorize / read the docs |
| Schema validation | rmcp enforces `JsonSchema` on args | Clap enforces CLI arg shapes; less rich |
| Error shape | Uniform typed MCP errors | Bash exit codes + stderr text |
| Context cost at session start | ~2–5k tokens (7 BrainServer tools) | ~0 tokens (no schemas injected) |
| Per-call latency | MCP JSON-RPC round-trip | subprocess spawn + parse |

## How to Configure

Claude Code reads MCP server registrations from `.mcp.json` in the
project root or `~/.claude/` (user level). To enable CLI mode, simply
**omit NeuroGrim's MCP server** from whichever `.mcp.json` governs
your session.

### MCP mode (default) — `.mcp.json`

```json
{
  "mcpServers": {
    "neurogrim": {
      "command": "neurogrim",
      "args": ["serve", "-r", ".claude/brain-registry.json"]
    }
  }
}
```

With this config, Claude Code launches `neurogrim serve` as a
subprocess and injects the seven BrainServer tool schemas
(`get_health_score`, `get_trajectory`, `get_recommendations`,
`refresh_sensory`, `validate_registry`, `get_local_awareness`,
`record_subagent_outcome`) into the session context.

### CLI mode — `.mcp.json` without the `neurogrim` entry

```json
{
  "mcpServers": {}
}
```

or omit the file entirely. NeuroGrim is then invoked via Bash
calls the agent makes to the CLI. See
`docs/cli-sensory-surface.md` for the full mapping from each
BrainServer tool to its CLI equivalent.

## How the Agent Uses It

Load the [cli-mode skill](../.claude/skills/cli-mode.md) at session
start so the agent knows to reach for Bash instead of MCP tools.
The skill cites the same CLI surface doc and enumerates the
tradeoffs from the agent's perspective.

Typical session shape under CLI mode:

```
Agent: (needs health score)
Agent → Bash: neurogrim score --plain
Bash → Agent: <JSON or human-readable output>
```

vs. MCP mode:

```
Agent: (needs health score)
Agent → MCP: get_health_score({hat: "..."})
MCP → Agent: <typed JSON>
```

## When to Choose Which

**Choose MCP (default) when:**
- New to the Brain — you want discoverable tools and typed errors.
- Running non-trivial multi-turn sessions where schema validation
  catches mistakes early.
- Sharing a session with collaborators who may not know the CLI
  surface by heart.
- Not operating under tight context-token pressure.

**Choose CLI when:**
- You know the CLI surface — this is not your first session.
- Session context pressure is real (small window, long conversation
  history, or you want to leave maximum headroom for task content).
- You are benchmarking the Brain's own overhead (see
  `roadmap/data/b09-bench-*.json` for reference numbers).

## Benchmark

See `roadmap/data/b09-bench-<date>.json` for the measured token
delta between MCP and CLI modes in a reference session. Run the
benchmark yourself with:

```bash
cd neurogrim
cargo test -p neurogrim-cli --test context_overhead -- --nocapture
```

## Related Reading

- `docs/cli-sensory-surface.md` — full BrainServer tool → CLI
  subcommand mapping.
- `.claude/skills/cli-mode.md` — agent-facing skill that routes
  Bash calls instead of MCP tool calls.
- `README.md` § Command aliases — grimoire-themed aliases for
  every primary command (e.g., `scry`, `divine`, `drift`).
