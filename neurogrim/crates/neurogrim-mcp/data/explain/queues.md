<!-- topic: queues — bundled in neurogrim-cli v3.5 -->
# Queues — the agent coordination bus

The bus is the third pillar of v4.x — alongside the publish-gates
pipeline (v4.0) and encrypted secrets (v4.2). It's the substrate
agents, humans, and dashboards use to coordinate without spawning
ad-hoc IPC mechanisms for every cross-process need.

This topic covers v4.1 S13's foundational stories (B-1, B-2, B-4,
B-7, B-8). The autonomy-enforcement wiring (B-5), the SQLite
opt-in backend (B-3), the Approvals UI (B-6), and the cross-Brain
A2A subscription (B-9) ship in subsequent stories.

<!-- anchor: two-patterns -->
## The two patterns, one primitive

Same underlying append-only JSONL log; two distinct API surfaces:

- **Pattern 1 — event log.** Append-only, fan-out, multi-consumer,
  no acknowledgement. Each consumer tracks its own offset (the bus
  doesn't track who's read what). Topics: `_neurogrim/notifications`,
  `_neurogrim/config-changes`, adopter-defined `pc-state/alerts`.
  Surface: `queue_publish` / `queue_consume` / `queue_peek`.
- **Pattern 2 — request/response coordination.** Synchronous-feeling
  but asynchronous: agent emits → blocks → operator (or another
  agent) responds → agent unblocks. Topics: `_neurogrim/approvals`.
  Surface: `await_approval(action_id) -> ApprovalDecision`. Builds
  on Pattern 1 underneath but the API hides the queue mechanics.
  Lands in S13-B-5.

In v4.1 v1 (this release): Pattern 1 ships. Pattern 2's API surface
is reserved.

<!-- anchor: reserved-namespace -->
## Reserved namespace

- `_neurogrim/<name>` — system topics (approvals, notifications,
  config-changes). Adopters MUST NOT publish into them. The
  `Topic::is_valid_adopter_topic` check in `neurogrim-core::queue`
  enforces this; `neurogrim doctor` will validate against
  `queue-config.yaml` once that schema lands.
- `<scope>/<name>` — adopter-defined topics. Lowercase ASCII
  alphanumeric + `-` + `/`. No leading/trailing separators, no
  consecutive separators, no `:` (reserved per spec).

Examples: `pc-state/alerts`, `release/candidates`, `scratch`.

<!-- anchor: writes -->
## Writes

Three ways to publish:

```bash
# CLI — operator-driven, no dashboard needed
neurogrim queue publish pc-state/alerts '{"severity":"warn","msg":"disk 80%"}'

# HTTP — gated by --allow-mutations
curl -X POST http://127.0.0.1:8420/api/brains/<id>/queues/pc-state/alerts \
  -H 'Content-Type: application/json' \
  -d '{"payload":{"severity":"warn","msg":"disk 80%"}}'

# MCP — agents call this through their tool surface
queue_publish(topic="pc-state/alerts", payload={"severity":"warn"})
```

Optional fields on each surface: `priority` (`low|normal|high`,
default `normal`) and `expires_in_ms` (TTL; default never expires).
Each publish writes one JSONL line to
`<project>/.claude/brain/queues/<topic>.jsonl` (slashes in the
topic become directory levels, preserving `cat` inspectability).

<!-- anchor: reads -->
## Reads

Offset-based; consumers persist their own cursor.

```bash
# CLI — print last N messages
neurogrim queue tail pc-state/alerts -n 50

# Single-topic stats
neurogrim queue stats pc-state/alerts

# Every topic with stats
neurogrim queue list

# HTTP — read from offset
curl 'http://127.0.0.1:8420/api/brains/<id>/queues/pc-state/alerts?since=42&limit=100'

# SSE — live tail (newly-published only; older messages via the read endpoint)
curl -N 'http://127.0.0.1:8420/api/brains/<id>/queues/pc-state/alerts/events'
```

The MCP tools mirror: `queue_consume(topic, since_offset, limit)`
returns `{messages, next_offset}`; `queue_peek(topic, count)`
returns the most recent N without advancing any cursor.

## Storage layout

```
<project>/.claude/brain/queues/
├── _neurogrim/
│   ├── approvals.jsonl
│   └── notifications.jsonl
├── pc-state/
│   └── alerts.jsonl
└── scratch.jsonl
```

Subdirs for slash segments. Adopters can `tail -f` any of these
files directly — the bus is built on top of "everything inspectable
as files," not in spite of it.

<!-- anchor: live-updates -->
## Live updates

Each topic has a per-process broadcast channel (capacity 64).
Subscribers via `GET /api/brains/<id>/queues/<topic>/events`
receive each new message as an SSE `data:` event. Subscribers
joining mid-stream receive only newly-published messages — they
must read the backlog separately via the offset endpoint.

The dashboard's React frontend (S13-B-6 in v4.1) renders an
`approvals-feed` widget that subscribes to `_neurogrim/approvals`.
Operator-driven approvals emit on
`_neurogrim/approval-resolutions` once S13-B-6 ships.

## What's NOT in v4.1 v1 (deferred)

- **SQLite backend** — `ack_required: true` topics with exactly-
  once consumption land in S13-B-3. Default remains JSONL.
- **`compact` / `migrate` / `inspect` CLI sub-commands** — depend
  on the SQLite backend or its migration tooling.
- **Auto-compaction** — daily retention sweep ships in S13-B-7's
  expanded scope.
- **Cross-Brain A2A subscription** — peer Agent Cards advertising
  queue endpoints land in S13-B-9.
- **Hard autonomy gates wired into MCP dispatch** — S13-B-5 is the
  single most important v4.1 change, but it depends on Pattern 2's
  approval round-trip which uses this bus underneath.

<!-- anchor: hot-reload -->
## queue-config.yaml hot-reload (v4.3-era follow-on)

When operators edit `<project>/.claude/brain/queue-config.yaml`
while the dashboard is running, the dashboard's filesystem watcher
detects the change and reloads the bus's in-memory config without
a restart. Topics that should now route to a different backend
get re-evaluated on next access; in-flight uses of the previous
backend handle proceed (the Arc keeps it alive until last release)
— eventual consistency.

The Settings page's Queue config viewer also live-refreshes via
the same SSE event so the displayed YAML reflects the new file
content without a manual reload.

Edge cases:
- **File saved mid-edit with broken YAML** → reload preserves the
  previously-loaded config + logs a parse warning. Operators don't
  lose their working bus while typing.
- **File deleted entirely** → bus reverts to the no-config posture
  (every topic falls back to JSONL on next access).
- **Existing SQLite handles** → survive the reload via Arc clones;
  only the cache slot is dropped, so a topic actively being read
  doesn't see its connection ripped out from under it.

## See also

- `neurogrim explain methodology` — the conceptual model
- `neurogrim explain cli` — full CLI surface
- `neurogrim explain publish-gates` — the v4.0 sibling pipeline
- `roadmap/epics/S13-agent-coordination-bus.md` — story-level plan
- `crates/neurogrim-core/src/queue.rs` — the core primitive
- `crates/neurogrim-dashboard/src/bus.rs` — HTTP + SSE wrapper
