<!-- topic: ui — bundled in neurogrim-cli v3.4 -->
# The dashboard — a visual surface for the Brain

NeuroGrim ships a self-contained HTTP + React dashboard alongside
the CLI and MCP server. It's the third audience surface — the one
for humans inspecting the Brain's state at a glance, with charts,
sparklines, and a sortable view of every domain.

The dashboard is **read-only** in v3.4: it shows the Brain's view
of itself but does not mutate registry, CMDBs, or ledgers.
Mutations (sensor refresh, registry edits, ledger pruning) are
gated behind a `--allow-mutations` flag planned for v3.5.

## Run it

```bash
neurogrim ui
```

Defaults to `http://127.0.0.1:8420/` and opens your browser
automatically. Useful flags:

| Flag | Purpose |
|------|---------|
| `--port <N>` | Pick a non-default port (e.g. when 8420 collides with an A2A peer) |
| `--bind <addr>` | Bind address (default `127.0.0.1`; use `0.0.0.0` to expose on the LAN) |
| `--no-browser` | Print the URL but don't try to open a browser tab |
| `--registry <path>` | Point at a non-standard `brain-registry.json` |

Browser launch is best-effort and self-skipping: in CI
(`CI=true`), on Linux without a `DISPLAY`, or in headless SSH
sessions, the dashboard prints the URL and tells you why it
didn't launch (instead of letting `webbrowser` hang). Inside
WSL it routes through `cmd.exe /c start` so the URL opens in
the Windows host browser.

## What you see — the five pages

### Overview (`/`)

The landing page. Shows the Brain's identity, the unified score
(rendered as a gauge), trajectory direction, top three strongest
domain signals, and top three recommendations. The all-advisory
case (every domain is weight 0.0 — the v3.2 stub-domain pattern
or a Brain that hasn't promoted anything yet) renders as
`N/A · observe-only posture` instead of a misleading 0/100.

### Domains (`/domains`)

Sortable table — every declared domain with weight, raw score,
effective score, confidence, trajectory, and last-updated
timestamp. Color-coded: green for high effective scores, amber
for mid, red for low. Click any row to drill into the per-domain
detail page.

### Domain detail (`/domains/<name>`)

The findings table for a single domain (one row per finding from
the most recent sensor run, with status badge + signed point
delta), plus a Recharts sparkline of the recent score history,
plus the CMDB metadata block (path on disk, last-updated). When
the sensor hasn't been written yet, the page surfaces the
authoring intent block (the `_todo_<name>` placeholder set by
`domain new --sensor-intent`).

### Federation (`/federation`)

One-hop view of the Brain's declared peers. Self in the middle,
peers as nodes around it, color-coded by liveness:

- **alive** — peer responded to its `/.well-known/agent-card.json`
  probe within 1.5 s
- **unreachable** — probe timed out or the peer rejected the
  connection
- **unprobed** — subprocess transport (we don't probe those)
- **disabled** — `enabled: false` in the registry

Click any peer for the Agent Card excerpt: id, name, version,
interface version, schema version, declared transport protocol,
topology role + parent.

### Skills (`/skills`)

Inventory of every skill under `.claude/skills/` paired with
invocation-ledger stats (count, last-invoked, alive-window
membership). Filter chips narrow by hygiene status (alive / dead
/ new / no-ledger) and a search box filters by name or
description. Click a row to expand the full description and
exact path.

When `.claude/brain/invocation-ledger.jsonl` doesn't exist
(PostToolUse hook not yet wired), the page shows a banner
explaining how to set up the ledger and classifies every skill
as `no-ledger`.

## Live updates — SSE under the hood

The dashboard subscribes to `/api/events` (Server-Sent Events)
on first load. Filesystem changes to:

- `.claude/brain-registry.json` → all queries refetch
- `.claude/<name>-cmdb.json` → score-aware queries refetch
- `.claude/brain/score-history.json` → score-aware queries refetch
- `.claude/brain/invocation-ledger.jsonl` → skills query refetches

…produce events on the wire within ~250 ms, and TanStack Query
invalidates only the relevant query keys. A small dot in the
sidebar footer shows connection status (live / connecting /
offline / disabled).

If the filesystem watcher couldn't start, the page falls back
to polling and the dot shows `static` — pages refresh on tab
focus or manual reload.

## The hat lens

A dropdown in the sidebar lists every hat declared in
`config.hats` plus a synthetic `default` entry. Selecting a hat
adds `?hat=<name>` to every score-aware request, so the Brain
output is filtered through that hat's `domain_multipliers`. The
selection persists in `localStorage` under `neurogrim:hat`.

When the registry has no hats declared, the picker collapses to
a static "no hats" label.

## Theme

The dashboard ships with both a dark and light palette, toggled
via the sun/moon button in the sidebar footer. The selection
persists in `localStorage` under `neurogrim:theme` and falls
back to the OS-level preference on first load.

## When to use which surface

| Audience / task | Surface |
|-----------------|---------|
| Agent reading the canonical contract (programmatic) | `neurogrim agent` (CLI) or MCP `agent` tool |
| Operator quickly checking score / trend in a terminal | `neurogrim health` |
| Human exploring "what's here" with charts | The dashboard |
| Operator drilling into one finding's history | The dashboard's Domains → detail flow |
| Author of a new domain wanting to scaffold it | `neurogrim domain new <name>` (CLI) |
| CI gate that blocks PRs on advisory floor | `neurogrim score --json` (CLI) |
| Live "what is the Brain doing right now" view | The dashboard with SSE |

The dashboard is not a replacement for the CLI or MCP
surfaces — it's a complement aimed at humans who want
diagrammatic state. The CLI remains the canonical contract; the
dashboard reads from the same code paths but renders for eyes.

## Architecture

- **Server**: an embedded `axum` HTTP server in the
  `neurogrim-dashboard` crate. The frontend (Vite + React) is
  built into `frontend/dist/` and bundled at compile time via
  `rust-embed`. Users `cargo install neurogrim-cli` and the
  dashboard ships with it — no Node.js required at runtime.
- **API surface**: `/api/health`, `/api/overview`,
  `/api/domains`, `/api/domains/:name`, `/api/federation`,
  `/api/skills`, `/api/hats`, `/api/events` (SSE).
- **TS bindings**: every wire-format type derives `ts_rs::TS`
  and exports to `crates/neurogrim-dashboard/bindings/` at
  `cargo test` time. The frontend imports these via
  `@bindings/<TypeName>`. CI fails the build if the generated
  bindings drift from what's committed.
- **Routing**: TanStack Router (typed routes) with five top-
  level paths plus the `/domains/$name` detail.

## Cross-references

- `neurogrim explain cli` — the canonical contract surface
- `neurogrim explain methodology` — what the dashboard renders
- `neurogrim explain federation` — what the Federation page
  visualizes
- `neurogrim explain hat` — what the hat lens applies
- `crates/neurogrim-dashboard/README.md` — implementation notes
  for contributors
