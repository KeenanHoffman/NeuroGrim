<!-- topic: publish-gates — bundled in neurogrim-cli v3.5 -->
# Publish gates — ship without surprise

A **publish gate** is a check that runs before a Brain (or the
project it watches) ships a release. NeuroGrim v4.0 introduces a
structured gate pipeline so adopters can codify what "ready to
publish" means for their project — fast automated checks, browser-
level E2E tests, and operator-validated manual checklists, all in
one declarative manifest.

This is the v4.0 epic the rest of the v4.x stages depend on. Every
later stage's publishes go through the gates this stage establishes.

<!-- anchor: gate-types -->
## The three gate types

Every gate has a `gate_type` from a closed vocabulary:

- **`automated`** — runner shells out to `check_command` (sh on
  Unix, cmd on Windows); exit 0 = pass, exit ≠ 0 = fail. Bounded
  by `timeout_seconds` (default 600s).
- **`manual`** — runner prints `instructions` to the operator and
  emits a `pending` ledger entry. Operator marks passed via
  `neurogrim publish-gate ack --gate <id>` once verified.
- **`e2e`** — runner invokes the Playwright suite at
  `crates/neurogrim-dashboard/frontend/`. NeuroGrim-internal in v1;
  adopters who want their own browser tests should use `automated`
  with a custom playwright command.

A gate's `blocking` field controls whether a failure stops the
publish. Default `true`. Advisory gates (`blocking: false`) are
recorded but never drive the runner's exit code.

<!-- anchor: manifest -->
## The manifest

Adopters declare gates in
`<brain>/.claude/brain/publish-gates.yaml`. The manifest is
schema-versioned; v1 ships with NeuroGrim v4.0 and is validated by
`neurogrim doctor` against
`crates/neurogrim-mcp/data/schemas/publish-gates-v1.schema.json`.

```yaml
schema_version: "1"
gates:
  - id: tests-pass
    gate_type: automated
    description: All tests green via `neurogrim test`
    blocking: true
    timeout_seconds: 120
    check_command: "neurogrim test"

  - id: changelog-dated
    gate_type: automated
    description: CHANGELOG's [Unreleased] section converted to a date stamp
    blocking: true
    check_command: "grep -E '\\[\\d+\\.\\d+\\.\\d+\\] - 20\\d\\d' CHANGELOG.md"

  - id: dashboard-loads-locally
    gate_type: manual
    description: Operator visits dashboard, verifies new feature renders
    instructions: |
      1. Run `neurogrim ui --allow-mutations`
      2. Navigate to /brains/<id>/<feature-page>
      3. Verify <specific behavior>
    operator_required: true

  - id: e2e-smoke
    gate_type: e2e
    description: Playwright smoke covering Overview, Federation, Layout edit
    blocking: true
    timeout_seconds: 240
```

`id` must be kebab-case and unique within the manifest — it's the
ledger's primary key. Schema enforcement is closed-vocabulary at
every level (`additionalProperties: false`); adding a field
requires a v2 schema bump with a METHODOLOGY-EVOLUTION entry.

<!-- anchor: runner -->
## The runner

```
neurogrim publish-gate run [--gate <id>] [--mode {pre-commit|pre-publish|full}] [-v]
neurogrim publish-gate ack --gate <id> [--operator <handle>]
```

`run` executes the manifest's gates in declared order, prints a
per-gate outcome, and appends one JSONL entry per gate to
`<brain>/.claude/brain/publish-gate-ledger.jsonl`.

**Exit code precedence (failed > pending > passed):**

- 0 = every blocking gate passed
- 1 = any blocking gate failed, timed_out, or errored
- 2 = any blocking gate is pending (and none failed)

Non-blocking gate failures appear in the ledger and the summary
line but never drive the exit code.

**Mode filter** (heuristic in v1; schema v2 will introduce explicit
per-gate mode tags):

- `pre-commit` → `automated` gates with `timeout_seconds ≤ 30` (or
  unset). Cheap fast checks; suitable for git pre-commit hooks.
- `pre-publish` → all `blocking: true` gates regardless of type.
  The full release-eve battery.
