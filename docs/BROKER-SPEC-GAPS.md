# Broker Spec — Discovered Gap Ledger

> **Pre-implementation status.** The 38-BB broker framework spec is feature-complete
> at the design level but no reference implementation has run yet. This ledger is the
> append-only record of spec gaps discovered during S0-T → S1-T → S2-T → S\*-C
> implementation. Each gap surfaces a question or ambiguity the spec did not
> anticipate; the resolution is patched back into the relevant doc(s) and recorded
> here with provenance.

**Created:** 2026-06-24 (Phase 9 risk-triage closure of R-X-14). **Status
(2026-06-24, post-V0):** 15 gaps logged. 13 ratified (10 from V0
implementation + 5 new V0 surfacings of which 3 ratified inline; gaps #11
+ #13 + #14 ratified spec but implementation pending in Wave 5.5). 2 open
(#9 Frame defaults — awaits BB #35 deferred; #11 R-O-4 backend
validation — awaits Wave 5.5 benchmark). The original R-X-14 kill criterion
predicted "100 unspecified things" from first implementation; actual rate
~15 is significantly lower — net positive for the spec's pre-impl
thoroughness.

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
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed) | proposed-patch | ratified | declined-with-rationale
- **Patch ref:** <commit SHA / PR link when ratified>
```

---

## Gaps (chronological)

### Gap 01: `Broker` trait signature unspecified
- **Discovered during:** P2 pre-implementation (paper-spec scaffolding)
- **Discoverer:** Phase 9 pre-impl discovery work
- **Date:** 2026-06-24
- **Affected docs:** `BROKER-INTERNALS.md` §3 BB #1
- **Affected BBs:** #1 Broker capsule
- **Problem:** the spec lists `read_overlay()`, `legal_pipelines(state)`, `governance_pipelines()`, `tick(WorldEvent)`, role-set declaration as the trait surface. But: parameter types, return types, async-vs-sync, generic-over-Overlay-shape are all unspecified.
- **Workaround (P2 scaffold):** assumed `#[async_trait]` + associated types `OverlayShape` + `WorkingState` to make the trait usable. Used `Vec<Pipeline>` for both legal/governance return; assumed `tick(&mut self, WorldEvent) → Result<()>` shape.
- **Proposed resolution:** add a Rust trait declaration to BB #1 row in the table (or a separate "trait shape" appendix). Pin async-trait + associated types + parameter shapes.
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)
- **Patch ref:** TBD

### Gap 02: Pipeline struct concrete type fields
- **Discovered during:** P2 pre-impl scaffolding
- **Affected docs:** `BROKER-INTERNALS.md` §1.3
- **Affected BBs:** #7 Pipeline type
- **Problem:** illustrative shape shown but: PipelineId type (UUID? hash? operator-assigned string?), ParamSchema type (JSONSchema? Rust type?), GovernancePolicy struct shape, EffectClass enum variants — all undeclared.
- **Workaround:** stringly-typed all fields in P2 scaffold; deferred decisions to operator.
- **Proposed resolution:** declare the canonical type per field in §1.3 illustrative struct.
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)

### Gap 03: Sub-pipeline parameter passing type
- **Discovered during:** P2
- **Affected docs:** `BROKER-INTERNALS.md` §1.3 Step enum
- **Affected BBs:** #8 Step type
- **Problem:** `Step::SubPipeline(PipelineId, ParamMap)` — ParamMap type unspecified (HashMap<String, Value>? typed per pipeline contract? other?)
- **Workaround:** used `()` placeholder in P2 scaffold.
- **Proposed resolution:** pin ParamMap = `serde_json::Map<String, serde_json::Value>` OR per-pipeline-typed Rust struct.
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)

### Gap 04: WorldEvent shape never declared
- **Discovered during:** P2
- **Affected docs:** `BROKER-INTERNALS.md` §3 BB #1 row mentions `tick(WorldEvent)` but no shape
- **Affected BBs:** #1 Broker capsule + #15 Tick Source
- **Problem:** what fields does WorldEvent carry? topic + payload? source-class + audit_class + timestamp?
- **Workaround:** P2 scaffold used `{topic: String, payload: serde_json::Value}`.
- **Proposed resolution:** declare in §3 BB #15 row + give canonical shape.
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)

### Gap 05: Atomic-swap mechanism for Overlay updates unspecified
- **Discovered during:** P2
- **Affected docs:** `BROKER-CONTRACT.md` §"The Overlay contract"
- **Affected BBs:** #2a Overlay primitive
- **Problem:** spec says "atomic-swap updates, versioned read, no-torn-read enforcement" but doesn't pin the implementation pattern (arc-swap crate? RwLock? per-cell atomic? generational arena?).
- **Workaround:** P2 deferred; left as `todo!()`.
- **Proposed resolution:** name the canonical impl pattern (probably `arc-swap` crate per Rust idiom).
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)

