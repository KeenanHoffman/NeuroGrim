# Epic: Agent Coordination Bus + Hard Gates — Stage 13

**Stage:** 13
**Release:** v4.1 — "Coordinate Across Agents"
**Status:** PLANNED (drafted 2026-04-29)
**Priority:** Foundation — closes the autonomy-block enforcement gap; spine for v4.2+ message traffic
**Goal:** Provide a generic, append-only-by-default queue surface (humans + agents + dashboards consume). Wire `resolve_autonomy()` from `crates/neurogrim-core/src/governance.rs:244` into MCP tool dispatch so `Approve`-level actions actually block until operator resolves; `Blocked` actions reject deterministically and cannot be circumvented.

**Depends on:**
- S12 (publishes use the new gates from this point on)
- Existing `resolve_autonomy()` algorithm at `governance.rs:244` (already implemented + tested; never called from dispatch — gap closed in this stage)
- Existing JSONL ledger pattern (reused for the queue writer)

**Blocks:**
- S14 (secret-fetch operations route through the bus's approval flow)
- S15 (UI edits emit on the bus)

**Master roadmap:** `roadmap/v4-roadmap.md`

---

## Architectural refinements (2026-04-29 conversation)

After the initial draft was reviewed, these refinements were locked in:

### 1. Hard gates default-on, no opt-in flag

**Original plan:** ship dispatch middleware behind `--enforce-autonomy` for one minor release before flipping default.
**Refined:** since NeuroGrim has a single adopter as of 2026-04-29 and no other Brains depend on the documented-but-unenforced autonomy block, S13 ships hard gates **default-on** in v4.1. The `--enforce-autonomy` flag is reserved as a future *escape hatch* for adopters who eventually arrive and need a migration window — it is NOT the default-off opt-in originally planned.

This **removes one flag, one code path, one test matrix.** Net simplification.

### 2. Two patterns, one primitive

**Original plan:** treat the message bus as a single abstraction (`queue_publish` + `queue_consume`).
**Refined:** distinguish two patterns built on the same JSONL primitive:

- **Pattern 1 — Event log:** append-only, replayable, multi-consumer, no acknowledgment. JSONL substrate. Fan-out, eventually-consistent. Topics: `_neurogrim/notifications`, `_neurogrim/config-changes`, `pc-state/alerts`, etc. Each consumer tracks its own offset.
- **Pattern 2 — Request/response coordination:** synchronous-feeling-but-asynchronous. Agent emits → blocks → operator (or another agent) responds → agent unblocks. Topics: `_neurogrim/approvals`. The agent abstraction is `await_approval(action_id) -> ApprovalResult`, not raw `consume`.

The implementation may use queues underneath both, but the API surface is split. This affects S13-B-4 (MCP tools split into `queue_*` for Pattern 1 and `await_approval` / `resolve_approval` for Pattern 2) and S13-B-5 (the dispatcher's approval round-trip uses Pattern 2 specifically).

### 3. Reserved namespace + naming convention (normative)

- `_neurogrim/<name>` — system topics (approvals, notifications, config-changes). Schema-versioned. Documented in `neurogrim explain queues`. The `_neurogrim/` prefix is reserved; adopters MUST NOT publish into it.
- `<scope>/<name>` — project / adopter-defined topics. Lowercase, kebab-case, `:`-free.

Validated by `neurogrim doctor` against `queue-config.yaml`.

### 4. Consumer groups via SQLite ack-required topics (the SQLite earned-keep)

**The thing JSONL alone can't do cleanly:** two consumers reading the same topic with exactly-once guarantees. JSONL offsets are per-reader; coordination across readers needs transactional storage.

**Therefore:** the SQLite opt-in earns its keep specifically for `ack_required: true` topics. JSONL topics ignore the flag (they're fan-out logs); SQLite topics honor it via transactional `SELECT … FOR UPDATE; UPDATE consumed = true`.

Default remains JSONL.

### 5. Cross-backend inspectability

**Methodology preservation:** "everything inspectable as files" is real. SQLite breaks `cat`. **Mitigation:** `neurogrim queue inspect <topic>` reads from either backend and emits JSONL on stdout. Adopters always have a way to "see what's in there" regardless of how it's stored. Same shape, same `tail -f`-able output.

```bash
neurogrim queue inspect _neurogrim/approvals --since 24h | grep "blocked"
# Works whether the topic is jsonl- or sqlite-backed
```

### 6. Cross-Brain queue subscription via A2A

**Original plan:** bus is per-Brain.
**Refined:** SSE pubsub endpoint (`/api/brains/:id/queues/:topic/events`) is surfaced via the Agent Card as a discoverable capability. Cross-Brain consumption (e.g., ecosystem Brain subscribing to NeuroGrim's `_neurogrim/notifications`) becomes a feature of federation rather than a bolt-on. This is the v3.4 "child cards live data" pattern extended to event-time.

New story added: S13-B-9 below.

### 7. Bounded retention by default

**Original plan:** manual `queue compact` is the only retention story.
**Refined:** `queue-config.yaml` has `retention_days` (default 30) and `retention_messages` (default 10k). The dashboard runs a compaction sweep daily. Operators can disable per-topic when they want git-tracked permanent logs.

S13-B-7 expanded to include the auto-compaction scheduler.

---

## Stage 13 Is Done When

- [ ] `crates/neurogrim-core/src/queue.rs` module ships with `QueueMessage`, `JsonlQueueWriter`, `JsonlQueueReader` + 10+ unit tests
- [ ] `crates/neurogrim-dashboard/src/bus.rs` module wraps queue I/O behind HTTP endpoints
- [ ] `POST /api/brains/:id/queues/:topic`, `GET /api/brains/:id/queues/:topic?since=N&limit=M`, SSE pubsub at `/queues/:topic/events` all green
- [ ] Optional SQLite backend trait + adapter ships; per-topic configuration in `<brain>/.claude/brain/queue-config.yaml`
- [ ] `neurogrim queue migrate <topic> <from> <to>` CLI for backend transitions
- [ ] 3 new MCP tools: `queue_publish`, `queue_consume`, `queue_peek`
- [ ] Autonomy enforcement wired into MCP dispatch: `Approve` blocks via Pattern 2 round-trip; `Blocked` rejects deterministically; `Notify` runs but emits notification; `Auto` runs silently
- [ ] Hard gates **default-on** in v4.1 (no `--enforce-autonomy` flag — see refinement #1)
- [ ] Pattern 1 (event log) and Pattern 2 (request/response coordination) APIs distinct on the agent side (see refinement #2)
- [ ] Reserved `_neurogrim/<name>` namespace validated by `neurogrim doctor`
- [ ] Consumer groups: SQLite topics with `ack_required: true` provide exactly-once consumption; JSONL topics ignore the flag (fan-out only)
- [ ] `neurogrim queue inspect <topic>` reads from either backend and emits JSONL on stdout — preserves "everything inspectable as files" methodology
- [ ] Cross-Brain queue subscription via A2A: peer's Agent Card advertises queue endpoints; ecosystem Brain can subscribe to children's topics (S13-B-9)
- [ ] Default retention: 30 days OR 10k messages per topic; daily auto-compaction; per-topic override via `queue-config.yaml`
- [ ] Approvals UI widget (`approvals-feed`) + page (`/brains/:id/approvals`) render pending requests with Approve/Deny
- [ ] `neurogrim queue list | tail | publish | stats | compact | inspect | migrate` CLI ships
- [ ] 13th explain topic: `neurogrim explain queues` ships, documenting both patterns + namespace + retention
- [ ] NeuroGrim's own publish gates from S12 use bus events for cross-pipeline visibility

---

## Stories

### S13-B-1: Queue module in `neurogrim-core` (5 days)

**What:** New module `crates/neurogrim-core/src/queue.rs` (sync, no I/O dependency creep into core).

```rust
pub struct QueueMessage {
    pub id: Uuid,
    pub topic: String,
    pub payload: serde_json::Value,
    pub produced_at: DateTime<Utc>,
    pub priority: Priority,        // Low | Normal | High
    pub expires_at: Option<DateTime<Utc>>,
}

pub fn append(path: &Path, msg: &QueueMessage) -> std::io::Result<()>;
pub struct JsonlQueueReader { /* iterator with since_offset */ }
```

Mirrors the existing `disposition.rs:48` and `calibration_ledger.rs:306` writer signature. No new dependency surface.

**Done when:**
- [ ] Module + struct + writer + iterator + tests
- [ ] 10+ unit tests: append + iterate, malformed-line skip, since_offset resume, expires_at filtering
- [ ] Documentation in `core/src/lib.rs` module map

### S13-B-2: Bus service in `neurogrim-dashboard` (4 days)

**What:** `crates/neurogrim-dashboard/src/bus.rs` exposes queue I/O over HTTP. Reuses v3.4 SSE plumbing for pubsub.

**Endpoints:**
- `POST /api/brains/:id/queues/:topic` — publish (gated by `--allow-mutations`)
- `GET /api/brains/:id/queues/:topic?since=N&limit=M` — read since offset
- `GET /api/brains/:id/queues/:topic/events` — SSE pubsub (broadcast::Sender bounded channel; same pattern as v3.4 events.rs)
- `GET /api/brains/:id/queues` — list configured topics + stats
- `POST /api/brains/:id/queues/:topic/compact` — rotate old entries to archive

**Done when:**
- [ ] Endpoint handlers + 12+ tests (route coverage + auth gate)
- [ ] Bounded channel cap (capacity 64) prevents memory growth from idle subscribers
- [ ] ts-rs bindings for new DTOs

### S13-B-3: SQLite persistent backend (5 days, opt-in)

**What:** Trait `QueueBackend` in `neurogrim-core`. Implementations: `JsonlBackend` (default), `SqliteBackend`. Per-topic config in `<brain>/.claude/brain/queue-config.yaml`:

```yaml
schema_version: "1"
topics:
  _neurogrim/approvals:
    backend: jsonl
    retention_days: 30
  pc-state/alerts:
    backend: sqlite
    retention_messages: 10000
    ack_required: true
```

**Why opt-in:** Defaults to JSONL — preserves "everything inspectable as files" property. Adopters who need transactional consume / high concurrency opt in per-topic.

**Done when:**
- [ ] Trait + 2 adapters
- [ ] WAL-mode SQLite (mirrors a2a-token store)
- [ ] `neurogrim queue migrate <topic> jsonl sqlite` works both directions
- [ ] 8+ tests across both backends with same property-suite

### S13-B-4: MCP queue tools (3 days)

**What:** Three new MCP tools registered in `crates/neurogrim-mcp/src/server.rs`:

- `queue_publish(topic: String, payload: Value, priority?: Priority, expires_in_ms?: u64) -> {message_id}`
- `queue_consume(topic: String, since_offset: u64, limit: u32) -> {messages: [...], next_offset}`  (offset-based; tool does NOT mark consumed)
- `queue_peek(topic: String, count: u32) -> {messages: [...]}`  (no offset advance)

Default autonomy levels per tool documented in `tool_action_types.yaml`: all three are `notify` (cheap, low-blast). Operator can tighten in registry.

**Done when:**
- [ ] 3 tools registered + ts-rs bindings + 6+ tests
- [ ] Documentation in `cli.md` (CLI parity) + `queues.md` (new topic)

### S13-B-5: Wire `resolve_autonomy()` into MCP dispatch (4 days, the load-bearing one)

**What:** Middleware in `crates/neurogrim-mcp/src/server.rs` wraps every tool call:

1. Map tool name → declared `action_type` from `config.autonomy.action_types` (or default from `tool_action_types.yaml`)
2. Call `resolve_autonomy(action_type, autonomy_config, confidence)` from `governance.rs:244`
3. Dispatch by level:
   - `Auto` → execute immediately
   - `Notify` → execute, then publish on `_neurogrim/notifications` queue
   - `Approve` → publish on `_neurogrim/approvals` queue with `{action_id, action_type, payload, requires_approval_by, blast_radius}`; return `pending_approval` response with action_id; agent waits or polls via `queue_consume`
   - `Blocked` → reject with `{"error":"blocked","reason":"..."}`. Never executes.

**Why:** Single most important change in v4.x. The autonomy resolver exists and is tested; this story is purely the wiring.

**Done when:**
- [ ] Middleware ships **default-on** in v4.1 (single adopter as of 2026-04-29; no migration window needed — see refinement #1)
- [ ] `--enforce-autonomy` flag reserved as a future escape-hatch arg for adopters who eventually arrive and need a migration window — NOT shipped as default-off opt-in
- [ ] 15+ tests: each level + safety invariant + missing action_type + unknown tool
- [ ] Approval-resolution loop: `_neurogrim/approval-resolutions` queue carries `{action_id, decision, decided_by, decided_at}`; dispatcher consumes and unblocks pending tools
- [ ] Pattern 2 abstraction on the agent side: `await_approval(action_id) -> ApprovalDecision` rather than raw queue consume
- [ ] Documentation: CHANGELOG documents the autonomy-block behavior change as a v4.1 feature, not a breaking change

### S13-B-6: Approvals UI widget + page (5 days)

**What:** New widget `approvals-feed` in v3.5 widget catalog. Shows pending approvals with Approve / Deny buttons + payload viewer. New page `/brains/:id/approvals` for full list + history.

Approving emits on `_neurogrim/approval-resolutions` queue with operator handle from `$NEUROGRIM_OPERATOR`. Dispatcher's poll consumes and unblocks the agent.

**Done when:**
- [ ] Widget added to catalog (`lib/widget-catalog.ts`) + dispatcher
- [ ] Page route + component + tests
- [ ] Operator handle threaded through approval emission
- [ ] vitest coverage for the approval flow

### S13-B-7: CLI inspection (3 days)

**What:** `neurogrim queue` subcommand:

- `neurogrim queue list` — list configured topics + stats
- `neurogrim queue tail <topic> [--follow]` — tail messages
- `neurogrim queue publish <topic> <payload>` — manual produce (operator-only flow)
- `neurogrim queue stats <topic>` — message rate, oldest pending, retention status
- `neurogrim queue compact <topic>` — rotate old entries to archive (mirrors v3.5.1 test-failures pattern)
- `neurogrim queue migrate <topic> <from-backend> <to-backend>` — backend transitions

**Done when:**
- [ ] All 6 subcommands + tests
- [ ] CLI parity with HTTP endpoints documented

### S13-B-9: Cross-Brain queue subscription via A2A (4 days, post-refinement)

**What:** Surface each Brain's pubsub endpoint as a discoverable A2A capability. Add `queue_endpoints` to the Agent Card schema (additive; older peers ignore it). Cross-Brain consumers connect via A2A transport with bearer auth (reuses existing token store).

**Why:** v3.4's "child cards" pattern showed adopters wanting to see live data from peer Brains. Extending to event-time means an ecosystem Brain can react when its NeuroGrim child publishes `_neurogrim/notifications` without polling.

**Done when:**
- [ ] Agent Card schema additive field `queue_endpoints` validated
- [ ] `neurogrim a2a-discover` surfaces queue endpoints in its prose output
- [ ] Cross-process integration test: ecosystem Brain subscribes to NeuroGrim Brain's `_neurogrim/notifications` over A2A; consumes 3 messages successfully
- [ ] Documentation: federation explain topic gains a "queue subscription" subsection

### S13-B-8: Documentation (3 days)

**What:**
- 13th explain topic: `neurogrim explain queues`
- Hard-gate flow diagram added to `dashboard-layouts.md` (illustrates approval round-trip)
- Migration guide for adopters: how to wire `tool_action_types.yaml`, how to opt into `--enforce-autonomy`, how to author per-topic queue config
- README extension covering queue + approval workflow

**Done when:**
- [ ] 13th topic ships + version-stamp consistent
- [ ] BUNDLED_VERSION bumped to v4.1
- [ ] Adopter walkthrough end-to-end for a fresh Brain

---

## Risks (plan-critic concerns brought forward)

🟡 **Concern (downgraded from 🔴 after refinement #1): behavior change for adopters with autonomy blocks declared but not enforced.** As of 2026-04-29 there is one adopter (the user). Future adopters arriving post-v4.1 will receive enforcement on their first run; if they have unenforced autonomy blocks declared in their existing registry, agents that worked under v3.5 may block under v4.1. **Mitigation: `--enforce-autonomy` flag retained as a *future* escape hatch (not default-off); v4.1 CHANGELOG documents the behavior precisely; new adopter docs explain the autonomy block semantics upfront.**

🟡 **SQLite locking on Windows can be flaky.** WAL-mode (proven by a2a-token store) helps; document the failure mode; allow per-topic fallback to JSONL via `queue migrate`.

🟡 **Queue retention without a janitor.** JSONL files grow unbounded. Mitigation: `queue compact` ships in S13-B-7.

🟡 **SSE clients accumulating.** Each browser tab connects forever. Mitigation: cap concurrent SSE connections per Brain (default 16); reuse v3.4 `events.rs` bounded channel pattern.

🔵 **Suggestion: build a "queue-health" advisory domain in S13.** Reads queue stats; emits findings if any topic has zero consumers AND non-zero producers (silent drop), or if approval queue has pending items >24h.

🔵 **Suggestion: MCP autonomy middleware should also check `safety_invariants` against blast_radius.** Today's `resolve_autonomy()` already does this in step 4; the middleware just plumbs it through.

---

## Cross-references

- Master roadmap: `roadmap/v4-roadmap.md`
- Autonomy resolver (existing): `crates/neurogrim-core/src/governance.rs:244`
- Autonomy types (existing): `crates/neurogrim-core/src/types.rs:128`
- Existing MCP tools: `crates/neurogrim-mcp/src/server.rs:307–630`
- Ledger pattern reused: `crates/neurogrim-cli/src/commands/disposition.rs:48`
- SSE precedent: `crates/neurogrim-dashboard/src/events.rs`
- SQLite precedent: `crates/neurogrim-a2a/src/token_store.rs`
- S12 dependency: `roadmap/epics/S12-publish-gates.md`
