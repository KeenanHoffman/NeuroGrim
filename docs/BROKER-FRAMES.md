# NeuroGrim — Broker Frames

**Status:** SPEC-STABLE (Phase 5 closure). The Frame primitive is now BB #35
(Layer C, Substrate composition). The seven open design questions are pinned in §7;
implementation can begin against this contract.

The Frame primitive — a brief named lens that shifts how brokers process work without
changing what the work is. Hats are one *type* of Frame; this doc generalizes the
pattern to six other dimensions of context sharing + adds two structural extensions:
broker-prescribed Frames and Frame-rotation pipelines.

Companion to [`BROKER-CONTRACT.md`](BROKER-CONTRACT.md) (the named primitive),
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) (framework internals + 35 building blocks),
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

**Capitalization convention (P-7).** Canonical Frame type names are capitalized in
spec prose (Hat, Stakes, Tempo, Mode, Confidence, Audience, Scope). TOML field names
in cluster + broker manifests are lowercase per TOML convention (`hat = "architect"`,
`stakes = "production"`, etc.). The two are interchangeable references to the same
Frame type; prefer capitalized in prose, lowercase in code/config examples.

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

The 35 building blocks stay uniform; Frames change *how* they apply. The Frame stack is
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

**Depth bound — `MaxFrameRotationDepth`:** Frame-rotation steps can technically nest
(a rotation's sub-pipeline could itself be a rotation pipeline). Without a bound, the
expansion balloons. Framework enforces `MaxFrameRotationDepth` (default 2,
operator-tunable per cluster manifest) — catalog loader rejects pipelines whose
`frame_rotation:` nesting exceeds this. Validated at load time (not runtime); startup
fails loudly on violation. Distinct from MaxBrokerDepth (which bounds broker wrapping)
and MaxCrossBrokerCompositionDepth (which bounds sub-pipeline calls across brokers).

**Budget bound — load-time `rotation_budget` validation (per
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) BB #9):** the depth bound alone does not
prevent a *wide* rotation (e.g., `frame_rotation:` over 100 Frame values at depth 1)
from blowing budget at dispatch. The Pipeline Catalog loader pre-computes
`rotation_budget = N × single_pipeline_budget × tempo_multiplier + synthesis_budget`
(per §7.3 below) against the active Tempo's multiplier and per-broker budget ceiling;
catalogs whose rotations exceed ceiling are rejected at load with
`failure_reason: rotation_budget_exceeds_ceiling`. Depth and budget are independent
constraints — both validated at load, neither at dispatch.

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

**Broker-prescription authorization (R-S-1 closure, Phase 9).** Default-deny posture:
brokers cannot prescribe Frames unless the cluster manifest explicitly authorizes
them. Cluster manifest declares per-broker:

```toml
[cluster.brokers.topology-broker]
allowed_prescribed_frame_types = ["hat", "audience", "confidence"]

[cluster.brokers.work-broker]
allowed_prescribed_frame_types = []   # explicitly empty = cannot prescribe
```

Brokers attempting to prescribe Frame types not in their authorized list are rejected
with `failure_reason: prescribed_frame_not_authorized` + the rejected prescription
is logged to BB #28 Diagnostics as `audit_class: governance` for operator review.

This closes the supply-chain attack vector where a compromised broker (or an
operator typo in cluster manifest) silently shifts governance composition via
unauthorized Frame prescription. Cluster manifest is the authority on which
brokers can teach Frames; the framework enforces it. Tunability:
**OperatorOnly** per the cluster-manifest field-level annotations
(see [`CLUSTER-MANIFEST-SCHEMA.md`](CLUSTER-MANIFEST-SCHEMA.md)).

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

## §7 — Resolved design questions (Phase 5)

### 7.1 — Frame conflict resolution (PINNED)

Conflicts between Frame values are declared at the cluster manifest level as a
**precedence matrix**. When two active Frames carry mutually-incompatible operational
implications (e.g., `hat: rubber-duck` says "listen, don't solve" while
`tempo: rapid-prototype` says "move fast"), the framework consults the precedence
matrix to resolve. Default precedence (operator-overridable):

```
Stakes > Hat > Mode > Confidence > Tempo > Audience > Scope
```

Rationale: governance-bearing Frames win (Stakes is most governance-relevant; Scope
least). Within the matrix, when Frame A has precedence over Frame B and they conflict,
Frame A's operational implications apply and Frame B is annotated `suppressed-by: A`
in the active Frame stack. The agent sees both Frames AND the suppression annotation,
preserving mutual visibility — operator and LLM can both reason about *why* the
unexpected behavior held.

**Stakes-with-governance hard floor (R-S-17 closure, Phase 9).** The precedence matrix
is operator-overridable for non-governance-bearing Frame types — operator may reorder
Hat vs Mode vs Tempo vs Audience vs Scope as their deployment demands. **But Stakes
values that carry governance implications (`production`, `irreversible`, `rehearsal`)
can NEVER be suppressed by any other Frame type, regardless of operator-declared
precedence matrix.** Operator-declared matrices that would suppress
governance-bearing Stakes are rejected at manifest load with
`failure_reason: precedence_matrix_violates_stakes_floor`. Closes the
governance-bypass attack where re-ordering precedence would let `hat: malicious-actor`
suppress `stakes: production` enforcement. Operator may still adjust the relative
ordering of `exploratory` Stakes (which carries no governance backstops) against other
Frames; only governance-bearing Stakes values are floor-protected. The schema-hard
list of governance-bearing Stakes values is in this doc §2; extending the list is a
spec amendment, not an operator config change.

**Conflict-detection mechanism:** each Frame type's value enum carries an
optional `incompatible_with` field per value (e.g., `rubber-duck` declares
`incompatible_with: [rapid-prototype, campaign]`). Framework validates conflicts at
Frame-stack merge time; logs annotations to the audit trail with `audit_class:
governance`.

### 7.2 — Frame stack inheritance semantics (PINNED)

Merge order: **dispatch overrides win innermost, cluster defaults are outermost.** The
full hierarchy:

```
dispatch override
  → pipeline default
    → role default (the broker's role-set declaration in manifest)
      → broker default (the broker's manifest)
        → cluster default (cluster manifest)
```

Each level overlays the next; the innermost-specified value wins. For nested Frame
structures (e.g., a Hat value with sub-fields), the merge is **shallow override per
Frame type** — the entire Frame value is replaced, not deep-merged. Deep merge was
rejected because nested-field-level merges create surprising emergent values not
declared at any level; shallow override is predictable.

### 7.3 — Frame-rotation budget arithmetic (PINNED)

A Frame-rotation pipeline runs N sub-pipelines under N Frame stacks + 1 synthesis
step. Compute cost = N × (single-pipeline cost) + (synthesis cost). Budget formula:

```
rotation_budget = N * single_pipeline_budget * tempo_multiplier + synthesis_budget

where:
  N = count of Frames in the rotation
  single_pipeline_budget = base cost per pipeline dispatch (in trust-budget units)
  tempo_multiplier:
    rapid-prototype: 0.3 (minimal review; many Frames may be dropped)
    deliberate: 1.0 (default)
    campaign: 1.5 (sustained reasoning; richer rotation justified)
    steady: 1.0
  synthesis_budget = 2 * single_pipeline_budget (synthesis sees N inputs)
```

**Per-tempo Frame-count override:** `rapid-prototype` drops rotation to minimum N=2
(one review + synthesis); `deliberate` runs full operator-declared rotation;
`campaign` may extend rotation by up to 50% with operator-confirmed budget grant.

**Budget enforcement:** the calling pipeline's `check-trust-budget` step (composed by
governance) computes the rotation_budget at dispatch time and refuses if the broker's
remaining budget can't cover it. No partial-rotation execution — either full rotation
or refusal (preserves the rotation's discipline-enforcement guarantee).

### 7.4 — Frame-set coverage audit (PINNED)

A meta-rotation pipeline — `frame-set-coverage-review` — runs operator-scheduled
(default: weekly) to audit whether the operator-declared Frame set covers known
quality concerns. The pipeline:

1. Reads the broker's invocation ledger for the past N days (default 30).
2. For each dispatched pipeline, computes which Frames were active during dispatch.
3. Emits a coverage matrix: Frame-type × pipeline-class × frequency.
4. Flags Frame types with anomalously low usage (potential operator dead values to
   retire) AND Frame types absent from high-frequency pipeline-classes (potential
   missing coverage on common work).
5. Surfaces findings to operator via Operator Telemetry Summarizer (BB #32) in a
   `frame-coverage-report` segment.

**No recursive Frame-on-Frame design needed.** The coverage audit is a Tier 2
internal pipeline running periodic analysis, not a meta-Frame on top of Frames.
Operator reviews findings + decides whether to add new Frame values, retire dead
ones, or expand rotations to cover gaps.

### 7.5 — Frame extension protocol (PINNED)

Three tiers of extension, formalized:

| Extension | Authority | Mechanism |
|---|---|---|
| **New Frame value** (within an existing Frame type) | Operator-only via cluster manifest | Add to the Frame type's value enum; framework validates against the type's value-rule constraints; hot-reload picks it up |
| **Promotion of a recurring custom Frame value to baseline** | Operator-initiated, framework-recorded | Operator declares promotion; framework records `promoted_from: <cluster_id>` in the substrate Frame-type definition; baseline value enum updated in a framework release |
| **New Frame type** (e.g., `Trust`, `Audience-Specificity`) | Contract amendment (framework spec change) | Requires BROKER-FRAMES.md amendment + new BB-table entry if substrate-bearing + version bump |

Promotion path: when a custom Frame value appears in 3+ unrelated clusters' manifests,
the framework's `frame-promotion-candidates` periodic report (under BB #32 Operator
Telemetry Summarizer) flags it for operator consideration. Operator decides whether to
promote to baseline.

### 7.6 — Frame negotiation refusal protocol at IAB (PINNED)

When a target peer-agent's IAB refuses a requested Frame (per §5 negotiation
patterns), the refusal carries a structured response:

```json
{
  "schema_version": "1",
  "negotiation_id": "<UUID>",
  "calling_peer_agent": "<id>",
  "target_peer_agent": "<id>",
  "requested_frames": {<Frame stack>},
  "accepted_frames": {<subset that target accepts>},
  "modified_frames": {<frames target proposes alternative values for>},
  "rejected_frames": [
    {
      "frame_type": "<type>",
      "requested_value": "<value>",
      "rejection_reason": "incompatible-with-cluster-default | violates-target-frame-constraint | governance-policy-blocks | target-broker-lacks-capability",
      "suggested_alternative": "<value | null>"
    }
  ],
  "calling_agent_fallback": "accept-modified | retry-with-suggestions | escalate-to-operator | abort-dispatch"
}
```

The calling peer-agent's IAB consumes this response and (per the `calling_agent_fallback`
field, which the calling agent declared in its original request) takes the
appropriate path. Default fallback: `escalate-to-operator` (no silent acceptance of
modified Frames). Operator can override per cluster-pipeline to allow autonomous
fallback for low-stakes routing.

**Escalation routing (M-8 closure).** "Escalate-to-operator" requires pinning WHICH
operator receives the escalation. The framework dispatches a
**`FrameNegotiationFailure`** event to **both** the calling agent's operator AND the
target agent's operator simultaneously (each via their cluster's Sensory Queue under
`audit_class: governance`). Both events carry the full negotiation payload (above) +
a shared `negotiation_id`. The event is tracked in a per-cluster
**escalation ledger** (`<cold-store>/escalation-ledger.jsonl`); operators see a
chronological list of in-flight Frame negotiations. Resolution: either operator may
post a resolution (calling-side operator can downgrade their request; target-side
operator can relax their constraint); resolution propagates via webhook (per
INTER-AGENT-BROKER.md Q5) to both sides; the next dispatch attempt either succeeds or
the negotiation cycles. If neither operator resolves within a cluster-manifest-tunable
TTL (default 24h), the escalation auto-aborts and the calling workflow transitions to
`failed-no-operator-resolution`. Closes the dead-end where escalation could route to
an operator without authority over the constraint.

