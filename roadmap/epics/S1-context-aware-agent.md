---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Context-Aware Agent (Hats System)

**Stage:** 1 — Honest Single Brain
**Status:** Complete (all 4 stories done)
**Priority:** Medium
**Depends on:** S1-learning-brain (hats need context to "read the room")
**Blocks:** S3-multi-agent (agent specialization needs hat system)
**Stage gate:** Satisfies "3+ hats defined with domain emphasis weights"

---

## Summary

Evolve the persona system from "personas" (be someone else) to "hats" (same agent, different
focus). A hat adjusts which Brain signals are salient — the security hat amplifies
least-privilege and supply-chain signals; the operator hat amplifies gates and artifacts.
Combined with the feedback loop, the agent can "read the room" like a person who acts
differently at dinner vs. a board meeting.

---

## Stories

### S1-CA-1: Hat Definitions — SHIPPED

**Status:** Complete
**Effort:** M
**Depends on:** none (design can start before s1-lb completes)

Define 3+ well-specified hats. Each hat has: a mindset, domain emphasis weights, and
operational focus. Replace or evolve `personas.md`.

**Key decisions:**
- How many hats to start? **3: operator, security, architect** — sufficient for Stage 1
- Do hats replace personas entirely, or coexist? **Coexist** — personas change mindset (who), hats change focus (what). The operator hat can be combined with the incident-commander persona.
- What's the file format? **Structured config in brain-registry.json** (domain_emphasis, suggest_when) + **skill file** (hats.md) for documentation and trigger phrases

**Acceptance criteria:**
- [x] At least 3 hats defined: operator, security, architect
- [x] Each hat has documented domain emphasis (which signals matter more)
- [x] Each hat has operational focus (what questions it asks first)
- [x] Skill file created/evolved (`.claude/skills/hats.md`)

---

### S1-CA-2: Hat-Specific Domain Emphasis — SHIPPED

**Status:** Complete
**Effort:** M
**Depends on:** S1-CA-1, S1-HS-2 (needs confidence-weighted scoring to be meaningful)

Each hat adjusts which domains the Brain emphasizes. Not different scores — different
*attention*. The security hat might weight least-privilege at 25% instead of 10% when
filtering recommendations.

**Key decisions:**
- Do hats modify scoring weights, or just filter/sort recommendations? **Just filter/sort** — hat emphasis is a multiplier on recommendation priority, not on domain scoring weights. Unified score is unchanged.
- Are hat emphasis weights in brain-registry.json or in hats.md? **brain-registry.json** (`config.hats.<name>.domain_emphasis`) — machine-readable, co-located with other scoring config
- How does the agent "put on" a hat? **Explicit `-Hat` parameter** on Find-Brain.ps1. Auto-suggested via `suggested_hat` in agent output.

**Acceptance criteria:**
- [x] Each hat has documented domain emphasis weights
- [x] `-Mode recommend` can be influenced by current hat
- [x] Hat emphasis is distinct from scoring weights (doesn't change the unified score)

---

### S1-CA-3: Context-Aware Hat Suggestion — SHIPPED

**Status:** Complete
**Effort:** S
**Depends on:** S1-CA-1, S1-CA-2

The Brain recommends which hat to wear based on current state. Many dirty gates? "Consider
the operator hat." Unreviewed IAM bindings? "Consider the security hat."

**Key decisions:**
- Where does the suggestion surface? **Both** `-Mode health` (visual) and `-Mode agent` (JSON `suggested_hat` field)
- How does the suggestion avoid being annoying? **Unambiguous winner only** — if two hats tie on signal count, no suggestion is made. Only suggests when one hat has strictly more matching signals.
- Can the agent auto-suggest or must the user request? **Auto-suggested** — appears in health and agent output when signals match. Non-intrusive; just informational.

**Acceptance criteria:**
- [x] Brain can suggest a hat based on current domain state
- [x] Suggestions are contextual (not random — tied to specific signals)
- [x] Suggestions are non-intrusive (only when strongly indicated)

---

### S1-CA-4: Hat Memory — SHIPPED

**Status:** Complete
**Effort:** M
**Depends on:** S1-CA-1, S1-LB-1 (needs proposal ledger for tag storage)

When wearing a hat, observations and decisions are tagged with the hat in the proposal
ledger. Switching back later, the agent can recall "last time I wore the security hat,
I noticed these three unreviewed bindings."

**Key decisions:**
- How is hat state persisted? **Tag in proposal ledger** — optional `hat` field added to ledger entries when `-Hat` is active
- Can hat memory span sessions? **Yes** — via proposal-ledger.json persistence on disk
- How does the agent query hat-specific history? **`Get-RecentOutcomes -HatFilter <hat>`** — agent mode auto-filters when `-Hat` is specified; without `-Hat`, all outcomes returned

**Acceptance criteria:**
- [x] Proposal ledger entries include optional `hat` field
- [x] `-Mode agent` can filter recent_outcomes by hat
- [x] Hat switches mid-session without losing context
- [x] Hat-specific observations are queryable across sessions

---

## Epic Completion Criteria

This epic is **done** when:
- [x] 3+ hats defined and documented
- [x] Each hat has domain emphasis that influences recommendations
- [x] Brain suggests hats based on state
- [x] Hat observations persist across sessions via ledger
- [x] Hats can switch mid-session without ceremony

## Data Architecture Notes

Hat definitions are Configuration (Pattern 3) — stored in `.claude/skills/hats.md`.
Hat observation tags are co-located in proposal ledger entries (Pattern 2).
No new persistent state files — reuses proposal-ledger.json from S1-LB.
See `DATA-ARCHITECTURE.md`.

## North Star Check

- Does this make the pattern more general? **Yes** — any project can define hats relevant
  to its domains. A mobile app might have "performance hat" and "accessibility hat."
- Does this make the ecosystem Brain easier? **Yes** — different hats could focus on
  different child projects or cross-project concerns.

## Files to Modify

- `.claude/skills/hats.md` — NEW (or evolved from personas.md)
- `.claude/skills/personas.md` — potentially deprecated or linked to hats.md
- `scripts/dev/brain/modes-display.ps1` — hat suggestion in Invoke-Health
- `scripts/dev/brain/modes-agent.ps1` — hat field in Invoke-Agent output
- `.claude/brain-registry.json` — optional hat emphasis weights

## See Also

- `S1-learning-brain.md` — prerequisite (hats need the context layer)
- `S3-multi-agent.md` — builds on hats for agent specialization
- `.claude/skills/personas.md` — current persona system being evolved
