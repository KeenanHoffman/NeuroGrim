# Epic: v5 Coherence + Docs (Theme D)

**Theme:** D
**Release:** v5 (entry pinned 2026-05-01; this epic is gated on Theme C close — see `v5-roadmap.md` §"v5 Entry Decision Tracker")
**Status:** PLANNED (drafted 2026-05-01)
**Priority:** Closure — docs describe shipped reality; written last
**Goal:** Author the modular composition guide from real recipes; update LSP-Brains spec §9 + §F to reflect SDK trait shapes; add VISION principle #20 ("Pluggability is justified by use, not aspiration"); preserve culture-coherence byte-identity across all four `.claude/culture.yaml` copies.

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

### V5-DOC-1: Modular composition guide (~7–10 days)

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

### V5-DOC-2: VISION + spec alignment (~3–5 days)

**Status:** Planned
**Effort:** S
**Depends on:** V5-DOC-1

**What:** Update VISION.md (currently 19 principles per pre-plan exploration) with one v5-shaped principle (proposed wording, subject to LSP-Brains spec coordination): "**#20 Pluggability is justified by use, not aspiration.**" Update LSP-Brains spec §9 (fractal composition) and §F (MCP sensory tools) for SDK-shape changes. If any culture update emerges during implementation (none currently proposed), mirror byte-identically across all four `.claude/culture.yaml` copies per CLAUDE.md culture-changes-propagation rule.

**Why:** Principle #20 is the codification of the adversary trim that gated v5. Without it, future major-version planning may slip back into "everything is an interface" without the discipline. Spec updates ensure LSP-Brains spec reflects what v5 actually shipped.

**Architectural decision: dual-review on principle #20.** New VISION principles change agent behavior across the ecosystem. T+P (technical + philosophy) review via the `dual-review` skill is required before merge. The Philosophy reviewer specifically checks alignment with #1 (declarations over dashboards) and #8 (absorption over invention) since #20 builds on both.

**Done when:**
- [ ] VISION.md updated with proposed principle #20
- [ ] Principle wording reviewed via `dual-review` skill (T+P) — both reviewers approve
- [ ] LSP-Brains spec §9 + §F updated to reflect v5 SDK trait shapes
- [ ] If `culture.yaml` changes (probably not), all four copies updated byte-identically (ecosystem + NeuroGrim + LSP-Brains + python-starter)
- [ ] `culture-coherence` advisory domain still 100% (no drift introduced by v5)
- [ ] `terminology-coherence` advisory domain still passes (SDK / module / plugin / sensor / scoring source terms catalogued in `.claude/terminology-catalog.json`)
- [ ] `spec-impl-alignment` advisory domain confirms NeuroGrim conforms to LSP-Brains spec at v5 version

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

🟡 **Principle #20 inflation.** "Pluggability is justified by use" risks becoming a slogan that doesn't actually constrain decisions. Mitigation: dual-review skill (T+P) gates the wording; reviewers must cite a specific past decision the principle would have changed.

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
