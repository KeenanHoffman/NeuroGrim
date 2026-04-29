# Changelog

All notable changes to NeuroGrim + the LSP Brains specification live
here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.4.0] - 2026-04-28

*The third audience surface — a self-contained HTTP + React
dashboard (`neurogrim ui`) that gives humans a visual companion
to the CLI and MCP server. Five pages, SSE-driven live updates,
hat-lens picker, dark/light theme. New crate
`neurogrim-dashboard` (the seventh in the workspace).*

### Added — `neurogrim ui` command + `neurogrim-dashboard` crate

The dashboard ships as a single binary: the React frontend is
built into `frontend/dist/` and embedded at compile time via
`rust-embed`. Operators `cargo install neurogrim-cli` and the UI
ships with it — no Node.js required at runtime. Read-only in
v3.4 by design; mutation endpoints are gated behind a
`--allow-mutations` flag planned for v3.5.

#### Five pages

- **Overview** (`/`) — landing page with score gauge, trajectory
  badge, top three strongest signals, top three recommendations,
  and the all-advisory "N/A · observe-only posture" rendering for
  Brains that haven't promoted any domain to weighted yet.
- **Domains** (`/domains`) — sortable table per declared domain
  with weight, raw score, effective score, confidence,
  trajectory, last-updated. Color-coded scores (green/amber/red).
  Click a row to drill in.
- **Domain detail** (`/domains/:name`) — findings table with
  status badges + signed point deltas, Recharts sparkline of
  recent score history, CMDB metadata, and the sensor authoring
  intent block (`_todo_<name>`) when the sensor hasn't been
  written yet.
- **Federation** (`/federation`) — one-hop view of declared
  peers with hand-drawn SVG topology (self + peers, color-coded
  by liveness, dashed edges for read-only siblings) and an
  Agent Card excerpt panel on click. A2A peers are probed at
  `/.well-known/agent-card.json` with a 1.5s timeout;
  subprocess peers stay `unprobed` by design.
- **Skills** (`/skills`) — inventory of every skill under
  `.claude/skills/` paired with invocation-ledger stats. Filter
  chips (alive / dead / new / no-ledger), search by name +
  description, click-to-expand detail. Surfaces a CTA banner
  when the PostToolUse hook hasn't been wired yet so the ledger
  is missing.

#### Live updates over SSE

A `notify::RecommendedWatcher` translates filesystem changes
into typed `DashboardEvent` variants
(`RegistryChanged` / `ScoreChanged{domain}` / `SkillInvoked`)
and broadcasts them over `tokio::sync::broadcast`. The
`/api/events` SSE handler subscribes a fresh receiver per
connection; the React frontend's `useDashboardEvents` hook
parses each event and invalidates only the relevant TanStack
Query keys. End-to-end latency on Windows: ~250 ms.

A small color-coded indicator in the sidebar footer surfaces
connection status (live / connecting / offline / disabled) so
the operator can tell at a glance whether they're getting
real-time updates.

#### Hat-lens picker

Dropdown in the sidebar that lists every hat declared in
`config.hats` plus a synthetic `default` entry. Selecting a hat
adds `?hat=<name>` to every score-aware request so the Brain
output flows through that hat's `domain_multipliers`. Selection
persists in `localStorage`. The picker collapses to a static
"no hats" label when the registry has no hats declared.

#### Theme

Dark/light toggle in the sidebar footer. Persists in
`localStorage`; first-load fallback honors
`prefers-color-scheme`. Tailwind's `darkMode: ["class"]`
strategy applies the `dark` class to `<html>`.

#### Cross-platform browser launch

`neurogrim ui` opens the default browser by default with
hardened detection logic: skips on `CI=true`, on Linux without
`DISPLAY` / `WAYLAND_DISPLAY`, and in headless SSH sessions —
each with a distinct stderr message explaining *why* it
skipped. WSL is detected via `/proc/version` and the URL is
routed through `cmd.exe /c start` to reach the Windows host
browser. Always honors `--no-browser`.

### Added — `neurogrim explain ui`

Tenth bundled methodology topic covering the dashboard surface:
the five pages, `neurogrim ui` flags, browser-launch decisions,
SSE wire details, hat lens, theme, surface comparison
(CLI vs MCP vs dashboard), architecture (rust-embed bundling,
ts-rs bindings, TanStack Router, the seven `/api/*` routes).

