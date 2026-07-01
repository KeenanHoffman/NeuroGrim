---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# supply-chain-calibration — Three-Layer Calibration Harness

The `supply-chain-calibration` framework is NeuroGrim's evidence-
collection mechanism for deciding whether any of the three
supply-chain domains can be promoted past advisory weight 0.0.
Per spec §15.5 + §16, all three supply-chain domains
(`supply-chain-sca`, `supply-chain-vigilance`, `supply-chain-review`)
default to advisory weight; promotion requires calibration evidence
that the sensor's false-positive / false-negative rates meet
documented targets.

Shipped 2026-04-26 as part of E-SC-8.

## What calibration measures

Three different shapes per layer:

### Layer 1 — Mechanical SCA (regression-check)

Sensor is exact-match deterministic (lockfile parse + OSV +
RustSec + accepted-advisories). Calibration verifies the sensor
doesn't panic on edge cases + produces a CMDB on every fixture.
**Any non-zero FP/FN is a critical regression** (a bug, not a
calibration miss).

Target: <1% FP, <1% FN. v1 has 5 structural fixtures; advisory-
matching coverage is exercised separately by
`crates/neurogrim-sensory/tests/sensor_behavior.rs` against live
OSV.

### Layer 2 — Vigilance (probabilistic-heuristic)

Sensors are heuristic; calibration measures FP/FN against frozen
registry-metadata snapshots. Target: <5% FP, <20% FN.

v1 ships 8 deterministic fixtures using a relative-date format
(e.g., `published_days_ago: 30`) so fixtures don't decay over
time. Ad-hoc additions encouraged.

### Layer 3 — Agent-assisted review (human-agreement)

Calibration is fundamentally different here. v1 ships **framework
only** — no automated agent invocation, no human-agreement data.
Real measurement requires ≥30 days of operator triage history
collected against the fixture library.

Target: ≥80% human-agreement after first month. v1 has 0 days
of triage data because the L3 framework just shipped in E-SC-6.

## The fixture library

Lives at `tests/supply-chain-fixtures/` per the layout documented
in [the library README](../neurogrim/tests/supply-chain-fixtures/README.md).

```
tests/supply-chain-fixtures/
├── README.md                                   (library guide)
├── layer-1/  (Mechanical SCA)
│   ├── README.md
│   ├── 01-rust-clean-cargo-lock/
│   ├── 02-pypi-clean-uv-lock/
│   ├── 03-no-lockfile-error/
│   ├── 04-rust-with-rustsec-paste/
│   └── 05-empty-lockfile/
├── layer-2/  (Vigilance — 8 fixtures)
│   └── ...
└── layer-3/  (Agent-assisted Review — 4 fixtures)
    └── ...
```

v1 ships ~5-8 fixtures per layer. The smallest measurable rate
at that sample size is ~10-20%, well above the spec targets.
Calibration status flags this as `pass-with-sample-size-warning`
rather than fake-precise `pass`.

Adopters' clean lockfiles + their own discovered patterns are
encouraged contributions. The library grows over time toward
the ≥30-fixtures-per-layer target.

## Running calibration

### CLI (recommended)

```bash
neurogrim sca-calibrate \
    --project-root . \
    --output .claude/supply-chain-calibration-report.json
```

Prints a human-friendly summary to stderr; emits the JSON report
to the `--output` path. The report is gitignored at
`.claude/supply-chain-calibration-report.json` (operator-local
artifact).

### CI integration with `--check-promotion-ready`

```bash
neurogrim sca-calibrate \
    --project-root . \
    --output .claude/supply-chain-calibration-report.json \
    --check-promotion-ready
```

**Exit-code contract (clarified per 2026-04-26 PRE-RELEASE C15):**

| Invocation | Exit code | Meaning |
|---|---|---|
| `sca-calibrate` (no flag) | 0 on harness success, 1 on harness error | The calibration ran; report was emitted. Operators reading this exit code learn only "did the harness itself succeed", not "is the supply-chain stack promotion-ready". |
| `sca-calibrate --check-promotion-ready` | 0 if all three layers pass + sample size sufficient + ≥80% L3 human-agreement; 1 otherwise | The CI-gating shape. **v1 always returns exit 1 with this flag** — we lack ≥30 fixtures + ≥30 days of L3 triage data. This is honest signal: do not promote any supply-chain domain past advisory weight yet. |

