# CLI Sensory Surface — MCP Tool ↔ CLI Subcommand Mapping

This document lets an agent reproduce any NeuroGrim MCP tool call via
a Bash subprocess when running in [CLI mode](../.claude/skills/cli-mode/SKILL.md) — no MCP
tool schemas injected into session context.

**Scope:** the seven `BrainServer` MCP tools exposed by
`neurogrim serve`. The eleven sensory subprocesses
(`git-health`, `code-quality`, etc.) are internal to the Brain's
scoring pipeline; they are NOT registered in Claude Code's MCP
config and therefore are NOT part of this mapping. They remain
invokable directly via `neurogrim sensory <name>` in either mode.

---

## Tool Mapping Table

| MCP tool | CLI equivalent | Parity | Notes |
|---|---|---|---|
| `get_health_score` | `neurogrim agent` | Full | Equivalent JSON shape |
| `get_trajectory` (unified) | `neurogrim trend` | Full | |
| `get_trajectory(domain=X)` | — | **Gap** | No per-domain CLI today; MCP-only |
| `get_recommendations` | `neurogrim agent` + parse | Full | Filter JSON `.top_recommendations` |
| `refresh_sensory` | `neurogrim score` | Full | Re-reads CMDB on each invocation |
| `validate_registry` | `neurogrim validate` | Full | |
| `get_local_awareness` | `neurogrim awareness` | Full | Default is list-all |
| `record_subagent_outcome` | — | **MCP-only** | Writes JSONL + CMDB; no CLI surface |

---

## 1. `get_health_score` → `neurogrim agent`

**MCP call:**
```json
{"tool": "get_health_score",
 "arguments": {"hat": "adversary", "human_persona": "developer"}}
```

**CLI equivalent:**
```bash
neurogrim agent --hat adversary --human-persona developer
```

**Arguments:**
- `--hat <name>` (optional) — one of the hat names defined in
  `brain-registry.json` under `config.hats`. Emphasizes certain
  domains for scoring.
- `--human-persona <name>` (optional) — one of `executive`,
  `manager`, `developer`, `specialist`, `product-manager`.
  Controls the shape of human-facing fields in the output.
- `--registry <path>` (optional) — defaults to
  `.claude/brain-registry.json`.

**Output:** full agent-mode JSON (the complete `agent-output-v1`
envelope per [agent-output-v1.schema.json](../../LSP-Brains/schemas/agent-output-v1.schema.json)).
Same shape the MCP tool emits.

**Exit codes:** 0 on success; non-zero on registry load failure
or scoring error.

**Notes:** `score`, `health`, and `agent` all run the same
scoring pipeline internally (via `BrainContext::load`). The
difference is presentation:
- `neurogrim score` — compact single-number + delta display
- `neurogrim health` — human dashboard with domain breakdown
- `neurogrim agent` — **full JSON** (use this for agent/LLM
  consumption; parity with `get_health_score`)

---

## 2. `get_trajectory` (unified) → `neurogrim trend`

**MCP call:**
```json
{"tool": "get_trajectory", "arguments": {}}
```

**CLI equivalent:**
```bash
neurogrim trend --plain
```

**Arguments:**
- `--plain` (optional) — no ANSI colors; recommended for agent parsing.
- `--registry <path>` (optional).

**Output:** trajectory JSON (velocity, acceleration,
classification) for the unified score. The `--plain` human view
prints a textual summary; for agent consumption, parse
`neurogrim agent` output and read `.trajectory`.

**Exit codes:** 0 on success.

---

## 2b. `get_trajectory(domain=X)` → **Gap**

Per-domain trajectory analysis is available via MCP only today.
There is no `neurogrim trend --domain <X>` flag. Workarounds:

- Parse the full trajectory data from `neurogrim agent` JSON
  and filter by domain client-side (the agent_output carries
  per-domain trajectories under `.domain_trajectories`).
- If per-domain CLI access becomes recurring friction, extend
  the `Trend` subcommand in `main.rs` and `commands/trend.rs` —
  tracked implicitly as a B-09 gap.

---

## 3. `get_recommendations` → `neurogrim agent` + JSON parse

**MCP call:**
```json
{"tool": "get_recommendations", "arguments": {}}
```

**CLI equivalent:**
```bash
neurogrim agent | jq '.top_recommendations'
```

or, without jq:

```bash
neurogrim agent
# then read the .top_recommendations field from the JSON
```

**Arguments:** same as `neurogrim agent`; omit hat/persona for
neutral recommendations.

**Output:** the MCP tool returns the `top_recommendations` array
directly; the CLI returns the full agent-output envelope — pick
the one field out of it.

**Exit codes:** 0 on success.

**Note:** there is no Rust-level shortcut that emits
recommendations alone today. The MCP tool runs full scoring and
returns the `top_recommendations` slice. Since the CLI already
runs full scoring, parsing the slice client-side is functionally
identical — the cost is one JSON field access, not an extra
scoring pass.

