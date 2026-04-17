# Epic: Honest Scoring

**Stage:** 1 — Honest Single Brain
**Status:** Complete
**Priority:** High
**Depends on:** Brain decomposition (complete)
**Blocks:** S1-learning-brain
**Stage gate:** Satisfies "unified score incorporates confidence" and "fully-observed > partially-observed"

---

## Summary

The unified score currently lies by omission. Domains with no data default to optimistic
scores. Supply-chain returns 100 at 0% confidence. Unknown artifacts score 30 instead of 0.
This epic makes the score honest by implementing two complementary weighting models, applying
confidence to the unified calculation, and defining when advisory domains are mature enough
to participate.

---

## Stories

### S1-HS-1: Floor and Multiplier Scoring Models

**Status:** Complete
**Effort:** L
**Depends on:** none

Implement two confidence-handling models in `Build-Scorecard` and `Get-UnifiedScore`:

- **Floor model:** Confidence below threshold caps domain contribution. Actionable — tells
  you "go collect data to unlock this score."
- **Multiplier model:** `effective_score = raw_score * confidence`. Honest — a 90 at 20%
  confidence contributes 18, not 90.

Both models should be available. The Brain selects based on context: multiplier for
diagnostic modes (health, agent), floor for action-oriented modes (recommend, gate-advisor).

**Key decisions:**
- What confidence threshold defines the floor? (30%? 50%?)
- Should the floor be a hard cutoff or a smooth curve (sigmoid)?
- How do we handle domains that can never reach high confidence in fast mode?

**Acceptance criteria:**
- [x] `Build-Scorecard` returns both `raw_scores` and `effective_scores` per domain
- [x] Floor model caps contribution when confidence < threshold
- [x] Multiplier model computes `raw * confidence` per domain
- [x] Tests verify: domain at 100/0% contributes less than domain at 50/100%
- [x] Both models are selectable (parameter or mode-dependent)

---

### S1-HS-2: Confidence-Weighted Unified Score

**Status:** Complete
**Effort:** L
**Depends on:** S1-HS-1

Apply the selected scoring model to `Get-UnifiedScore`. The unified number becomes
trustworthy — running a drift check visibly improves the score because confidence rises.

**Key decisions:**
- Which mathematical algorithm for the weighted sum? Candidates:
  - Simple multiplicative: `sum(weight * score * confidence)`
  - Sigmoid curve: `sum(weight * score * sigmoid(confidence))`
  - Geometric mean: penalizes weak links more than arithmetic mean
  - Bayesian: treat scores as estimates with confidence as prior strength
- Should the algorithm be configurable in brain-registry.json?
- How to prevent confidence manipulation (running gates just to boost confidence)?

**Acceptance criteria:**
- [x] Unified score uses confidence, not just raw scores
- [x] Increasing confidence (by running checks) visibly improves unified score
- [x] A fully-observed system at 70 scores higher than a partially-observed system at 85
- [x] Algorithm is documented in brain.md
- [x] `-Mode score` output format updated to show effective scores

---

### S1-HS-3: Advisory Domain Promotion Criteria

**Status:** Complete
**Effort:** S
**Depends on:** S1-HS-2

Define and document when an advisory domain (weight=0) should be promoted to scored
(weight>0). This is a governance decision, not a code change — but needs clear criteria.

**Key decisions:**
- Should promotion be automatic (criteria-based) or manual (human decision)?
- Can a domain be demoted back to advisory?
- What weight should promoted domains receive? Redistribute from existing?

**Acceptance criteria:**
- [x] Promotion criteria documented in brain.md
- [x] Infrastructure and git-tree evaluated against criteria
- [x] brain-registry.json supports advisory flag per domain weight

---

## Epic Completion Criteria

This epic is **done** when:
- [x] Unknown/no-data domains visibly drag the score down
- [x] The unified score is honest enough to inform deploy decisions
- [x] Both floor and multiplier models are implemented and tested
- [x] Advisory domain promotion path is documented
- [x] `-Mode score`, `-Mode health`, and `-Mode agent` all use honest scoring

## Data Architecture Notes

No new persistent state. Changes are in-memory scoring logic within `scoring.ps1`.
May add `scoring_algorithm` config to `brain-registry.json` (Pattern 3: Configuration).
See `DATA-ARCHITECTURE.md`.

## North Star Check

- Does this make the pattern more general? **Yes** — honest scoring transfers to any domain
  set. The floor/multiplier models are domain-agnostic.
- Does this make the ecosystem Brain easier? **Yes** — child Brain scores that incorporate
  confidence give the parent Brain honest signals to aggregate.

## Files to Modify

- `scripts/dev/brain/scoring.ps1` — Build-Scorecard, Get-UnifiedScore, all Get-*Score
- `scripts/dev/brain/modes-display.ps1` — Invoke-Score, Invoke-Health output format
- `scripts/dev/brain/modes-agent.ps1` — Invoke-Agent JSON output
- `.claude/brain-registry.json` — optional scoring_algorithm config
- `.claude/skills/brain.md` — document scoring formula
- `scripts/verify/dev.Tests.ps1` — new tests for honest scoring

## See Also

- `S1-learning-brain.md` — blocked by this epic (needs trustworthy scores for deltas)
- `S1-diagnostic-reasoner.md` — parallel track (independent of scoring changes)
- `.claude/roadmap/DATA-ARCHITECTURE.md` — canonical state locations
