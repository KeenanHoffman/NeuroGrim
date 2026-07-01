---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Calibration ledger test fixtures (E-B2-2 C8)

Three pre-canned `<domain>-calibration-ledger.jsonl` states used by
the schema-conformance and sensor-behavior test suites. Authored
2026-04-27 alongside the C8 commit.

## Files

| Fixture | State | Used by |
|---|---|---|
| `calibration-ledger-pending-only.jsonl` | One open pending entry awaiting triage. | C1 schema test; C4 sensor test |
| `calibration-ledger-pending-triaged.jsonl` | One pending + one triaged entry that supersedes it. open_pending=0; triaged=1. | C1 schema test; C4 sensor test |
| `calibration-ledger-malformed.jsonl` | One valid pending + one malformed line (`this is not json at all {`). Exercises the §17.2 reader's malformed-line skip path. | C1 schema test (validation skips malformed line); C4 sensor test (sensor doesn't crash) |

## Why fixed timestamps

All entries use the literal Unix-time anchor `1745000000.0` (~2025-04)
and offsets from it. The fixtures are reproducible — they don't
depend on `chrono::Utc::now()`. Tests that need fresh-confidence
semantics build their own ledgers inline using `now_secs()`; tests
that exercise schema or reader behavior use these stable fixtures.

## E-B2-8 dogfood handoff

These fixtures roll forward into the cross-Brain dogfood at E-B2-8.
When the four Brains' calibration ledgers are exercised end-to-end
(operator triage → sensor read → A2A advisory aggregate), the same
three states cover the canonical scenarios:

- **pending-only** — operator has been notified but hasn't responded
  (the §17 "calibration backlog unattended" case).
- **pending+triaged** — full lifecycle round-trip (the §17 "operator
  reviewed and decided" case).
- **malformed** — defensive invariant: a corrupted line never
  crashes the sensor (§17.2 MUST).

E-B2-8 may copy these into per-Brain `.claude/brain/` paths to seed
dogfood scenarios; see Layer-2 plan (E-B2-2 C8 → E-B2-8) for the
hand-off contract.

## Fixture schema invariants (pinned)

Each well-formed entry conforms to
`LSP-Brains/schemas/domain-calibration-ledger-v1.schema.json` (C1).
The schema-conformance tests at
`tests/calibration_ledger_schema_conformance.rs` validate this
end-to-end: every line in every fixture (modulo the malformed one)
parses + validates against the schema. Drift between fixtures and
schema is caught at test-time.
