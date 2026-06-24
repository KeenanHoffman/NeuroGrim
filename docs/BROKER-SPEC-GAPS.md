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

### Gap 01: `Broker` trait signature unspecified
- **Discovered during:** P2 pre-implementation (paper-spec scaffolding)
- **Discoverer:** Phase 9 pre-impl discovery work
- **Date:** 2026-06-24
- **Affected docs:** `BROKER-INTERNALS.md` §3 BB #1
- **Affected BBs:** #1 Broker capsule
- **Problem:** the spec lists `read_overlay()`, `legal_pipelines(state)`, `governance_pipelines()`, `tick(WorldEvent)`, role-set declaration as the trait surface. But: parameter types, return types, async-vs-sync, generic-over-Overlay-shape are all unspecified.
- **Workaround (P2 scaffold):** assumed `#[async_trait]` + associated types `OverlayShape` + `WorkingState` to make the trait usable. Used `Vec<Pipeline>` for both legal/governance return; assumed `tick(&mut self, WorldEvent) → Result<()>` shape.
- **Proposed resolution:** add a Rust trait declaration to BB #1 row in the table (or a separate "trait shape" appendix). Pin async-trait + associated types + parameter shapes.
- **Status:** open
- **Patch ref:** TBD

### Gap 02: Pipeline struct concrete type fields
- **Discovered during:** P2 pre-impl scaffolding
- **Affected docs:** `BROKER-INTERNALS.md` §1.3
- **Affected BBs:** #7 Pipeline type
- **Problem:** illustrative shape shown but: PipelineId type (UUID? hash? operator-assigned string?), ParamSchema type (JSONSchema? Rust type?), GovernancePolicy struct shape, EffectClass enum variants — all undeclared.
- **Workaround:** stringly-typed all fields in P2 scaffold; deferred decisions to operator.
- **Proposed resolution:** declare the canonical type per field in §1.3 illustrative struct.
- **Status:** open

### Gap 03: Sub-pipeline parameter passing type
- **Discovered during:** P2
- **Affected docs:** `BROKER-INTERNALS.md` §1.3 Step enum
- **Affected BBs:** #8 Step type
- **Problem:** `Step::SubPipeline(PipelineId, ParamMap)` — ParamMap type unspecified (HashMap<String, Value>? typed per pipeline contract? other?)
- **Workaround:** used `()` placeholder in P2 scaffold.
- **Proposed resolution:** pin ParamMap = `serde_json::Map<String, serde_json::Value>` OR per-pipeline-typed Rust struct.
- **Status:** open

### Gap 04: WorldEvent shape never declared
- **Discovered during:** P2
- **Affected docs:** `BROKER-INTERNALS.md` §3 BB #1 row mentions `tick(WorldEvent)` but no shape
- **Affected BBs:** #1 Broker capsule + #15 Tick Source
- **Problem:** what fields does WorldEvent carry? topic + payload? source-class + audit_class + timestamp?
- **Workaround:** P2 scaffold used `{topic: String, payload: serde_json::Value}`.
- **Proposed resolution:** declare in §3 BB #15 row + give canonical shape.
- **Status:** open

### Gap 05: Atomic-swap mechanism for Overlay updates unspecified
- **Discovered during:** P2
- **Affected docs:** `BROKER-CONTRACT.md` §"The Overlay contract"
- **Affected BBs:** #2a Overlay primitive
- **Problem:** spec says "atomic-swap updates, versioned read, no-torn-read enforcement" but doesn't pin the implementation pattern (arc-swap crate? RwLock? per-cell atomic? generational arena?).
- **Workaround:** P2 deferred; left as `todo!()`.
- **Proposed resolution:** name the canonical impl pattern (probably `arc-swap` crate per Rust idiom).
- **Status:** open

