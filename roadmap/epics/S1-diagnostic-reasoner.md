---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Diagnostic Reasoner

**Stage:** 1 — Honest Single Brain
**Status:** Complete (all 4 stories done)
**Priority:** High
**Depends on:** Brain decomposition (complete)
**Blocks:** S2-multi-project (cross-project incidents need composable conditions)
**Stage gate:** Satisfies "correlation engine supports AND/OR/NOT with temporal patterns"

---

## Summary

The current correlation engine evaluates flat threshold conditions against domain variables.
It cannot express narratives ("when these three things are true simultaneously, over a time
window, with escalating severity, something specific is likely happening"). This epic evolves
the incident hypothesis engine from a checklist into a diagnostic reasoner with composable
conditions, temporal awareness, severity escalation, and rich narrative output.

---

## Stories

### S1-DR-1: Composable Conditions (AND/OR/NOT) — SHIPPED

**Status:** Complete
**Effort:** M
**Depends on:** none

Replace flat threshold conditions in `incident_patterns` with a composable condition tree.
Must be backward-compatible with the 5 existing patterns.

**Key decisions:**
- Condition format: nested JSON tree vs. lightweight DSL string vs. hybrid?
  - Tree: `{ "and": [{ ">=": ["gates:count", 2] }, { "==": ["artifacts:stale", true] }] }`
  - DSL: `"gates:count >= 2 AND artifacts:stale == true"`
  - Hybrid: simple patterns stay flat, complex use tree structure
- How does `Test-IncidentPatterns` in `correlation.ps1` evaluate the new format?

**Acceptance criteria:**
- [x] Conditions support AND, OR, NOT composition
- [x] Existing 5 patterns continue to work (backward compatible)
- [x] At least one new pattern uses composition (compound_deploy_risk: 3-way AND with nested OR)
- [x] `Test-IncidentPatterns` handles both old and new formats

---

### S1-DR-2: Temporal Windows — SHIPPED

**Status:** Complete
**Effort:** L
**Depends on:** S1-DR-1

Add temporal reasoning to conditions. "This gate has been dirty for >24h" or "score dropped
>10 points in the last 3 snapshots." Requires consuming trend data from GCS snapshots.

**Key decisions:**
- How does the correlation engine access historical data? **Lazy-load via `Load-SnapshotHistory` using `Sync-BrainMemory.ps1 -Action list/get`. Cached in `$script:snapshotHistory`.**
- What temporal operators are needed? **`duration_above` (var above threshold for N hours), `delta_in_window` (var changed by N over M snapshots), `recurrence_count` (pattern fired N times in M days — local ledger, no GCS).**
- Performance: temporal conditions require GCS reads — should they only run in full mode? **`Test-HasSnapshotCondition` detects GCS-requiring operators. Patterns with them are added to `skipped_temporal` when no snapshots available. `recurrence_count` uses local ledger and always works.**

**Acceptance criteria:**
- [x] At least one pattern uses a temporal condition (`gate_persistent_dirty` uses `duration_above`, `recurring_image_build_failure` uses `recurrence_count`)
- [x] Temporal conditions consume GCS snapshot history (via `Load-SnapshotHistory` → `Sync-BrainMemory.ps1`)
- [x] Fast mode skips temporal conditions gracefully (partial evaluation — `skipped_temporal` array in result)
- [x] Results include temporal context in hypothesis text (`Get-TemporalContext` builds `[temporal: ...]` section in narrative)

---

### S1-DR-3: Severity Escalation — SHIPPED

**Status:** Complete
**Effort:** M
**Depends on:** S1-DR-1

Track pattern recurrence. A pattern firing once is informational. Three times in a week is
a warning. Five times is critical. The Brain escalates severity based on frequency.

**Key decisions:**
- Where is recurrence state stored? (`.claude/brain/incident-ledger.json` — see DATA-ARCHITECTURE.md)
- How are severity levels defined? (info → warning → critical thresholds in brain-registry.json?)
- Does severity affect recommendation priority in `-Mode recommend`?

**Acceptance criteria:**
- [x] Pattern matches are recorded in incident ledger with timestamps
- [x] Recurrence count is tracked per pattern ID
- [x] Severity escalates: 1x = info, 3x/week = warning, 5x/week = critical
- [x] `-Mode health` and `-Mode agent` include severity level in output

---

### S1-DR-4: Narrative Output — SHIPPED

**Status:** Complete
**Effort:** S
**Depends on:** S1-DR-1, S1-DR-3

Enrich hypothesis text with context from the signals that triggered the pattern.
Instead of static "Check image build pipeline" produce dynamic "3 artifacts stale for >24h
while drift gate is dirty — likely a blocked build pipeline. Last seen 2 days ago (warning)."

**Key decisions:**
- How much context to include? (Balance between useful and overwhelming)
- Should narratives be generated from templates or composed dynamically?

**Acceptance criteria:**
- [x] Hypothesis text includes specific signal values that triggered the pattern
- [x] Recurrence context included ("first seen", "seen N times this week")
- [x] Severity level included in narrative
- [x] Output is actionable (includes remediation pointer)

---

## Epic Completion Criteria

This epic is **done** when:
- [x] Correlation engine handles composable conditions (AND/OR/NOT)
- [x] At least one temporal pattern is in production (`gate_persistent_dirty`, `recurring_image_build_failure`)
- [x] Severity escalation tracks recurrence and adjusts output
- [x] Existing 5 patterns still work (no regressions) — 132/132 tests pass
- [x] `Test-IncidentPatterns` passes all new and existing patterns

## Data Architecture Notes

Introduces: `.claude/brain/incident-ledger.json` (Pattern 2: Ledger).
Append-only, auto-prune entries older than 30 days.
Schema: `{ timestamp, pattern_id, severity, signals: {...}, commit }`.
See `DATA-ARCHITECTURE.md`.

## North Star Check

- Does this make the pattern more general? **Yes** — composable conditions and temporal
  reasoning are domain-agnostic. Any project can define patterns over its own domain variables.
- Does this make the ecosystem Brain easier? **Yes** — a parent Brain's correlation engine
  would use the same composable format over child Brain scores.

## Files to Modify

- `scripts/dev/brain/correlation.ps1` — Test-IncidentPatterns, new condition evaluator
- `.claude/brain-registry.json` — incident_patterns format (backward compatible)
- `scripts/dev/brain/modes-display.ps1` — Invoke-Health incident output
- `scripts/dev/brain/modes-agent.ps1` — Invoke-Agent incident output
- `.claude/brain/incident-ledger.json` — NEW (ledger file)
- `scripts/verify/dev.Tests.ps1` — new tests for composable conditions

## See Also

- `S1-honest-scoring.md` — parallel track (scoring changes are independent)
- `S1-learning-brain.md` — feedback loop consumes incident data
- `S2-multi-project.md` — cross-project incidents build on composable conditions
