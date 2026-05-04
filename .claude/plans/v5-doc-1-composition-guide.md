# V5-DOC-1 — Modular composition guide

**Epic:** [`roadmap/epics/v5-coherence.md`](../../roadmap/epics/v5-coherence.md) § V5-DOC-1 · **Theme D** · Effort M (7–10d estimate; recent pacing — V5-MOD-1/2/3 + V5-SDK-1/2 partial + V5-FOUND-4 — has come in well under estimate, expect ~2–3 days actual; **may land in ~1 day** since this is prose-shaped lift-from-example-crates work, not new code — that's not under-delivery, it's the natural shape of the task) · Depends on V5-SDK-2 ✅ (Theme C ✅ COMPLETE 2026-05-04)
**Successor:** Closes the v5 ship-blocker pair. After V5-DOC-1 + V5-DOC-2, v5 release tag is gated only on the workspace CI green. v5 is shippable.
**Status:** drafted 2026-05-04; **plan-critic v1 ABSORBED 2026-05-04** (technical + methodology lenses in parallel; both PROCEED WITH CAUTION — no 🔴 blockers; 10 🟡 concerns absorbed in-place per the V5-FOUND-4 absorption pattern). Key absorptions: ASCII diagram corrected to 6 trait surfaces (was internally inconsistent 4-vs-5); `cargo doc` verification reframed as regression check; recipe Cargo.toml blocks explicitly include `[dev-dependencies] features = ["conformance"]`; recipe 4 names structural asymmetry (in-tree NextestRunner vs. out-of-tree examples 1–3); recipe 4 calls out `NextestRunnerFactory` defaults; v5.1 `ByCoverage` non-exhaustive note added to recipe 4; Phase 4 adds manual snippet-match check; effort estimate honesty noted; conformance-suite framing made load-bearing in Phase 1 intro; "v5.5/v6 horizon" section restructured as capability gaps with BACKLOG breadcrumbs (not BACKLOG-ID list); new "What this guide is NOT" section added. Fork decisions pending operator pin.

## Context

Theme D's first epic. Themes A, B, C are 3/4 + 4/4 + 2/2 complete (Theme A's V5-FOUND-3 deferred; everything else shipped). V5-DOC-1 documents v5's modularity surface for adopters — 4 recipes covering the trait surfaces shipped by V5-MOD-1/2/3 + V5-FOUND-4. The guide is a v5 release-tag blocker per [`v5-coherence.md:13`](../../roadmap/epics/v5-coherence.md): "Cannot ship v5 without docs that match shipped reality."

**Posture: written from shipped reality, not aspiration.** Every recipe lifts working code from an existing in-tree example crate (V5-MOD-1's `scoring-source-prom`, V5-MOD-2's `sensor-readme-quality` / `sensor-constant-score`, V5-MOD-3's `queue-backend-memory`, V5-FOUND-4's `NextestRunner`). The example crates ARE the CI-built source of truth — they live in the workspace and `cargo build --workspace` exercises them on every PR. The composition guide points adopters at them and inlines the load-bearing snippets. Doc rot is bounded by the existing CI gate; no new CI infrastructure needed.

**Why this matters for v5:** "Everything is Lego" is the v5 north star (per `v5-roadmap.md:12`). The composition guide is the proof — if a recipe doesn't work as written, either the guide is wrong (fix the guide) or the pluggable seam is broken (fix the seam). Recipes lifted verbatim from in-tree example crates make the proof load-bearing.

## V5-FOUND-4 → V5-DOC-1 framing handoff

V5-FOUND-4 closed today (2026-05-04) with one concrete `TestRunner` impl (`NextestRunner`) and AgentDrivenRunner deferred to v5.5 BACKLOG B-51. The V5-FOUND-4 plan-critic methodology lens explicitly flagged that V5-DOC-1's "drive tests with your own runner" recipe must be honest about the deferred second runner — V5-DOC-1 plan v1 absorbs this via Fork D (recipe 4 framing: documents the trait pattern using `NextestRunner` as the in-tree reference; explicitly notes v5.5 B-51/B-52 land the second runner + the `--runner=` CLI flag).