The previous prose conflated these two cases ("v1 always returns
exit 1 by design") in a way that suggested the unflagged
invocation also always failed. It does not — the unflagged form
exits 0 on a successful harness run, regardless of promotion-
readiness. CI scripts that want to gate on promotion-readiness
MUST use `--check-promotion-ready`.

### Cargo test integration

```bash
cargo test -p neurogrim-sensory --test calibration_behavior
```

Runs the calibration harness inline as integration tests. Useful
for CI to catch regressions in fixtures or evaluators.

## Calibration report shape

JSON output schema (NeuroGrim-internal v1; spec-promote candidate
for LSP-Brains v2.7):

```json
{
  "run_id": "cal-2026-04-26-192734-8a02",
  "schema_version": "1",
  "harness_version": "1.0.0",
  "started_at": "2026-04-26T19:27:34.123Z",
  "finished_at": "2026-04-26T19:27:34.456Z",
  "fixture_library_path": "tests/supply-chain-fixtures",
  "layer_1": {
    "layer": "1",
    "sample_size": 5,
    "fixtures_evaluated": 5,
    "fixtures_errored": 0,
    "tp_count": 1,
    "tn_count": 2,
    "fp_count": 0,
    "fn_count": 0,
    "fp_rate": 0.0,
    "fn_rate": 0.0,
    "target_fp_rate": 0.01,
    "target_fn_rate": 0.01,
    "min_sample_size": 30,
    "meets_target": true,
    "status": "pass-with-sample-size-warning",
    "fixture_results": [...]
  },
  "layer_2": { ... },
  "layer_3": {
    "layer": "3",
    "status": "framework-only",
    "human_agreement_data": "insufficient — framework just shipped...",
    "fixtures_with_reference_decision": 4,
    ...
  },
  "overall_status": "pass-with-sample-size-warning",
  "statistical_validity_note": "Layer 1: 5 fixtures evaluated; smallest measurable rate ~20%. ...",
  "promotion_ready": {
    "ready": false,
    "gaps": [
      "Layer 1 sample size 5 < required 30 for statistical validity",
      "Layer 2 sample size 8 < required 30 for statistical validity",
      "Layer 3: human-agreement data insufficient ..."
    ]
  }
}
```

## Status taxonomy

Per-layer + overall statuses, worst-to-best:

- **`red-miss`** — A known-bad fixture went UNDETECTED. Critical
  regression; the sensor missed an attack pattern it should
  catch.
- **`target-miss`** — Layer's FP or FN rate exceeded its target.
- **`pass-with-sample-size-warning`** — Targets met but below
  `MIN_SAMPLE_SIZE` (30 per layer). Honest disclosure: we can't
  claim the rates with statistical validity yet.
- **`framework-only`** — Layer 3 v1: framework shipped, no
  human-agreement data collected yet.
- **`no-fixtures`** — No fixtures discovered for this layer.
- **`pass`** — Targets met AND sample size sufficient. Suitable
  for promotion-readiness gating.

## Path to promotion-ready

For any supply-chain domain to promote past advisory weight 0.0
via the §15.5 protocol, the calibration report must show:

1. **L1**: status `pass`, sample size ≥30, FP+FN rates ≤1%.
2. **L2**: status `pass`, sample size ≥30, FP rate ≤5%, FN rate ≤20%.
3. **L3**: status `pass`, ≥30 days of operator triage history with
   ≥80% human-agreement against fixture-author reference decisions.

That's the bar. v1 ships none of those. The path:

- **Grow the fixture library** — adopters contribute fixtures
  from their own dep graphs. Fixture-authoring guide in the
  library README.
- **Triage real Layer 2 alerts** — the auto-create bridge from
  E-SC-6 generates real review tickets. Operator decisions on
  those tickets compound into the L3 human-agreement dataset.
- **Refresh fixtures quarterly** — calibration is a moving
  target; yesterday's attack patterns aren't necessarily today's.
  Stale fixtures + un-triaged tickets degrade the calibration
  signal.

## When calibration surfaces a regression

Per `audit/ROLLBACK-PLAYBOOK.md § E-SC-8`:

1. **`red-miss` on Layer 1 or Layer 2** — a known-bad fixture
   stopped firing. Likely cause: a recent sensor change broke
   detection, OR a fixture's expected outputs are stale. Bisect
   by running the sensor + fixture in isolation.
2. **`target-miss` on Layer 2** — FP or FN rate climbed above
   target. Likely cause: heuristic threshold drift (per
   E-SC-5's dogfood-tuning history). Document the signal in the
   ROLLBACK ledger; tune the heuristic OR add fixtures that
   illustrate the new attack pattern.
3. **Sample-size warning** — expected at v1; nothing to fix.
   Operators wanting `pass` add fixtures.

## Dogfood baseline (v1 ship, 2026-04-26)

NeuroGrim's own calibration baseline at v1 ship:

| Layer | Sample | Status | FP | FN |
|---|---|---|---|---|
| Layer 1 | 5 | pass-with-sample-size-warning | 0 | 0 |
| Layer 2 | 8 | pass-with-sample-size-warning | 0 | 0 |
| Layer 3 | 4 | framework-only | n/a | n/a |
| **Overall** | **17** | **pass-with-sample-size-warning** | — | — |

`promotion_ready: false` — gaps documented. This is the honest
v1 baseline; it grows over time as fixtures + L3 triage history
accumulate.

## Cross-references

**Spec (LSP-Brains v2.6, 2026-04-25):**

- §15.5 Promotion Path — the governance mechanism calibration
  evidence supports.
- §16 Supply-chain Awareness — the three-layer framework being
  calibrated.
- METHODOLOGY-EVOLUTION §15 — rationale for the three-layer
  scaffolding.

**Plans + scaffolding:**

- Ecosystem plan: `~/.claude/plans/parallel-hugging-eich.md` (E-SC-8)
- Per-epic plan: `~/.claude/plans/parallel-hugging-eich-e-sc-8.md`
- Rollback procedures: `audit/ROLLBACK-PLAYBOOK.md § E-SC-8`
- Trust-chain notes: `audit/TOOL-TRUST-NOTES.md` 2026-04-26 entry

**Companion docs:**

- [`docs/supply-chain-sca.md`](supply-chain-sca.md) — Layer 1
- [`docs/supply-chain-vigilance.md`](supply-chain-vigilance.md) — Layer 2
- [`docs/supply-chain-review.md`](supply-chain-review.md) — Layer 3
- [`tests/supply-chain-fixtures/README.md`](../neurogrim/tests/supply-chain-fixtures/README.md) — fixture library guide

## Out of scope (v1; deferred)

- **JSON schema for the report** — v1 ships the Rust types as
  the schema. Spec-promote to LSP-Brains v2.7 candidate once the
  shape settles.
- **Cross-layer composition** — each layer calibrated independently.
  Composite calibration ("does L1 + L2 + L3 together hit the right
  end-to-end FP/FN?") is v2 candidate.
- **Live LiteLLM payload** — v1 ships SYNTHETIC reproduction (per
  the 2026-04-26 user-locked decision). Operators wanting strictest
  test fetch the live payload separately + run E-SC-5's
  exfil_indicator with `NEUROGRIM_VIGILANCE_EXFIL=1`.
- **Pre-cached OSV responses for L1 fixtures** — v1 doesn't
  cache OSV responses per-fixture (would require fixture authors
  to capture + commit). v2 candidate; would make L1 advisory-
  matching deterministic.
- **Cross-operator agreement signal for L3** — when ≥2 operators
  triage the same fixture, their decisions can be compared as a
  meta-signal of fixture-author bias. v2 candidate.
- **Calibration-drift sensor** — `capability-hygiene`-style drift
  detection on fixture freshness (last refresh date, fixture-
  references-stale-CVE, etc.). v2 candidate.