`BUNDLED_VERSION` bumped to `v3.4` and every topic file's
version header updated accordingly.

### Added — `neurogrim-dashboard` crate API surface

- `GET /api/health` — server version + registry path
- `GET /api/overview?hat=<name>` — score gauge data + top recs
- `GET /api/domains?hat=<name>` — sortable list view
- `GET /api/domains/:name?hat=<name>` — drill-in detail
- `GET /api/federation` — peers + Agent Card probes
- `GET /api/skills` — inventory + ledger stats
- `GET /api/hats` — picker choices
- `GET /api/events` — SSE stream

Every wire-format type derives `ts_rs::TS` and exports to
`crates/neurogrim-dashboard/bindings/` at `cargo test` time so
the frontend stays type-aligned with the Rust source of truth.

### Tooling — frontend stack

React 18 + TypeScript 5.7 + Vite 5.4 + Tailwind 3.4 + shadcn/ui
copy-paste components + Recharts (gauge + sparkline) +
TanStack Query 5.62 + TanStack Router 1.95. No new npm
dependencies introduced beyond the initial Phase 0 set —
phases 1.3 and 2.1 explicitly avoided installing `react-flow`
and a separate SSE client library, building both surfaces
on existing primitives instead.

### Tooling — supply-chain discipline (interrupt during Phase 0)

- New bundled `dependency-discipline` skill in
  `init-skills/` (152 lines) — captures the discipline
  NeuroGrim's methodology requires before any dep enters
  the project's trust boundary. Federated to all Brain
  copies.
- `cargo xtask sca-check` runs `cargo audit` + `npm audit`
  with severity gating; alias declared in
  `neurogrim/.cargo/config.toml`.
- `audit/dep-accepted-2026-04-28.md` documents the five
  esbuild moderate findings (GHSA-67mh-4wv8-2f99) accepted
  as dev-only.

### Changed
- **Workspace `version` 3.3.0 → 3.4.0** across all 7 crates +
  `[workspace.dependencies]`. Frontend `package.json` synced.
- `neurogrim-cli` now depends on `neurogrim-dashboard` (new).
- `BrainContext` relocated from `neurogrim-cli` to
  `neurogrim-mcp` (Phase 0.1) so both the CLI and the
  dashboard server share a single source of truth for
  registry + scoring pipeline loading.

### Test surface delta
- 51 Rust dashboard tests (events classification, watcher
  integration, route smoke, skills scanner with YAML
  block-scalar fix, hats endpoint, ScoreQuery normalization)
- 10 ui-cmd tests (browser-launch decision matrix)
- 104 vitest tests across 14 files (component rendering,
  routing, theme persistence, SSE hook lifecycle, hat-picker
  Context wiring)
- 1 new explain regression test (`ui_topic_describes_the_five_pages_and_sse`)

### Breaking changes
None. The CLI surface is unchanged; the new `ui` subcommand is
additive. The dashboard ships read-only by design — any
existing automation pointed at the registry is untouched.

### Background — what prompted this release

The CLI gives agents a canonical contract; the MCP server gives
LLMs typed tools. But neither surface is reach-and-glance for
humans wanting charts, sparklines, and a sortable view of every
domain. The v3.4 dashboard closes that audience gap, while
adding the SSE substrate that v3.5+ live mutation flows will
need. The supply-chain interrupt early in the phase strengthened
the npm-dependency policy across the project; that discipline
work was a precondition for ever shipping a frontend.

## [3.3.0] - 2026-04-28

*Agent-authoring substrate. Closes all 10 friction points (F1–F10)
surfaced by the v3.2.2 agent-driven adoption test on the job-hunt
pilot, including a real runtime liveness bug (port 8423 collision
across the federation tree).*

### Added — explain topic
- **`neurogrim explain autonomy`** (F1) — bundled topic covering the
  autonomy block schema: levels, action_types, safety_invariants. The
  v3.2.2 agent had to grep the binary for field names; this closes
  that discoverability gap. Also serves as the worked example for the
  v3.2.2-era schema divergence (the agent invented `autonomy_bias`
  instead of using the canonical `blast_radius` / `reversible` /
  `description` set).

