<!-- topic: scoring — bundled in neurogrim-cli v3.2 -->
# Scoring — from per-domain measurements to unified score

Every Brain produces a single 0–100 unified score that summarizes
project health. That score is a *weighted, confidence-adjusted
aggregation* of per-domain scores, with optional floor gates that
let a single critical domain cap the unified score.

This document explains how scoring works so agents know what the
number means (and what it doesn't).

## The aggregation

```
unified_score = round( sum(d.effective_score * d.weight)
                     / sum(d.weight) )
```

Where:
- `d.effective_score = d.raw_score * (d.confidence / 100)` —
  domains with stale data contribute proportionally less.
- `d.raw_score` is straight from the CMDB envelope.
- `d.confidence` is from the envelope (if supplied) or computed
  from `updated_at` age decay.
- The sum runs over *weighted* domains only; advisory domains
  (weight 0.0) are visible but don't influence the score.

This is the **multiplier** model, the default. A `weighted_average`
model is also supported (raw scores, no confidence multiplier) but
rarely chosen — confidence-weighting is the spec default for
reasons documented in METHODOLOGY-EVOLUTION §4.

## Confidence

Confidence is the load-bearing concept. Without it, scoring would
treat a 7-day-old test result as equivalent to a fresh one, which
is dishonest.

Confidence comes from one of two sources:

1. **Envelope-supplied** (preferred). The sensor reports
   `confidence: 0..100` directly. Use when the sensor knows
   something the aggregator can't infer from age alone (e.g.,
   "this score is degraded because we couldn't reach the upstream
   API; trust it less").

2. **Age decay** (fallback). When the envelope omits
   `confidence`, the aggregator computes it from `updated_at`
   against the registry's `confidence_thresholds`:
   - `cmdb_fresh_days` (default 1.0) → confidence 100
   - `cmdb_stale_days` (default 3.0) → confidence linearly
     interpolating to ~50
   - `cmdb_very_stale_days` (default 7.0) → confidence 0

Low confidence pulls the unified score *down* (multiplicatively),
not up. A weighted domain at score 90 with confidence 20 contributes
the same as a domain at score 18 with confidence 100.

## Floor gates

A floor gate lets a single domain cap the unified score regardless
of others.

```json
"test-health": {
  "scoring_source": { ... },
  "floor": {
    "min_score": 25,
    "unified_cap": 50,
    "message": "Critical test health failure caps the score"
  }
}
```

Read this as: *if `test-health` falls below 25, the unified score
is capped at 50, regardless of how good every other domain looks.*

Floors are for "we don't ship if X is broken" gates — they encode
explicit veto power. Use sparingly; one or two floors per Brain is
typical.

## What the unified score *is* and *isn't*

- **Is**: a continuous, confidence-adjusted summary of how this
  project is doing across declared domains.
- **Isn't**: a quality grade, a productivity metric, an objective
  measure of code excellence. The score is only as honest as the
  domains' weights and the sensors' calibration.

The score is a *signal*. Agents should treat trends (improving /
degrading) as more meaningful than absolute values. A score of 78
with degrading trajectory is worse than 73 with stable trajectory.

## Trajectory

After ~5 score samples accumulate (`min_samples_for_trend`,
default 5), the trajectory classifier reports one of:

- `improving` — recent samples trending up beyond noise threshold
- `degrading` — recent samples trending down beyond noise threshold
- `stable` — within noise threshold
- `volatile` — high stddev across samples
- `no-data` (or "insufficient data") — not enough samples yet

`neurogrim trend` (alias `drift`) shows the full trajectory analysis.

## How to inspect scoring

- `neurogrim score` — single-line unified score with per-domain
  effective scores
- `neurogrim agent` — full JSON envelope including confidence,
  effective_score, trajectory per domain
- `neurogrim agent --prose` — agent-friendly prose summary
- `neurogrim health` — formatted dashboard (human-readable)
- `neurogrim trend` — trajectory analysis only
- `neurogrim doctor` — verify scoring sources resolve to readable files

## Cross-references

- `neurogrim explain domain` — domain anatomy + weight tiers
- `neurogrim explain sensor` — what produces a domain's per-sample score
- Spec §4 — scoring contract; §4.4 — confidence computation
- Spec §7 — trajectory analysis