### Gap 06: Framework-provided governance pipeline registry mechanism
- **Discovered during:** P2 (catalog.yaml authoring)
- **Affected docs:** `BROKER-INTERNALS.md` §2.4 + BB #19
- **Affected BBs:** #19 Governance Composer
- **Problem:** spec says brokers reference framework-provided governance pipelines by name in their YAML (`compose: [check-trust-budget, check-kill-switch, ...]`) but doesn't specify where the registry of framework-provided pipelines lives or how brokers discover available ones.
- **Workaround:** P2 assumed naming convention works (operator declares + framework provides matching pipelines).
- **Proposed resolution:** declare a `<neurogrim-brokers>/src/governance/` module containing the registered framework-provided governance pipelines; spec how brokers reference them.
- **Status:** open

### Gap 07: Precondition predicate DSL unspecified
- **Discovered during:** P2 catalog.yaml authoring
- **Affected docs:** `BROKER-INTERNALS.md` §1.3 + BB #9 + BROKER-CONTRACT central invariant
- **Affected BBs:** #7 Pipeline type, #9 Pipeline Catalog, #10 Pipeline Runner
- **Problem:** the spec's central invariant ("LLM never sees a capability whose preconditions aren't met") depends on preconditions being evaluable. But the predicate DSL is undeclared (free-text strings? RHAI script? Rust closures? boolean field references?).
- **Workaround:** P2 used string identifiers (`overlay_has_score`) assuming convention-based name-to-evaluator lookup.
- **Proposed resolution:** pin the predicate language (proposal: limited YAML expression DSL evaluating against the hot store).
- **Status:** open

### Gap 08: Manifest TOML shape for tick subscriptions
- **Discovered during:** P2 manifest.toml authoring
- **Affected docs:** `BROKER-MANIFEST-SCHEMA.md`
- **Affected BBs:** #15 Tick Source
- **Problem:** broker manifest must declare its tick subscriptions but BROKER-MANIFEST-SCHEMA doesn't show the section/field layout.
- **Workaround:** P2 used `tick_subscriptions = ["coherence_score_changed"]` top-level field.
- **Proposed resolution:** add tick-subscription field declaration to BROKER-MANIFEST-SCHEMA.
- **Status:** open

### Gap 09: Frame defaults — required vs optional in broker manifest
- **Discovered during:** P2 manifest.toml authoring
- **Affected docs:** `BROKER-FRAMES.md` §7.2 inheritance + BROKER-MANIFEST-SCHEMA.md
- **Affected BBs:** #35 Frame stack
- **Problem:** broker manifest may declare per-broker Frame defaults. But: are all 7 Frame types required? Or can broker omit some (inherit from cluster)? Spec is silent on partial-declaration semantics.
- **Workaround:** P2 declared 2 Frame defaults (Hat, Stakes) under assumption inheritance fills the rest.
- **Proposed resolution:** declare partial-declaration semantics explicitly in BROKER-FRAMES §7.2.
- **Status:** open

### Gap 10: Test-fixture setup pattern (no replay harness exists pre-S0-T)
- **Discovered during:** P2 tests/fixture.rs authoring
- **Affected docs:** `BROKER-INTERNALS.md` §3 BB #13 Replay tooling
- **Affected BBs:** #13 Replay tooling
- **Problem:** spec says replay tooling subsumes test-fixture machinery; tests are replay test cases. But pre-S0-T, the replay harness doesn't exist. What's the bootstrap pattern for the FIRST test before BB #13 exists?
- **Workaround:** P2 scaffold left as `todo!()`; deferred.
- **Proposed resolution:** declare a pre-BB-#13 bootstrap-test pattern (manual cold-store fixture + manual Overlay seed) for the first dozen tests; migrate to replay harness once BB #13 lands.
- **Status:** open

### Pre-populated future gap entries (discovered during S0-T entry; structure prepared)

*(Operator: continue populating as S0-T encounters gaps. Each entry above
demonstrates the format; structure is consistent + scannable.)*

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