- `full` → every gate. Default. Includes advisory gates.

`--gate <id>` runs a single gate and overrides `--mode`.

<!-- anchor: manual-ack -->
## Manual gate ack flow

When `publish-gate run` encounters a manual gate, it:

1. Prints `description` + `instructions`
2. Emits a `pending` ledger entry (no `completed_at`)
3. Continues to the next gate

The operator follows the instructions, then marks the gate passed:

```bash
NEUROGRIM_OPERATOR=alice neurogrim publish-gate ack --gate dashboard-loads-locally
```

`ack` walks the ledger backwards to find the most recent `pending`
entry for `--gate <id>`. If the prior entry for that gate was
already resolved (passed/failed/etc.), `ack` rejects with a clear
message — preventing an operator from re-acking a gate that wasn't
actually pending.

Operator handle is required: `--operator <handle>` flag, then
`$NEUROGRIM_OPERATOR` env, then reject. No "unknown" fallback —
audit-trail discipline (LSP-Brains spec §17.6).

<!-- anchor: ledger -->
## The ledger

`<brain>/.claude/brain/publish-gate-ledger.jsonl` is append-only
JSONL (gitignored). One entry per gate execution:

```json
{
  "schema_version": "1",
  "run_id": "<uuid v4>",
  "gate_id": "<id>",
  "gate_type": "automated",
  "mode": "full",
  "started_at": "2026-04-29T12:00:00Z",
  "completed_at": "2026-04-29T12:00:01Z",
  "status": "passed",
  "blocking": true,
  "exit_code": 0,
  "stdout_truncated": "..."
}
```

All gates from one `run` invocation share a `run_id`. `ack` entries
share the original pending entry's `run_id` so audit replays can
trace the full lifecycle.

`stdout_truncated` and `stderr_truncated` cap captured output at 4
KB head + 4 KB tail (with a `…[truncated N bytes]…` marker), keeping
typical entries under PIPE_BUF for `O_APPEND` atomicity.

## E2E gates and Playwright

The `e2e` gate type runs the suite at
`crates/neurogrim-dashboard/frontend/`. One-time setup:

```bash
cd crates/neurogrim-dashboard/frontend
npm install
PLAYWRIGHT_BROWSERS_PATH=/d/playwright-browsers npx playwright install chromium
```

(On Windows, set `PLAYWRIGHT_BROWSERS_PATH` to a non-`C:` location
when `C:` is under disk pressure — Chromium is ~150 MB.)

Each E2E run rebuilds the test binary's webServer:

```bash
cd crates/neurogrim-dashboard/frontend && npm run build
cd ../../.. && cargo build --bin neurogrim
neurogrim test --e2e   # or trigger via a publish-gate manifest
```

The Rust binary embeds `frontend/dist/` via RustEmbed at compile
time — so the frontend AND the binary need rebuilding after a UI
change.

Adopter brains that don't ship the dashboard should use
`automated` gates with their own playwright command (or other
E2E tooling) instead. The `e2e` gate type errors out if no
`crates/neurogrim-dashboard/frontend/playwright.config.ts` is
found, with a hint pointing at the `automated` alternative.

## Adopter onboarding

To start using publish gates in a fresh adopter Brain:

1. Author `<brain>/.claude/brain/publish-gates.yaml` with at least
   one `automated` gate (e.g., `tests-pass` running your project's
   test command).
2. Run `neurogrim doctor` — it will validate the manifest against
   the v1 schema and report any structural issues.
3. Run `neurogrim publish-gate run --mode pre-publish` to exercise
   the pipeline end-to-end. The first run produces an empty ledger
   under `<brain>/.claude/brain/publish-gate-ledger.jsonl`.
4. Iterate: add more gates as your release ritual evolves.

NeuroGrim's own publishes will go through this pipeline starting
v4.0 (S12-G-7 self-hosting milestone). The CHANGELOG documents
the requirement once active.

## See also

- `neurogrim explain methodology` — the conceptual model
- `neurogrim explain cli` — the full CLI surface
- `neurogrim doctor` — validates `publish-gates.yaml`
- `roadmap/epics/S12-publish-gates.md` — the epic with story-level detail
