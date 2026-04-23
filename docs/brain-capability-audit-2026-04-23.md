# Brain Capability Audit — 2026-04-23

**Purpose:** For each Brain capability currently shipped in NeuroGrim +
the LSP Brains spec, mark its status against the ledger evidence
produced by the Phases 1-3 brain-vs-control experiment.

**Scope:** This is an *evidence-gap* audit, not a *retire-list*. Its
job is to surface where the methodology's claims rest on direct
measurement vs. where they rest on untested mechanism. No
retirement decisions are derived from this audit; those require their
own adversarial review.

**Prompting context:** Tier 2b-rule surfaced a one-sentence dispatch
rule that captured 89% of the oracle ceiling on 12 tasks. The rule's
sharpness raised a reasonable question — what *else* in the
methodology rests on assumptions we haven't directly measured? This
document answers that honestly.

---

## What the experiments actually measured

**Measurement corpus:** 432-row `comparison-ledger.jsonl` across
Phases 1-3. Three arms (L0 no-Brain / L1 static context / L2 live
`brain_query` tool), 12 tasks across 3 classes (repo-aware /
brain-neutral / anti-Brain), Sonnet 4.5 + Haiku 4.5 agents, Sonnet
judge blind to arm + tool trace.

**What was varied:**
- Presence/absence/shape of Brain context injection
- Agent model (Haiku for pilots, Sonnet for full runs)
- Task class

**What was held constant (and is therefore untested):**
- The Brain's *internal composition* — individual sensor domains
  contributed to L1's injected content as an aggregate, never
  independently.
- Correlation firings — load-bearing in the methodology's theory
  of cross-domain insight, but not isolated as an experimental
  variable.
- Skill invocations — skills were referenced in L1's TOC but were
  not the load-bearing element of any tested arm.
- Hat system — agents weren't instructed to wear hats during the
  experiment.
- Culture substrate — embedded in L1's CLAUDE.md excerpt but not
  attributionally tested.
- Trajectory intelligence — the Brain's historical-momentum signal
  was part of the injected content, never isolated.
- A2A + MCP protocols — experiments used direct `/v1/messages` API
  calls, bypassing MCP entirely; A2A cross-Brain communication was
  never exercised.
- Invocation ledger + capability-hygiene as uplift mechanisms — both
  are self-observability layers, not tested as agent-behavior
  influences.

---

## Per-capability audit table

| Capability | Status | Basis |
|---|---|---|
| **L1 static context for factual-definition queries** | Empirically supported | Phase 2: +58 pts on `repo-principle-application`. Rule back-test 89% ceiling capture. |
| **L1 static context for project-state queries** | Empirically supported | Phase 2: +35 pts on `repo-ship-readiness`. |
| **Agent self-routing at tool-invocation time** | Empirically supported | Phase 3 L2: 100% tool use on repo-aware, 0% on brain-neutral + anti-Brain. |
| **L1 unconditional injection as deployment pattern** | Potentially contradicted | Phase 2: wins equal-weighted at L0 (79.5), not L1 (78.4); L1 catastrophic on 4/12 tasks. |
| **L2 current synthesis under multi-turn tool use** | Potentially contradicted | Phase 3 L2 repo-aware: −12.75 vs L1, CI does not cross 0. Hypothesis: prompt-engineering problem, not architectural. |
| **Framing "agents broadly benefit from the Brain"** | Potentially contradicted | L0 wins 7/12 tasks. Brain's value is concentrated, not broad. |
| Sensors individually (git-health, test-health, code-quality, deploy-readiness, coherence, etc. — 12 domains) | Plausibly valuable, untested | Aggregated into L1; not individually varied. No attribution of which domain drove L1's wins. |
| Correlation engine + condition-tree firings | Plausibly valuable, untested | Not load-bearing in any tested arm. Mechanism is sound; empirical uplift claim isn't measured. |
| Skill library (20 plugin skills) | Plausibly valuable, untested | Present in L1's TOC; never tested as uplift mechanism. The one place skills were directly measured (Axis 4 invocation ledger) is a usage tracker, not a capability test. |
| Hat system (7 hats) | Plausibly valuable, untested | Not applied to agents-under-test. The `adversary` hat has been applied by the operator during this very experiment and appears useful — but that's anecdote, not measurement. |
| Cultural substrate (5-value invariant) | Plausibly valuable, untested | Embedded in L1's CLAUDE.md excerpt; no attributional measurement. |
| Unified scoring + weights | Plausibly valuable, untested | The L1 injection included health-snapshot scores; not varied. |
| Trajectory intelligence (velocity, acceleration) | Plausibly valuable, untested | Included in L1 content; not varied. Would need time-series experiments. |
| Gated governance (promotion ledger, swing detector, red mode) | Empirically supported for *its own* purpose | These are infrastructure *for testing*, not capabilities directly measured for agent uplift. Their correctness *as governance* is established. |
| A2A peer protocol | Plausibly valuable, untested for agent uplift | Works at the protocol level; no experiment tested whether cross-Brain queries improve agent behavior. |
| MCP server surface | Plausibly valuable, untested | Experiments bypassed MCP; L1 used HTTP directly. |
| Invocation ledger (Axis 4 v1) | Plausibly valuable, untested for uplift | Orthogonal observability layer; not an agent-behavior input. |
| Capability-hygiene domain (scoring) | Plausibly valuable, untested | Self-observability; not an agent-behavior mechanism. |
| CLI-mode (MCP omitted) as agent-uplift pattern | Plausibly valuable, untested | Session-start token economics measured (~983 tokens saved in B-09 bench); agent-behavior impact not measured. |

