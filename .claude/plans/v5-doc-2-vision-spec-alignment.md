# V5-DOC-2 — VISION + spec alignment

**Epic:** [`roadmap/epics/v5-coherence.md`](../../roadmap/epics/v5-coherence.md) § V5-DOC-2 · **Theme D** · Effort S (3–5d estimate; recent pacing — V5-MOD-1/2/3 + V5-SDK-1/2 + V5-FOUND-4 + V5-DOC-1 — has come in well under estimate, expect ~1 day actual; mostly mechanical edits + one dual-review round) · Depends on V5-DOC-1 ✅ (Theme D 1/2 complete)
**Successor:** Final v5 ship blocker. After V5-DOC-2, v5 release tag is gated only on workspace CI green. v5 is shippable.
**Status:** drafted 2026-05-04; **plan-critic v1 ABSORBED 2026-05-04** (technical + methodology lenses in parallel; technical = REVISE, methodology = PROCEED WITH CAUTION). Major absorptions: (a) **Phase 3 spec scope reduced** — `LSP-BRAINS-SPEC.md` § F.6 (lines 3935–3949) **already** documents the V5-MOD-2 trait pattern + conformance suite + example crate cross-refs (the spec is at v3.0; F.6 was added during V5-MOD-2 ship-out). The §F edit is largely no-op; §9 edit scoped narrowly to fractal-composition-relevant trait surfaces (TestRunner, Transport). Spec version bumps from v3.0 → v3.1 per LSP-Brains changelog discipline. (b) **Fork D default flipped from D1 → D2** — `terminology-coherence` is registered with `scoring_source.path = ".claude/terminology-coherence-cmdb.json"` (a **CMDB**, not a catalog), and no sensory tool currently emits that CMDB. Creating an aspirational catalog at the wrong filename doesn't unblock the domain; defer to v5.5 BACKLOG B-53. (c) **Phase 6 verification profile** switched from `--profile ci` (fail-fast = true; pre-existing init_scaffold failure causes abort before reaching new tests) to `--profile default` (fail-fast = false; full test count visible). (d) Phase 5 cross-repo commit ordering made explicit. (e) VISION.md header bug honesty — line 3 has been stale since #19 landed; Phase 2 acknowledges the incidental fix. (f) Cross-ref sweep is paraphrastic in 3 of 5 sites — wording-frozen gate added between Phase 1 + Phase 2. (g) Dual-review brief P-probe extended with binary-contrast template question. (h) Phase 6 adds CHANGELOG.md check + V5-SDK-1 `sdk_surface_assertion.rs` baseline re-pin verification post V5-FOUND-4. Fork decisions pending operator pin.

## Context

Theme D's second epic and the **last v5 ship blocker**. V5-DOC-1 closed today (`465b472`); composition guide is shipped; Theme D is 1/2 complete. V5-DOC-2 closes the methodology + spec coherence gap that v5 work opened — most concretely, the proposed VISION principle #20 ("Pluggability is justified by use, not aspiration") that has been informally ground-truthed across V5-FOUND-4 + V5-SDK-2 + V5-DOC-1 plan-critic rounds and now needs formal dual-review T+P validation.

**Posture:** mostly mechanical doc edits. One genuine deliberation point — the dual-review on principle #20 wording. Everything else is "ship the obvious thing": add a principle to a list, append two paragraphs to two spec sections, verify culture-coherence is unchanged, add a small terminology-catalog file.

## Recon-confirmed state