This is the same framing discipline V5-FOUND-4 used internally — show what's shipped, name what's deferred, point at the BACKLOG entries that track the gap. Adopters can write their own runner today by implementing the trait + factory; what they can't do is select between runners via `--runner=` (one runner exists; flag is moot until B-52 lands).

## Architectural anchors (extending, not inventing)

- **`docs/sdk.md`** (109 lines, v3.0-era; references `neurogrim-core` + `neurogrim-sensory` directly). Pre-dates the V5-SDK-1 `neurogrim-sdk` crate. The composition guide complements rather than replaces it — sdk.md covers "minimum viable custom sensor"; composition guide covers "swap backends, add scoring sources, ship sensors as crates, drive tests via the runner trait".
- **`crates/neurogrim-sdk/README.md`** (V5-SDK-2 partial Phase 4) — already inlines the writing-a-conformant-Sensor walkthrough. Cross-references between the two: composition guide cites the SDK README for trait-by-trait API depth; SDK README cites the composition guide for cross-cutting patterns.
- **4 example crates in `neurogrim/examples/`** — `queue-backend-memory`, `scoring-source-prom`, `sensor-constant-score`, `sensor-readme-quality`. Each has a README + working code + conformance integration test. The composition guide recipes lift their code patterns verbatim.
- **`neurogrim-cli/src/commands/test_runner_impls/nextest.rs`** (V5-FOUND-4 Phase 2) — concrete `NextestRunner` impl. Used as recipe 4's reference.
- **CI baseline** — `.github/workflows/ci.yml` runs `cargo nextest run --workspace --profile ci` on every PR (V5-FOUND-2 Phase 5 + V5-SDK-2 partial Phase 3 dev-dep posture). The 4 example crates' tests are part of this run; if any recipe's referenced code stops compiling or its conformance test fails, the workspace CI catches it. Doc rot mitigation = existing CI; no new CI gates needed.

## Recon-confirmed state

- `docs/` directory has 25 markdown files (cosmetic correction post-recon). The composition guide will be the 26th. Length range of existing docs: ~100–600 lines (median ~250). Composition guide should target ~300–400 lines: 4 recipes × ~50–80 lines each + skeleton/intro/diagram/closing ≈ 350 lines.
- Each example crate's README runs ~50–100 lines and includes a Cargo.toml block + minimum-viable impl + conformance test snippet. The composition guide can lift these directly with attribution.
- The SDK README (`crates/neurogrim-sdk/README.md`, ~285 lines post V5-SDK-2 partial Phase 4) already has the Sensor walkthrough inlined. The composition guide should NOT duplicate that — instead it cites the SDK README for the Sensor recipe's depth and focuses on the cross-cutting "wire factories into the registry, dispatch through the trait" story.
- VISION.md exists at `roadmap/VISION.md` and lists 19 principles. Principle #20 ("pluggability by use, not aspiration" — wording finalized via V5-DOC-2 dual-review T+P 2026-05-04; revised from initial draft "pluggability is justified by use, not aspiration") is V5-DOC-2's deliverable. V5-DOC-1 can reference proposed-#20 without modifying VISION.md.

## Phases

### Phase 0 — plan + plan-critic + fork pins (this commit)

Plan written. Plan-critic spawn pending (technical + methodology lenses in parallel per established cadence). Operator pinning pending.

### Phase 1 — skeleton + intro + architecture diagram

1. New file `docs/v5-composition-guide.md`. Header: title, status (1-line "shipped reality" framing), table of contents, audience.
2. Intro section: "Everything is Lego" framing from `v5-roadmap.md`. The 6 trait surfaces v5 ships. The reshape rule. **Conformance-suite stance is load-bearing** (plan-critic methodology agent C4) — the intro states explicitly: *"To ship a conformant `Sensor` / `ScoringSource` / `QueueBackend` / `TestRunner`, you MUST run the conformance suite — it is the contract, not a recommendation. Each recipe shows the wiring."* Mirrors the v5-roadmap §E adversary concern's framing ("alternate impls are 80% feature-complete… counter — each new trait ships with a shared conformance test suite that any impl must pass").
3. **New "What this guide is NOT" section** (plan-critic methodology agent suggestion 7) — sets adopter expectations before the recipe-by-recipe deep dive. Items:
   - NOT trait-shape rationale (e.g., why `TestSelection` is `non_exhaustive`) — that's V5-FOUND-4 retrospective material; lives in `roadmap/epics/v5-foundation.md`.
   - NOT a v4→v5 migration guide — v4 didn't ship the SDK crate; there's no migration path to document.
   - NOT performance characteristics — V5-MOD-1's perf-gate data has its own home (`roadmap/data/v5-scoring-baseline-2026-05-02.json`).
   - NOT a tour of every built-in impl — the SDK README + rustdoc on docs.rs cover that depth.