### Added — `doctor` checks
- **`check_autonomy`** (F3) — validates the autonomy block end-to-end:
  declared levels include the four canonical names; `action_types[].default_level`
  references a known level; `safety_invariants[]` entries have
  `rule` + at least one of `minimum_level` / `enforced_level`; both
  set is ambiguous (warn); unknown fields trigger warnings (catches
  invented fields like `autonomy_bias`); `description` recommended
  on action_types + safety_invariants.
- **`check_federation_ports` walks transitively** (F7) — the v3.2.2
  port-uniqueness check only considered direct children. v3.3 walks
  each peer's `brain_path/.claude/brain-registry.json` recursively
  and reports clashes across the entire federation tree. Includes
  cycle-guard via visited-paths set.

### Added — CLI flags
- **`neurogrim agent --prose --all-domains`** (F4) — list every
  declared domain in the prose signals section instead of capping
  at top 3. Auto-expands when the Brain is all-advisory (the
  "strongest signals" framing is misleading when no domain has
  weight > 0). MCP `orient` tool has matching `all_domains: bool`.
- **`neurogrim skill new --stub`** (F2) — produces a minimal-but-routable
  skill: sensible-default frontmatter (no literal `TODO —` strings)
  + a single-paragraph body identifying the file as a stub. Routing
  index has something to match against immediately. Use this when
  the operator's intent is "scaffold stubs, fill bodies later."
- **`neurogrim init --description "<text>"`** (F8) — operator-supplied
  Brain description for `meta.description`. Replaces the generic
  "initialized via `neurogrim init` ..." boilerplate when bespoke
  framing is needed.
- **`neurogrim init --domain-describe "NAME=DESCRIPTION"`** (F10) —
  repeatable. Each entry becomes a `_todo_<name>` field on the
  domain's definition, capturing operator-supplied sensor authoring
  intent for when the sensor is later written.
- **`neurogrim domain new --sensor-intent "<text>"`** (F10) — same
  but for adding domains one-at-a-time after init. MCP `domain_new`
  tool has matching `sensor_intent: Option<String>`.

