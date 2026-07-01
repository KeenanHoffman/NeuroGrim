---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# supply-chain-fixtures — Calibration Fixture Library (E-SC-8)

This directory holds the **calibration fixtures** for NeuroGrim's
three-layer supply-chain stack. Each fixture is a small, self-
contained input that one of the supply-chain sensors should
produce a specific output for. The harness runs the sensor against
each fixture, compares actual output to expected, and tallies
false-positive / false-negative rates.

**Calibration is what makes trust-promotion possible.** Per spec
§16 + METHODOLOGY-EVOLUTION §15, all three supply-chain domains
default to advisory weight 0.0; they CAN be promoted past advisory
only via the §15.5 promotion path with calibration evidence
attached. This library is that evidence.

## Layout

```
tests/supply-chain-fixtures/
├── README.md                                      (this file)
├── layer-1/                                       (Mechanical SCA)
│   ├── README.md
│   └── <fixture-id>/
│       ├── fixture.toml                           (metadata)
│       └── (Cargo.lock | uv.lock | requirements.txt | package-lock.json)
├── layer-2/                                       (Vigilance)
│   ├── README.md
│   └── <fixture-id>/
│       ├── fixture.toml
│       └── metadata.json                          (frozen registry response)
│       └── (optional) prior-state.json            (per-package state for delta sensors)
└── layer-3/                                       (Agent-assisted review)
    ├── README.md
    └── <fixture-id>/
        ├── fixture.toml
        ├── ticket.json                            (review-ticket fixture)
        └── (optional) signals.json                (Layer 1+2 signals fed into the ticket)
```

## `fixture.toml` schema

Every fixture has a `fixture.toml` with the following fields:

```toml
# Stable, kebab-case identifier. Matches the directory name.
id = "litellm-1.82.7-synthetic"

# One of: "known-bad" | "known-good" | "edge-case"
label = "known-bad"

# Layer this fixture exercises: "1" | "2" | "3"
layer = "2"

# Human-readable description of what this fixture demonstrates.
description = """
Synthetic reproduction of the LiteLLM 1.82.7 publish-cadence
acceleration pattern. Recreates the timing signature without
the live malicious payload — see ../../README.md for the
synthetic-vs-live posture.
"""

# Optional attack-pattern tag for grouping reports.
attack_pattern = "publish-cadence-acceleration"

# Optional list of references (URLs, advisory ids).
references = [
  "https://news.ycombinator.com/item?id=47501426",
  "https://github.com/BerriAI/litellm/issues/...",
]

# Optional fixture author + date.
author = "operator-handle"
authored_at = "2026-04-26"

# === Layer-specific expected outputs ===

# For layer = "1", expected advisory IDs that should be detected.
# Empty array on known-good fixtures.
[expected]
advisory_ids = ["RUSTSEC-YYYY-NNNN"]

# For layer = "2", expected vigilance finding kinds + packages.
# Empty array on known-good fixtures (clean control).
[expected]
findings = [
  { kind = "publish-cadence-acceleration", package = "litellm-synthetic", ecosystem = "PyPI" },
]

# For layer = "3", expected operator-decision (the reference
# decision a fixture-author asserts an operator-with-hat should
# reach). Plus a confidence note from the fixture-author.
[expected]
reference_decision = "pin-to-last-good"
reference_rationale = """
The publish-cadence acceleration is diagnostic; operator should
pin to the last known-good version pending upstream context.
"""
fixture_author_confidence = 0.75
```

## LiteLLM-fixture safety posture

The 2026-04-23 LiteLLM 1.82.7/.8 incident is the canonical
motivating case for this entire scaffolding (METHODOLOGY-EVOLUTION
§15). Two fixtures reference it:

- `layer-2/litellm-1.82.7-synthetic/` — synthetic reproduction of
  the publish-cadence + maintainer-delta + exfil-pattern signal
  set. Safe to commit; uses extracted patterns, not live malicious
  code.
- `layer-2/litellm-1.82.7-live-fetch/` (optional, NOT shipped) —
  the live malicious tarball, fetched on demand by operators who
  want strictest tests. The fixture's `fixture.toml` documents the
  fetch URL + SHA-256; the harness fetches + verifies + safely
  extracts under E-SC-5's tarball-extraction discipline. Operators
  on air-gap or on repos where VirusTotal flags are problematic
  skip this fixture (graceful skip + report note).

## How to add a fixture

1. Pick the layer (1, 2, or 3) the fixture exercises.
2. Choose a kebab-case id; create
   `<layer>/<id>/fixture.toml`.
3. Fill in metadata + expected outputs.
4. Add the layer-specific artifact (lockfile / metadata.json /
   ticket.json).
5. Run `cargo test --features calibration -p neurogrim-sensory
   <layer>` to verify the harness reads + processes your fixture
   correctly.
6. (Optional but encouraged) Run
   `neurogrim sca-calibrate --project-root .` to see your
   fixture in the aggregate report.
7. Commit. Operators reviewing your PR check:
   - Is the fixture honest about what it tests?
   - Are `references[]` actually relevant (no fake citations)?
   - Are `expected_outputs` reproducible (others reading the
     fixture would predict the same)?
   - Is `attack_pattern` mapped to a real attack class?

## Sample-size honesty

v1 ships ~8-12 fixtures per layer. The smallest measurable false-
positive rate at that sample size is ~10%. This is BELOW the
scaffolding's target rates (L1 <1%, L2 <5%). The calibration
report explicitly flags `pass-with-sample-size-warning` rather
than `pass` until the library grows to ≥30 per layer.

This is intentional honesty: v1 ships the MECHANISM. The DATA
to back trust-promotion claims grows over time as adopters
contribute fixtures from their own dep graphs.

Quarterly cadence is the recommended baseline for re-running
calibration + auditing fixture relevance. Stale fixtures (e.g.,
references to long-resolved CVEs) are candidates for retirement.

## Cross-references

- Per-epic plan: `~/.claude/plans/parallel-hugging-eich-e-sc-8.md`
- Spec normative: LSP-Brains v2.6 §16, METHODOLOGY-EVOLUTION §15
- Existing patterns:
  - `D:/Brains/agent-behavior-runner/scenarios/` — ABR's per-
    scenario directory format inspired this layout.
  - `D:/Brains/LSP-Brains/schemas/calibration-report-v1.schema.json` — the supply-chain-calibration-report schema aligns with this shape for future spec-promotion (v2.7 candidate).
- Operator guide: `docs/supply-chain-calibration.md`
