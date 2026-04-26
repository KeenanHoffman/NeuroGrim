# layer-3/ — Agent-Assisted Review fixtures

Each subdirectory is one fixture exercising the
`supply-chain-review` framework. Per spec §16.4, Layer 3
calibration is fundamentally different from L1/L2:

- L1 + L2 calibrate via **deterministic FP/FN** — the sensor's
  output is compared to a fixed reference.
- L3 calibrates via **human-agreement** — operators wear the
  `supply-chain-auditor` hat and reach decisions; we measure
  whether the decisions align with the fixture-author's
  reference and across operators over time.

## v1 limitation honestly disclosed

v1 ships the framework + 4-6 reference fixtures. We have ZERO
real human-agreement data because the framework just shipped in
E-SC-6. The calibration report flags
`l3_human_agreement_data: "insufficient — framework just shipped;
collect after 30 days of real triage"`.

Operators MUST gather data themselves before any L3 promotion
past advisory weight.

## Fixture-specific files

Each layer-3 fixture's directory contains:

- `fixture.toml` — metadata (required).
- `ticket.json` — pre-built review ticket matching
  `crate::supply_chain_review::ticket::ReviewTicket` shape.
- (optional) `signals.json` — additional Layer 1+2 signal context
  the operator-with-hat should consider while triaging.

## Expected outputs format

```toml
[expected]
# Reference decision the fixture-author asserts an
# operator-with-hat SHOULD reach.
# One of: "accept" | "reject" | "pin-to-last-good" | "no-action"
reference_decision = "pin-to-last-good"

# Reference rationale (prose). Used in human-agreement reports
# to surface why the fixture-author chose this decision.
reference_rationale = """
The publish-cadence acceleration paired with a maintainer-delta
in the same window is a "watch this dep" pattern. Pinning to
last-known-good is the conservative posture pending upstream
context.
"""

# Fixture-author's self-assessed confidence in the reference
# decision (0.0 to 1.0). Lower confidence = more room for
# operator disagreement; higher = clearer "right answer."
fixture_author_confidence = 0.75

# Optional: alternate decisions the fixture-author considers
# defensible. Listed here so a calibration run that returns one
# of these is "near-agreement" rather than full disagreement.
defensible_alternatives = ["no-action"]
```

## Layer 3 harness behavior at v1

Per E-SC-6 Option A (the v1 ship), there is NO automated agent
invocation. The harness for L3 fixtures:

1. Loads the fixture's `ticket.json`.
2. Validates it against the §16.7 schema.
3. Reports "framework-ready" status — i.e., the fixture is
   well-formed and would be triageable by a human-with-hat.
4. Records the fixture's `reference_decision` for later
   human-agreement comparison.

That's it. No actual decision is reached at calibration time —
because the only way to reach a real decision is with a human in
the loop.

When operators DO triage these fixtures (manually, off-fixture-
library), they record their decision in the standard
supply-chain-decision-ledger. A future calibration-pass-2 run
can read the ledger + match against `reference_decision` to
compute human-agreement.

That cross-run measurement is **out of scope for v1**. v2
candidate.

## v1 known-good controls for L3

For L3, "known-good" means: a ticket that was opened by mistake
or for a benign reason, where the right operator decision is
`no-action` or `accept`. Useful to surface fixture-author bias
("of course operators should pin this") — if a fixture leans
heavily on one decision, the library author balances with a
known-good control of the opposite shape.

## v1 fixture seed set

For v1, the fixture-library author seeds 4-6 L3 fixtures
spanning:

- Two `known-bad` (signals point to a real issue; reference
  decision is `pin-to-last-good` or `reject`).
- Two `known-good` (signals are FP-prone Layer 2 noise;
  reference decision is `no-action` or `accept`).
- One or two `edge-case` (the decision genuinely depends on
  operator context — fixture flags this with low
  `fixture_author_confidence`).

## Layer 3 calibration semantics

Per scaffolding: target ≥80% human-agreement after first month.
v1 has 0% human-agreement data; framework-only ship.

Real measurement requires:
1. Operators run calibration triage as part of their normal flow.
2. Their decisions land in the ledger.
3. A future calibration run reads the ledger + the fixture's
   `reference_decision` and computes `agreement_rate`.

That's a v2 capability. v1 ships the foundation.