### Added — automatic transitive port allocation
- **`neurogrim federation register` allocator walks transitively** (F7).
  The v3.2.2-era port-8423 collision (job-hunt allocated to a port
  already used by python-starter — the ecosystem's grandchild) is no
  longer possible. The allocator reads each peer's registry from disk
  and considers the full transitive port set when picking the next
  free slot.

### Added — bundled skill
- **`cli-mode` ships in `init-skills/`** (F5) — the `neurogrim-onboarding`
  skill referenced `cli-mode` but it wasn't bundled, producing a
  Read-fail for any agent following the reference. v3.3 ships it
  in the abstract-project / code-project / mixed templates.

### Changed
- **`init` no longer logs two `.gitignore` updates** (F6) — when
  `--template` is set, the registry-phase awareness-only update is
  skipped because the template's gitignore-snippet covers the same
  entry. Single update operation, single log line.
- **`validate` surfaces autonomy-block counts** — adds `Autonomy: N
  levels, M action_types, K safety_invariants` to the summary so
  operators can see at a glance whether they have safety declarations.
- **Workspace `version` 3.2.2 → 3.3.0** across all 6 crates +
  `[workspace.dependencies]` synchronized.

### Background — what prompted this release

A fresh Claude Code session re-bootstrapped the job-hunt project
end-to-end via the published v3.2.2 CLI (`cargo install neurogrim-cli`).
The agent's adoption-feedback report surfaced six friction points
(F1–F6); the operator-side audit added four more (F7–F10) including
the F7 port collision. Per-finding rationale is in
`audit/job-hunt-rebootstrap-prompt-2026-04-28.md` and the
`adoption-feedback-2026-04-28.md` report at
`D:/job-hunt/archive/adoption-feedback-2026-04-28.md`.

## [3.2.2] - 2026-04-28

*Publish-prep release. No new features; closes the last `cargo publish`
blockers identified in the v3.2.1 audit.*

### Changed
- Workspace `version` bumped from `3.0.0` (stale through v3.1, v3.1.1,
  v3.2, v3.2.1) to `3.2.2`. Inter-workspace deps in
  `[workspace.dependencies]` synchronized.
- **Schema vendoring**: 5 `include_str!` references in `neurogrim-a2a`
  and `neurogrim-sensory` previously pointed at
  `../../../../../LSP-Brains/schemas/*.schema.json` (sibling-repo
  relative), which broke `cargo publish` since LSP-Brains isn't part of
  the published tarball. Schemas are now vendored into each consuming
  crate's `data/schemas/` directory; canonical source remains in the
  LSP-Brains repo, drift caught by existing schema-conformance tests.
  - `neurogrim-a2a/data/schemas/a2a-federated-pattern-v1.schema.json`
  - `neurogrim-sensory/data/schemas/hat-contract-v1.schema.json`
  - `neurogrim-sensory/data/schemas/invocation-ledger-v1.schema.json`
  - `neurogrim-sensory/data/schemas/pattern-aggregation-ledger-v1.schema.json`
  - `neurogrim-sensory/data/schemas/trust-budget-v1.schema.json`

### Added
- Crate-level rustdoc on `neurogrim-core/src/lib.rs` covering each
  public module's role + stability stance.

## [3.2.1] - 2026-04-28

*Closes the two real onboarding gaps from the v3.2 audit: MCP
exposure of the new commands + propagating the bundled
`neurogrim-onboarding` skill to existing federation Brains.*

### Added
- **Four new MCP tools** mirroring the v3.2 CLI commands so agents
  using `neurogrim serve` (the default tool-invocation surface) can
  reach onboarding entry points without bash:
  - `orient` — agent-friendly prose summary (= `agent --prose`)
  - `doctor` — config audit returning structured JSON findings
  - `explain` — bundled methodology primer (8 topics)
  - `domain_new` — domain scaffolder
- `neurogrim-onboarding` skill propagated byte-identical across all
  6 federation copies (canonical bundled source +
  ecosystem/NeuroGrim/LSP-Brains/python-starter/job-hunt).

### Changed
- **Architecture refactor**: shared logic moved from `neurogrim-cli`
  to `neurogrim-mcp` so MCP tools and CLI commands use a single
  source of truth — new modules `mcp::prose`, `mcp::doctor`,
  `mcp::explain`, `mcp::domain`. The 8 `data/explain/*.md` files
  relocated to `neurogrim-mcp/data/explain/` accordingly.
- `neurogrim-cli/src/output/prose.rs`, `commands/doctor.rs`,
  `commands/explain.rs`, `commands/domain.rs` are now thin clap +
  printing wrappers.

## [3.2.0] - 2026-04-28

*Agent Onboarding & Domain Authoring campaign. Three workstreams
(Phase A introspection, Phase B methodology primer, Phase C domain
scaffolder) close the entry-point gap for AI agents on first contact
with a NeuroGrim project.*

### Added — Phase A: Brain introspection
- **`neurogrim agent --prose [--plain]`**: agent-friendly prose
  orientation summary. 8 sections — Brain identity, current state,
  strongest signals, calls to action, available skills, available
  hats, federation peers, footer. All-advisory Brains render
  "Score: N/A (observe-only posture)" rather than a misleading 0/100.
- **`neurogrim doctor [--plain]`**: read-only configuration auditor
  distinct from `validate` (registry-shape only) and `health`/`score`
  (run scoring pipeline). Six check families: schema validate,
  domain-definitions alignment, principle_map alignment, CMDB path
  resolution, culture.yaml presence, federation port uniqueness.
  Exit 0/1/2 by severity; advisory-orphan vs weighted-orphan severity
  split caught NeuroGrim's own pre-existing `_todo_rust-health`
  placeholder as a warn (not error).

### Added — Phase B: Methodology primer
- **`neurogrim explain <topic>`**: 8 bundled topic files
  (methodology, domain, sensor, hat, scoring, federation, cli,
  culture). Loaded via `include_str!` at compile time; ~80 KB binary
  growth. With no topic, lists topics with one-line summaries. With
  `--version`, prints bundle metadata.
- **Bundled `neurogrim-onboarding` skill** in `init-skills/`. Every
  new project from `init --template` gets it automatically; routing
  frontmatter triggers on "what is this Brain", "where do I start",
  "I just entered this project", etc.
- `tests/methodology_drift.rs`: 5 integration checks verifying bundled
  topic files are well-formed (presence, version-header uniformity,
  substantive content, valid command references).
- `NeuroGrim/CLAUDE.md` gains a "Getting Oriented" section pointing
  at the four onboarding commands.

### Added — Phase C: Domain scaffolder
- **`neurogrim domain new <name>`**: scaffolder mirroring `skill new`
  UX. Mutates `brain-registry.json` atomically across 3 sections
  (`domain_weights`, `principle_map`, `domain_definitions`), generates
  a stub CMDB, and optionally scaffolds a Python sensor skeleton at
  `sensory/check_<name>.py`. Idempotent re-registration with `--force`.
- `--type stub|python` (default `stub`); `--type rust` is intentionally
  unsupported (contributor work, not adopter work).
- 7 subprocess integration tests covering stub path, python path,
  force, kebab-case validation, post-mutation `validate` + `agent --prose`.

## [3.1.1] - 2026-04-28

*Init automation. The 50-step manual sibling-Brain onboarding from
v3.1 B'1 collapses to a few CLI commands using bundled templates.*

### Added
- **`neurogrim init --template <kind>`**: full Brain-integration
  scaffolding beyond the registry. Three templates: `abstract-project`
  (no primary code), `code-project` (software project; default
  detection), `mixed`. Generates `culture.yaml`, stub CMDBs,
  bundled skills, PostToolUse hook script, `CLAUDE.md`, `.gitignore`
  extension.
- **`neurogrim skill new <name>`**: scaffolds a project-specific
  `SKILL.md` skeleton. kebab-case validated; idempotent with `--force`.
- **`neurogrim federation register --name <peer> --path <path>`**:
  adds a child Brain to local federation. Auto-allocates the next
  unused A2A port from 8421. `--read-only` for sibling-project peers
  (bumps registry schema_version 2 → 2.1 if needed).
- **Bundled artifacts** at compile time: 3 init templates with
  manifests, 6 general-purpose skills (hats, imagination-mode,
  north-star, rubber-duck, human-comms, write-skill), culture.yaml,
  hook script, narration templates.
- 6 subprocess integration tests covering the full bootstrap flow.

## [3.1.0] - 2026-04-28

*"Activate the Grammar" campaign. Five workstreams (A–E) take the
v3.0 structurally-complete grammar and put it to work.*

### Added — Workstream A: Sensor authoring
- **`rust-health` sensor** at `neurogrim-sensory/src/rust_health.rs`:
  static-only signals — Cargo.toml, Cargo.lock, MSRV, rustfmt,
  clippy config, cargo-deny, `[lints]`, CI integration.
- PostToolUse invocation-ledger hook enabled in all 4 federation
  Brains (was: only ecosystem). 30-day calibration window opens.

### Added — Workstream B: Sibling project as observed peer
- `read_only: true` flag on registry `config.children` entries
  (LSP-Brains schema additive v2 → v2.1, commit 9cb83cf).
- `check_culture_coherence.py` extended to discover the federation
  set dynamically (was: hardcoded 4-path list, commit 6be88da).
- `check_observed_peers.py` ecosystem-level sensor reads sibling
  scores via A2A.
- B'1 Pilot #1: job-hunt registered as the 5th Brain (read-only
  sibling, port 8424).