- **VISION.md** lives at `roadmap/VISION.md` (381 lines). Currently has **19 principles** (#1 declarations-over-dashboards through #19 agents-are-sensed). The header line at line 3 says "principle #18: sensors need sensors" — outdated; #19 already exists. V5-DOC-2 adds #20 after #19 + updates the header line.
- **LSP-Brains spec** lives at `D:\Brains\LSP-Brains\spec\LSP-BRAINS-SPEC.md` (a separate git submodule, ~4000 lines). §9 "Fractal Composition Protocol" at line 1585; Appendix F "MCP Integration" at line 3811. Both are language-agnostic spec sections — v5 SDK is implementation work and the spec doesn't structurally need to change. Edits are minimal additive notes acknowledging the v5 SDK trait surfaces as a conformance path.
- **No `.claude/terminology-catalog.json` exists today.** Epic Done-When references it for the `terminology-coherence` advisory domain. The domain currently runs at weight 0.0 (advisory only); without a catalog, it falls back to a default that doesn't gate anything. Fork D decides whether to create a minimal catalog or defer to v5.5 BACKLOG.
- **`.claude/culture.yaml`** lives at 4 places per CLAUDE.md culture-changes-propagation rule (NeuroGrim, LSP-Brains, NeuroGrim/python-starter, ecosystem `D:\Brains\.claude\`). V5-DOC-2 currently proposes no culture update; verification step confirms byte-identity.
- **Cross-references to update if dual-review changes wording:** wording "Pluggability is justified by use, not aspiration" appears in V5-FOUND-4 retrospective (`epics/v5-foundation.md`), V5-DOC-1 horizon section + recipe 4 (`docs/v5-composition-guide.md`), v5-roadmap.md "Proposed #20" line, V5-SDK-2 closure references. ~5–7 cross-refs total. If wording stays, sweep is no-op.
- **Ecosystem submodule pointer:** `D:\Brains\` parent repo references `D:\Brains\NeuroGrim\` (one submodule) and `D:\Brains\LSP-Brains\` (another submodule) by commit pointer. After V5-DOC-2's spec edits land in the LSP-Brains submodule, the ecosystem repo's pointer needs bumping. NeuroGrim's pointer also needs bumping after V5-DOC-2's NeuroGrim-side commits. Phase 5 covers both bumps.

## Architectural anchors (extending, not inventing)

- **VISION.md principle list.** Sequential numbering at H3-style "N. **Title.** Body" indentation. New principle joins at the end of `## Design Principles` after #19; matches existing style verbatim.
- **LSP-Brains spec section conventions.** RFC2119 conformance language ("MUST", "SHOULD", "MAY"); H2 section headers with leading number; H3 subsections with leading "N.M". Spec edits respect this style.
- **`dual-review` skill.** Defined at `.claude/skills/dual-review/`. Spawns Technical Reviewer + Philosophy Reviewer in parallel, synthesizes, requires both to approve before merge. Per epic Done-When ("Principle wording reviewed via `dual-review` skill (T+P) — both reviewers approve").
- **Submodule pointer-bump pattern.** Established by ecosystem session's recent pointer bumps (commits at `D:\Brains\` like `eeead65 ecosystem: bump NeuroGrim submodule to v5...`). Single-line gitlink change in the parent repo + commit message describing the child commit being pinned.

## Phases

### Phase 0 — plan + plan-critic + fork pins (this commit)

Plan written. Plan-critic spawn pending (technical + methodology lenses in parallel per cadence). Operator pinning pending.

### Phase 1 — dual-review on principle #20 wording

**Skill protocol clarification (plan-critic technical agent S1):** the `dual-review` skill at `.claude/skills/dual-review/SKILL.md` is a **protocol document**, not an `Agent`-tool wrapper. The pilot agent reads dual-review's T1–T5 / P1–P4 questions and uses the `Agent` tool (or `subagent-patterns` skill) to dispatch them. No recursion risk; the skill is data, not a wrapper.

1. Spawn Technical Reviewer (T) and Philosophy Reviewer (P) in parallel via two `Agent` tool tasks following the `dual-review` skill protocol. Each reviewer evaluates the proposed wording independently against:
   - **T (Technical):** is the wording precise? Does it carry an actionable rule for adopters? Does it cover the cases v5 actually trimmed (V5-FOUND-4 AgentDrivenRunner deferral, V5-MOD-* god-object resistance, V5-SDK-2 conformance-as-contract)? Does it match how it's been used informally in 5+ existing cross-references?
   - **P (Philosophy):** does the wording align with VISION's existing 19 principles, especially #1 (declarations over dashboards), #6 (fractal by design), #8 (absorption over invention)? Does it have load-bearing weight in the design, or is it slogan-shaped? Does it constrain decisions, or is it descriptive-only? **Binary-contrast template probe (plan-critic methodology agent finding):** does the principle fit the V1/V8 binary-contrast pattern ("X over Y") better as "Pluggability by use, not aspiration." or "Use-justified pluggability over aspirational pluggability."? Or is the longer Fork A2 form ("Pluggability earns its place when ≥2 plausible alternate impls exist...") more load-bearing?
2. Synthesis: if both approve, the wording lands as Phase 2's VISION update. If either rejects, the rejection is the input to a single revision pass; re-spawn the rejecting reviewer (or both, if revisions touch both lenses); land at most 2 revision rounds total.
3. **Wording-frozen gate (plan-critic methodology agent finding 7):** at the end of Phase 1, the wording is FROZEN. Any further wording revisions trigger a Phase 1 re-entry, NOT an in-Phase-2 sweep amendment. The cross-ref sweep (Phase 2) is paraphrastic in 3 of 5 sites (`v5-roadmap.md:160` paraphrase, `v5-roadmap.md:176` paraphrase, `epics/v5-foundation.md:312` paraphrase; `v5-doc-1-composition-guide.md:34` quote, `v5-sdk-2-partial.md:24` quote with attribution) — once started, the sweep can't be paused for wording revisions without manual cleanup.
4. **Honesty floor:** capture the dual-review verdict in a footnote in the V5-DOC-2 plan record (this file). Even if both approve verbatim, the formal review verdict is the methodology gate per the epic Done-When.

### Phase 1 dual-review verdict (captured 2026-05-04 — Phase 1 close-out)

Both T (Technical Reviewer) and P (Philosophy Reviewer) returned `passed: true` with **convergent recommendation: revise A1 → A3a + body paragraph**. Operator authorized the revision (option 1 — accept synthesis).

**Frozen wording (final):**

> **20. Pluggability by use, not aspiration.**

Body paragraph (drafted; lands at Phase 2 in VISION.md style — 4–15 lines, comparable to #16 / #17 / #18 / #19 depth):

> Pluggability earns its place when actual use exists for it: ≥2 plausible alternate
> implementations already in scope, an external adopter has asked for it, or leaving
> the seam concrete is provably blocking adoption. Aspirational pluggability — adding
> a trait or factory because it *might* be useful, or extracting a seam to ship a
> stub-as-second-impl — manufactures a maintenance burden against a hypothetical
> future. v5's reshape rule operationalizes this: each Theme B trait extraction
> (V5-MOD-1/2/3) cleared the bar via real built-in impls; V5-FOUND-4 deliberately
> deferred AgentDrivenRunner to v5.5 (BACKLOG B-51) because the second impl was
> aspirational at v5.0. Items that fail the rule today but might pass it later live
> in the v5.5 / v6 successor pipeline (BACKLOG B-37..B-45 + B-51..B-53), not in the
> current trait surface. See `roadmap/v5-roadmap.md` § Adversary findings A.

**T verdict summary (`passed: true`):** T1/T2/T4/T5 warn; T3 pass. "Use" semantically ambiguous in isolation; headline alone under-actionable without body; 2 of 5 cited cross-ref line numbers in plan v2 were stale (corrected list below). Recommendation: approve A1 *contingent on body*; otherwise revise to A3.

**P verdict summary (`passed: true`):** P1/P2/P3/P4 pass; P5 warn (binary-contrast template probe). A1's passive "is justified by" is softer than peer principles' #1 ("Declarations over dashboards.") and #8 ("Absorption over invention.") imperative templates. Recommendation: revise to **"Pluggability by use, not aspiration."** as headline + body paragraph operationalizing the reshape rule.

**Synthesis (per dual-review skill — philosophy takes precedence):** P's recommendation lands; T's recommendation aligns. No genuine conflict; named tension absorbed via P's split between headline (binary-contrast template) and body (reshape rule operationalization). Wording is FROZEN at Phase 1 close per Fork F1 wording-frozen gate.

**Cross-reference sweep site list (CORRECTED post T4 — supersedes plan v1 / v2 list):**

| Site | Type | Notes |
|---|---|---|
| `roadmap/v5-roadmap.md:154` | verbatim | (corrected — v1 plan said `:160` paraphrase; T verified line 154 carries verbatim) |
| `roadmap/v5-roadmap.md:176` | verbatim | exact wording |
| `roadmap/epics/v5-coherence.md:7` | verbatim | (added — v1 plan missed this site) |
| `roadmap/epics/v5-coherence.md:102` | verbatim | (added — v1 plan missed this site) |
| `.claude/plans/v5-sdk-2-partial.md:24` | verbatim with attribution | unchanged from v1 |
| `roadmap/epics/v5-foundation.md:161, 175` | paraphrase ("aspirational stub-as-second-impl") | (corrected — v1 plan said `:312`; that line is in `.claude/plans/v5-found-4-test-runner-trait.md`, not v5-foundation.md) |
| `.claude/plans/v5-found-4-test-runner-trait.md:312` | paraphrase ("aspirational-pluggability hazard") | (added — v1 plan mis-attributed to v5-foundation.md) |
| `docs/v5-composition-guide.md:4` | paraphrase ("reality, not aspiration") | (corrected — v1 plan said `:34` "verbatim"; T verified line 34 is a section header and the only relevant occurrence at line 4 is paraphrase) |

**Sweep behavior (Phase 2):**

- **Verbatim sites** (5): replace "Pluggability is justified by use, not aspiration" → "Pluggability by use, not aspiration".
- **Paraphrase sites** (3): leave as-is. The paraphrases ("aspirational stub-as-second-impl", "aspirational-pluggability hazard", "reality, not aspiration") don't quote the principle directly; they survive both A1 and A3a wording without revision.

### Phase 2 — VISION.md update

1. Add principle #20 to `roadmap/VISION.md` at the end of `## Design Principles` (after #19, before `---`). Format mirrors existing principles:
   ```
   20. **Pluggability is justified by use, not aspiration.** Body text — 4–8 lines
       explaining the reshape rule, citing v5-roadmap §A, citing the V5-FOUND-4
       AgentDrivenRunner deferral as the canonical example. Cross-refs to spec
       (TBD per Phase 3).
   ```
   The exact body text lands per Fork A (default = wording from cross-refs; dual-review may revise).
2. Update VISION.md line 3 header. **Plan-critic v1 finding C2 (technical) + methodology lens C2 absorbed:** the header has been stale since #19 landed (says `#16 ... #17 ... #18` and stops; #19 "Agents are sensed" exists in body at lines 320-335 but never made it to the header). V5-DOC-2 incidentally corrects this drift while adding #20. Final line 3: `**Last updated:** 2026-05-04 (principle #16: right protocol for the role; #17: culture as substrate; #18: sensors need sensors; #19: agents are sensed [HEADER CORRECTED — was missing since 2026-04 ship]; #20: pluggability is justified by use, not aspiration)`. Honest framing: don't paper over the missing-#19 fix.
3. Verify: render markdown locally; confirm new principle nests with existing list.

### Phase 3 — LSP-Brains spec §9 update + §F.6 verification + spec changelog (cross-repo)

**Plan-critic v1 reduction:** §F.6 (lines 3935–3949) already documents the V5-MOD-2 trait pattern: "trait + factory + registry pattern within its own implementation language ... NeuroGrim ships this pattern as of v5.0.0 (V5-MOD-2, 2026-05-02): the `Sensor` trait + `SensorFactory` + `SensorRegistry`... A conformance suite published alongside the trait gives third-party authors a verifiable target. See ... `crates/neurogrim-core/src/sensor.rs`". §F is essentially done for sensors. §9 doesn't carry analogous language for fractal-composition-relevant trait surfaces (TestRunner, Transport) — that's where the narrow new edit goes.

Edits go in the **LSP-Brains submodule** (`D:\Brains\LSP-Brains\spec\LSP-BRAINS-SPEC.md`), then a parent submodule pointer bump in Phase 5.

1. **§9 "Fractal Composition Protocol"** — add a paragraph under the most appropriate subsection (likely §9.1 "Child Brain Contract" or §9.7 "A2A Transport Mapping" — verify during Phase 3 recon) noting that v5+ implementations MAY expose **fractal-composition-relevant trait surfaces** (`TestRunner` for child-Brain test orchestration; `Transport` for child-Brain coordination) as a contract crate. Cite NeuroGrim's `neurogrim-sdk` as the reference impl. Keep language-agnostic — refer to "the canonical Rust implementation's contract crate" without Rust-specific syntax in the spec body. **Explicitly NOT duplicating §F.6's sensor language** (which already covers V5-MOD-2).

2. **Appendix F.6 verification (no edit expected, NOT a new paragraph).** Confirm §F.6's existing text covers the V5-DOC-2 deliverable for sensors. If gaps surface (e.g., §F.6 doesn't cite the V5-DOC-1 composition guide that landed today), add a one-line cross-reference. If §F.6 is sufficient as-is, Phase 3 step 2 is a no-op — record as "verified; §F.6 already covers the V5-DOC-2 §F deliverable per V5-MOD-2 ship-out".

3. **Spec version bump v3.0 → v3.1** (per LSP-Brains changelog discipline). Add a changelog entry naming the §9 trait-surfaces paragraph + the V5-DOC-2 cross-references; cite this commit. Spec header line should reflect v3.1.

4. Verify: render markdown locally; confirm §9 paragraph respects RFC2119 / spec conventions; spec version bump lands cleanly.

### Phase 4 — terminology-catalog (Fork D revised — D2 default; defer to v5.5 BACKLOG B-53)

**Plan-critic v1 finding C5 (technical) absorbed:** `terminology-coherence` is registered at `D:\Brains\.claude\brain-registry.json:268-272` with `scoring_source.path = ".claude/terminology-coherence-cmdb.json"` — a **CMDB** file (sensory-tool output), not a free-form catalog. No sensory tool currently emits that CMDB. Creating a `.claude/terminology-catalog.json` (the v1 plan's proposal) doesn't unblock the registered domain (filename mismatch + shape mismatch — CMDB requires `score`, `findings`, RFC3339 timestamps, etc.). The advisory domain runs at weight 0.0 today regardless of file presence.

**Fork D revised default = D2 (defer entirely to v5.5 BACKLOG B-53).** Phase 4 is now a no-op + a BACKLOG entry. The plan still covers the original D1 path as an alternative if the operator wants to ship a minimal CMDB stub (with the caveat that a sensory tool to keep it fresh is the actual missing piece — which is the v5.5 epic).

**Fork D1' (revised; alternative)** — minimal CMDB stub at the registered filename + "What this CMDB is NOT" framing per V5-DOC-1 discipline. Only choose if the operator wants any v5-vocabulary index file at all; not strictly needed.

(If D1 → D1' or D1 → D2, the original v1 plan's terminology-catalog.json content below is retained as reference for the v5.5 epic.)

Original v1 plan (Fork D1 — create minimal `.claude/terminology-catalog.json` with the v5 vocabulary):
```json
{
  "schema_version": "1",
  "terms": {
    "SDK":               { "definition": "...", "see_also": [...] },
    "module":            { "definition": "...", "see_also": [...] },
    "plugin":            { "definition": "...", "see_also": [...] },
    "sensor":            { "definition": "...", "see_also": [...] },
    "scoring source":    { "definition": "...", "see_also": [...] },
    "TestRunner":        { "definition": "...", "see_also": [...] },
    "queue backend":     { "definition": "...", "see_also": [...] },
    "Transport":         { "definition": "...", "see_also": [...] },
    "SecretBackend":     { "definition": "...", "see_also": [...] },
    "factory":           { "definition": "...", "see_also": [...] },
    "registry":          { "definition": "...", "see_also": [...] },
    "conformance":       { "definition": "...", "see_also": [...] }
  }
}
```
~12-15 terms; one-line definitions; cross-refs to the spec / VISION / composition guide. Schema is informal (no JSON-schema for this file at v5; v5.5 polish if the catalog grows).

Fork D2 (REVISED DEFAULT post plan-critic) — defer to v5.5 BACKLOG (add B-53 entry: `V5.5-DOC-TERMINOLOGY-CMDB` — sensory tool emits `.claude/terminology-coherence-cmdb.json`; tracks the v5 vocabulary index AND its lifecycle); skip Phase 4 file-creation entirely; the `terminology-coherence` advisory domain continues at weight 0.0 unchanged. **Rationale:** the registered domain expects a CMDB, not a catalog; creating any file at v5 without a sensory tool to maintain it is drift bait. v5.5 epic ships the sensory tool + the CMDB shape together.

### Phase 5 — submodule pointer bumps + culture-coherence verification (cross-repo orchestration)

**Cross-repo commit sequence — explicit ordering (plan-critic v1 finding C1 absorbed):**

| Order | Repo | Phase | Subject |
|---|---|---|---|
| 1 | `D:\Brains\NeuroGrim\` | 2 | VISION.md update with principle #20 + cross-ref sweep |
| 2 | `D:\Brains\LSP-Brains\` | 3 | spec §9 trait-surfaces paragraph + version bump v3.0 → v3.1 |
| 3 | `D:\Brains\NeuroGrim\` | 4 | (D2 default = no commit; D1'  = CMDB stub commit) |
| 4 | `D:\Brains\` (ecosystem) | 5.1 | bump NeuroGrim submodule pointer |
| 5 | `D:\Brains\` (ecosystem) | 5.2 | bump LSP-Brains submodule pointer |
| 6 | `D:\Brains\NeuroGrim\` | 6 | V5-DOC-2 + Theme D close-out + v5 SHIPPABLE callout |
| 7 | `D:\Brains\` (ecosystem) | 6 | bump NeuroGrim submodule pointer (final close-out) |

Each child-repo commit is its own commit *inside the submodule*; the ecosystem repo never atomic-commits across both children. Order matters: child commits land first, then ecosystem-side bumps reference the new child SHAs.

1. **NeuroGrim submodule pointer bump in ecosystem `D:\Brains\`** (commit 4 in the sequence above): after Phase 2 lands the NeuroGrim-side VISION + cross-ref-sweep commit (and Phase 4 if Fork D1' fires), the ecosystem repo's pointer to NeuroGrim needs bumping. `git -C D:\Brains add NeuroGrim && git -C D:\Brains commit -m "ecosystem: bump NeuroGrim submodule for V5-DOC-2 (VISION #20 + cross-ref sweep)"`.
2. **LSP-Brains submodule pointer bump in ecosystem** (commit 5 in the sequence): after Phase 3 lands the spec edits in LSP-Brains, the ecosystem pointer to LSP-Brains needs bumping. Mirror commit: `ecosystem: bump LSP-Brains submodule for V5-DOC-2 (spec §9 trait-surfaces + v3.1)`.
3. **Culture-coherence verification:** confirm `.claude/culture.yaml` is byte-identical across all 4 locations:
   - `D:\Brains\.claude\culture.yaml`
   - `D:\Brains\NeuroGrim\.claude\culture.yaml`
   - `D:\Brains\LSP-Brains\.claude\culture.yaml`
   - `D:\Brains\NeuroGrim\NeuroGrim-python-starter\.claude\culture.yaml`
   
   Use `Compare-Object` or simple file-hash diff. Expected: all 4 identical (V5-DOC-2 doesn't propose culture changes per Fork E1).
4. **Terminology-coherence + spec-impl-alignment domains:** per epic Done-When, both should still pass after V5-DOC-2 lands. `terminology-coherence` checks the catalog file's freshness + drift; `spec-impl-alignment` checks NeuroGrim conforms to LSP-Brains spec at v5 version. Both run via `neurogrim health` or similar advisory-mode dispatch.

### Phase 6 — V5-DOC-2 + Theme D close-out + v5 ship readiness

1. `roadmap/epics/v5-coherence.md` § V5-DOC-2 — flip status `Planned` → `✅ COMPLETE 2026-05-04`. Flip all 7 Done-When checkboxes. Add retrospective bullet citing the dual-review verdict + cross-ref sweep outcome.
2. `roadmap/v5-roadmap.md` Theme D row — V5-DOC-2 ✅; Theme D 2/2 ✅ COMPLETE.
3. `roadmap/v5-roadmap.md` master status — flip the v5 status line if it exists; otherwise add a "v5 SHIPPABLE" callout. **Per Fork G1**, do NOT actually create the v5 release tag — that's an operator-controlled action. The plan + roadmap update marks v5 as ship-ready; operator runs `git tag v5.0.0 -m "..."` separately.
4. Verify (plan-critic v1 finding B1 absorbed — `--profile ci` has `fail-fast = true` and pre-existing `commands::init_scaffold::tests::scaffold_full_writes_expected_files` failure aborts the run before "no NEW regressions" can be checked):
   - `cargo nextest run --workspace --profile default --color never` ✓ (`fail-fast = false`; full test count visible; pre-existing failures captured but not aborting)
   - `cargo doc --workspace --features conformance --no-deps` ✓ (regression baseline; no new warnings introduced by V5-DOC-2)
   - All 4 culture.yaml copies byte-identical (`Get-FileHash` confirms — verified pre-plan at SHA256 `94201AA4...A787FE0E`; should match post-V5-DOC-2 since Fork E1 = no culture changes)
   - **CHANGELOG.md presence check** (plan-critic methodology agent finding 8): `Test-Path D:\Brains\NeuroGrim\CHANGELOG.md`. If yes, add a v5.0.0 entry referencing the V5-DOC-2 commits. If no, the v5.0.0 git-tag annotation absorbs the changelog content (Fork G1 deferred to operator).
   - **V5-SDK-1 surface-assertion baseline** (plan-critic methodology agent finding 8): `cargo nextest run -p neurogrim-sdk -E 'test(sdk_surface_signatures_unchanged)'` ✓ — confirms the compile-test gate at `crates/neurogrim-sdk/tests/sdk_surface_assertion.rs` still passes post V5-FOUND-4 (TestRunner re-export added in V5-FOUND-4 Phase 4 — already pinned per `8b98599`; this verifies the pin held under V5-DOC-2 changes).

## Forks (operator-pinnable)

- **Fork A — Principle #20 wording**:
  - **A1** = "Pluggability is justified by use, not aspiration." *(default — the wording referenced in 5+ existing cross-refs across V5-FOUND-4, V5-DOC-1, v5-roadmap.md adversary findings; if dual-review approves, no sweep needed; if dual-review revises, the revision lands here)*
  - A2 = longer-form variant with explicit reshape-rule body (e.g., "Pluggability earns its place when ≥2 plausible alternate impls exist, OR an external user has asked for it, OR concrete code is provably blocking adoption — never on aspiration alone."). More explicit; less slogan-shaped.
  - A3 = revised wording surfaced by dual-review (open). The plan-critic + dual-review may produce something neither A1 nor A2 cleanly captures.

- **Fork B — Principle #20 placement**:
  - **B1** = numerical position 20, end of `## Design Principles` *(default — sequential; matches existing convention; all current cross-refs say "#20")*.
  - B2 = themed grouping (e.g., near #6 "Fractal by design" + #8 "Absorption over invention" since #20 is conceptually adjacent). Renumbers nothing but disrupts the sequential flow.

- **Fork C — Spec §9 + §F update scope** (REVISED post plan-critic v1):
  - **C1' (revised default)** = §9 narrow paragraph (TestRunner + Transport — fractal-composition-relevant trait surfaces); §F.6 verified-no-edit (already done at V5-MOD-2 ship-out); spec version v3.0 → v3.1 with changelog entry. The original v1 C1 ("one paragraph in §9 + one paragraph in §F") double-counted §F since F.6 already lands the V5-MOD-2 trait pattern.
  - C2 = larger structural updates that promote SDK to first-class spec concern. Heavier; binds the spec to a Rust-implementation choice.
  - C3 = no spec update. v5 SDK is implementation-specific; spec stays unchanged. Defensible (especially since §F.6 already covers the methodology-load-bearing piece) but loses the V5-DOC-2 epic's stated §9 deliverable.

- **Fork D — Terminology-catalog scope** (REVISED post plan-critic v1):
  - **D2 (revised default)** = defer entirely to v5.5 BACKLOG B-53 (`V5.5-DOC-TERMINOLOGY-CMDB`). Skip Phase 4 file-creation. The registered `terminology-coherence` advisory domain expects a **CMDB at `.claude/terminology-coherence-cmdb.json`** (NOT a catalog); creating any file at v5 without a sensory tool to maintain it is drift bait. v5.5 epic ships the sensory tool + the CMDB shape together.
  - D1' (revised alternative — minimal CMDB stub) = create `.claude/terminology-coherence-cmdb.json` (CMDB shape — score=0, findings=[], schema_version="1") with v5 vocabulary in `extras` field + "What this CMDB is NOT" framing per V5-DOC-1 discipline (`docs/v5-composition-guide.md:117–140` precedent). Only choose if the operator wants any v5-vocabulary index file at all; not strictly needed for v5 ship.
  - D1 (ORIGINAL — REJECTED) = `.claude/terminology-catalog.json` with v5 vocabulary. Filename + shape mismatch with the registered domain's expectations. v1 plan's Fork D1 was based on outdated recon.
  - D3 = create catalog AND document the schema. Heavier; v5 release blocker if dual-review decides to formalize.

- **Fork E — Culture update scope**:
  - **E1** = no culture update; verify byte-identity across all 4 copies *(default — V5-DOC-2 doesn't currently propose any culture change; principle #20 is a VISION addition, not a culture invariant)*.
  - E2 = preview a culture v6 candidate (e.g., "the dual-review T+P pattern is itself a culture invariant — agents must subject architectural decisions to T+P review"). Out of v5 scope; v5.5+ work.

- **Fork F — Dual-review path**:
  - **F1** = single round of T+P parallel reviewers; if either rejects, single revision round + re-review. Cap at 2 rounds *(default — bounded effort; matches recent plan-critic absorption pattern)*.
  - F2 = use the `review-loop` skill (T → P → Code Reviewer synthesis → revision → repeat until approved). Heavier; closer match to the epic's "T+P review" wording but adds more rounds.

- **Fork G — v5 release tag creation**:
  - **G1** = Phase 6 marks v5 SHIPPABLE; operator runs `git tag v5.0.0 -m "..."` separately *(default — release tag is operator-controlled, not auto-created by plan execution)*.
  - G2 = Phase 6 creates the v5 release tag annotated commit as part of the close-out. Stronger automation but commits to a specific tag-creation moment that operators may want to defer for soak time.

Defaults pinned: **A1 / B1 / C1 / D1 / E1 / F1 / G1**. Seven forks; user-pinnable.

## Mutual-exclusion + conflict checks

| Combination | Behavior |
|---|---|
| Dual-review revises wording (Fork A2 or A3 fires) | Cross-reference sweep: ~5-7 sites use "Pluggability is justified by use, not aspiration." (V5-FOUND-4 retro, V5-DOC-1 horizon + recipe 4, v5-roadmap §A, V5-SDK-2 closures, V5-FOUND-4 plan-critic ref). Phase 2 step 1 includes the sweep. |
| Spec edit lands but ecosystem submodule pointer not bumped (Phase 3 commits without Phase 5) | Ecosystem repo's NeuroGrim/LSP-Brains pointers stale; advisory `spec-impl-alignment` domain detects drift. Phase 5 is mandatory; Phase 6 close-out depends on Phase 5. |
| Culture.yaml byte-identity check fails (Phase 5 step 3) | V5-DOC-2 BLOCKED. Mirror byte-identically across all 4 copies before proceeding. Most likely cause: someone edited culture out-of-band during the V5 work. Investigate via `git log` on the 4 culture.yaml files. |
| `cargo nextest run --workspace --profile ci` fails at Phase 6 | V5 ship gate not met. Investigate before flipping Theme D 2/2 status. Possible causes: pre-existing flaky `commands::init_scaffold::tests::scaffold_full_writes_expected_files` (out-of-scope per V5-FOUND-2 retrospective), or a regression introduced during Phase 1-5. |

## Verification (consolidated)

- **Phase 1:** dual-review verdict captured (both T + P approve, or revision rounds documented).
- **Phase 2:** VISION.md renders correctly; principle #20 sits at numerical position 20; header line updated.
- **Phase 3:** spec markdown renders correctly; §9 and §F additive paragraphs respect RFC2119 / spec conventions; new content stays language-agnostic.
- **Phase 4 (Fork D1):** `.claude/terminology-catalog.json` parses as valid JSON; ≥12 terms with definitions + see-also.
- **Phase 5:** all 4 culture.yaml copies byte-identical (`Get-FileHash` confirms); `terminology-coherence` advisory domain runs without error; `spec-impl-alignment` advisory domain confirms NeuroGrim at v5 conforms to spec; submodule pointer bumps land without conflict.
- **Phase 6:** epic + roadmap status flips clean; `cargo nextest run --workspace --profile ci` passes (v5 ship gate); `cargo doc --workspace --features conformance` clean.

## Deliverable shape

7 phase commits per established cadence (the highest count of any v5 epic — but each phase is small):

1. Phase 0 — plan v1 + fork pins (this commit).
2. Phase 1 — dual-review verdict + (if needed) revision capture in plan record.
3. Phase 2 — VISION.md updates (NeuroGrim repo).
4. Phase 3 — LSP-Brains spec §9 + §F updates (LSP-Brains submodule).
5. Phase 4 — terminology-catalog.json (NeuroGrim repo, Fork D1) OR skipped (Fork D2).
6. Phase 5 — ecosystem submodule pointer bumps + culture-coherence verification (D:\Brains\ ecosystem repo).
7. Phase 6 — V5-DOC-2 + Theme D close-out + v5 SHIPPABLE callout.

(Phase 5 may bundle into Phase 6 if the bumps + verification are short.)

## Risks / adversary concerns brought forward

🟡 **Dual-review may surface wording the cross-refs don't cleanly carry.** If A2 or A3 fires, ~5-7 cross-refs need updating (V5-FOUND-4 retro, V5-DOC-1 horizon + recipe 4, v5-roadmap §A, V5-SDK-2 closures, V5-FOUND-4 plan-critic ref). Mitigation: Phase 2 includes the sweep; Phase 1's revision-round cap (Fork F1 = 2 rounds) bounds effort.

🟡 **Cross-repo coordination — the LSP-Brains submodule edit + ecosystem pointer bump dance.** Established pattern in recent ecosystem commits, but easy to forget the ecosystem-side bump and ship a stale pointer. Mitigation: Phase 5 is explicit; Phase 6 verification confirms the bumps landed before flipping epic status.

🟡 **Terminology-catalog Fork D may inflate.** D1 minimal catalog is ~12-15 terms. If dual-review or plan-critic surfaces additional v5 vocabulary that should be catalogued, scope creeps. Mitigation: timebox the catalog creation to "what V5-FOUND-4 + V5-SDK-2 + V5-DOC-1 already named"; defer net-new vocabulary to v5.5 polish.

🟡 **`cargo nextest run --workspace --profile ci` may surface the pre-existing init_scaffold failure** at Phase 6. V5-FOUND-2 retrospective documented this as out-of-scope; V5-DOC-2 should not be the epic that surprise-fixes it. Mitigation: Phase 6 verification accepts the known pre-existing failure; v5 ship gate is "no NEW regressions," not "all tests green."

🔵 **Suggestion — V5-DOC-2 retrospective should capture the dual-review verdict explicitly.** Even if both T + P approve verbatim, the retrospective records WHY the wording landed as it did. Future contributors evaluating the principle's load-bearing weight benefit from the dual-review citation.

🔵 **Suggestion — v5 release tag content.** The actual `v5.0.0` annotated tag (operator-controlled per Fork G1) should reference the V5-DOC-2 close-out commit + summarize Theme A/B/C/D status. Suggested annotation body:
```
v5.0.0 — "Everything is Lego" release

Theme A: 3/4 (V5-FOUND-3 deferred to v5.1/v6)
Theme B: ✅ COMPLETE
Theme C: ✅ COMPLETE
Theme D: ✅ COMPLETE

VISION principle #20 added: "Pluggability is justified by use,
not aspiration."

See `docs/v5-composition-guide.md` for the modularity surface.
```
Operator runs `git tag -a v5.0.0 -F <body-file>` after Phase 6 lands.

🔵 **Suggestion — schedule a v5 retrospective doc post-ship.** Composition guide names what shipped; a retrospective names what got trimmed, what surprised the implementation, which v5.5/v6 candidates moved up. v5.5 polish per the V5-DOC-1 V5-DOC-2 epic suggestion 1.