### Gap 06: Framework-provided governance pipeline registry mechanism
- **Discovered during:** P2 (catalog.yaml authoring)
- **Affected docs:** `BROKER-INTERNALS.md` §2.4 + BB #19
- **Affected BBs:** #19 Governance Composer
- **Problem:** spec says brokers reference framework-provided governance pipelines by name in their YAML (`compose: [check-trust-budget, check-kill-switch, ...]`) but doesn't specify where the registry of framework-provided pipelines lives or how brokers discover available ones.
- **Workaround:** P2 assumed naming convention works (operator declares + framework provides matching pipelines).
- **Proposed resolution:** declare a `<neurogrim-brokers>/src/governance/` module containing the registered framework-provided governance pipelines; spec how brokers reference them.
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)

### Gap 07: Precondition predicate DSL unspecified
- **Discovered during:** P2 catalog.yaml authoring
- **Affected docs:** `BROKER-INTERNALS.md` §1.3 + BB #9 + BROKER-CONTRACT central invariant
- **Affected BBs:** #7 Pipeline type, #9 Pipeline Catalog, #10 Pipeline Runner
- **Problem:** the spec's central invariant ("LLM never sees a capability whose preconditions aren't met") depends on preconditions being evaluable. But the predicate DSL is undeclared (free-text strings? RHAI script? Rust closures? boolean field references?).
- **Workaround:** P2 used string identifiers (`overlay_has_score`) assuming convention-based name-to-evaluator lookup.
- **Proposed resolution:** pin the predicate language (proposal: limited YAML expression DSL evaluating against the hot store).
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)

### Gap 08: Manifest TOML shape for tick subscriptions
- **Discovered during:** P2 manifest.toml authoring
- **Affected docs:** `BROKER-MANIFEST-SCHEMA.md`
- **Affected BBs:** #15 Tick Source
- **Problem:** broker manifest must declare its tick subscriptions but BROKER-MANIFEST-SCHEMA doesn't show the section/field layout.
- **Workaround:** P2 used `tick_subscriptions = ["coherence_score_changed"]` top-level field.
- **Proposed resolution:** add tick-subscription field declaration to BROKER-MANIFEST-SCHEMA.
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)

### Gap 09: Frame defaults — required vs optional in broker manifest
- **Discovered during:** P2 manifest.toml authoring
- **Affected docs:** `BROKER-FRAMES.md` §7.2 inheritance + BROKER-MANIFEST-SCHEMA.md
- **Affected BBs:** #35 Frame stack
- **Problem:** broker manifest may declare per-broker Frame defaults. But: are all 7 Frame types required? Or can broker omit some (inherit from cluster)? Spec is silent on partial-declaration semantics.
- **Workaround:** P2 declared 2 Frame defaults (Hat, Stakes) under assumption inheritance fills the rest.
- **Proposed resolution:** declare partial-declaration semantics explicitly in BROKER-FRAMES §7.2.
- **Status:** open — BB #35 Frame stack deferred to post-MVP per S\*-T plan; V0-RETROSPECTIVE.md §C2 explicitly documents tunability declarations are currently metadata-only because Frame stack isn't implemented. Resolves when BB #35 lands (S1-T candidate; operator decision).
- **Patch ref:** N/A (deferred)

### Gap 10: Test-fixture setup pattern (no replay harness exists pre-S0-T)
- **Discovered during:** P2 tests/fixture.rs authoring
- **Affected docs:** `BROKER-INTERNALS.md` §3 BB #13 Replay tooling
- **Affected BBs:** #13 Replay tooling
- **Problem:** spec says replay tooling subsumes test-fixture machinery; tests are replay test cases. But pre-S0-T, the replay harness doesn't exist. What's the bootstrap pattern for the FIRST test before BB #13 exists?
- **Workaround:** P2 scaffold left as `todo!()`; deferred.
- **Proposed resolution:** declare a pre-BB-#13 bootstrap-test pattern (manual cold-store fixture + manual Overlay seed) for the first dozen tests; migrate to replay harness once BB #13 lands.
- **Status:** ratified (V0 implementation + post-V0 spec amendments landed)

### Pre-populated future gap entries (discovered during S0-T entry; structure prepared)

*(Operator: continue populating as S0-T encounters gaps. Each entry above
demonstrates the format; structure is consistent + scannable.)*

---

## Gaps surfaced by V0 implementation (Wave 0-5, 2026-06-24)

These gaps emerged during the V0 prototype build — things the spec didn't
anticipate that implementation forced a choice on. See
`cereGrim/docs/V0-RETROSPECTIVE.md` for the full context per finding.

### Gap 11: Per-broker SQLite isolation (R-O-4) unvalidated
- **Discovered during:** V0 Wave 5 (Work Broker uses in-memory BacklogState)
- **Date:** 2026-06-24
- **Affected docs:** `BROKER-INTERNALS.md` BB #6
- **Affected BBs:** #6 Cold Store
- **Problem:** R-O-4 closure claimed "per-broker cold-store file isolation by default" but no real SQLite or JSONL backend has been exercised in V0; the contention claim is a paper claim until benchmarked.
- **Workaround (V0):** Work Broker uses in-memory `BacklogState` for MVP demo; no backend dependency.
- **Proposed resolution:** Wave 5.5 or S1-T must land a JSONL backend behind a feature flag + run the 10-broker contention benchmark per BRK-04-COLD-STORE-CONTENTION test plan in `broker-framework-backlog.md`. Until then, R-O-4 isolation is unvalidated.
- **Status:** open — flagged in BB #6 row (post-V0 amendment)
- **Patch ref:** spec-amendment commit (post-V0)

