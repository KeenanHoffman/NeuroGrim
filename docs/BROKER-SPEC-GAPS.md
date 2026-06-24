# Broker Spec — Discovered Gap Ledger

> **Pre-implementation status.** The 38-BB broker framework spec is feature-complete
> at the design level but no reference implementation has run yet. This ledger is the
> append-only record of spec gaps discovered during S0-T → S1-T → S2-T → S\*-C
> implementation. Each gap surfaces a question or ambiguity the spec did not
> anticipate; the resolution is patched back into the relevant doc(s) and recorded
> here with provenance.

**Created:** 2026-06-24 (Phase 9 risk-triage closure of R-X-14). **Status:**
zero gaps logged (pre-S0-T). Update as S0-T+ implementers surface them.

---

## Why this ledger exists

The broker framework has been audited 8 times (Phases 1-8) plus a risk audit (Phase
8.1) plus a triage pass (Phase 9). At this point the spec is comprehensive **on paper**.
But first implementation will surface 100 things the spec didn't anticipate — edge
cases in precondition evaluation, error-handling ambiguities, parameter-sourcing
quirks, performance cliffs, operator-UX surprises, integration-test surprises.

Pre-registering this ledger is a hedge against the spec's false-stability claim:
**the spec is "draft" not "stable" until first runtime validates it.** Implementers
are expected to populate this ledger continuously; spec-maintainers ratify gap-fixes
into the canonical docs.

---

## Gap entry format

Each gap follows this structure:

```
### Gap NN: <short title>
- **Discovered during:** S0-T | S1-T | S2-T | S\*-C | post-launch
- **Discoverer:** <who hit it; agent-id or contributor-name>
- **Date:** YYYY-MM-DD
- **Affected docs:** <list of broker docs this touches>
- **Affected BBs:** <list of building-block IDs this implicates>
- **Problem:** <2-3 sentences on what the spec didn't anticipate>
- **Workaround:** <what the implementer did locally to keep moving>
- **Proposed resolution:** <spec patch sketch>
- **Status:** open | proposed-patch | ratified | declined-with-rationale
- **Patch ref:** <commit SHA / PR link when ratified>
```

---

## Gaps (chronological)

*(None yet — pre-implementation. Append entries below as S0-T+ implementers surface them.)*

---

## Severity guidance

Gaps fall into three categories:

1. **Visibility gaps** — the spec is correct but unclear; reader couldn't tell what
   the framework does. **Resolution:** documentation patch only; no behavior change.
2. **Underspecification** — the spec didn't say what to do in case X. **Resolution:**
   add the missing case; document the choice + rationale.
3. **Contradiction** — the spec says incompatible things in different docs.
   **Resolution:** decide which side wins; patch the loser; record the decision.

The fourth category (genuine design errors that require backward-incompatible
changes) requires bumping the spec's contract version (per BB #34) and a deprecation
plan for any deployed implementations. Treat these with care — they may indicate the
spec needed to be drafted differently and prior reviews missed it.

---

## Patch-rate metric (post-launch)

Once S0-T ships and the first 100 gaps are logged, the rate of new gap discovery
becomes a stability signal:

- **High gap rate (>5/week)**: spec is materially underspecified; patch cycles are
  active; stability claim is far away.
- **Moderate gap rate (1-5/week)**: spec is converging; integration is exercising
  edge cases.
- **Low gap rate (<1/week)**: spec is approaching stability; consumer adoption
  is safer.
- **Sustained zero gap rate for 6 months**: the "DRAFT" status (per
  [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) frontmatter) can be lifted; spec is
  battle-stable.

---

## Cross-references

- [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) — primary spec; DRAFT status pinned in
  frontmatter pending this ledger's resolution.
- [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) — same DRAFT status.
- [`../roadmap/broker-framework-backlog.md`](../roadmap/broker-framework-backlog.md)
  — implementation backlog where gaps may surface during BB authoring.
- [`../../cereGrim/docs/RISK-REGISTER.md`](../../cereGrim/docs/RISK-REGISTER.md)
  R-X-14 — the audit finding this ledger closes.
- Phase 9 triage decisions: in the operator's plan-mode workspace.
