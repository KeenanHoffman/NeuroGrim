# Plan — Documentation Broker (+ the Doc v5.0 Version Upgrade it powers)

> **Provenance:** authored + approved 2026-06-30 (D:\Brains ecosystem session). Sister to the work/backlog
> broker. First execution gate: run `plan-critic` against this file. Substrate code on a NeuroGrim branch
> (never `main`); the doc-upgrade edits span all projects and are broker-driven, one reconcile at a time.

## Context

The ecosystem's documentation has drifted: front doors disagree on versions (spec stated as v2.1 / v3.1 /
actual **v3.2**; NeuroGrim as 3.4.0 / v3.2 / **5.0.0**), the root `README.md` predates cereGrim + the v5
release, the `broker-pattern.drawio.svg` is the v3 diagram (still says "Federation Broker"), and 5 `IDE-LIFT-*`
docs are orphaned (the IDE moved to a separate repo). There's no unified doc version and four cross-link
styles coexist. The goal is a project-wide **documentation v5.0 upgrade** — refresh stale content, redo/remove
mermaid diagrams, unify linking, lay clear reading paths so a user can follow logical paths easily — and,
crucially, **a documentation broker (sister to the work/backlog broker) to drive and then guard it**. Build the
broker FIRST; it becomes the deterministic work-queue for the upgrade ("run `docs next`, do what it says,
re-run until idle"), composing with `sync-ecosystem`'s flag-don't-auto-update ethos.

**Decisions locked with the operator:** (1) **ecosystem-root, cross-project** scope (one instance at `D:\Brains`
sees all children — `build_graph` already recurses); (2) **both** the InnateAbility next-doc broker *and* a
scored `documentation-health` domain; (3) **the broker decides the convention** — no mandated cross-link style;
it flags genuinely-broken links via a scope-aware classifier rather than enforcing one form. (Version *anchor*
reconciliation to v5.0 / spec v3.2 / neurogrim 5.0.0 still happens — only the link-style mandate is deferred.)

## Design at a glance — mirror the work broker exactly

The work broker is a 2-layer split; the doc broker clones it. **Big reuse:** the parse layer largely exists in
`neurogrim-sensory/src/documentation_graph.rs` (`build_graph(root) -> GraphReport{docs, orphans, broken_links,
cycles}`, pulldown-cmark, Tarjan SCC). We **augment** it (do not fork) with a freshness/version model + a tiered
`next_doc` dispatcher, then wrap it in a `DocBroker`.

| Layer | Work broker | Doc broker |
|---|---|---|
| A — pure sensory | `backlog.rs`: `parse_backlog`+`next_ready`+`analyze_backlog` | **extend** `documentation_graph.rs`: `build_doc_report`+`next_doc`+`analyze_documentation_health` (reuse `build_graph` verbatim) |
| B — broker | `work_broker.rs` `WorkBroker` (InnateAbility) | **new** `doc_broker.rs` `DocBroker` (InnateAbility) |
| factory | `broker_type=""` → boot fallback | `broker_type="doc-broker"` factory (opt-in; NOT the fallback) |
| CLI | `neurogrim backlog next-ready` | `neurogrim docs next` (+ `neurogrim docs map`) |

## Layer A — `documentation_graph.rs` additions (pure, no new module)

- **Reuse verbatim** (call, don't fork): `build_graph`, `walk_markdown`, `relpath`, `normalize_relative`,
  `extract_link_targets`, `compute_orphans`, `compute_non_trivial_sccs`, `SKIPPED_DIR_NAMES`. **`build_doc_report`
  is net-new** (wraps `build_graph`); it must **re-read each file** for front-matter + headings because
  `build_graph` discards file text (plan-critic 🟡 — double I/O, acceptable post-exclusion at a few-hundred-doc
  corpus; or extend `DocNode` to carry the already-read text if it matters). Parse front-matter with the
  already-present **`serde_yaml`** workspace dep (no new dep) — `parse_front_matter` must be panic-free on
  missing/foreign/malformed front-matter (Option/Result). (`serde_yaml` 0.9 is upstream-archived — reuse it,
  don't pull a competing crate.)
- **Front-matter** (the module's Phase-2-deferred item, now built): `parse_front_matter(text) -> FrontMatter`
  — optional YAML fence, every field optional (honest-unknown): `doc-version, date, status (current|draft|stale|
  superseded|archived), supersedes[], superseded-by, anchored-to (ecosystem|spec|neurogrim|none), owner,
  front-door`. `raw_present:false` (no front-matter) is itself a mild signal.
- **`EcosystemAnchor`** `{ecosystem_version, spec_version, neurogrim_version, anchor_date}` — resolved at
  runtime (priority: explicit param → sentinel file e.g. root `CLAUDE.md`/`VERSION.toml` → compile-time default
  matching today's aligned values). Keeps Layer A pure; makes the upgrade re-runnable as the anchor advances.
- **`DocReport`** augments `GraphReport`: per-doc `DocMeta{front_matter, staleness: Vec<StalenessSignal>,
  is_front_door, git_mtime?}` + `front_doors`, `unreachable_from_front_door`, `references_to_deleted`,
  `version_drift`, `stale_diagrams`. Signals: `StatusStaleOrSuperseded`, `VersionMarkerDrift` (stated version ≠
  governing anchor, unless `anchored-to: none`), `ReferencesDeleted`, `Orphan`/`BrokenLink` (read straight from
  the graph), `Unreachable` (BFS from front doors — richer than orphan), `StaleDiagram`, `NoFrontMatter`,
  `GitStale` (optional, off by default, advisory tiebreaker only — git-mtime breaks Layer A purity).
- **Stale-diagram model** (composes with `DIAGRAM-V4-SPEC.md`, "prose wins over diagram"): deterministic, no
  rendering — `PendingSpec` (a `*-V*-SPEC.md` says PENDING while the old diagram still exists → catches
  `broker-pattern.drawio.svg`), `ForbiddenTermDrift` (grep diagram text for retired terms e.g. "Federation
  Broker"), `MissingFromMmdConvention` (md embeds mermaid but no sibling `.mmd`). Diagrams are non-`.md`, so a
  small sibling walker (generalize `walk_markdown` to extensions) — do NOT bolt into `build_graph`.
- **`next_doc(report) -> Value`** — tiered dispatcher mirroring `next_ready` (deterministic, single item, never
  invents filler, JSON envelope with `tier`/`ready`/item/`rationale`/`explanation`). Tiers ranked:
  **1 reconcile-front-door** (a front door has drift/stale — gates every reading path) → **2 refresh-stale** →
  **3 fix-broken-link** → **4 update-diagram** → **5 cover-orphan/unreachable** → **6 idle** (returns counts).
  5-tuple key `(tier_rank, front_door_first, severity, anchor_distance, idx)`; sort ascending, return first.
  Root README (v3.0-rc.1 vs 5.0 = major-gap `anchor_distance`) sorts to the very top.

## Layer B — `neurogrim-brokers/src/doc_broker.rs` (structural clone of `work_broker.rs`)

- `DocBroker{ id, working_state: WorkingState<DocsState>, overlay: Arc<Overlay<ActiveDocsOverlay>>, governance,
  project_root, anchor, scope }`. Role **InnateAbility**. `cmdb_path() -> None` (the Sense CMDB is separate).
- `DocsState{doc_units}`, `DocUnit{id, path, tier, action, status}`, `ActiveDocsOverlay{active_docs,
  recent_reconciles}`. `new` + `new_with_project_root` (resolves anchor).
- `project_state_from_sensor`: `build_doc_report` + `next_doc`; surfaces a single `DocUnit` **only** when tier ≠
  `idle` (the one divergence from work broker's single-actionable-tier — justified: each actionable tier carries
  a distinct repair `action` string). `curate_overlay` projects active vs recent.
- **Catalog:** Surfaced `doc-broker/dispatch-doc-unit` (param `doc_unit_id`; steps `[reconcile_doc_unit,
  refresh_overlay]`; OperatorConfirmed/Capability/HotStoreUpdate; precondition on `active_docs`) + Internal
  `doc-broker/doc-broker-tick` + `canonical_governance_pipelines`. **Leaf-ops:** `reconcile_doc_unit` (marks the
  unit addressed in working-state — the broker does NOT edit docs; the agent does the rewrite, exactly as
  `claim_work_unit` doesn't do the work), `refresh_overlay`, `arm_kill_switch`, `disengage_kill_switch`.
- **Wiring:** `pub mod doc_broker;` + re-exports in `lib.rs`; a `doc_broker_factory()` registered via
  `BrokerFactoryRegistry::register("doc-broker", …)` before `BrokerHost::boot` (host.rs factory dispatch already
  routes by `broker_type`). Per-broker manifest sets `broker_type="doc-broker"`.
- **CLI:** `DocsCmd` + `neurogrim docs next [--project-root .] [--anchor-version 5.0] [--use-git-mtime]` and
  `neurogrim docs map [--validate]` — 5-line clones of `run_backlog_next_ready`.

## [Sense] companion — ENRICH the existing `documentation-graph` domain (no new domain)

**Plan-critic 🟡:** a `documentation-graph` domain ALREADY exists (weight 0.0, bound to
`analyze_documentation_graph`'s CMDB at `.claude/documentation-graph-cmdb.json`). Do NOT add a duplicate
`documentation-health` domain over the same sensor. Instead **enrich `analyze_documentation_graph` in place**
to consume the richer `DocReport` and layer new penalty terms onto the current orphan/broken/cycle blend:
version-drift (front-door drift heavier), stale/superseded status, references-to-deleted, stale-diagram,
unreachable-from-front-door. Same CMDB path, same domain entry (stays advisory weight 0.0; sum unaffected) — so
`neurogrim score` tracks doc health with zero registry churn. [Sense] and [InnateAbility] both read the same
`DocReport` (single source of truth, like `analyze_backlog`/`next_ready` both read `parse_backlog`). **Defer**
promoting the dormant LSP-Brains spec-quality domains (link-integrity, diagram-sync, spec-completeness,
glossary-freshness, changelog-hygiene, rfc-2119-compliance) from score-0 stubs — they're the eventual consumers
of this surface but each needs its own calibration. Do not block the broker on them.

## Reading-path / front-door support ("follow logical paths easily")

- **Reachability tier** (tier-5): front doors = `front-door:true` ∪ canonical names (`CLAUDE.md`, `README.md`,
  `ROADMAP.md`, `VISION.md`, `*AGENT-PRIMER*`, `SCAFFOLDING.md`); BFS over outbound edges; unreached → flagged
  (catches the IDE-LIFT island even though those docs link each other).
- **SCAFFOLDING.md** (cereGrim's is the model): `neurogrim docs map --validate` diffs the table's path column
  against `report.docs` (missing-from-map, dead-rows, Status-column drift → dispatches); `neurogrim docs map`
  generates a SCAFFOLDING-shaped table for projects lacking one (advisory tool, agent authors the doc).
- **Cross-project entry graph** (ecosystem-root scope): ecosystem `CLAUDE.md` → child `CLAUDE.md` edges; an
  unreached child front door is a top-tier reconcile signal.

## Scope — ecosystem-root, with an exclusion set + privacy filter + scope-aware link classifier

One code path, a `scope: Project | Ecosystem` flag. Ecosystem-root (`project_root=D:\Brains`) is the default for
the upgrade (front doors span repos).

- **Exclusion set (plan-critic 🔴 — without this the walk is 73% noise).** `build_graph` at `D:\Brains` indexes
  **1448 .md, 1064 of them vendored `rustsec-advisory-db`** (`NeuroGrim/vendor/`), plus `archive/` (11), two
  `.claude/skills/archived/` copies (24 each), `audit/` (10). `build_graph` currently accepts **no** exclude
  param — Phase 0 must (a) add `vendor` to `SKIPPED_DIR_NAMES`, and (b) thread an **exclude-prefix list** into
  `build_graph`/`walk_markdown` covering `archive/`, `**/.claude/skills/archived/`, `audit/`. Submodule-crossing
  (NeuroGrim/LSP-Brains/python-starter) is INTENDED (cross-project coherence), not pruned.
- **Thesis-privacy (PUBLIC-VS-PROPRIETARY invariant):** exclude **`cereGrim/thesis/` ONLY** (cereGrim/docs is
  cereGrim-public and is exactly the cross-project content we want unified — do NOT exclude it); **suppress** any
  dispatch whose action would create a public→thesis edge; **flag** (never create) any existing public→thesis
  link as a broken-invariant defect; regression test asserts no thesis path appears in any finding.
- **Cross-repo link portability** (the "broker decides convention" decision): a 4-class link resolver —
  in-repo / out-of-repo-but-in-ecosystem / external-URL / broken. **Only `broken` counts against health.** A
  `../NeuroGrim/...` link is valid at root scope and "externally-anchored" (not broken) at per-project scope, so
  per-project runs don't drown in false-broken cross-repo links. No canonical style is mandated.

## Sequencing — broker built FIRST, then drives the upgrade

- **Phase 0 (build broker, no doc edits):** Layer A (exclude-prefix param + `vendor` skip; front-matter, anchor,
  DocReport, next_doc, enriched `analyze_documentation_graph`) + tests; Layer B (doc_broker + lib exports +
  factory) + tests; CLI (`docs next`/`map`, **`#[cfg(feature="sensor-documentation-graph")]`-gated**). **Wiring
  (plan-critic 🔴): add `sensor-documentation-graph` to `neurogrim-brokers`'s `neurogrim-sensory` features in
  Cargo.toml** (currently only `sensor-backlog`) or `doc_broker.rs` can't see the module. Enrich the existing
  `documentation-graph` domain (no new domain entry). `cargo test` green.
- **Phase 1 (anchor + front doors, tier-1):** set anchor = v5.0 / spec v3.2 / neurogrim 5.0.0 / 2026-05-09. Run
  `docs next` at root; reconcile top-down as it emits: root README → spec-version strings (README v2.1 /
  V5-PRIMER v3.1 → v3.2) → NeuroGrim-version strings (README 3.4.0 / CLAUDE.md v3.2 → 5.0.0). Add front-matter
  to each front door so future runs are drift-checkable.
- **Phase 2 (broken links, tier-3):** drain `broken_links` + `references_to_deleted`. Fix genuinely-broken only
  (no style mandate, per the scope-aware classifier).
- **Phase 3 (diagrams, tier-4):** regenerate broker-pattern v4 as `.mmd` from `DIAGRAM-V4-SPEC.md`; remove the
  stale `broker-pattern.drawio.svg` via `skill-deprecation` archival-with-provenance; standardize `.mmd`
  (every embedded mermaid gets a sibling `.mmd`; the `MissingFromMmdConvention` signal enumerates gaps).
- **Phase 4 (archive orphans, tier-5):** run the 5 `IDE-LIFT-*` + `PHASE-PROGRESS` through `skill-deprecation`
  (archive-with-provenance + redirect-block + cross-ref sweep); `references_to_deleted` verifies nothing live
  still points at them.
- **Phase 5 (reading paths):** generate/validate `SCAFFOLDING.md` for NeuroGrim + ecosystem root; re-run until
  tier = `idle`; `documentation-health` score becomes the standing regression guard.

## Critical files

| File | Role |
|---|---|
| `NeuroGrim/neurogrim/crates/neurogrim-sensory/src/documentation_graph.rs` | extend: front-matter, `DocReport`, `next_doc`, `analyze_documentation_health` (reuse `build_graph`) |
| `NeuroGrim/neurogrim/crates/neurogrim-brokers/src/doc_broker.rs` *(new)* | `DocBroker` — clone of `work_broker.rs` |
| `…/neurogrim-brokers/src/work_broker.rs` | **verbatim template** (read-only) |
| `…/neurogrim-brokers/src/{lib.rs,host.rs,registry.rs}` | module/re-export, `doc-broker` factory dispatch, `broker_type` manifest |
| `…/neurogrim-sensory/src/backlog.rs` | `next_ready` tiered-dispatcher pattern to mirror (read-only) |
| `…/neurogrim-cli/src/main.rs` | `DocsCmd` + `neurogrim docs next`/`map` |
| `NeuroGrim/docs/diagrams/DIAGRAM-V4-SPEC.md` · `docs/PUBLIC-VS-PROPRIETARY.md` · `cereGrim/docs/SCAFFOLDING.md` | Phase-3 diagram source · privacy invariant · front-door template |

## Risks / rollback

- **build_graph reuse boundary** — no `§N.M` anchor verification (still deferred); diagrams are non-`.md` →
  separate walker. Don't bolt diagram detection into `build_graph`.
- **Thesis-privacy** (highest) — path-exclusion + public→thesis edge-suppression + regression test + re-run the
  grep invariant after each upgrade phase.
- **Cross-repo link false-positives** — the 4-class scope-aware resolver; per-project treats out-of-repo as
  external not broken.
- **Freshness over-fire** — `anchored-to: none` opts out; absent front-matter is mild-only; git-mtime
  advisory/off; `documentation-health` ships weight 0.0 until calibrated.
- **Anchor staleness** — resolve from a sentinel file; the constant is a last resort.
- **Determinism** — BTreeMap iteration + path-sorted `idx` tiebreaker (same guarantee `next_ready` relies on).
- **Rollback** — Phase 0 is pure-additive Rust on a branch (`git revert`); the upgrade phases are doc edits,
  each its own commit, reversible per file. The broker never auto-edits docs.

## Verification

- **Layer A:** `cargo test -p neurogrim-sensory --features sensor-documentation-graph` — front-matter parse,
  version-drift (README v3.0 vs anchor 5.0), reference-to-deleted, reachability vs orphan, stale-diagram
  (Federation-term + PENDING-spec fixtures), each `next_doc` tier arm, idle-when-current, "never invents filler".
- **Layer B:** `cargo test -p neurogrim-brokers` — overlay filters to actionable tier, role = InnateAbility,
  `new_with_project_root` over a fixture tree, empty overlay when idle, `dispatch-doc-unit` end-to-end via
  `PipelineRunner` (trust budget + trace), refuses unknown `doc_unit_id`.
- **Boot + dispatch:** host test with a `broker_type="doc-broker"` cluster manifest + factory registered →
  boot succeeds, catalog exposes `doc-broker/dispatch-doc-unit`, kill-switch reachable, projection mentions it.
- **CLI:** `neurogrim docs next --project-root <fixture>` → expected tier JSON; `docs map --validate` flags a
  planted SCAFFOLDING/disk mismatch.
- **Privacy regression:** root-scope run over a fixture with a `thesis/` dir → zero thesis paths in output + grep
  invariant clean.
- **Upgrade end-to-end:** after Phases 1–5, `neurogrim docs next` at root returns `tier: idle`;
  `documentation-health` score stabilizes high; every front door carries reconciled front-matter.
