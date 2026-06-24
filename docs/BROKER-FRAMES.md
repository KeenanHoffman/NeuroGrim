# NeuroGrim — Broker Frames (design stub)

**Status:** STUB. This doc introduces the **Frame primitive** — a brief named lens that
shifts how brokers process work without changing what the work is. Hats are one
*type* of Frame; this doc generalizes the pattern to six other dimensions of context
sharing + adds two structural extensions: broker-prescribed Frames and Frame-rotation
pipelines. Full content lands as the design questions below resolve.

Companion to [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) (the named primitive),
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) (framework internals + 30 building blocks),
[`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) (how agents see broker output), and
cereGrim's [`INTER-AGENT-BROKER.md`](../../cereGrim/docs/INTER-AGENT-BROKER.md) (the
cluster-level recursion where Frame negotiation between peer-agents becomes
load-bearing). Terminology defined in CONTRACT §"Glossary."

> **IP boundary.** The Frame primitive as substrate (the trait, the taxonomy, the
> rotation pattern, the prescription mechanism) is NeuroGrim public per
> [`PUBLIC-VS-PROPRIETARY.md`](PUBLIC-VS-PROPRIETARY.md). Consuming-project-specific
> Frame *values* (e.g., cereGrim's dual-lobe-specific Mode lifecycle stages) live in
> those projects' composition docs. The cost-thesis framing of *why* Frame-rotation
> pipelines absorb operator discipline as a load-bearing lever stays in consuming
> projects' thesis tier.

---

## §1 — What a Frame is

A **Frame** is a brief named lens declared by an agent, an operator, or a broker that
shifts how a piece of work is processed without changing what the work is. The
primitive's load-bearing properties:

- **Brief** — one or two words; agent-facing routing-signal-grade budget.
- **Open-ended** — operator-extensible; new Frame values declarable per cluster /
  per broker / per deployment.
- **Stackable** — multiple Frames compose into an active *Frame stack* that brokers
  read together (not one Frame at a time).
- **Default-driven** — Frames carry inherited defaults at every level (cluster
  defaults → broker defaults → role defaults → pipeline defaults → dispatch overrides);
  agents declare only what differs from the inherited stack.
- **Negotiable at boundaries** — at the IAB, Frames are part of the cluster-pipeline
  contract: caller requests, target peer-agent acknowledges or modifies, broker
  enforces.
- **Substrate-meaningful** — the framework knows what each Frame value does
  (Governance Composer reads Stakes; Skill Filter reads Mode; Overlay curation reads
  Audience; Workflow Engine reads Tempo + Time-Horizon).
- **Tunability-tiered per Frame type** — some Frames are LLM-pickable (`Hat:
  autonomous`); some are operator-only (`Scope: operator-only`); per the existing
  tunability tier mechanism in [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4.

Hats prove the pattern works. The other six dimensions in §2 prove it generalizes.

---

## §2 — The seven canonical Frame types

| Frame type | Example values | What brokers modulate when this Frame changes |
|---|---|---|
| **Hat** (mindset) | `adversary` · `architect` · `visionary` · `rubber-duck` · `security-auditor` · `supply-chain-auditor` · `incident-commander` · `source-reader` | Subagent briefing style; output tone; lens the LLM applies to interpretation |
| **Stakes** (risk profile) | `exploratory` · `production` · `irreversible` · `rehearsal` | Governance threshold (auto-compose `require-operator-confirmation` at production); trust budget allocation; audit anchoring depth |
| **Tempo** (cadence) | `rapid-prototype` · `deliberate` · `campaign` · `steady` | Tick cadence; governance check frequency; workflow checkpoint frequency; replay anchoring granularity; Frame-rotation depth (see §4) |
| **Mode** (lifecycle phase) | `discovery` · `design` · `implementation` · `validation` · `retrospective` | Which brokers are active (validation activates Sensory-heavy curation; implementation activates Work-heavy); pipeline catalog filtering |
| **Confidence** (certainty) | `high` · `tentative` · `uncertain` · `speculative` | Escalation threshold (`uncertain` auto-routes to Meta lobe when wired); governance compose (`speculative` requires explicit "this is a guess" anchor); refusal threshold |
| **Audience** (who you're addressing) | `operator-direct` · `stakeholder-summary` · `newcomer-onboarding` · `agent-internal` | Output format; Overlay curation richness; routing-signal verbosity in awareness; integration with `human-comms` domain |
| **Scope** (blast radius) | `local` · `module` · `cross-project` · `experimental` | Which brokers' ACLs activate; cross-broker pipeline composition permissions; cluster-pipeline reach (a `scope: local` dispatch can't escalate via IAB) |

**The seven are a starting set, not a closed contract.** Per the deprecation discipline
in [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3, new Frame types must justify
net-new surface or replacement of existing surface. Adding a Frame type is a contract
amendment; adding Frame *values* within an existing type is a manifest declaration.

---

## §3 — How brokers consume Frames

Same uniform mechanism across all Frame types — brokers compose their behavior from the
active Frame stack. Four consumption surfaces:

1. **Governance Composer reads Frames.** A `dispatch-work-unit` pipeline composed
   against `{stakes: production, confidence: tentative}` automatically adds
   `require-operator-confirmation` + `record-extended-trace` to its governance
   composition. Same pipeline, different governance stack.
2. **Skill Filter reads Frames.** With `{mode: discovery, tempo: rapid-prototype}`,
   exploratory pipelines get rank boosts in `legal_pipelines()`. With
   `{mode: validation, stakes: production}`, audit-heavy pipelines get rank boosts.
   Per-Frame weight cells live in the Skill Filter's per-broker config; operators
   declare them.
3. **Overlay curation reads Frames.** Context Broker's curation policy with
   `{audience: newcomer-onboarding}` projects richer skill bodies + glossary entries
   into Overlay. With `{audience: agent-internal}`, terse projections only.
4. **Workflow Engine reads Frames.** A workflow started with `{tempo: campaign,
   time-horizon: long-term}` gets a longer TTL + more aggressive checkpointing. With
   `{tempo: rapid-prototype}`, shorter TTL + lighter checkpointing.

The 30 building blocks stay uniform; Frames change *how* they apply. The Frame stack is
a typed map in broker state; framework reads it at every consumption point.

---

## §4 — Frame-rotation pipelines (governance via structure)

A pipeline whose internal steps shift the active Frame stack between sub-pipeline calls.
The pipeline *is* the orchestration of perspectives; each sub-pipeline runs with its own
Frame stack; a synthesis step aggregates findings across.

**Worked example — comprehensive change review:**

```yaml
- id: comprehensive-change-review-rotation
  visibility: surfaced
  audit_class: capability
  description: "Multi-perspective review of a code change — security, QA, code-quality, accessibility, performance — synthesized into one verdict"
  when_to_use: "before merging non-trivial PRs; when stakes=production; when the work touches user-facing surfaces"
  preconditions:
    - artifact_exists: hot.review_queue.has(params.artifact_id)
  steps:
    - sub_pipeline: review-pass
      with_frame: {hat: security-auditor, stakes: production, confidence: uncertain}
    - sub_pipeline: review-pass
      with_frame: {hat: qa-engineer, mode: validation}
    - sub_pipeline: review-pass
      with_frame: {hat: code-quality-reviewer, tempo: deliberate}
    - sub_pipeline: review-pass
      with_frame: {hat: accessibility-reviewer, audience: stakeholder-summary}
    - sub_pipeline: review-pass
      with_frame: {hat: performance-auditor, confidence: tentative}
    - sub_pipeline: synthesize-rotation-findings
      with_frame: {hat: architect, mode: validation, audience: operator-direct}
  governance:
    compose: [check-trust-budget, check-kill-switch, record-dispatch, record-rotation-trail, record-outcome]
  expected_effect: verifies_change_quality_multi_dimensional
```

**The agent dispatches one pipeline.** The broker runs five Frame-shifted review passes
+ a synthesis step. The LLM never had to remember "oh I should also check
accessibility" — the rotation structurally enforces it.

### The deeper claim

**Brokers absorb operator discipline by structurally enforcing what the agent would
otherwise have to remember.** Frame-rotation is to operator discipline what
`legal_pipelines()` is to operator preferences — the broker holds the multi-dimensional
review checklist so the LLM doesn't have to.

Agents working on long-running problems become both **more independent** (less
prompting needed; the rotation runs without human reminders) AND **more thorough**
(hidden quality concerns — security, accessibility, performance, supply-chain — get
systematically addressed because the pipeline structure enumerates them).

### Frame-rotation as a first-class Step type (optional)

Sugar over manually-authored rotations; framework expands at load time:

```yaml
- frame_rotation:
    over: [security-auditor, qa-engineer, code-quality, accessibility, performance]
    each_runs: review-pass
    synthesize_with: {hat: architect, mode: validation}
```

Operator-extensible (add a new hat value to the rotation list; framework picks up
without code change). Default implementation: expand at load time into the explicit
`steps:` form above.

### Conditional Frame loops

Workflows that loop through Frames until convergence:

```yaml
- loop_until: {predicate: "all_critical_findings_resolved"}
  frame_rotation_cycle:
    - {hat: implementer}                          # apply fixes
    - {hat: security-auditor}                     # re-review
    - {hat: qa-engineer}
    - {hat: code-quality-reviewer}
    - {hat: architect}                            # synthesize remaining
  max_iterations: 5
```

The workflow self-iterates with Frame-shifting until convergence or budget exhaustion.
**This is the long-running autonomous-correction loop** — exactly the pattern that
enables overnight runs / multi-session campaigns where hidden quality concerns must
keep getting addressed without operator prompts.

**Mandatory safeguards** (per [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4
critical invariants):
- `max_iterations` is required (no unbounded loops).
- The loop's escape clause is operator-confirmable (if budget exhausted but predicate
  not satisfied, governance composes `escalate-to-operator-with-state-snapshot`).
- Frame-loops carry `audit_class: capability` AND `record-rotation-trail` so the loop's
  iteration history is fully replayable.

---

## §5 — Broker-prescribed Frames (the directional inversion)

Beyond agents declaring Frames they want to work under, **brokers can prescribe Frames
to agents** as part of dispatch responses. The direction inverts:

- **Agent → broker (request):** "process this work for me with `{hat: visionary,
  stakes: exploratory}`"
- **Broker → agent (prescription):** "I'll run this; you should adopt `{hat:
  incident-commander, audience: operator-direct}` to interpret the results"

Concrete shape:

```json
{
  "dispatch_response": {
    "pipeline_id": "topology-query-cluster-pipeline",
    "result": { ... },
    "active_frames_during_dispatch": {
      "hat": "security-auditor",
      "stakes": "production"
    },
    "suggested_frames_for_response_interpretation": {
      "hat": "incident-commander",
      "audience": "operator-direct",
      "confidence": "uncertain"
    },
    "rationale": "findings include a CVE in active dependencies; respond with operational urgency + flag uncertainty for operator triage"
  }
}
```

The agent picks up the suggested Frame for its response interpretation. The broker
becomes a Frame teacher — it doesn't just answer queries, it tells the agent *how to
think about the answer*.

At the IAB, this becomes a per-cluster-pipeline negotiation pattern (three modes from
[`../../cereGrim/docs/INTER-AGENT-BROKER.md`](../../cereGrim/docs/INTER-AGENT-BROKER.md)
Q2 + Q4):

| Pattern | Direction | Who decides |
|---|---|---|
| **Caller-requested** | Calling cognitive-agent → IAB | Calling agent declares request_frames |
| **Cluster-enforced** | Cluster manifest → IAB | Operator declares cluster default_frames |
| **Peer-prescribed** | Target peer-agent's IAB → calling agent | Target agent's broker prescribes interpretation_frames |

All three negotiation patterns coexist. The Frame stack flows bidirectionally through
the inter-agent boundary, not just inward.

---

## §6 — Tunability per Frame type

Frames inherit the four-tier tunability system from
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4:

| Frame type | Default tunability | Rationale |
|---|---|---|
| Hat | `Autonomous` (with operator-defined hat-set bounds) | Hats are mindset choices; the agent picks within the operator-defined enum |
| Stakes | `OperatorConfirmed` | Stakes governs governance threshold; agent proposes, operator confirms (especially for `production` → `exploratory` downgrades) |
| Tempo | `Autonomous` | Speed-vs-precision choice within operator-defined bounds |
| Mode | `OperatorOnly` | Lifecycle phase is a project-level decision; not LLM-tunable per-dispatch |
| Confidence | `Autonomous` | The LLM is the best judge of its own confidence (with the caveat that confidence-driven escalation has governance backstops) |
| Audience | `OperatorOnly` (with per-session overrides via cluster manifest) | Audience is the human-comms domain's concern; operator-led |
| Scope | `OperatorOnly` | Blast radius is a policy decision, not an LLM tactical choice |

The `Stakes` defaults specifically prevent **Frame manipulation** (agent declares
`stakes: exploratory` to evade governance) — the operator confirms downgrades, so the
agent can request lighter governance but cannot grant it to itself.

---

## §7 — Open design questions

This stub becomes a real spec when these resolve:

1. **Frame conflict resolution.** What if `hat: rubber-duck` (listen, don't solve)
   conflicts with `tempo: rapid-prototype` (move fast)? Operator declares per-cluster
   Frame conflict precedence rules — but the syntax + the conflict-detection mechanism
   are open.
2. **Frame stack inheritance semantics.** Cluster defaults → broker defaults → role
   defaults → pipeline defaults → dispatch overrides. The framework must enforce a
   merge order. Likely: dispatch wins, then pipeline, then role, then broker, then
   cluster (innermost wins). But the override-vs-deep-merge question for nested Frame
   structures needs a pin.
3. **Frame-rotation budget arithmetic.** A 7-Frame rotation × 7-tier governance compose
   = N×M cost. Per-rotation budgets + tempo-driven scaling: needs concrete formulas,
   not vibes.
4. **Frame-set coverage audit.** Frame-rotation only covers Frames you've enumerated.
   What ensures we don't miss "hidden quality" axes? Periodic operator-run
   `frame-set-coverage-review` pipeline that rotates through "what could we be
   missing" lenses — but the pattern itself is recursive (a Frame for auditing the
   Frame set) and needs design.
5. **Frame extension protocol.** Operators add new Frame *values* via manifest. New
   Frame *types* are contract amendments. What's the path between the two — when does
   a recurring custom Frame justify promotion to a canonical type?
6. **Frame negotiation refusal protocol at IAB.** When a target peer-agent refuses a
   requested Frame (per the negotiation shape in §5), what's the structured refusal
   message + the calling agent's fallback path? Needs schema design alongside the
   IAB's contract-version webhook protocol.
7. **Frame visibility in the awareness mechanism.** Should the LLM see the *current
   Frame stack* in its L1 context (per [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md)
   §2)? Strong prior: yes — the agent should know what stance it's operating under,
   especially when the broker prescribed Frames the agent didn't request. But the
   injection format + budget cost need design.

---

## §8 — What this commits the framework to

The Frame primitive **will become a new building block** in
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3 — Layer C (Substrate composition) when
this stub matures into a real spec. **No BB number is assigned yet** (numbers #25-#30
were taken by Phase 3 hole-closure additions: Pipeline Cancellation Handler, Schema
Migration Runner, Cross-Broker Composition Policy, Diagnostics Collector, Broker
Lifecycle, Onboarding Projection). Indicative table entry (numbering deferred):

| # | Building block | Framework provides | Broker author provides |
|---|---|---|---|
| TBD | **Frame stack** | The typed Frame map in broker state; the merge order across inheritance levels; the consumption surfaces in Governance Composer / Skill Filter / Overlay curation / Workflow Engine; the `with_frame:` step modifier; the `frame_rotation:` step sugar; the IAB negotiation protocol | Per-broker Frame defaults; per-pipeline Frame requirements; per-cluster Frame manifest values |

`displaces / deprecates: nothing` — net-new substrate surface. Mirrors the hats system's
substrate-level discipline but applies to six other context dimensions.

cereGrim-side composition (which Frame values the dual-lobe harness uses for its
specific Mode lifecycle, which Hat values populate the cluster manifest, how Frame
prescription wires into the Meta-lobe's escalation contract) lives in a future
`cereGrim/docs/FRAMES-COMPOSITION.md` once this substrate stub matures into a real
spec.

---

## Cross-references

- [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) §"Glossary" — Frame-related terms will land
  here when stub matures (currently the Glossary documents broker-role / cluster-role
  / Overlay tiers but not Frame types).
- [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3 — Frame stack as a future Layer C
  building block (number deferred until stub matures; #25-#30 occupied by Phase 3
  hole-closure additions).
- [`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §4 critical invariants — Frame
  manipulation safeguards (tunability tiers per Frame type); Frame-rotation max
  iterations; Frame-loop escape clauses.
- [`BROKER-AWARENESS.md`](BROKER-AWARENESS.md) §2 — open question 7 (Frame visibility
  in agent L1 context).
- [`BROKER-WRAPPING.md`](BROKER-WRAPPING.md) — MCP-wrapped pipelines should accept
  Frame-driven precondition + governance variants (extension to Path 1).
- [`PUBLIC-VS-PROPRIETARY.md`](PUBLIC-VS-PROPRIETARY.md) §"Audit by example" — the
  Frame primitive itself is substrate-level (no cost-thesis leakage); cost-thesis
  framing of "Frame-rotation absorbs operator discipline as a load-bearing lever" stays
  in consuming projects' thesis tier.
- [`../../cereGrim/docs/INTER-AGENT-BROKER.md`](../../cereGrim/docs/INTER-AGENT-BROKER.md)
  Q2 + Q4 — Frame negotiation at the IAB; per-cluster Frame manifests; peer-agent
  Frame refusal protocol.
- [`../../cereGrim/docs/COGNITION-LOOP.md`](../../cereGrim/docs/COGNITION-LOOP.md) Q5
  — escalation contract; Confidence Frame triggers Meta-lobe escalation when wired.
- `D:\Brains\NeuroGrim\.claude\skills\hats\SKILL.md` — the precedent. Hats are one
  Frame type; this doc generalizes the pattern.

## Exit

This stub becomes the design note (no longer stub) when:
1. All seven open design questions in §7 have pinned answers.
2. The IAB Frame negotiation protocol is specified concretely (caller-requested +
   cluster-enforced + peer-prescribed shapes documented in
   `INTER-AGENT-BROKER.md`).
3. The Frame-stack building block (number deferred; see §8) lands in
   `BROKER-INTERNALS.md` §3 with full framework-vs-author
   split.
4. The first reference Frame-rotation pipeline (likely a cereGrim-specific
   `comprehensive-change-review-rotation`) is authored and tested against a real
   substrate; bias-free latency + thoroughness measurements published.
5. The `BROKER-CONTRACT.md` Glossary is extended with Frame-related terms.

At that point, the substrate-level Frame primitive is spec-stable; consuming-project
Frame value sets evolve under their own composition docs.
