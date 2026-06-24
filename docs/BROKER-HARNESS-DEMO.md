# Broker Harness — Operator Demo Procedure (S*-T MVP)

> **Wave 5.5 deliverable.** End-to-end procedure for running the V0 broker
> harness against a live Claude Code session. ~10 minutes from `cargo
> install` to a working agent dispatch.

Companion docs:
- [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) — substrate spec
- [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) — building-block details
- [`BROKER-OPERATOR-GUIDE.md`](BROKER-OPERATOR-GUIDE.md) — tiered operator
  onboarding (Tier 1-5)
- `../../cereGrim/docs/V0-RETROSPECTIVE.md` — what V0 implementation proved

---

## Prerequisites

- Rust toolchain (1.75+ per workspace rust-version pin)
- Claude Code installed + working
- A project directory where you want to try the broker harness

---

## Setup (one-time, ~3 min)

### 1. Build the `neurogrim` CLI binary

```bash
cd D:/Brains/NeuroGrim/neurogrim
cargo build --release -p neurogrim-cli --bin neurogrim
# binary lands at target/release/neurogrim
```

Add to PATH or use the full path in the next steps.

### 2. Initialize the broker harness in your project

```bash
cd /path/to/your/project
neurogrim broker-init
```

This creates:
- `.claude/brain/broker/cluster.toml` (sample cluster manifest)
- `.claude/brain/broker/work-broker.toml` (sample broker manifest)
- `.claude/brain/broker/segments/` (empty; will be populated)
- `.claude/.mcp.json` (registers `neurogrim broker-serve` as MCP server)
- `CLAUDE.md` (appends `@.claude/brain/broker/current-projection.md`
  auto-load reference)

Idempotent: re-running preserves existing files.

### 3. (Optional) Customize the manifests

Edit `.claude/brain/broker/cluster.toml` to declare additional brokers
or change the materializer composition order.

Edit `.claude/brain/broker/work-broker.toml` to change the cold-store
path or catalog source.

---

## Run the demo (~5 min)

### 4. Launch Claude Code in the project directory

```bash
cd /path/to/your/project
claude
```

Claude Code reads `.claude/.mcp.json` on startup + spawns the broker-serve
MCP server as a child process. The broker harness initializes:
- Loads `cluster.toml` + per-broker manifests
- Constructs WorkBroker concrete brokers
- Performs initial materialization → writes `current-projection.md`
- Stands ready to handle `dispatch_pipeline` MCP calls

Verify the harness is running: in Claude Code, you should see
`neurogrim-broker` listed in `/mcp` output, exposing one tool:
`dispatch_pipeline`.

### 5. Verify CLAUDE.md auto-load picked up the projection

`CLAUDE.md` should reference `@.claude/brain/broker/current-projection.md`.
On agent turn 1, Claude Code auto-loads this file. The agent now sees:
- The governance pipelines section (always first per R-O-3)
- Each broker's overlay state segment
- Each broker's awareness routing (Surfaced pipelines + descriptions +
  when-to-use + parameter schemas)

Look for "Active brokers in this cluster: `work-broker`" in the agent's
context.

### 6. Dispatch a pipeline from the agent

Ask Claude Code (the agent):
> "Look at the broker harness's current-projection.md. What Surfaced
> pipelines are available + how would I dispatch one?"

The agent should:
1. Quote the awareness-routing segment listing
   `work-broker/dispatch-work-unit` + `work-broker/arm-kill-switch`
2. Explain each pipeline's purpose + parameter schema
3. Offer to dispatch via the single `dispatch_pipeline` MCP tool

To trigger an actual dispatch, ask:
> "Dispatch the dispatch-work-unit pipeline with work_unit_id `B-100`"

The agent should invoke the `dispatch_pipeline` MCP tool with arguments:
```json
{
  "broker_id": "work-broker",
  "pipeline_id": "work-broker/dispatch-work-unit",
  "params": {"work_unit_id": "B-100"}
}
```

**Expected behavior on this V0 demo:** the dispatch returns
`failure_reason: "leaf-op failed: claim_work_unit: ..."` because the V0
Work Broker initializes with an EMPTY BacklogState (no work units to
claim). This is the correct behavior demonstrating:
- The MCP tool wire works
- The framework finds the broker + pipeline
- Governance composition runs (trust-budget consumed, kill-switch checked)
- Preconditions evaluate
- The leaf-op runs + returns its structured refusal

The demo's success criterion is "the round-trip completed + the framework
reported a structured outcome," NOT "the work unit was claimed."

---

## Verify framework behaviors (~2 min)

### 7. Inspect the trace ledger

```bash
cat .claude/brain/broker/trace.jsonl
```

You should see one JSONL record per dispatch attempt with the full
`TraceRecord` schema:
- `pipeline_id`, `broker_id`, `params`
- `snapshot_delta_from` + `snapshot_delta` (delta from prior dispatch)
- `outcome` (success or refusal with failure_reason)
- `audit_class: capability` (or `governance` for kill-switch arming)
- `duration_ms`

### 8. Verify re-materialization fires after dispatch

`current-projection.md` should have been updated after step 6's dispatch
(the `on_dispatch_complete` callback re-runs the Materializer Composer
asynchronously). Check the file's mtime:

```bash
ls -la .claude/brain/broker/current-projection.md
```