### Added — Workstream C: Hat-calibrated narration
- **`neurogrim narrate --hat <name>`**: 3–5 lines of
  hat-templated prose. 7 declared hats; deterministic templates
  (no LLM) loaded from bundled TOML.

### Added — Workstream D: Spec dogfooding
- 11 missing Appendix E glossary entries authored in
  `LSP-BRAINS-SPEC.md` (`glossary-freshness` score 37 → 84).

### Added — Workstream E: Federated patterns intelligence
- `cross_peer_co_occurrence` finding kind in
  `neurogrim-sensory/src/federated_patterns.rs`. Detection: ≥2
  distinct anonymized origins emitting semantically-similar feature
  vectors within a 7-day window.

## [3.0.0] - 2026-04-27

*Stable consolidated release. Closes the supply-chain campaign
(E-SC-0..E-SC-10) + the Brains-2.0 self-observability campaign
(E-B2-0..E-B2-8). Both master gates 11 + 12 in `BEFORE-PUBLIC-
RELEASE.md` are 🟢; remaining 🟡 gates are operator-controlled.
`cargo publish` is operator-decision per
`docs/publish-day-runbook.md`.*

The version jump from `0.1.0` (workspace `Cargo.toml` default) to
`3.0.0` reflects methodology maturity across stages S1–S10 + the
two post-S10 master-gate campaigns (supply-chain + self-observability).
The intermediate `3.0.0-rc.1` plan was paused 2026-04-24 to ship
the supply-chain master gate first; that plan's content is folded
into this stable release alongside the Brains-2.0 work that
followed.