### 7.7 — Frame visibility in awareness (PINNED)

**The LLM SEES its current Frame stack in L1 context.** Mutual-visibility of stance
is load-bearing — agents reasoning under a Frame they didn't declare (broker-
prescribed Frames per §5) need to know about it to act coherently.

Injection format (operator-tunable per cluster manifest):

```markdown
## Active Frame Stack

| Frame | Value | Source |
|---|---|---|
| hat | adversary | dispatch override |
| stakes | production | broker default |
| tempo | deliberate | cluster default |
| mode | implementation | broker default |
| confidence | tentative | dispatch override |
| audience | operator-direct | cluster default |
| scope | local | cluster default |

Suppressed (per conflict resolution): none
Source-of-truth: see BROKER-FRAMES.md §7.2 inheritance order
```

**Frame conflict L1 surface (M-6 closure).** When the conflict-precedence matrix
(§7.1) suppresses a Frame, the L1 segment surfaces the suppression **before** the
agent acts, not post-hoc in trace:

```markdown
## ⚠️ Frame Conflicts Resolved

| Suppressed Frame | Suppressed Value | Conflict With | Precedence Winner | Reason |
|---|---|---|---|---|
| hat | rubber-duck | stakes: production | stakes | "Stakes overrides Hat per cluster precedence matrix (§7.1)" |
```