If you can't tell from mtime, dispatch a couple more times + verify the
trace ledger grows.

### 9. (Optional) Arm the kill switch + verify it halts dispatches

Ask Claude Code:
> "Dispatch arm-kill-switch on work-broker."

Then try to dispatch dispatch-work-unit again. It should refuse with
`failure_reason` containing `KillSwitchArmed`.

### 10. (Optional) Add seed work units

V0 Work Broker initializes with empty BacklogState. To make
`dispatch-work-unit` succeed, you'd need to extend the broker manifest to
declare seed work units OR write a custom broker that wraps
`neurogrim_sensory::backlog::next_ready()` against your actual BACKLOG.md.
That's Wave 5.5+ work; the V0 demo's success criterion is the MCP wire +
governance composition + materializer cycle.

---

## What you've verified

By completing steps 4-9 you have empirically demonstrated:
- ✅ Operator-side setup (broker-init) works end-to-end
- ✅ Claude Code launches the broker-serve MCP server via .mcp.json
- ✅ The broker substrate loads the cluster + per-broker manifests
- ✅ Initial materialization produces a well-formed
  `current-projection.md` with governance segment first (R-O-3 closure)
- ✅ Awareness Materializer surfaces per-pipeline parameter schemas
  (ultra-pass U1 closure)
- ✅ The single `dispatch_pipeline` MCP tool routes correctly to the
  Pipeline Runner
- ✅ Framework composes governance (trust-budget + kill-switch +
  record-dispatch + record-outcome) automatically for Surfaced pipelines
  (BB #19)
- ✅ Trace Sink records each dispatch with snapshot deltas (ultra-pass U9)
- ✅ Materializer Composer auto-re-runs after dispatch via
  on_dispatch_complete callback (V0-RETROSPECTIVE §C6 closure)
- ✅ Kill switch enforcement works in production wire-up (step 9)

You have NOT yet verified (Wave 5.5+ / S1-T):
- Real backlog parsing (`next_ready()` integration) — V0 Work Broker uses
  in-memory empty state
- Multi-broker materialization scale (only 1 broker in V0 demo)
- Real cold-store backend (JSONL backend exists in code per gap #11
  closure but Work Broker doesn't use it yet)
- Workflow Engine (BB #11) — V0 is single-tick only
- Frame stack (BB #35) — V0 tunability is metadata-only

---

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Claude Code doesn't show `neurogrim-broker` in `/mcp` | `.mcp.json` not picked up | Restart Claude Code; ensure you're in the right project dir |
| Agent doesn't see current-projection.md content | CLAUDE.md auto-load missing | Verify CLAUDE.md contains `@.claude/brain/broker/current-projection.md` line |
| broker-serve fails with `ManifestNotFound` | Wrong --cluster path | Use absolute path or run from project root |
| broker-serve fails with `BrokerNotDeclared` | Manifest declared a broker the binary doesn't know how to construct | MVP only supports WorkBroker; future versions will dispatch by broker_type |
| Dispatch returns `params_must_be_object_or_null` | Agent sent params as a string | Agent should send params as a JSON object; awareness routing shows the schema |
| Dispatch returns `governance refused: trust budget exhausted` | Default 10k dispatch budget consumed | Restart broker-serve; budget resets |

---

## Where the magic happens (architecture summary)

```
┌─────────────────────────────────────────────────────────────┐
│  Claude Code (the agent)                                    │
│                                                              │
│  CLAUDE.md → @.claude/brain/broker/current-projection.md ───┐
│                                                              │
│  Agent reads:                                                │
│  - Governance pipelines (always-reachable, always first)    │
│  - Per-broker Overlay state (e.g., active_work)             │
│  - Per-broker Awareness routing                              │
│    (Surfaced pipelines + descriptions + when_to_use +       │
│     PARAMETER SCHEMAS — needed since MCP tool is opaque)    │
│                                                              │
│  Agent dispatches via single MCP tool:                      │
│  `dispatch_pipeline(broker_id, pipeline_id, params)`        │
└──────────────────────┬──────────────────────────────────────┘
                       │ MCP/stdio
                       ↓
┌──────────────────────────────────────────────────────────────┐
│  neurogrim broker-serve (child process of Claude Code)      │
│                                                              │
│  BrokerMcpServer.dispatch_pipeline()                        │
│      ↓                                                       │
│  PipelineRunner.dispatch()                                   │
│      ↓ Governance composer pre-checks                       │
│      ↓ Precondition evaluation (Overlay snapshot)           │
│      ↓ Step execution (broker.execute_leaf for each leaf)   │
│      ↓ Trace recording (delta from prior snapshot)          │
│      ↓ on_dispatch_complete callback                        │
│            ↓                                                 │
│   ┌─────── Materializer Composer (async)                    │
│   │        ↓ Per-broker hot-store materializer              │
│   │        ↓ Per-broker awareness materializer              │
│   │        ↓ Governance-first composition                   │
│   │        ↓ Write current-projection.md                    │
│   │                                                          │
│   └→ Next agent turn re-reads CLAUDE.md → sees fresh state  │
└──────────────────────────────────────────────────────────────┘
```

Single MCP tool. L1 file-injection as the discovery surface. The broker
pattern's value (curation + preconditions + governance + audit) lives in
the materializer output + the substrate's per-dispatch enforcement —
exactly where the broker contract intends, NOT smeared across MCP tools.
