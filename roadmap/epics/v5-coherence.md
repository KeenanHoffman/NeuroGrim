# Epic: v5 Coherence + Docs (Theme D)

**Theme:** D
**Release:** v5 (entry decide-later; sequenced after Theme C)
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

**Status:** Planned
**Effort:** M
**Depends on:** V5-SDK-2

**What:** Author `docs/v5-composition-guide.md`. Concrete recipes: "swap the queue backend," "add a custom scoring source," "ship a sensor as a crate," "drive tests with your own runner." Architecture diagram of pluggable seams. Adversary check: written from shipped reality, not aspiration.

**Why:** "Everything is Lego" only matters if users can actually combine pieces. Composition guide is the proof — if a recipe doesn't work as written, the guide is wrong (and the pluggable seam is broken). CI builds the recipe code samples to prevent doc rot.

**Done when:**
- [ ] Guide ships with ≥4 working recipes (queue backend swap, scoring source addition, sensor crate, custom test runner)
- [ ] All recipes cross-link to `neurogrim-sdk` API docs
- [ ] Diagram shows actual cargo workspace boundaries (not idealized ones)
- [ ] CI builds all code samples in the guide; broken samples fail the build (prevents doc rot)
- [ ] Each recipe lists what's NOT yet possible — explicit links to v5.5/v6 successor pipeline (BACKLOG B-37..B-45) for the limits

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
