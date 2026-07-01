---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Learning Brain

**Stage:** 1 — Honest Single Brain
**Status:** Complete (all 4 stories done)
**Priority:** High
**Depends on:** S1-honest-scoring (needs trustworthy scores to measure deltas)
**Blocks:** S1-context-aware-agent, S4-prescriptive-autonomy
**Stage gate:** Satisfies "proposal ledger records outcomes; can rank by effectiveness"

---

## Summary

The Brain is stateless between invocations — it scores, recommends, forgets. This epic
closes the feedback loop: track what was proposed, what was executed, what the score delta
was. Over time the Brain builds a model of which actions actually improve the system.
Not ML — just bookkeeping. But it transforms the Brain from a static oracle into an advisor
that gets better with use.

This is also the foundation for the **context layer** — persistent awareness that empowers
the agent to manage both big picture and small picture without losing focus during context
switching.

---

## Stories

### S1-LB-1: Proposal Ledger

**Status:** Complete
**Effort:** M
**Depends on:** none (can begin once S1-honest-scoring is in progress)

Record every `-Mode propose` output with timestamp, proposals, and pre-score.
When a gate clears or score changes, record the post-score.

**Key decisions:**
- Ledger format: JSONL (one entry per line, easy append) or JSON array (easier to query)?
- Trigger: who writes post-score? A hook after gate completion? Explicit `-Mode record`?
- How to correlate a proposal with its outcome? (timestamp proximity? explicit proposal ID?)

**Acceptance criteria:**
- [x] `-Mode propose` writes proposals + pre-score to `.claude/brain/proposal-ledger.json`
- [x] Post-score is recorded when score changes (resolved on next propose run)
- [x] Each ledger entry has: timestamp, proposals[], pre_score, post_score (nullable), commit
- [x] Auto-prune entries older than 90 days on write
- [x] Ledger degrades gracefully (missing file = empty history, write failure = warn + continue)

---

### S1-LB-2: Outcome Correlation

**Status:** Complete
**Effort:** M
**Depends on:** S1-LB-1

After 10+ recorded outcomes, rank proposal types by average score delta. "Drift checks
average +8. Smoke tests average +4. Topology updates average +2."

**Key decisions:**
- How to categorize proposals? By gate key? By domain? By action type (clear-gate, rebuild-artifact)?
- Minimum sample size before correlations are reported? (10? 20?)
- How to handle proposals where post-score was never recorded? (session ended, no follow-up)

**Acceptance criteria:**
- [x] Outcome correlations computed from ledger data on demand (`Get-ProposalEffectiveness`)
- [x] `-Mode agent` output includes `proposal_effectiveness` when data exists
- [x] Categories are meaningful (grouped by action_type: clear-gate, rebuild-artifact, topology-refresh, drift-check)
- [x] Missing post-scores are excluded from correlation (entries with null post_score skipped)

---

### S1-LB-3: Recommendation Boosting

**Status:** Complete
**Effort:** M
**Depends on:** S1-LB-2

Proposals with historically high score deltas get priority weight in `-Mode recommend`.
The Brain stops recommending actions that don't help.

**Key decisions:**
- How much should historical effectiveness influence priority? (additive boost? multiplicative?)
- Should the Brain ever *suppress* a recommendation that historically has low impact?
- How does boosting interact with tier weights and downstream impact?

**Acceptance criteria:**
- [x] `-Mode recommend` factors historical effectiveness into priority ranking (multiplicative: `1 + avg_delta/50`)
- [x] Actions with proven high impact rank higher than equal-tier actions without history
- [x] Boosting is transparent: output includes `boosted:` line with avg delta and sample count
- [x] Boosting respects minimum sample size (requires `sufficient` = 5+ samples)

---

### S1-LB-4: Session Continuity (Context Layer)

**Status:** Complete
**Effort:** L
**Depends on:** S1-LB-1

The context layer persists across conversations. A new session can ask "what did we do last
time and did it work?" Combined with the hats system (S1-CA), this means the agent can
"read the room" based on recent operational history.

**Key decisions:**
- How does session start integrate with the ledger? Auto-load last N entries?
- Should `-Mode agent` always include recent proposal outcomes?
- How does the context layer interact with `session-recap.md`?
- Is this the same as operational-memory.md queries, or a new capability?

**Acceptance criteria:**
- [x] `-Mode agent` includes `recent_outcomes` from the last 7 days
- [x] Session recap can reference what was proposed and what happened (via `recent_outcomes`)
- [x] Context layer is queryable via `-Mode agent` JSON output
- [x] Works across sessions (persisted to `.claude/brain/proposal-ledger.json`)

---

## Epic Completion Criteria

This epic is **done** when:
- [x] Proposals are recorded with outcomes in the ledger
- [x] Historical effectiveness is computed and influences recommendations
- [x] The Brain can answer "what did we do last time and did it work?" (via `recent_outcomes`)
- [x] Context persists across sessions (proposal-ledger.json on disk)
- [x] All ledger operations degrade gracefully (try/catch, best-effort, missing file = empty)

## Data Architecture Notes

Introduces: `.claude/brain/proposal-ledger.json` (Pattern 2: Ledger).
Append-only, auto-prune entries older than 90 days.
Schema: `{ timestamp, proposals: [{id, command, domain, action_type}], pre_score, post_score, commit, hat? }`.
See `DATA-ARCHITECTURE.md`.

## North Star Check

- Does this make the pattern more general? **Yes** — any project with a scoring function
  can track proposal outcomes. The ledger schema is domain-agnostic.
- Does this make the ecosystem Brain easier? **Yes** — a parent Brain could aggregate
  proposal effectiveness across child projects to find org-wide patterns.

## Files to Modify

- `scripts/dev/brain/modes-agent.ps1` — Invoke-Agent (include recent_outcomes), Invoke-Propose (write ledger)
- `scripts/dev/brain/modes-display.ps1` — Invoke-Recommend (boosting)
- `.claude/brain/proposal-ledger.json` — NEW (ledger file)
- `.claude/skills/session-recap.md` — integrate ledger context
- `scripts/verify/dev.Tests.ps1` — new tests for ledger operations

## See Also

- `S1-honest-scoring.md` — prerequisite (needs trustworthy scores)
- `S1-context-aware-agent.md` — builds on this (hats need context to "read the room")
- `S4-prescriptive-autonomy.md` — builds on this (autonomous action needs outcome data)
- `.claude/skills/operational-memory.md` — existing query patterns for GCS data
