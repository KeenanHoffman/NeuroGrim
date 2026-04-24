# Changelog

All notable changes to NeuroGrim + the LSP Brains specification live
here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed — v3.0-rc.1 publish gated on supply-chain work (2026-04-24)

Following a 2026-04-23 PyPI supply-chain incident (a second-order
scanner-chain compromise: a trojanized security-scanner binary
captured CI credentials, which were used to publish trojanized
releases of an otherwise-legitimate package), we adopted a stricter
publish posture:

- **No `cargo publish` until a full three-layer supply-chain
  awareness feature ships** and is running self-green against
  NeuroGrim's own dependency graph. Scope: native-Rust SCA (no
  external scanner-tool shell-outs), deep-signal vigilance,
  agent-assisted human review with a normative decision ledger.
- **Tag `v3.0.0-rc.1` is not created** until the supply-chain
  gate closes. The artifacts labeled `3.0.0-rc.1` below remain the
  target content for that release; they are not withdrawn.
- **Python SDK becomes dogfood-only**, not a shipped artifact.
  `neurogrim-core` + `neurogrim-sensory` are the canonical Rust
  SDK for downstream extension. See [`docs/sdk.md`](docs/sdk.md).
  BACKLOG B-20 (PyPI publish) is re-framed from "deferred pending
  incident" to "no current plan to publish."
- **Phase 0 self-audit baseline captured** (epic E-SC-0). Full
  transitive dependency graph (261 Rust + 32 Python packages)
  queried against OSV.dev; one active advisory remediated
  (`rustls-webpki` 0.103.12 → 0.103.13, RUSTSEC-2026-0104); one
  unmaintained-notice accepted with escalation trigger
  (`paste` 1.0.15 via rmcp; transitive; proc-macro-only). Full
  audit artifacts committed at `D:/Brains/audit/` in the ecosystem
  repo.

Why this is captured in the CHANGELOG rather than a release note:
no release has shipped. When v3.0-rc.1 ultimately tags, its
release-notes section will subsume this block as the "why
publishing was delayed" context.

Full scaffolding of the supply-chain work (11 epics, ~8-12 weeks):
`audit/ROLLBACK-PLAYBOOK.md` and the ecosystem's plan at
`~/.claude/plans/parallel-hugging-eich.md`.

## [3.0.0-rc.1] — intended; NOT YET TAGGED

*Originally scoped for 2026-04-23; retained here as the target
content for the tag once the supply-chain gate closes. See
`[Unreleased]` above for the current posture.*

The version jump from `0.1.0` (workspace `Cargo.toml` default) to
`3.0.0-rc.1` reflects methodology maturity across stages S1–S10
rather than a tagged-release history (no prior version was ever
published to crates.io or PyPI).

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

### Known open gates
See `BEFORE-PUBLIC-RELEASE.md` for the full status; short form:
- 🟡 Legal / trademark formal clearance (operator-led).
- 🟡 Cargo dry-run on final metadata.
- 🟡 Metadata completeness pass.
- 🔴 **Supply-chain security (SCA Layers 1+2+3 shipped + self-
  green against own deps).** **This is now the master gate for
  v3.0-rc.1 publish** per the 2026-04-24 scaffolding. Scope:
  three-layer (mechanical / vigilance / agent-assisted review)
  across Rust + Python + Node, with a normative decision ledger
  and a `supply-chain-auditor` hat. See the scaffolding plan and
  `audit/ROLLBACK-PLAYBOOK.md`. Phase 0 (self-audit baseline) has
  shipped; Layers 1-3 are E-SC-2 through E-SC-6.
- 🟡 Documentation (this release closes most of this gate).
- ⚪ **PyPI publish — no current plan.** The Python SDK is
  dogfood-only per the 2026-04-24 Python SDK reframe. BACKLOG B-20
  tracks the dormant roadmap item; source install via `pip install
  -e sdk-python/` is the supported path for adopters who need
  Python. See [`docs/sdk.md`](docs/sdk.md) for the canonical Rust
  SDK story.
- 🟡 CI matrix enablement.

### Known deferred to post-RC
- **S5-TP-3** (team outside LaaS adopts the framework): re-framed as
  a post-publication milestone rather than a release blocker. v3.0-rc
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

- Full release notes for this version: `docs/release-notes/v3.0-rc.1.md`.
- Publish-day runbook: `docs/publish-day-runbook.md`.
- Pre-publish status tracker: `BEFORE-PUBLIC-RELEASE.md`.
- Spec changelog (per-version normative diff): `D:/Brains/LSP-Brains/spec/LSP-BRAINS-SPEC.md` § Changelog.
- Methodology evolution log (per-insight discovery history): `D:/Brains/LSP-Brains/spec/METHODOLOGY-EVOLUTION.md`.
