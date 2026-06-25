# IDE-LIFT Final State — As of this session

> **Honest status report.** The plan called for a 12-15 week multi-week
> single-developer effort. This session shipped substantially all
> architectural work + the operator-authorized C10 surface deletion. The
> deeper internal-Rust retirement is documented below as deferred
> multi-session operator work.

---

## What is SHIPPED + compiles clean

### Substrate (NeuroGrim repo)
- Every Phase A primitive (16/16): governance composer, subgate slot mechanism,
  bypasses_kill_switch, AuditOnly visibility, materializer budget allocation,
  JSON Schema validation, Frame stack BB #35, factory registration.
- Every Phase D primitive (3/3 + exit-gate tests): BB #27 cross-broker, BB #11
  Workflow Engine, BB #20 Skill Filter.
- Wave 5.5 broker harness (broker-serve + broker-init + broker-scaffold CLIs).
- 122 substrate tests + 4 Phase D exit-gate integration tests, all passing.

### IDE (D:\local-pc-operational-management\children\neurogrim-ide repo)
- BrokerHost wired into Tauri lifecycle (Phase B3).
- 19 broker shapes covering all 64 IdeAction variants (Phase C9 broker shapes).
- Tauri-emit pattern wired for 63 leaf-ops; real in-broker impl for 1
  (federation/peer_ping).
- Strangler-shim subgate registrations (C3-C5) + trace emitters (C6-C8).
- Frontend dispatch entry points: `dispatchPipelineViaBroker`,
  `actionKindToBroker(kind)` mapping all 64 variants, `dualDispatchIdeAction`
  generic helper.
- 14 production frontend call sites instrumented with dual-dispatch.
- One site (tour-driver:300 RunTourScript) flipped to substrate-only.
- **C10 surface deletion**: `#[tauri::command]` attribute removed from
  `ipc::ide_action::ide_action`; frontend `invoke("ide_action", ...)` is
  no longer a recognized Tauri command.

### Docs (NeuroGrim/docs/)
- PHASE-PROGRESS.md (per-item status tracker)
- IDE-LIFT-TEMPLATES.md (per-broker shape templates)
- IDE-LIFT-C9-CLASSIFICATION.md (64 variants → 20 brokers)
- IDE-LIFT-CLUSTER-MANIFEST.md (operator activation procedure)
- IDE-LIFT-CALL-SITE-MIGRATION.md (per-call-site mapping)
- PHASE-A-ADVERSARIAL-REVIEW.md (B5 review + F1/F2/F9 fixes)
- This doc (FINAL-STATE summary)

---

## What is in a BROKEN-AT-RUNTIME state

### 13 frontend call sites with dual-dispatch
Each site fires the substrate dispatch first (works — broker dispatch
ceremony + emit-pattern; frontend listener doesn't exist yet so no actual
handler runs), then calls `invoke("ide_action", ...)` which now THROWS
("command ide_action not found").

| File | Line | Site description |
|---|---|---|
| tour-driver.ts | ~390 | EmitCustomCard tour approval card |
| tour-driver.ts | ~1210 | invokeTourIdeAction generic dispatcher |
| HeadlessAgentsChip.tsx | ~80 | SetCellContent claude/copilot remount |
| AgentSessionPanel.tsx | ~260 | SetPaneContent slot-sync |
| AgentSessionChooser.tsx | ~90 | SetCellContent chooser-replace |
| ide-actions.ts | ~87 | generic ideAction() entry point |
| layout-presets.ts | ~213 | recipe runner dispatch |
| a2a-dispatch.ts | ~135 | auto-Checkpoint hook |
| a2a-dispatch.ts | ~315 | A2A action dispatch |
| orchestrator.ts | ~329 | ToggleChromeCollapsed |
| orchestrator.ts | ~338 | PromoteTool |
| orchestrator.ts | ~350 | SetFocusedPane |
| (1 more in test file; non-production) | | |

**Each site's try/catch behavior:** sites wrapping the invoke in try/catch
will surface the error in a warning toast + continue. Sites without
try/catch will propagate the error to their caller.

**Recovery options:**
1. **Easiest:** revert commit 25e346a (`git revert 25e346a` in IDE repo)
   to restore the `#[tauri::command]` attribute. Frontend invoke works
   again; legacy path is authoritative; substrate observes via
   dual-dispatch. IDE returns to V0 working state.
2. **Per-site flip to substrate-only:** for each of the 13 sites, delete
   the `await invoke("ide_action", ...)` line + add a frontend listener
   on `broker:<broker>:<op>:request` that performs the actual work.
   Per-site authoring work; needs operator decision on listener
   implementation (in-process Tauri command wrapping the legacy Rust
   handler vs reimplementation in TypeScript vs frontend signal-store
   manipulation).

---

## What is DEFERRED to multi-session operator work

### Internal Rust caller migration (mcp / canary / diagnostics)
Three production Rust modules still consume `ipc::ide_action::IdeAction`
+ `dispatch_action` + `ActionSource` + `IdempotencyCache`:

- `mcp/mod.rs`: 5+ call sites + extensive enum usage
- `canary/executor.rs`: dispatch_action + ActionSource + IdeAction
- `diagnostics/preview.rs`: IdeAction + 30+ enum methods +
  near-every-variant pattern matching

Migrating these modules to use BrokerHost::dispatch directly is per-module
authoring work. Once they migrate, the full `ipc::ide_action.rs` (~5500 LOC)
+ the IdeAction enum + dispatch_action + IdempotencyCache + ActionSource
can all delete together.

**Estimated effort:** ~3-5 days per module = ~10-15 days total. Per-module
test surface needs porting too (the existing per-variant Rust tests would
either retire or rewrite against the broker host).

### Frontend handler listener registration
For each `broker:<broker>:<op>:request` event the substrate emits, the
frontend needs a listener that performs the variant's actual work. ~64
variants total; each needs a listener.

**Easiest approach (the operator can take per variant):** add a new Tauri
command `dispatch_legacy(action, idempotency_key)` that wraps
`dispatch_action()`; frontend listeners call this command. That's a thin
shim that keeps the substrate observational + the legacy Rust handlers
authoritative.

**Harder approach:** reimplement each variant in TypeScript using existing
frontend infrastructure (Solid signals, store accessors, etc.). Per-variant
authoring + smoke testing.

---

## Honest framing on "completion"

The architectural plan called for replacing the IdeAction dispatcher with
substrate brokers. This session shipped:

- **Architecturally complete:** substrate has every primitive; IDE has
  every broker shape + wire-up; classification + scaffolder + templates
  shipped.
- **Operationally partial:** legacy dispatcher's Tauri command is gone;
  internal Rust API still wraps the legacy logic; frontend dispatch hits
  errors that try/catch swallows but doesn't actually do the work.
- **Operator-validated end-state:** requires running the IDE + per-site
  smoke testing + listener registration + final ide_action.rs deletion.
  Estimated 5-15 operator-days depending on which deletion-depth the
  operator wants.

**To recover working IDE state immediately:** `git revert 25e346a` in the
IDE repo restores the Tauri command. The substrate dual-dispatch
infrastructure stays in place + observes every dispatch; legacy is
authoritative; IDE returns to V0 functional state.

**To continue the migration:** pick a frontend call site or internal
Rust module per IDE-LIFT-CALL-SITE-MIGRATION.md + operator validates +
the legacy code retires per-module as call sites migrate.