When this subsection is non-empty, the active-frame-stack table's "Suppressed" row
points at it. The agent reads the suppression entry, can dispatch the Surfaced
governance pipeline `challenge-frame-conflict-resolution` (tunability:
operator-confirmed) to propose an override if it disagrees with the resolution. The
challenge writes to the Proposal Ledger (BB #21) with `type: frame-conflict-challenge`
and the operator confirms or rejects the proposed override. Without this L1 surface,
the mutual-visibility invariant pinned in §7.1 ("the agent sees both Frames AND the
suppression annotation") would be defeated at the awareness layer — the trace would
show suppression but the agent's reasoning happened under the suppressed Frame
without knowing it.

Surface lives in a dedicated `.claude/brain/broker/segments/active-frame-stack.md`
segment, composed by the Materializer Composer (#22a) into `current-projection.md`.
Budget: ~200 tokens per agent at session-start (+ ~80 tokens per active conflict
row); updates per tick if Frame stack changes (rare). Operator can disable per
cluster manifest for context-tight deployments (single-Frame deployments don't need
the table) — but the "⚠️ Frame Conflicts Resolved" subsection cannot be selectively
disabled when present (suppression visibility is load-bearing for the mutual-visibility
invariant).

---

## §8 — What this commits the framework to

The Frame primitive is **building block #35** in
[`BROKER-INTERNALS.md`](BROKER-INTERNALS.md) §3 — Layer C (Substrate composition).
Phase 5 closure assigns the number (#25-#34 occupied by Phase 3 + Phase 4 additions).

| # | Building block | Framework provides | Broker author provides |
|---|---|---|---|
| 35 | **Frame stack** | The typed Frame map in broker state per §1; the merge order across inheritance levels (§7.2); the consumption surfaces in Governance Composer / Skill Filter / Overlay curation / Workflow Engine (§3); the `with_frame:` step modifier; the `frame_rotation:` step sugar with MaxFrameRotationDepth bound (§4); the IAB negotiation protocol with refusal schema (§7.6); the seven canonical Frame types (§2); conflict precedence matrix (§7.1); rotation budget arithmetic (§7.3); coverage audit pipeline (§7.4); extension protocol (§7.5); awareness L1 injection format (§7.7) | Per-broker Frame defaults; per-pipeline Frame requirements; per-cluster Frame manifest values; conflict precedence overrides; per-Frame-type weight cells |

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