### Added — Core implementation
- **Rust workspace** (`neurogrim/crates/*`): `neurogrim-core` (pure
  scoring, zero I/O), `neurogrim-sensory` (12 built-in sensor
  domains), `neurogrim-mcp` (MCP server + client), `neurogrim-a2a`
  (peer protocol), `neurogrim-cli` (binary entry point).
- **12 sensor domains**: `git-health`, `test-health`, `code-quality`,
  `deploy-readiness`, `security-standards`, `coherence`,
  `human-comms`, `secret-refs`, `docker-topology`, `agent-behavior`,
  `skill-coherence`, `capability-hygiene`.
- **Correlation engine** with condition-tree operators
  (comparison + branch) evaluated against domain variables.
- **Unified scoring** with per-domain weights + confidence model +
  floor constraints + non-linear aggregation. Trajectory intelligence
  (velocity / acceleration) from ledger history.
- **Dual tool-invocation modes**: MCP server (`neurogrim serve`,
  ~983 tokens at session start) and CLI-only (0 tokens; opt in via
  the `cli-mode` skill).

### Added — LSP Brains spec v2.5
- 15 normative sections + 7 appendices.
- Covers: Brain architecture, registry schema, CMDB envelope, scoring
  model, correlation engine, MCP + A2A protocols (§13), agent-behavior
  verification (§15), domain promotion path (§15.5).
- Companion `METHODOLOGY-EVOLUTION.md` with 14 discovery-log entries
  tracking how the spec got here.

### Added — Skill + hat system
- **20 plugin-format skills** (`.claude/skills/<name>/SKILL.md` with
  YAML frontmatter). `capability-hygiene` domain scores authoring
  quality against a 1,536-char description+when_to_use budget.
- **7 hats**: adversary, architect, incident-commander, rubber-duck,
  security-auditor, visionary, source-reader.
- **Culture substrate**: 5-value invariant
  (positivity / integrity / honesty / critical-but-kind / respect),
  byte-identical across peer Brains, enforced by `culture-coherence`
  at the ecosystem level.

### Added — Governance infrastructure
- **Axis 4 v1 invocation ledger**: PostToolUse hook captures every
  `Skill` tool invocation (name + timestamp only — privacy by
  design). `capability-hygiene` classifies skills as alive / dead /
  new against a 30-day grace period.
- **Gated domain promotion**: `abv-run promote` / `rollback` /
  `promotion-watch` with append-only `domain-promotion-ledger.jsonl`,
  three rebalance strategies (proportional / explicit / refuse), and
  `ABV_OPERATOR` guard. Stage 10 spec §15.5 normative.
- **Judge-integrity ledger**: red-sample calibration gate with
  triage CLI (`abv-run judge-integrity list | triage`).
- **Red-mode sweeps**: mock-bad-agent generation +
  13-sample / 6-scenario failure-mode library.

### Added — Peer + adoption topology
- **A2A peer protocol** (spec §13): agent card + envelope + task
  client/server. Fractal composition (parent↔child) and dual-brain
  (local↔external) topologies demonstrated across the four-Brain
  ecosystem.
- **Ecosystem Brain** (`.claude/`): six advisory domains
  (spec-impl-alignment, terminology-coherence, protocol-boundary,
  north-star-alignment, ecosystem-trajectory, culture-coherence).
- **Python starter template** (`NeuroGrim-python-starter/`): child
  Brain with 4 advisory domains, demonstrating the adoption pattern.