---

## 4. `refresh_sensory` → `neurogrim score`

**MCP call:**
```json
{"tool": "refresh_sensory", "arguments": {}}
```

**CLI equivalent:**
```bash
neurogrim score
```

**Arguments:** `--registry`, `--plain`, `--hat`, `--human-persona`
all optional.

**Output:** score display + side-effect of re-reading CMDB from
disk. The CLI's scoring pipeline reads CMDB fresh on every
invocation (no in-memory cache between subprocess calls), so every
CLI call is structurally equivalent to `refresh_sensory`.

**Exit codes:** 0 on success.

**Note:** MCP's `refresh_sensory` explicitly bust an in-memory
cache. The CLI has no persistent cache to bust — every call is
a cold read. Result: CLI mode has `refresh_sensory` semantics by
default.

---

## 5. `validate_registry` → `neurogrim validate`

**MCP call:**
```json
{"tool": "validate_registry", "arguments": {}}
```

**CLI equivalent:**
```bash
neurogrim validate --registry .claude/brain-registry.json
```

**Arguments:**
- `--registry <path>` (optional) — defaults to
  `.claude/brain-registry.json`.

**Output:** human-readable validation report printed to stdout.
Fields include schema version, domain count, weight sum, scoring
model, hats, correlations, incident patterns, human personas,
sensory server count, confidence thresholds, trajectory config,
and a VALID / INVALID verdict.

**Exit codes:** 0 if VALID; 1 if INVALID.

**Note:** MCP emits JSON (`{"valid": bool, "domains": int,
"schema_version": str}`); CLI emits a human report. If an agent
needs JSON in CLI mode, grep for "Result: VALID" on exit code 0,
or consume the `neurogrim agent` output (invalid registry would
have already failed the scoring pipeline).

---

## 6. `get_local_awareness` → `neurogrim awareness`

**MCP call:**
```json
{"tool": "get_local_awareness", "arguments": {}}
```

**CLI equivalent:**
```bash
neurogrim awareness
```

**Arguments:**
- `--project-root <path>` (optional) — defaults to `.`.

**Output:** lists all facts/notes from
`.claude/brain/local-awareness.json`. The MCP tool returns the
full JSON; the CLI's default subcommand pretty-prints the same
data.

**Exit codes:** 0 on success.

**Subcommands (CLI-only extras):**
- `neurogrim awareness add --key <k> --value <v>
  [--category <c>] [--note <n>]` — record a new fact.
- `neurogrim awareness note "<content>" [--category <c>]` —
  record a free-form note.
- `neurogrim awareness get <key>` — fetch one fact by key.

**Note:** the CLI surface here is strictly richer than the MCP
surface. MCP exposes read-only `get_local_awareness`; the CLI
gives read + write.

---

## 7. `record_subagent_outcome` → **MCP-only**

**No CLI equivalent.** This tool writes to
`.claude/brain/subagent-outcomes.jsonl` and recomputes
`.claude/brain/subagent-health-cmdb.json` after every subagent
invocation. It is called by agents after subagent completes,
inside the session's MCP turn — not a natural fit for a CLI
escape hatch.

**Workaround (rarely needed):**
- Direct write to `.claude/brain/subagent-outcomes.jsonl`
  (append one JSON line per outcome; fields: `ts`, `request_id`,
  `capability`, `responsibility`, `required_hat`, `worn_hat`,
  `status`, `envelope_found`, `schema_conformant`,
  `hat_compliant`, `confidence`, `symbol_count`, `retry_count`).
- Re-run `neurogrim score` to trigger CMDB recomputation via
  the `subagent-health` sensory tool's analyzer.

**If you find yourself routinely needing this in CLI mode,** add
a `neurogrim subagent record` CLI subcommand — tracked
implicitly as a B-09 gap.

---

## Error Shape Differences

MCP errors are typed JSON:
```json
{"error": "<message>"}
```

CLI errors are:
- `stderr` text (human-readable diagnostic)
- Non-zero exit code

**Agent handling in CLI mode:** wrap every `neurogrim *` call and
read both `stdout` (expected output) and `stderr` (diagnostic on
failure). Treat non-zero exit as the authoritative failure signal.

---

## Related

- [cli-mode/SKILL.md](../.claude/skills/cli-mode/SKILL.md) — when to choose CLI over MCP,
  `.mcp.json` config patterns, benchmark pointer.
- [.claude/skills/cli-mode/SKILL.md](../.claude/skills/cli-mode/SKILL.md) —
  agent-facing skill routing Bash instead of MCP tool calls.
- [agent-output-v1.schema.json](../../LSP-Brains/schemas/agent-output-v1.schema.json) —
  canonical JSON envelope used by `neurogrim agent`.