3. Architecture diagram: cargo workspace dep graph showing the trait surfaces' actual home crates + the SDK re-export. ASCII art (matches the codebase's existing markdown-table-and-ascii style; Fork B default — no new mermaid dep). Diagram shape (corrected to 6 trait surfaces post plan-critic technical agent C1 — `Transport` lives in `neurogrim-a2a`, `SecretBackend` in `neurogrim-secrets`):
   ```
   ┌──────────────────┐    ┌──────────────────┐
   │  neurogrim-sdk   │───▶│  neurogrim-core  │
   │ (contract crate) │    │ (4 traits below) │
   │  re-export only  │    └────────┬─────────┘
   └──────────────────┘             │
            │                       ├── ScoringSource → cmdb / a2a / function
            │                       ├── Sensor        → 21 built-ins (neurogrim-sensory)
            │                       ├── QueueBackend  → JsonlBackend / SqliteBackend
            │                       └── TestRunner    → NextestRunner (neurogrim-cli)
            │
            ├──▶ neurogrim-a2a      (1 trait: Transport — A2A peer protocol)
            └──▶ neurogrim-secrets  (1 trait: SecretBackend — encrypted-secrets)
   ```
   The SDK is the single contract surface adopters import; arrows from SDK to the impl-home crates mean "re-exports". The 4 V5 trait pairs (factory + impl-base) live in `neurogrim-core`; `Transport` and `SecretBackend` live in their own crates and the SDK re-exports them transitively.
4. Verify: `cargo doc --workspace --features conformance` runs without REGRESSION (no new warnings introduced by the markdown file's existence — rustdoc doesn't process `docs/*.md`, so this is a regression-baseline check, not a check-the-new-file check). The actual verification for the markdown is "render locally" per Risks § first 🔵 suggestion.

### Phase 2 — Theme B recipes (queue backend swap, scoring source, sensor)

Three recipes, lifted from the existing example crates. Each follows a consistent shape with the SAME Cargo.toml dev-dep posture (plan-critic technical agent S1 — example-crate READMEs are silent on the conformance feature gate, so the composition guide carries the load):

```toml
[dependencies]
neurogrim-sdk = "0.1"
# ... trait-specific deps (async-trait, anyhow, etc.)

[dev-dependencies]
# REQUIRED to run the conformance suite at test time:
neurogrim-sdk = { version = "0.1", features = ["conformance"] }
# ... test deps (tokio, tempfile, etc.)
```

The dev-dep self-reference is what activates the gated `*_conformance` modules at `cargo test` time without polluting production builds with `tokio`. V5-SDK-2 partial Phase 3 established this posture for the in-tree example crates (commit `fa19288`); the composition guide makes it explicit for adopters.

