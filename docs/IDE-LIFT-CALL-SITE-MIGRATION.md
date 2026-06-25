# C10 — Per-Call-Site Migration Mapping

> Every `invoke("ide_action", ...)` call site in the IDE frontend mapped
> to its substrate broker target. Operator drives the actual code edits
> + live-IDE validation per variant; this doc supplies the mechanical
> (broker_id, pipeline_id, params-shape) translation so no per-call-site
> classification work is needed at edit time.

**Source:** grep `invoke("ide_action", ` over `src/` (14 production sites
across 9 files as of 2026-06-25).

---

## Migration shape

For each call site, the migration follows one of two patterns:

### Pattern A — drop-in replacement (preferred)

```typescript
// BEFORE:
await invoke("ide_action", {
  action: { kind: "<variant-name>", ...variant_params },
  idempotencyKey: "...",
});

// AFTER:
import { dispatchPipelineViaBroker } from "~/lib/agent/ide-action-dispatch";
await dispatchPipelineViaBroker("<broker-id>", "<broker-id>/<pipeline-name>", {
  ...variant_params,
});
```

### Pattern B — dual-dispatch with feature flag (during migration window)

When operator wants to observe the broker dispatch in trace.jsonl WHILE
keeping the legacy path authoritative:

```typescript
// V0 dual-dispatch pattern:
if (await brokerLiftEnabled()) {
  await dispatchPipelineViaBroker("<broker-id>", "<broker-id>/<pipeline-name>", params);
}
await invoke("ide_action", { action: { kind: "<variant-name>", ...params }, idempotencyKey });
```

`brokerLiftEnabled()` reads a LocalAwareness fact (e.g., `ide-broker-lift-enabled`)
so the operator can flip per-variant during validation.

---

## Per-call-site mapping table

### tour-driver.ts:300 — RunTourScript

| Field | Value |
|---|---|
| Variant | `run_tour_script` |
| broker_id | `tour` |
| pipeline_id | `tour/run-tour-script` |
| Params | `{ script_id: gridTarget }` |
| Notes | The `target_chat_cell_id` legacy param renames to `script_id` per the C9.0 classification's tour-broker schema. |

### tour-driver.ts:367 — RunTourScript (continuation)

Same as above; different code path through the tour driver.

### tour-driver.ts:1191 — RunTourScript (third occurrence)

Same as above.

### ide-actions.ts:87 — generic dispatcher

This is the legacy generic dispatcher used by other code. Migration
pattern: keep the function signature; replace the body with a
variant→(broker_id, pipeline_id) lookup table + dispatchPipelineViaBroker
call. ~40 LOC of switch-statement bridging.

### HeadlessAgentsChip.tsx:73 — SendPrompt

| Field | Value |
|---|---|
| Variant | `send_prompt` |
| broker_id | `agent` |
| pipeline_id | `agent/send-prompt` |
| Params | `{ session_id, prompt }` |

### a2a-dispatch.ts:132 — Checkpoint (auto)

| Field | Value |
|---|---|
| Variant | `checkpoint` |
| broker_id | `checkpoint` |
| pipeline_id | `checkpoint/checkpoint` |
| Params | `{ label }` |
| Notes | The auto-checkpoint hook fires before A2A dispatch; migration preserves the pre-dispatch hook semantics. |

### a2a-dispatch.ts:311 — varies by A2A action

Multiple variants reach this site depending on the A2A action's target.
The migration adds a switch on action kind that routes to the
appropriate broker. ~30 LOC bridging.

### AgentSessionPanel.tsx:255 — SendPrompt

Same shape as HeadlessAgentsChip.tsx:73.

### AgentSessionChooser.tsx:83 — varies (likely SwitchActiveTab)

Per the C9.0 classification:

| Field | Value |
|---|---|
| Variant | `switch_active_tab` |
| broker_id | `ui-state` |
| pipeline_id | `ui-state/switch-active-tab` |
| Params | `{ tab_id }` |

### layout-presets.ts:211 — PlacePane or SetPaneContent

Per C9.0 classification:

| Field | Value |
|---|---|
| broker_id | `layout` |
| pipeline_id | `layout/place-pane` OR `layout/set-pane-content` (depends on caller) |
| Params | `{ pane_id, cell_id }` or `{ pane_id, content_kind, content_ref }` |

### orchestrator.ts:326, :336, :348 — session-broker actions

Three call sites in the session-broker orchestrator. Each dispatches a
specific session-management action. Map to the `agent` broker:

| Site | Variant | broker/pipeline |
|---|---|---|
| 326 | likely SendPrompt or session-start | `agent/send-prompt` |
| 336 | similar | `agent/send-prompt` |
| 348 | session-state mutation | `ui-state` or `agent` (depends on action) |

---

## Migration sequence (operator workflow)

For each call site (recommended order: lowest-risk first per smoke-test economics):

1. **Identify the variant** from the call site's `action.kind` field.
2. **Look up (broker_id, pipeline_id) + params shape** from the table above.
3. **Add the substrate path alongside the legacy** (Pattern B dual-dispatch).
4. **Run the IDE; exercise the call site via real UI / agent invocation.**
5. **Observe `.claude/brain/broker/trace.jsonl`** — the substrate dispatch
   should appear with the right pipeline_id + audit_class.
6. **Confirm the frontend handler still works correctly** (legacy path is
   still doing the actual work; substrate is purely observational at
   this stage).
7. **Commit the dual-dispatch addition.**
8. **After N variants in dual-dispatch mode, flip them to substrate-only**
   (delete the legacy `invoke("ide_action", ...)` line; substrate
   becomes authoritative).
9. **After all 14 production sites migrated**, the legacy `dispatchIdeAction`
   wrapper has zero non-test callers.
10. **Delete the legacy Tauri command** (`#[tauri::command] fn ide_action`)
    + `ipc/ide_action.rs` + all the variant adapter functions across
    `browser/`, `agent/`, etc. ~1500 LOC delta.
11. **Run `cargo check --workspace`** to confirm no broken references.
12. **Run the IDE end-to-end** to confirm every previously-tested workflow
    still works through the substrate.

---

## Effort estimate (operator workflow)

- Per-site Pattern B addition (steps 1-3): ~10 min × 14 sites = ~2.5 hr
- Per-site smoke test (steps 4-6): ~5-15 min × 14 sites = ~1-3.5 hr
- Bulk flip to Pattern A (step 8): ~5 min × 14 sites = ~1 hr
- End-to-end retest (step 12): ~30 min
- Legacy code deletion (step 10): ~30 min
- **Total operator time: ~5-8 hours of focused work.**

The 12-18 day estimate from the plan's ultra-pass U10 was for the full
C9 wire-up; the C10 frontend migration with the substrate already shipped
is much smaller — most of the architectural authoring is done.

---

## What the AI shipped vs what the operator must drive

**AI shipped:**
- All 19 broker shapes + catalogs + Pipeline literals + wire-up via Tauri-emit
- BrokerFactoryRegistry substrate extension
- IDE-side factory registration
- BrokerHost wired into Tauri lifecycle
- Frontend `dispatchPipelineViaBroker()` entry point
- This migration mapping table

**Operator must drive:**
- Per-call-site code edit per the mapping
- Per-call-site live-IDE smoke test
- Decision on when to flip from Pattern B (dual-dispatch) to Pattern A
  (substrate-authoritative) per variant
- Final legacy code deletion after operator-validated all 14 migrations

The operator workflow is well-bounded (~5-8 hours focused) but requires
the live IDE running on the operator's hardware. No AI can substitute.
