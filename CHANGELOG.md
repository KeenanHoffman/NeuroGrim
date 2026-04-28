# Changelog

All notable changes to NeuroGrim + the LSP Brains specification live
here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