**Count:** 3 empirically supported for agent-uplift, 3 potentially
contradicted (as *framings*, not as capabilities), 13 plausibly
valuable but untested.

---

## What this doc is emphatically NOT saying

The distribution above is uncomfortable-looking. Let me preempt the
wrong reading:

**This is not a retirement list.** "Plausibly valuable but untested"
is an evidence-gap classification, not a value judgment. Most of the
13 items in that bucket have coherent mechanisms — they just haven't
been the independent variable in a controlled experiment.

**Absence of evidence ≠ evidence of absence.** We tested *context
injection patterns* across 12 tasks. We did not test hats, skill
routing, correlation firings, trajectory-based recommendations, or
cross-Brain coordination. Silence from our experiments about these
capabilities is a limitation of our experiments, not a finding
against the capabilities.

**The contradicted items are framings, not capabilities.** "L1
unconditional injection as deployment pattern" being contradicted
doesn't retire L1 as a capability — it sharpens WHEN to use it
(per the dispatch rule). "Agents broadly benefit from the Brain" is
a *marketing-level claim*; the Brain's concrete capabilities aren't
retired by its falsification.

**This is a static snapshot.** As new experiments land (expanded
task sets, held-out validation, capability-specific measurements),
statuses will move. Many "plausibly valuable, untested" items could
promote to "empirically supported" with targeted experiments;
others could demote. The audit's job is to name where we are, not
to freeze it.

---

## Operator implications

1. **The dispatch rule is the most actionable output of the
   experiments to date.** Operator guidance in CLAUDE.md has been
   consolidated to lead with the rule.

2. **Evidence-gap prioritization.** The 13 "plausibly valuable,
   untested" items are candidates for *future targeted experiments*,
   not for retirement. The order we invest in those experiments
   should prioritize capabilities load-bearing for current adopter
   workflows.

3. **The reframe exploration is on a branch** (`reframe/factual-
   augmentation`) where the speculative architectural work is
   isolated from main. This audit informs that branch's evidence-
   expansion phase (B-1) — held-out tasks should target
   under-sampled capability shapes: factual-lookup, multi-domain
   correlation, cross-Brain queries, multi-hop reasoning.

4. **No capabilities retired.** All retirement decisions remain
   their own case-by-case reviews. This audit is a map, not a
   mandate.

---

## Pointers

- **Plan:** `C:/Users/koff0/.claude/plans/parallel-hugging-eich.md` —
  methodology reframe exploration (Tracks A + B).
- **Experiments:** `D:/Brains/.claude/experiments/brain-vs-control/`
  — ledger (432 rows) + per-phase reports + dispatch-rule analysis.
- **BACKLOG follow-up:** B-15 (this audit) + B-14 (dispatch rule
  candidate) — both CANDIDATE, neither executing changes.
- **Spec note (observational):** LSP-Brains `METHODOLOGY-EVOLUTION.md`
  §14 — access-pattern polymorphism as observed-not-normative.