- **Recipe 1: Swap the queue backend.** Lift from `examples/queue-backend-memory/`. Show: minimum-viable `QueueBackend` impl + `QueueBackendFactory`, conformance test wiring via `neurogrim_sdk::queue_backend_conformance::run_factory_conformance`. "What's NOT possible at v5.0" pointer (dynamic .so loading → BACKLOG B-40).
- **Recipe 2: Add a custom scoring source.** Lift from `examples/scoring-source-prom/`. Show: HTTP-fetch pattern, `ScoringSource` + `ScoringSourceFactory` impls, registry registration in the consuming binary's `main.rs`. Limits: `ScoringSourceConfig` not re-exported by SDK at v0.1.0 (cyclic-dep concern); third-party authors take a direct `neurogrim-core` dep alongside `neurogrim-sdk`. Tracked for SDK 0.2.0 polish.
- **Recipe 3: Ship a sensor as a crate.** Lift from `examples/sensor-readme-quality/` (FS-read pattern) and `examples/sensor-constant-score/` (minimal-deps pattern). Show: `Sensor` impl, async-trait usage, factory shape, conformance test invocation via `neurogrim_sdk::sensor_conformance::run_factory_conformance`. Cross-reference `crates/neurogrim-sdk/README.md` § "Writing a conformant Sensor" for the deeper walkthrough (don't duplicate).

Each recipe ends with a 2–3 line "what's NOT possible" callout linking to the relevant BACKLOG entries.

### Phase 3 — Recipe 4 (custom test runner)

**Structural-asymmetry transparency sentence** (plan-critic methodology agent C1 — must be in the recipe, not papered over): recipes 1–3 lift from out-of-tree adopter-pattern example crates (`examples/queue-backend-memory`, `examples/scoring-source-prom`, `examples/sensor-readme-quality`). Recipe 4 lifts from `neurogrim-cli` because `NextestRunner` is the bundled default impl that ships inside NeuroGrim itself — there is no out-of-tree `TestRunner` example crate at v5.0. **An out-of-tree `TestRunner` written by a third-party adopter would live in their own crate exactly like recipes 1–3.** The recipe states this up-front so adopters understand the structural difference is positional (where the impl lives in the workspace), not patternal (the trait shape, factory contract, and conformance discipline are identical).

Lift from V5-FOUND-4's `NextestRunner` (`crates/neurogrim-cli/src/commands/test_runner_impls/nextest.rs`). Show:

- `TestRunner` trait + `TestRunnerFactory` shape.
- Minimum-viable impl skeleton (the `NextestRunner` body, simplified — just the trait method, no error context).
- Selection translation (`match selection` with `_ => bail!()` arm — `TestSelection` is `#[non_exhaustive]`; v5.1+ may add a `ByCoverage(...)` variant for coverage-driven selection per V5-FOUND-3 deferral chain (BACKLOG B-44 v6 successor pipeline). **Always include a wildcard arm** (plan-critic methodology agent C2 — the V5-FOUND-3 deferral was implicit in plan v1; v2 surfaces it to adopters in one sentence).
- Conformance test wiring via `neurogrim_sdk::test_runner_conformance::run_factory_conformance`.
- **Production-construction caveat** (plan-critic technical agent S2): `NextestRunnerFactory::build()` returns a `NextestRunner` with hardcoded defaults (`project_root="."`, `profile="default"`, `slow=false`); production code constructs `NextestRunner::new(project_root, profile, slow)` directly with operator-supplied values. The factory pattern exists for the future v5.5 BACKLOG B-52 (`--runner=` registry dispatch). Recipe 4 calls this out so adopters don't copy a non-functional `Box::new(NextestRunnerFactory)` pattern.

**Honesty section** — Fork D default: explicit ~5-line callout. v5.0 ships only `NextestRunner`; the `--runner=` CLI flag is deferred to v5.5 BACKLOG B-52 (only one runner exists, flag would be ceremony); AgentDrivenRunner is deferred to v5.5 BACKLOG B-51 (requires Rust LLM client, currently blocking V5-FOUND-1.1 too). Adopters CAN write their own `TestRunner` impl today and register it with `TestRunnerRegistry`; they CAN'T select between runners at the CLI surface until B-52 lands.

The "what's NOT possible" framing is more substantial here than for recipes 1–3 because the trait extraction at v5.0 deliberately deferred the second-impl story per V5-FOUND-4 plan-critic methodology lens. Honest framing matters more than recipe-shape symmetry.

### Phase 4 — cross-references + verification

1. Add cross-references at the end of each recipe — link to the example crate's README, the SDK README's § "Writing a conformant <Type>" subsection (Sensor only — others stay rustdoc-only per Fork F1 from V5-SDK-2 partial), the relevant epic file.
2. **"v5.5 / v6 horizon" closing section, restructured** (plan-critic methodology agent suggestion 6 — "audience drift mitigation"). Frame as **capability gaps** (what adopters can't do today), not BACKLOG-ID-leading list. Each gap entry: one-sentence capability description, then BACKLOG IDs as parenthetical breadcrumb. Sample shape:
   - **Dynamic `.so` / `.dll` plugin loading** — at v5.0, plugins are cargo-feature-gated at compile time. Runtime loading from a directory is v5.5 work (BACKLOG B-40).
   - **Per-test coverage-driven test selection** — `neurogrim test --select-by-coverage --since HEAD~1` is deferred while a Windows host coverage-toolchain gap is resolved (BACKLOG B-28 → V5-FOUND-3 deferred 2026-05-03; B-44 v6 promotion to a Brain domain).
   - **Agent-driven test orchestration** — `--runner=agent` dispatching to an LLM-orchestrated subset selector requires a Rust-side LLM client to land first (BACKLOG B-51; also blocks V5-FOUND-1.1 diagnostic synthesis).
   - **CLI runner selection (`--runner=<name>`)** — flag deferred until ≥2 runners exist (BACKLOG B-52).
   - **Per-domain custom CMDB types** — v6 horizon (BACKLOG B-41).
   - **Agent-card versioning** — v6 horizon (BACKLOG B-42).
   - **Trajectory model abstraction** — v6 horizon (BACKLOG B-43).
   
   Adopters care about "what can't I do today"; operators get the BACKLOG breadcrumbs to drill in. Both audiences served, neither leads.
3. Verify (per established epic-close cadence):
   - `cargo nextest run --workspace --profile ci` ✓ (no regressions; the 4 example crates' tests still pass per V5-SDK-2 partial Phase 3).
   - `cargo doc --workspace --features conformance` ✓ (regression baseline check — markdown file doesn't affect rustdoc; verifies no pre-existing rustdoc errors regressed during Phase 1–3 commits).
   - Render the markdown locally — `docs/v5-composition-guide.md` displays correctly (tables, code blocks, the ASCII diagram).
4. **Recipe verification gate (load-bearing):** for each recipe, the cited example crate compiles AND its conformance test passes under `cargo nextest run -p <crate-name>`. This is the load-bearing recipe-honesty check.
5. **Snippet-match operator-checklist step** (plan-critic methodology agent C3 — Done-When says "CI builds the code samples"; Fork E1 reinterprets to "CI builds the example crates". The reinterpretation moves the gate from "guide-snippet-compiles" to "cited-crate-compiles". To preserve doc-rot honesty: for each recipe, manually verify the inlined snippet matches the cited example crate's current source character-by-character — paste the snippet next to the source file, eyeball-diff, fix divergences before commit. v5.5 polish could automate this via Fork E2 doctest harness if drift becomes empirically common.

### Phase 5 — V5-DOC-1 close-out

1. `roadmap/epics/v5-coherence.md` § V5-DOC-1 — flip status `Planned` → `✅ COMPLETE 2026-05-XX`. Done-When checkboxes all flipped.
2. `roadmap/v5-roadmap.md` Theme D row — V5-DOC-1 ✅; V5-DOC-2 unblocked.
3. Add cross-references to V5-DOC-2's prep — VISION principle #20 wording draft can land here as a "starting point for V5-DOC-2 dual-review" footnote, OR strictly defer to V5-DOC-2 (Fork F default — defer to keep V5-DOC-1 scope tight).

## Forks (operator-pinnable)

- **Fork A — Doc location**:
  - **A1** = `docs/v5-composition-guide.md` *(default)*. Joins the existing `docs/` directory at the workspace root. Visible as `docs/` is the canonical NeuroGrim doc home.
  - A2 = `docs/composition-guide.md` (no v5- prefix). More future-proof — when v6 lands, the doc doesn't need a rename. But loses the "this was written for v5" framing.
  - A3 = `crates/neurogrim-sdk/COMPOSITION.md`. Co-located with the SDK README. Awkward because the guide covers material beyond what's in `neurogrim-sdk` (NextestRunner is in `neurogrim-cli`).

- **Fork B — Architecture diagram format**:
  - **B1** = ASCII art *(default)*. Matches the codebase's existing markdown-table-and-ascii style (cf. `docs/cli-mode.md`, `docs/cli-sensory-surface.md`). No new tooling; renders correctly on crates.io / GitHub / `less`.
  - B2 = Mermaid (`graph TD ...`). GitHub renders mermaid natively now; richer visuals. But not all markdown viewers render it; introduces a "GitHub-only" UX cliff.
  - B3 = Embedded PNG (binary asset committed to the repo). Most flexibility; adds a binary asset that's not text-diffable + opens the door to "diagram drift" with no good way to gate it in CI.

- **Fork C — Code sample sourcing strategy**:
  - **C1** = lift from existing example crates *(default)*. Cite + inline the load-bearing snippets; CI gate is "the example crates compile" (already in place per workspace CI). Doc rot is bounded by example-crate compile failures. Methodology-honest — the guide says "this is what works because this is what we ship."
  - C2 = `include_str!` from example crate source files into the doc at rustdoc time. Mechanically prevents drift but the doc isn't a rustdoc target; would require a custom build tool. Heavier than the gain.
  - C3 = re-implement code samples in the doc (orthogonal to example crates). Risky — drift hazard guaranteed; the doc says "this is what should work" without a CI gate.

- **Fork D — Recipe 4 (test runner) framing**:
  - **D1** = explicit deferral callout *(default)*. ~5-line section explaining what v5.0 ships (trait + NextestRunner), what's deferred (B-51 AgentDrivenRunner; B-52 `--runner=` flag), what adopters CAN do today (write impl + register with registry), what they CAN'T (CLI runner selection). Honest framing matches V5-FOUND-4's methodology discipline.
  - D2 = brief one-line pointer ("see BACKLOG B-51/B-52 for v5.5 work"). Shorter recipe but loses the load-bearing context that makes the trait extraction make sense at v5.0.
  - D3 = full speculative walkthrough of "what AgentDrivenRunner will look like once B-51 ships." Aspirational; rejected by V5-FOUND-4's same plan-critic methodology lens.

- **Fork E — CI integration approach**:
  - **E1** = leverage existing workspace CI *(default)*. The 4 example crates + NextestRunner are already exercised by `cargo nextest run --workspace --profile ci`. The composition guide's recipes reference these; if a recipe's cited code path breaks, workspace CI fails. No new CI infrastructure.
  - E2 = doctest harness for the composition guide. Custom tool that extracts ` ```rust ` blocks from the markdown and compiles them in a tmpdir. Heavier; lets the doc carry standalone snippets that aren't tied to example crates. Risk: tmpdir builds are slow; doctest harness becomes a maintenance burden.
  - E3 = no CI integration. The doc is documentation; treat doc rot as a manual-review cost. v5.5 polish could add a gate later.

- **Fork F — VISION #20 wording footnote**:
  - **F1** = no preview *(default)*. V5-DOC-1 keeps scope tight to recipes; V5-DOC-2 owns VISION principle #20 wording exclusively (with `dual-review` skill T+P enforcement per epic). V5-DOC-1 references proposed-#20 in passing without proposing wording.
  - F2 = preview principle #20 wording in V5-DOC-1's closing section. Lets the V5-DOC-2 dual-review work against a concrete starting point. But V5-DOC-2's whole point is rigor on the wording; previewing in V5-DOC-1 risks anchoring the dual-review against a not-yet-vetted draft.

Defaults pinned: **A1 / B1 / C1 / D1 / E1 / F1**. Six forks; user-pinnable.

## Mutual-exclusion + conflict checks

| Combination | Behavior |
|---|---|
| Recipe 4 with `--runner=agent` reference | NOT in v5.0 — the flag doesn't exist. Recipe 4 documents what's shipped (trait + NextestRunner) and explicitly defers `--runner=` to v5.5 B-52. |
| Recipe references in markdown to types that aren't yet re-exported by `neurogrim-sdk` | The recipe must use the SDK path (`neurogrim_sdk::TestRunner`) for re-exported types AND fall back to `neurogrim_core::*` for support types not yet on the SDK surface (e.g., `ScoringSourceConfig`). Documented per recipe. |
| Code sample compiles via the cited example crate but the example crate's Cargo.toml uses workspace-only path deps | The recipe's "if you write this from outside the workspace" Cargo.toml block uses `neurogrim-sdk = "0.1"` (crates.io path), not `path = "..."`. Each example crate's README already has this footnote; the composition guide lifts it. |
| `cargo doc --workspace --features conformance` warnings | Acceptable: pre-existing warnings from `neurogrim-secrets` + `neurogrim-cli`. New warnings from V5-DOC-1 are blocking — the verification step at Phase 4 step 3 catches them. |

## Exit-code spec

V5-DOC-1 does not add new CLI flags or commands. No exit-code spec needed.

## Verification (consolidated)

- Phase 1: `cargo doc --workspace --features conformance` ✓ (no broken intra-doc-links from new file).
- Phase 2: each of the 3 cited example crates compiles + conformance test passes (`cargo nextest run -p queue-backend-memory -p scoring-source-prom -p sensor-readme-quality -p sensor-constant-score`).
- Phase 3: `cargo nextest run -p neurogrim-cli -E 'test(test_runner_impls)'` ✓ (NextestRunner unit tests still pass post-recipe references).
- Phase 4: `cargo nextest run --workspace --profile ci` ✓; `cargo doc --workspace --features conformance` ✓; markdown renders correctly.
- Phase 5: `roadmap/epics/v5-coherence.md` § V5-DOC-1 status = ✅ COMPLETE; `roadmap/v5-roadmap.md` Theme D row updated.

## Deliverable shape

6 phase commits per established cadence:

1. Phase 0 — plan v1 + fork pins (this commit).
2. Phase 1 — skeleton + intro + ASCII architecture diagram.
3. Phase 2 — recipes 1–3 (Theme B reuse).
4. Phase 3 — recipe 4 (test runner with deferral framing).
5. Phase 4 — cross-references + verification + v5.5/v6 horizon section.
6. Phase 5 — V5-DOC-1 close-out (epic + roadmap status flips).

(Phase 4 may bundle into Phase 5 if the cross-references are short.)

## Risks / adversary concerns brought forward

🟡 **Doc rot.** Composition guide drifts from reality if not CI-tested. Mitigation: Fork C1 (lift from example crates) + Fork E1 (workspace CI exercises example crates). If a recipe's cited code stops compiling, workspace CI fails. Risk reduces to: composition guide's *prose* drifting from the example crates' *current shape* — caught at human review during the next adopter-facing change. v5.5 polish: Fork E2 (doctest harness) if drift becomes empirically common.

🟡 **Recipe 4 honesty pressure.** V5-FOUND-4's methodology lens explicitly flagged the temptation to over-promise the test-runner story. V5-DOC-1's recipe 4 must honor that — Fork D1 (explicit deferral callout) is the methodology-aligned default. If a future PR softens the callout to a one-line pointer (D2), it regresses the methodology stance.

🟡 **Cross-reference fragility.** The composition guide cites file paths + line numbers in some places (e.g., `crates/neurogrim-cli/src/commands/test_runner_impls/nextest.rs:N`). Line numbers drift; file paths are more stable. Mitigation: cite file paths only; cite line numbers only inside example crates (which are stable adopter-facing surfaces). For internal NeuroGrim source, prefer rustdoc anchors (`#method.run`) over line numbers.

🟡 **Audience drift.** The guide's audience is "third-party adopters writing their own plugins." Internal-state references (BACKLOG entry IDs, plan-critic findings) belong in retrospectives, not adopter docs. Mitigation: BACKLOG IDs appear in the closing "v5.5/v6 horizon" section as forward-looking pointers; plan-critic findings stay in the V5-DOC-1 plan file (this document) and don't leak into the guide.

🔵 **Suggestion — render the guide locally before commit.** GitHub markdown renderer occasionally surprises (table column alignment, code-block language tagging). A local render via any markdown viewer catches most surprises pre-commit. Manual; not in CI scope.

🔵 **Suggestion — V5-DOC-2 prep work.** V5-DOC-1 closes with a "v5.5/v6 horizon" section; V5-DOC-2 follows with VISION principle #20 + spec alignment. Suggest: V5-DOC-2's plan-critic round explicitly checks that proposed-#20 wording is *backward-readable* against V5-DOC-1's deferral framings (recipe 4's "what's NOT possible" callouts must align with #20's "by use, not aspiration" stance).
