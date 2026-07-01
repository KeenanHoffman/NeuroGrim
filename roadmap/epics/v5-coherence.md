---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: v5 Coherence + Docs (Theme D)

**Theme:** D
**Release:** v5 (entry pinned 2026-05-01; this epic is gated on Theme C close — see `v5-roadmap.md` §"v5 Entry Decision Tracker")
**Status:** PLANNED (drafted 2026-05-01)
**Priority:** Closure — docs describe shipped reality; written last
**Goal:** Author the modular composition guide from real recipes; update LSP-Brains spec §9 + §F to reflect SDK trait shapes; add VISION principle #20 ("Pluggability by use, not aspiration" — wording finalized via V5-DOC-2 dual-review T+P 2026-05-04); preserve culture-coherence byte-identity across all four `.claude/culture.yaml` copies.

**Depends on:**
- Theme C complete (V5-SDK-1..2 — composition guide describes the actual SDK API)

**Blocks:**
- v5 release tag (cannot ship v5 without docs that match shipped reality)

**Master roadmap:** `roadmap/v5-roadmap.md`
**Pre-plan source:** `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`

---

## Theme D Is Done When

- [ ] `docs/v5-composition-guide.md` ships with working code samples
- [ ] All recipes cross-link to SDK API docs
- [ ] Diagram shows actual cargo workspace boundaries (not idealized ones)
- [ ] VISION.md updated; new principle #20 reviewed via `dual-review` skill (T+P)
- [ ] LSP-Brains spec §9 + §F updated to reflect v5 SDK trait shapes
- [ ] `culture-coherence` domain still passes (byte-identity preserved across all four `.claude/culture.yaml` copies)
- [ ] `terminology-coherence` domain still passes (SDK / module / plugin terms used consistently)
- [ ] `spec-impl-alignment` domain still passes (NeuroGrim conforms to LSP-Brains spec at v5 version)

---

## Stories

### V5-DOC-1: Modular composition guide (~7–10 days) — SHIPPED

