<!-- topic: cli — bundled in neurogrim-cli v3.5 -->
# CLI surface — what neurogrim can do for you

NeuroGrim ships ~22 top-level commands grouped into four families.
This document is a curated tour. Run `neurogrim --help` for the
authoritative list, or `neurogrim <command> --help` for any one
command's full flag set.

## Family 1: Introspection — "what's here"

| Command | Purpose |
|---------|---------|
| `neurogrim agent` | Full machine-readable JSON envelope (canonical contract for A2A peers + ecosystem aggregation) |
| `neurogrim agent --prose` | Agent-friendly prose orientation summary (v3.2) |
| `neurogrim score` | Single-line unified score + per-domain effective scores |
| `neurogrim health` | Formatted dashboard, human-readable |
| `neurogrim trend` | Trajectory analysis (velocity, acceleration, classification) |
| `neurogrim narrate --hat <name>` | Hat-templated 3–5 line narration |
| `neurogrim doctor` | Configuration auditor — what's misconfigured (v3.2) |
| `neurogrim explain <topic>` | Methodology primer (this command) (v3.2) |
| `neurogrim validate` | Registry shape validation (lighter-weight than `doctor`) |
| `neurogrim awareness` | Local machine-specific facts and notes |

Default starting point for an unfamiliar Brain:
`neurogrim agent --prose && neurogrim doctor`.

## Family 2: Authoring — "make me one of these"

| Command | Purpose |
|---------|---------|
| `neurogrim init --template <kind>` | Bootstrap a new Brain (abstract / code / mixed templates) |
| `neurogrim domain new <name>` | Scaffold a new domain (registry + stub CMDB + optional Python sensor) (v3.2) |
| `neurogrim skill new <name>` | Scaffold a project-specific SKILL.md skeleton |
| `neurogrim federation register --name <peer> --path <path>` | Add a child Brain to local federation |
| `neurogrim federation rewire --child <name>` (v3.5+) | Rewrite parent's `a2a_endpoint` to match the child's persisted port (`ports.json::a2a_port`). Operator-explicit migration tool. Pass `--probe-only` to print the diff without modifying the registry. |

Authoring commands are idempotent and follow consistent UX:
kebab-case validated names, `--force` to overwrite, "next steps"
output pointing at follow-on commands.

## Family 3: Execution — "do the thing"

| Command | Purpose |
|---------|---------|
| `neurogrim sensory <name>` | Run a built-in sensor; emits CMDB JSON to stdout |
| `neurogrim serve` | Start the Brain as an MCP server (default tool-invocation path) |
| `neurogrim a2a-serve` | Start the Brain as an A2A peer |
| `neurogrim a2a-invoke` | Send a single A2A envelope to a peer |
| `neurogrim a2a-discover` | Fetch a peer's Agent Card |
| `neurogrim a2a-token` | Manage A2A bearer tokens (issue / list / revoke) |
| `neurogrim test` (v4.0+) | Quiet test wrapper with persisted failure ledger; mirrors cargo's exit code; supports `--keep-last`, `--show-only-new`, `--retry-failed`, `--slow`, `--verbose` |

Sensor invocation pattern: `neurogrim sensory <name> --project-root . > .claude/<name>-cmdb.json`.
This is how CMDBs are refreshed in CI or pre-commit hooks.

## Family 4: Bookkeeping — "record what happened"

| Command | Purpose |
|---------|---------|
| `neurogrim disposition record` | Log operator judgment of a prior skill invocation (calibration substrate) |
| `neurogrim domain-calibration` | Per-domain calibration ledger (list / triage / manual) |
| `neurogrim federated-pattern emit` | Operator-explicit federated-pattern emission |
| `neurogrim sca-review` | Supply-chain Layer 3 review tickets |
| `neurogrim sca-calibrate` | Supply-chain calibration against the fixture library |

These commands feed the calibration substrate that distinguishes
"the Brain says X" from "X is real" — the empirical layer that
turns advisory signals into weighted ones over time.

## Aliases

Several commands have grimoire-themed visible aliases:
`scry` (score), `divine` (agent), `drift` (trend), `seal` (validate),
`summon` (serve), `cast` (sensory), `conjure` (init), `commune`
(a2a-invoke), `beacon` (a2a-serve), `behold` (a2a-discover).

The canonical names are documented; aliases are for habit and
muscle memory.

## Common flags

- `--registry <path>` — every introspection / scoring command
  takes this; defaults to `.claude/brain-registry.json`
- `--project-root <path>` — sensor commands take this; defaults to `.`
- `--plain` — disables ANSI colors (good for piped output)
- `--hat <name>` — applies hat-bias to scoring + narration

## Ports & service lifecycle (v3.5+)

`neurogrim ui` and `neurogrim a2a-serve` no longer hardcode
ports. On first run in a project, both commands allocate two
ports from the IANA dynamic range (49152-65535), persist them to
`<project>/.claude/brain/ports.json`, and reuse them on every
subsequent invocation. Pass `--port <n>` explicitly to override
without disturbing the persisted allocation (useful for keeping
v3.4-era bookmarks at `:8420` working). When ports.json drifts
from a parent registry's hardcoded child endpoints, run
`neurogrim federation rewire --child <name>` to reconcile.

`neurogrim ui --allow-mutations` (v3.5+) enables a small set of
mutation endpoints — currently service start/stop from the
Federation page. When the flag is off (default) the dashboard
remains read-only and the Start/Stop buttons hide entirely.
Spawned services survive a dashboard restart by design (matches
the "leave running" power-user preference).

## Two invocation modes: MCP and CLI

NeuroGrim exposes its scoring tools two ways:

- **MCP (default)** — `neurogrim serve` exposes seven scoring
  tools to Claude Code as MCP tools. ~983 tokens injected at
  session start (the tool schemas).
- **CLI** — invoke directly via the Bash tool, zero session-start
  overhead. Loads on demand when needed.

The `cli-mode` skill explains when to opt out of MCP for context
efficiency. The MCP mode is the right default for newcomers.

## Cross-references

- `neurogrim explain methodology` — the conceptual model the CLI implements
- `neurogrim explain domain` — what `domain new` creates
- `neurogrim explain federation` — what `federation register` creates
- `.claude/skills/cli-mode/SKILL.md` — when to bypass MCP
- README.md "Quick Start" — the bootstrapping flow