### Gap 12: Reachability invariant violation possible via missing governance segment
- **Discovered during:** V0 Wave 3 + Wave 5 integration testing
- **Date:** 2026-06-24
- **Affected docs:** `BROKER-INTERNALS.md` BB #22a + §4
- **Affected BBs:** #22a Materializer Composer
- **Problem:** Materializer Composer's "missing governance segment" handling is a soft warning + continue; if a broker forgets to emit its governance segment in production, the agent silently loses governance visibility. Violates R-O-3 + LB-3 closures' structural reachability invariant.
- **Workaround (V0):** soft warning + emit a `_No governance pipelines segment present_` marker line; continue compose.
- **Proposed resolution:** add `enforce_governance_segment_present: bool` cluster-manifest field (default `true` in production; `false` for first-boot / dev). When strict, missing governance segment fails the compose with `failure_reason: governance_segment_missing`. V0 ships with soft warning; production MUST flip to strict.
- **Status:** ratified (V0-RETROSPECTIVE §C4 + BB #22a amendment landed)
- **Patch ref:** spec-amendment commit (post-V0)

### Gap 13: Broker Registry lacks `full_catalog()` aggregation
- **Discovered during:** V0 Wave 5 (Runner needs catalog per-dispatch; Registry didn't aggregate)
- **Date:** 2026-06-24
- **Affected docs:** `BROKER-INTERNALS.md` BB #14
- **Affected BBs:** #14 Broker Registry
- **Problem:** Runner receives catalog per-dispatch as a parameter; caller had to aggregate manually across brokers. Spec didn't include the aggregation method.
- **Workaround (V0):** end-to-end integration test pre-constructs the catalog from a separate WorkBroker.catalog() call.
- **Proposed resolution:** add `BrokerRegistry::full_catalog()` returning `Vec<Pipeline>` aggregated across all registered brokers. ~20 LOC; Wave 5.5 closes.
- **Status:** ratified in spec (BB #14 row amendment); implementation pending in Wave 5.5
- **Patch ref:** spec-amendment commit (post-V0); impl pending

### Gap 14: Materializer auto-trigger after dispatch not wired
- **Discovered during:** V0 Wave 5 integration testing
- **Date:** 2026-06-24
- **Affected docs:** `BROKER-INTERNALS.md` BB #22a + BB #10
- **Affected BBs:** #22a Materializer Composer, #10 Pipeline Runner
- **Problem:** Dispatch updates Overlay state but does NOT automatically re-materialize `current-projection.md`. Operator's main binary must explicitly re-trigger materialization after each dispatch. Spec was ambiguous about WHO re-triggers.
- **Workaround (V0):** integration test calls `materialize_all()` explicitly after dispatch.
- **Proposed resolution:** add `PipelineRunner::on_dispatch_complete` callback hook; operator's main binary registers a callback that re-runs the Materializer Composer. Decouples Runner ↔ Materializer (Runner doesn't depend on Composer); operator wires per their cadence preference.
- **Status:** ratified in spec (BB #22a amendment); implementation pending in Wave 5.5
- **Patch ref:** spec-amendment commit (post-V0); impl pending

### Gap 15: Tunability declarations are metadata-only without Frame stack
- **Discovered during:** V0 Wave 5 (Work Broker declared OperatorConfirmed tunability with no behavior shift)
- **Date:** 2026-06-24
- **Affected docs:** `BROKER-INTERNALS.md` §4 Tunability + `BROKER-FRAMES.md`
- **Affected BBs:** #19 Governance Composer + #35 Frame stack (deferred)
- **Problem:** Pipelines declare tunability tiers (OperatorConfirmed, Autonomous, etc.) but the Frame-stack-driven governance behaviors the spec premises (`stakes: production` auto-composes `require-operator-confirmation`) don't activate because BB #35 Frame stack is deferred to post-MVP. Agents + operators see the tunability declaration; framework doesn't USE Frame context to gate behavior beyond basic tier semantics.
- **Workaround (V0):** ship V0 with declarative-only tunability (current default); tunability tier is visible + the Untunable/OperatorOnly enforcement per R-S-18 closure works; Frame-driven behavior shifts await BB #35.
- **Proposed resolution:** operator decision — accept declarative-only V0 default, OR pull BB #35 Frame stack into MVP scope (~3-5 days additional). Recommendation: declarative-only V0; revisit when measurable agent-experience friction emerges.
- **Status:** ratified in spec (§4 V0 implementation note); BB #35 deferred per S\*-T plan
- **Patch ref:** spec-amendment commit (post-V0)

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
