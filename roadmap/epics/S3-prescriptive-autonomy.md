---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Prescriptive Autonomy

**Stage:** 3 — Prescriptive Autonomy (promoted from original Stage 4)
**Status:** Complete (S3-PA-1, S3-PA-2, S3-PA-3 complete)
**Priority:** --
**Depends on:** S1-learning-brain (outcome data), S1-context-aware-agent (hat system), S2-interface-contract
**Blocks:** S4-fractal-composition, S5-transferable-practice
**Stage gate:** Satisfies all Stage 3 transition criteria
**Supersedes:** S4-prescriptive-autonomy.md (archived — same stories, renumbered)

---

## Summary

The Brain evolves from diagnostic ("here's what's wrong") to prescriptive ("I'm going to
fix these safe things and present these risky things for your approval"). The human reviews
decisions, not checklists.

**Why this moved up from Stage 4:** The adversary review (2026-04-09) found that prescriptive
autonomy depends on outcome data (S1-LB, done) and an autonomy gradient (new work) — NOT on
multi-agent coordination (old S3, eliminated). Promoting this to Stage 3 shortens the
critical path and gives the fractal composition stage (new S4) a parent Brain with "teeth."

---

## Stories

### S3-PA-1: Autonomy Gradient Definition

**Status:** Complete
**Effort:** L
**Depends on:** S1-LB (outcome data exists), S2-IC-1 (interface contract for audit trail)
**Completed:** 2026-04-10

Define which classes of action the Brain can auto-execute and which require human approval.
The gradient is: fully automatic → auto with notification → present for approval → blocked.

**Key decisions:**
- 4 gradient levels: auto, notify, approve, blocked — all audited
- 6 action types: clear-gate, rebuild-artifact, refresh-snapshot, compile-derived, deploy, destroy
- Configuration location: `brain-registry.json` under `config.autonomy`
- Interaction with Claude Code's permission model: autonomy gradient operates WITHIN Claude's
  existing permission system, not outside it. Auto-execute means "the Brain recommends and
  the agent acts" — Claude's tool-use permissions still apply.
- 3 safety invariants: destroy always blocked, deploy never auto/notify, rebuild never auto
- Confidence thresholds: auto requires 0.8 effectiveness over 5+ samples; notify requires 0.6 over 3+
- Per-hat autonomy bias: operator gets wider autonomy for gate-clearing (auto); security
  narrows to notify for gates and snapshots; architect uses default for most actions

**Key decisions (hat interaction):**
- Per-hat autonomy bias is a configuration on each hat definition in `brain-registry.json`.
  The operator hat gets wider autonomy for reversible gate-clearing actions (drift checks,
  test reruns). The security hat gets narrower autonomy for IAM changes. The architect hat
  uses default autonomy. This is a required field in the hat definition schema, not optional.

**Resolved exploration questions:**
- Minimum effectiveness threshold: 0.8 over 5+ outcomes for auto, 0.6 over 3+ for notify
- Safety invariants: destroy = always blocked, deploy = minimum approve, rebuild = minimum notify

**Acceptance criteria:**
- [x] Autonomy gradient defined in brain-registry.json with 4 levels
- [x] Each action type (gate-clear, drift-check, artifact-refresh, etc.) mapped to a gradient level
- [x] Safety invariants documented and enforced (destroy = always blocked, etc.)
- [x] Per-hat autonomy bias defined in hat configurations (`autonomy_bias` field)
- [x] A Pester test validates gradient configuration schema

---

### S3-PA-2: Proposal Confidence Thresholds

**Status:** Complete
**Effort:** M
**Depends on:** S3-PA-1
**Completed:** 2026-04-10

Each proposal carries a confidence score based on historical effectiveness and blast radius.
High-confidence proposals can be auto-executed. Low-confidence proposals require approval.

**Key decisions:**
- Effectiveness rate = success rate: `(resolved entries where delta > 0) / (total resolved entries)`.
  NOT normalized avg_delta — success rate measures reliability, not magnitude.
- Autonomy level resolution: `max(hat_bias, confidence_level, safety_invariant_minimum)` where
  max means "most restrictive" (blocked > approve > notify > auto)
- `Get-AutonomyLevelOrder` utility in utilities.ps1 is the single source of truth for level ordering
- `Get-ProposalConfidence` in correlation.ps1 computes per-action-type effectiveness from ledger
- `Resolve-AutonomyLevel` in correlation.ps1 applies hat bias → confidence → safety invariants
- `Invoke-Propose` enriches each proposal with `confidence` and `autonomy_level` fields
- `Get-ProposalEffectiveness` enriched with `effectiveness_rate` and `success_count`
- Agent output schema updated: `effectiveness_rate` and `success_count` added to `proposal_effectiveness`
- Threshold configuration: in `config.autonomy.confidence_thresholds` (defined by PA-1, consumed by PA-2)

**Resolved exploration questions:**
- How is proposal confidence computed? Success rate (delta > 0 fraction) from resolved ledger entries
- How does proposal confidence differ from domain confidence? Proposal confidence is per-action-type
  from the proposal ledger; domain confidence is per-scoring-domain from CMDBs — different data sources
- Approval UX deferred to PA-3 (human-in-the-loop boundaries)

**Acceptance criteria:**
- [x] Proposal confidence computed for every `-Mode propose` output
- [x] Confidence thresholds map proposals to autonomy gradient levels
- [x] Proposal ledger entries include autonomy_level and confidence fields
- [x] A test validates that low-confidence proposals are never classified as auto-execute

---

### S3-PA-3: Human-in-the-Loop Boundaries

**Status:** Complete
**Effort:** M
**Depends on:** S3-PA-1, S3-PA-2
**Completed:** 2026-04-10

Define and enforce the boundary between autonomous and human-approved actions. The boundary
must be auditable (why was this auto-executed?) and adjustable (the user can tighten or
loosen the gradient).

**Key decisions:**
- "Enforcement" = visible, machine-readable classification. The Brain classifies and presents;
  it does not execute. Actual execution is an S4 concern (parent Brain acts on children).
- `execution_mode` values: `auto-eligible`, `notify-eligible`, `approval-required`, `blocked` —
  direct mapping from autonomy_level, avoids confusion with actual execution state
- `requires_approval` boolean on each proposal: true for approve/blocked, false for auto/notify
- `autonomy_groups` in propose output: `auto_eligible` (auto+notify) vs `approval_required`
  (approve+blocked) — makes the boundary visible in the output
- `config.autonomy.override`: when `"all_manual"`, forces all proposals to `approve` (except
  blocked stays blocked). Applied inside `Resolve-AutonomyLevel` as step 5, after safety
  invariants. Single point of resolution — all callers see the override.
- `$Registry` parameter on `Resolve-AutonomyLevel` for dependency injection (testability)
- Audit trail: `execution_mode` recorded in every proposal ledger entry
- Per-hat boundaries: `autonomy_bias` on hat definitions (PA-1) applied via `Resolve-AutonomyLevel` (PA-2)

**Resolved exploration questions:**
- Is the boundary a configuration or a runtime decision? Both — config sets the gradient, runtime
  resolves each proposal through hat bias + confidence + safety invariants + override
- Can the boundary differ per hat? Yes — `autonomy_bias` per hat (PA-1), applied in resolution (PA-2)
- What are the audit requirements? Every proposal in the ledger includes `execution_mode`

**Acceptance criteria:**
- [x] Human-in-the-loop boundary enforced — proposals above threshold require approval
- [x] Audit trail in proposal ledger shows execution_mode for every action
- [x] User can override to "all manual" via brain-registry.json config
- [x] Per-hat autonomy bias is configurable and applied

---

## Epic Completion Criteria

- [x] Autonomy gradient is defined and configurable in brain-registry.json
- [x] Safe proposals auto-execute with audit trail in proposal ledger
- [x] Human-in-the-loop boundary is enforced and adjustable
- [x] No auto-execution of destructive actions regardless of confidence
- [x] Per-hat autonomy bias is implemented

## North Star Check

- Does this make the pattern more general? **Yes** — autonomy gradients are a universal
  pattern for human-AI collaboration, not domain-specific.
- Does this make the ecosystem Brain easier? **Yes** — a parent Brain with prescriptive
  autonomy can coordinate child Brain actions without human intervention for safe operations.