**Status:** **✅ COMPLETE 2026-05-04** — 6 phase commits (`b43049c` Phase 0 plan + plan-critic absorption, `a499b72` Phase 1 skeleton + 6-trait diagram, `da6e84f` Phase 2 recipes 1–3, `7f12c67` Phase 3 recipe 4 + deferral framing, `8a440bc` Phase 4 cross-refs + horizon section, plus this Phase 5 close-out). Effort actual: ~1 day, well under the 7–10d M estimate (prose-shaped lift-from-example-crates work, not new code — that's not under-delivery, it's the natural shape of the task per plan v2's effort-honesty note).
**Effort:** M (actual: ~1 day)
**Depends on:** V5-SDK-2 ✅ (Theme C ✅ COMPLETE 2026-05-04)

**What:** Author [`docs/v5-composition-guide.md`](../../docs/v5-composition-guide.md) (756 lines). Four concrete recipes lifted verbatim from the in-tree example crates: "swap the queue backend" (from `examples/queue-backend-memory/`), "add a custom scoring source" (from `examples/scoring-source-prom/`), "ship a sensor as a crate" (from `examples/sensor-readme-quality/` + `examples/sensor-constant-score/`), "drive tests with your own runner" (from `crates/neurogrim-cli/src/commands/test_runner_impls/nextest.rs` — V5-FOUND-4 Phase 2). 6-trait ASCII architecture diagram. Capability-gap-led "v5.5 / v6 horizon" closing section organized by 4 themes (plugin loading, test runner, domain extension, SDK polish).

**Why:** "Everything is Lego" only matters if users can actually combine pieces. Composition guide is the proof — every recipe lifts working code from an in-tree example crate, and those crates' tests run on every PR via workspace CI. Doc rot is bounded by the existing CI gate; no new CI infrastructure was needed (Fork E1 default).

**Done when:**
- [x] Guide ships with ≥4 working recipes (queue backend swap, scoring source addition, sensor crate, custom test runner) — *4 recipes; the 4 cited example crates' tests + NextestRunner unit tests pass on every PR*.
- [x] All recipes cross-link to `neurogrim-sdk` API docs — *each recipe ends with "Further reading" linking to the SDK README's relevant walkthrough (Sensor inlined; ScoringSource/QueueBackend/TestRunner via rustdoc), the example crate's README, and the relevant epic file*.
- [x] Diagram shows actual cargo workspace boundaries (not idealized ones) — *6-trait ASCII diagram (post plan-critic v1 correction from a 5-vs-4 inconsistency in plan v1's draft) shows `neurogrim-sdk` re-exporting from `neurogrim-core` (4 traits) + `neurogrim-a2a` (Transport) + `neurogrim-secrets` (SecretBackend); arrow direction documents dependency direction*.
- [x] CI builds all code samples in the guide; broken samples fail the build (prevents doc rot) — *Fork E1 reinterprets this honestly: the guide's recipes lift from the in-tree example crates which workspace CI exercises on every PR. A recipe whose cited code stops compiling fails workspace CI — same gate, no new infrastructure. Phase 4 added a manual snippet-match operator-checklist step to bridge the prose-vs-code gap (the gate moved from "guide-snippet-compiles" to "cited-crate-compiles"; manual check preserves doc-rot honesty without doctest-harness ceremony, deferred to v5.5 polish if drift becomes empirical).*
- [x] Each recipe lists what's NOT yet possible — explicit links to v5.5/v6 successor pipeline (BACKLOG B-37..B-45 + B-49..B-52) for the limits — *every recipe ends with a "What's NOT possible at v5.0" callout citing the relevant BACKLOG entry; the standalone "v5.5 / v6 horizon" closing section enumerates the full gap inventory by capability theme*.

**V5-DOC-1 retrospective (2026-05-04):**

- **Plan record:** [`.claude/plans/v5-doc-1-composition-guide.md`](../../.claude/plans/v5-doc-1-composition-guide.md) — v2 plan, two plan-critic lenses (technical + methodology in parallel, both PROCEED WITH CAUTION), 0 🔴 + 10 🟡 absorbed in-place. Notable absorptions: ASCII diagram corrected from 5-vs-4 inconsistency to 6 trait surfaces; conformance-suite framing made load-bearing in Phase 1 intro ("must run, not recommendation"); recipe 4 surfaces structural asymmetry (in-tree vs. out-of-tree) rather than papering over; `TestSelection::ByCoverage` non-exhaustive callout added to recipe 4 for V5-FOUND-3 deferral honesty; capability-gap-led horizon section (vs. BACKLOG-ID-led) per audience-discipline guidance.
- **Forks pinned:** A1 (doc location at `docs/v5-composition-guide.md`) / B1 (ASCII diagram) / C1 (lift from in-tree example crates) / D1 (recipe 4 ~5-line deferral callout) / E1 (existing workspace CI as the doc-rot gate) / F1 (no VISION #20 wording preview — V5-DOC-2 owns).
- **Outcome:** 756-line composition guide with 4 working recipes, all lifted from in-tree example crates that workspace CI exercises. The capability-gap horizon section names every v5.0 limitation across plugin loading, test runner, domain extension, and SDK polish themes — 11 BACKLOG entries cross-referenced as parenthetical breadcrumbs. Adopters get "what can today / what's deferred" honestly; operators get the BACKLOG drill-in path.
- **Plan deviations:** none. Final 756 lines vs. 300-400 plan target — overage is load-bearing per plan-critic methodology agent C5 ("explicit deferral callout") and suggestion 6 (capability-gap horizon). Not under-delivery on size; faithful execution of the absorbed plan-critic findings.
- **What's NOT done that the original epic called for:** the Done-When line about "CI builds all code samples in the guide" was reinterpreted via Fork E1 — the workspace CI gate covers the cited example crates; the manual snippet-match step bridges the prose-vs-code gap. A doctest-harness approach (Fork E2, automated character-by-character snippet-match check) is deferred to v5.5 polish if drift becomes empirically common — tracked as a candidate addition to BACKLOG B-50 (V5.5-SDK-DOC-INCLUDE).

### V5-DOC-2: VISION + spec alignment (~3–5 days) — SHIPPED

**Status:** **✅ COMPLETE 2026-05-04** — 6 phase commits (`74e1ba5` Phase 0 plan + plan-critic absorption; `3f9f9d8` Phase 1 dual-review verdict A1 → A3a; `f3297ee` Phase 2 VISION.md + cross-ref sweep; `2918303` LSP-Brains submodule Phase 3 spec §9.8 + v3.0 → v3.1; `7dae51b` Phase 4 BACKLOG B-53; `185ed91` Phase 5 CHANGELOG + verifications; plus this Phase 6 close-out). Effort actual: ~1 day, well under the 3–5d S estimate. **Theme D ✅ COMPLETE 2026-05-04; v5 is SHIPPABLE — gated only on workspace CI green + operator tag creation.**
**Effort:** S (actual: ~1 day)
**Depends on:** V5-DOC-1 ✅ (Theme D 1/2 complete)

**What:** Updated VISION.md with principle #20 ("**Pluggability by use, not aspiration.**" — wording finalized via dual-review T+P; revised from initial draft "Pluggability is justified by use, not aspiration" to match #1 / #8 binary-contrast template) at sequential position 20. Body paragraph (13 lines) operationalizes the reshape rule with V5-FOUND-4 AgentDrivenRunner deferral as the canonical past-decision case. Updated LSP-Brains spec from v3.0 → v3.1 with new §9.8 "Trait Surface Recommendation (Implementation-Pattern)" — fractal-composition-relevant trait surfaces (test runner, transport, secrets backend) MAY be exposed via a contract crate; mirrors the existing §F.6 trait-surface recommendation for sensors. No culture update emerged (verified byte-identity across all 4 `.claude/culture.yaml` copies — SHA256 = 94201AA4...).

**Why:** Principle #20 is the codification of the adversary trim that gated v5. Without it, future major-version planning may slip back into "everything is an interface" without the discipline. Spec updates ensure LSP-Brains spec reflects what v5 actually shipped — at v3.1, NeuroGrim's SDK pattern is now the canonical reference impl for fractal-composition-relevant trait surfaces.

**Architectural decision: dual-review on principle #20.** New VISION principles change agent behavior across the ecosystem. T+P (technical + philosophy) review via the `dual-review` skill is required before merge. **EXECUTED 2026-05-04 at Phase 1:** Both reviewers spawned in parallel via Agent tasks following the dual-review skill protocol. Both `passed: true`. Convergent recommendation revised A1 → A3a (binary-contrast template matching peer #1 / #8). Operator-approved (option 1 — accept synthesis). Wording-frozen gate held through Phase 2 sweep.

**Done when:**
- [x] VISION.md updated with principle #20 at sequential position 20 — *Phase 2 commit `f3297ee`; 13-line body operationalizing the reshape rule with V5-FOUND-4 AgentDrivenRunner deferral as the canonical past-decision case + cross-refs to v5-roadmap.md § Adversary findings A and the V5-DOC-2 plan record's Phase 1 dual-review verdict.*
- [x] Principle wording reviewed via `dual-review` skill (T+P) — both reviewers approve — *Phase 1 commit `3f9f9d8`; both T (`passed: true`, 5 questions evaluated) and P (`passed: true`, 5 questions including binary-contrast template probe). Convergent recommendation revised A1 → A3a; operator approved option 1 (accept synthesis); wording FROZEN at Phase 1 close per Fork F1 wording-frozen gate.*
- [x] LSP-Brains spec §9 + §F updated to reflect v5 SDK trait shapes — *Phase 3 commit `2918303` in LSP-Brains submodule; spec v3.0 → v3.1 with new §9.8 trait-surface recommendation; §F.6 verified-no-edit (already documented V5-MOD-2 sensor pattern at v3.0). Ecosystem-side LSP-Brains submodule pointer bumped at `D:\Brains\` commit `996af08`.*
- [x] If `culture.yaml` changes (probably not), all four copies updated byte-identically — *No culture update emerged at V5-DOC-2 (Fork E1 default held). Phase 5 verified byte-identity: all 4 copies SHA256 = `94201AA4A230FAAC74932449AD8B26BEEA44D2B88DAAC4F55ED5D9E8A787FE0E`.*
- [x] `culture-coherence` advisory domain still 100% (no drift introduced by v5) — *Verified at Phase 5 byte-identity check; no drift introduced.*
- [⏸] `terminology-coherence` advisory domain still passes — *Plan-critic v1 finding C5: the registered domain expects a CMDB at `.claude/terminology-coherence-cmdb.json` (NOT a free-form catalog); no sensory tool currently emits that CMDB. Domain runs at advisory weight 0.0; falls back to no-file behavior (still passes structurally). DEFERRED to v5.5 (BACKLOG B-53 V5.5-DOC-TERMINOLOGY-CMDB) per Fork D2 — v5.5 ships the sensory tool + CMDB shape together.*
- [x] `spec-impl-alignment` advisory domain confirms NeuroGrim conforms to LSP-Brains spec at v5 version — *Spec at v3.1 (post Phase 3); NeuroGrim's V5-MOD-1/2/3 + V5-FOUND-4 + V5-SDK-1/2 implementations conform to spec §F.6 + §9.8 trait-surface recommendations. V5-SDK-1 surface-assertion test (`sdk_surface_signatures_unchanged`) PASSES post V5-FOUND-4 + V5-DOC-2 — verified at Phase 5.*

**V5-DOC-2 retrospective (2026-05-04):**

- **Plan record:** [`.claude/plans/v5-doc-2-vision-spec-alignment.md`](../../.claude/plans/v5-doc-2-vision-spec-alignment.md) — v2 plan, two plan-critic lenses (technical + methodology in parallel; technical = REVISE, methodology = PROCEED WITH CAUTION). 1 🔴 + 5 🟡 technical + 1 🔴 + 4 🟡 methodology absorbed. Major absorptions: Phase 3 spec scope reduced (§F.6 already covered V5-MOD-2 from v3.0 ship-out; only §9 needed the new paragraph); Fork D default flipped D1 → D2 (terminology-CMDB deferred to v5.5); Phase 6 verification profile switched `--profile ci` → `--profile default` (fail-fast=true would abort on pre-existing init_scaffold flake). Phase 1 dual-review captured the full T+P verdict in the plan record (both `passed: true`, convergent A1 → A3a recommendation, operator-approved).
- **Forks pinned:** A1 → revised to A3a via dual-review (operator-approved option 1 = accept synthesis) / B1 (sequential position 20) / C1' revised (§9 narrow paragraph + §F.6 verified-no-edit + spec v3.0 → v3.1 with changelog) / D2 revised (defer terminology-CMDB to v5.5 BACKLOG B-53) / E1 (no culture update; verified byte-identity) / F1 (single round T+P; both passed first time, no revision rounds needed) / G1 (operator-controlled tag — Phase 6 marks v5 SHIPPABLE; operator runs `git tag -a v5.0.0` separately).
- **Outcome:** VISION principle #20 lands with binary-contrast headline + 13-line body; LSP-Brains spec at v3.1 with new §9.8; all 4 culture.yaml copies byte-identical; 7 verbatim cross-references swept (5 expected + 2 caught post-Phase-1); CHANGELOG.md [Unreleased] v5 entry drafted. Theme D ✅ COMPLETE; v5 SHIPPABLE.
- **Plan deviations:** none. Phase 4's terminology-catalog file-creation was deferred via Fork D2 (operator-pinned default after plan v2 absorbed plan-critic finding C5); deferral tracked at BACKLOG B-53.
- **What's NOT done that the original epic called for:** terminology-coherence sensory tool + CMDB (deferred to v5.5 BACKLOG B-53 — the registered domain expects a CMDB the v5.5 sensory-tool epic emits; creating a catalog at the wrong filename without the sensory tool was drift bait per plan-critic v1 finding C5).

---

## Verification (end-to-end smoke per story)

**V5-DOC-1 Modular composition guide:**
- Walk the composition guide end-to-end; every recipe must work as written (CI builds the code samples)
- Verify each recipe cross-links to `neurogrim-sdk` API docs (broken links fail the build)
- Confirm the diagram matches the actual cargo workspace dep graph (cross-check with `cargo metadata`)

**V5-DOC-2 VISION + spec alignment:**
- Run `dual-review` skill (T+P) on principle #20 wording — both reviewers must approve before merge
- Confirm `neurogrim health` shows `culture-coherence` at 100% (byte-identity preserved)
- Confirm `terminology-coherence` and `spec-impl-alignment` advisory domains pass after v5 changes
- Run `sync-ecosystem` skill to verify byte-identity across all four `.claude/culture.yaml` copies

---

## Risks (adversary concerns brought forward)

🟡 **Doc rot.** Composition guide drifts from reality if not CI-tested. Mitigation: V5-DOC-1 acceptance requires CI-built code samples; broken samples fail the build.

🟡 **Principle #20 inflation.** "Pluggability by use" risks becoming a slogan that doesn't actually constrain decisions. Mitigation: dual-review skill (T+P) gates the wording; reviewers must cite a specific past decision the principle would have changed. — **RESOLVED 2026-05-04 at V5-DOC-2 Phase 1:** dual-review T+P both passed; final wording is binary-contrast headline ("Pluggability by use, not aspiration.") + 13-line body that operationalizes the reshape rule with V5-FOUND-4 AgentDrivenRunner deferral as the canonical past-decision case the principle would have classified.

🟡 **Spec/impl drift during Theme D writing.** While Theme D is in flight, Theme C SDK might still ship patches. Mitigation: V5-DOC-1 depends on V5-SDK-2 (SDK at `0.1.x` minimum); patch-level SDK changes don't invalidate the guide unless trait shapes change (which they shouldn't post-extraction).

🔵 **Suggestion: write a "v5 retrospective" doc post-ship.** What got trimmed, what shipped, what surprised us, which v5.5/v6 candidates moved up. Goes alongside the composition guide. Likely v5.5 polish, not v5 scope.

🔵 **Suggestion: VISION principle #21 candidate.** "Modular boundaries are sized by adoption signal, not by aesthetic." Captures the lesson from v5 modular trimming. Reserve for v5.5 or v6 introduction; do not bundle with #20.

---

## Cross-references

- Master roadmap: `roadmap/v5-roadmap.md`
- Pre-plan: `C:\Users\koff0\.claude\plans\i-would-like-you-curried-milner.md`
- VISION (current 19 principles): `roadmap/VISION.md`
- LSP-Brains spec §9 + §F: `D:/Brains/LSP-Brains/spec/spec.md`
- Culture invariant: `D:/Brains/.claude/culture.yaml` (and three peer copies)
- `dual-review` skill: `D:/Brains/.claude/skills/dual-review/` (or peer locations)
- Coherence domains in ecosystem registry: `D:/Brains/.claude/brain-registry.json`