### Added — Experimental evidence base
- 432-row `comparison-ledger.jsonl` from the 2026-04-22/23
  brain-vs-control experiment (Phases 1-3, plus 22-task held-out
  set). All pre-registered; falsification criteria locked before
  analysis; kill decisions honored. Reports, ledgers, and
  post-mortem at `.claude/experiments/brain-vs-control/`.
- **Evidence + Hypothesis posture** (ROADMAP): longitudinal value
  is the primary hypothesis; single-turn benchmarks are bounded
  instruments. METHODOLOGY-EVOLUTION §14 absorbs this honestly.

### Added — Adoption surface
- `docs/getting-started.md`: ~20-minute path from clone to working
  Brain.
- `examples/hello-brain/`: minimal standalone demo.
- Ecosystem + NeuroGrim + LSP-Brains `LICENSE` files (MIT).
- Release notes + publish-day runbook + prepublish-check script.

### Added — Supply-chain master gate (E-SC-0..E-SC-10, 2026-04-26)

- **Three-layer SCA awareness** across Rust + Python + Node ecosystems:
  Layer 1 mechanical SCA (native-Rust, no scanner-binary shell-outs);
  Layer 2 deep-signal vigilance (7 sub-sensors: typosquat,
  publish-cadence, maintainer-delta, transitive-surface, signature-gap,
  binary-reproducibility, exfil-indicator); Layer 3 agent-assisted
  human review framework (decision ledger + review tickets +
  auto-create bridge + `supply-chain-auditor` hat).
- **Spec normative**: LSP-Brains v2.6 §16 + METH-EV §15 + 2 new schemas
  (decision-ledger-v1, review-ticket-v1) + A2A enum extensions for
  `supply-chain-signal`.
- **Calibration framework**: fixture library + `sca-calibrate` CLI +
  `--check-promotion-ready` gate. v1 calibration:
  pass-with-sample-size-warning across all three layers;
  promotion-not-ready (gaps documented).
- **`prepublish-check.sh`** extended with strict-with-bypass for L2 +
  L3 + LiteLLM-equivalent fresh-OSV-rerun.
- **`publish-day-runbook.md`** documents the supply-chain rollback
  window between tag and publish.
- **Master gate 11** in `BEFORE-PUBLIC-RELEASE.md` 🟢.

### Added — Brains-2.0 self-observability master gate (E-B2-0..E-B2-8, 2026-04-27)

- **E-B2-1 Confidence as first-class envelope field** (spec §3.8) —
  numeric integer 0–100 at protocol; categorical (low/medium/high) at
  UI only.
- **E-B2-2 Self-coherence + domain-calibration ledgers** (spec §17) —
  one ledger per domain family at
  `.claude/brain/<domain>-calibration-ledger.jsonl`.
- **E-B2-3 Hat-as-formal-contract** (spec §5.4.1) — closed-set
  vocabulary + new `hat-contract-v1.schema.json` + per-hat
  frontmatter migration. Static (file audit) at v1; runtime checks
  deferred to v2 (BACKLOG B-23).
- **E-B2-4 Trust-budget primitive** (spec §16.8) — per-Brain
  `trust-budget.toml` declares allowed crates / shell-outs / external
  services. Soft (advisory) at v1; hard gates deferred to v2.
- **E-B2-5 METH-EV §16 multi-round assessment cadence** (METH-EV §16) —
  strict bar → surgical bar → diminishing-returns + Phase 1.5 escape
  hatch. RECOMMENDED for pre-release / epic-close-out contexts.
- **E-B2-6 Operator-calibration domain** (spec §17.12) — extends
  invocation-ledger schema with additive `disposition` field
  (accept/reject/modify; no transcript content). Aggregation-only
  export.
- **E-B2-7 Federated patterns A2A** (spec §16.6.1) — new
  `federated-pattern` A2A message type + `pattern-aggregation-ledger.jsonl`.
  Bidirectional opt-in posture; closed-set numeric-only feature
  vector; recursion guard at wire + source level; per-peer rate
  limit; aggregation-only export.
- **E-B2-8 Dogfooding + spec v3.0 stability marker** — all 4 Brains
  declare the 4 new advisory domains at weight 0.0; CMDBs present +
  schema-valid; cross-Brain federated-pattern integration test
  compiles + passes; hat-contract migration applied to LSP-Brains
  (2 hats: spec-editor, rubber-duck) + python-starter (2 hats:
  adopter, rubber-duck) extending NeuroGrim + ecosystem (8 hats each);
  `prepublish-check.sh` extended with strict gate-12 checks
  (CMDB-presence + advisory-weight invariant + cross-Brain integration).
- **Spec promoted v2.6 → v3.0** progressively (v2.7→v2.12 → v3.0
  stability marker). v3.0 = additive over v2.x; deprecation track
  deferred to v4.0 (no symbols deprecated, removed, or withdrawn).
- **Charter Amendment 2026-04-27** reframes the ≥30-day self-coherence
  + ≥50 operator-calibration record metrics from "before v3.0" to
  post-publish observation feeding a v3.1 calibration-report gate
  (mirrors gate-11 supply-chain "pass-with-sample-size-warning"
  precedent). See `audit/BRAINS-2-0-CHARTER.md` Charter Amendment +
  `audit/BRAINS-2-0-RETROSPECTIVE-2026-04-27.md`.
- **Master gate 12** in `BEFORE-PUBLIC-RELEASE.md` 🟢.

### Changed
- **Workspace version** `0.1.0` (default) → `3.0.0` final (intra-workspace
  dep pins also bumped from `3.0.0-rc.1` to `3.0.0`).
- **Spec header** v2.12 → v3.0 (stability marker; `Status: Active` →
  `Status: Stable v3.0`).
- **Top-level pre-release status** in `BEFORE-PUBLIC-RELEASE.md` 🔴 → 🟢
  (both master gates closed; remaining 🟡 gates are operator-controlled).

### Calibration window
- v3.0 ships the structural surface for the seven Brains-2.0 primitives
  without 30-day self-coherence + 50 operator-calibration records (per
  Charter Amendment 2026-04-27). The post-publish observation window
  feeds a v3.1 calibration-report gate. v3.0.x bug-fix releases may
  flow during the window without re-opening the master gate; v3.1.0
  ships when the calibration-report gate closes.

### Known open gates (operator-controlled)
See `BEFORE-PUBLIC-RELEASE.md` for the full status; short form:
- 🟡 Legal / trademark formal clearance.
- 🟡 Per-crate README + `cargo package --list` inspection.
- 🟡 CONTRIBUTING + per-crate rustdoc.
- 🟡 CI matrix enablement.
- ⚪ **PyPI publish — no current plan.** The Python SDK is
  dogfood-only per the 2026-04-24 Python SDK reframe. BACKLOG B-20
  tracks the dormant roadmap item; source install via `pip install
  -e sdk-python/` is the supported path for adopters who need
  Python. See [`docs/sdk.md`](docs/sdk.md) for the canonical Rust
  SDK story.

### Known deferred to post-publish
- **S5-TP-3** (team outside LaaS adopts the framework): re-framed as
  a post-publication milestone rather than a release blocker. v3.0.0
  ships the adoption surface; adopter-found is a separate track.
- **S10-DP-4** (agent-behavior weight flip 0.0 → 0.05): operator-
  gated on calibration + red-mode audit. Mechanism complete; flip
  ships when the operator runs the audit.
- **S7-ABV-6** worked-example first real-credential run: illustrative
  `+18` delta documented; ships with that caveat.
- **B-14 through B-19** (CANDIDATE BACKLOG items): dispatch rule
  generalization, content freshness, L2 synthesis, rubric
  sensitivity, longitudinal artifacts — all tracked, none committed.

### Known not in this release
- Python SDK on PyPI (gate 7; package-name reserved but not published).
- S6-DB-6 Python SDK A2A helper (stretch-only).
- Any claim that single-turn experiments prove the Brain's
  longitudinal value (see METH-EV §14 on instrument bounds).

---

## Release-note links

- Full release notes for this version: `docs/release-notes/v3.0.0.md`.
- Publish-day runbook: `docs/publish-day-runbook.md`.
- Pre-publish status tracker: `BEFORE-PUBLIC-RELEASE.md`.
- Spec changelog (per-version normative diff): `D:/Brains/LSP-Brains/spec/LSP-BRAINS-SPEC.md` § Changelog.
- Methodology evolution log (per-insight discovery history): `D:/Brains/LSP-Brains/spec/METHODOLOGY-EVOLUTION.md`.
