---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Before Public Release — Pre-Publish Readiness Checklist

**Status: 🟢 v3.0.0 stable — both master gates closed; remaining gates are operator-controlled.**
Master gate 11 (supply-chain security, E-SC-0..E-SC-10) closed
2026-04-26. Master gate 12 (Brains-2.0 self-observability,
E-B2-0..E-B2-8) closed 2026-04-27 with this release. Remaining
🟡 gates (1 trademark, 4 per-crate README, 6 CONTRIBUTING +
rustdoc, 8 CI matrix) are operator-controlled and do not block
the technical readiness of the release; the operator decides
when to flip them as part of the publish-day runbook. **`cargo
publish` is now an operator-decision per
`docs/publish-day-runbook.md`.** The adoption surface is in
place (getting-started, examples, whitepaper §11, CHANGELOG,
LICENSE, README entry-ramps); the publication mechanics are
prepared (prepublish-check script, metadata pass, CI-matrix
draft); the publish-day runbook is written.

**Posture:** `cargo publish` is *irrevocable* — published crate
names cannot be reused, yanking a version does not free the name,
and third-party tooling caches versions long after yank. This
document exists so that when we publish, every gate below has been
closed intentionally.

**Last refresh:** 2026-04-24 (post-E-SC-0 + SCA-master-gate
adoption; PyPI re-framed from "deferred post-incident" to "no
current plan to publish" per the Python-SDK-is-dogfood-only
decision).

---

## 1. Legal / naming gate 🟡

**Historical context (closed):** The original name for this project
was "Motherbrain," which surfaced two prior-use conflicts —
**EQT Motherbrain** (Swedish PE firm's AI deal-sourcing platform,
same software/AI class) and **Mother Brain / Motherbrain** (Nintendo
*Metroid* character, entertainment class). The project rebranded to
**NeuroGrim** on 2026-04-19. Every code/docs reference was swept in
the same session.

**Remaining work (operator-controlled):** An informal Google/GitHub
search on "NeuroGrim" came up empty, but informal is not dispositive.

- [ ] **USPTO TESS search** on "NeuroGrim" across classes 9
      (software), 42 (SaaS), and adjacent classes.
- [ ] **Common-law search** — Google, GitHub, Twitter/X, LinkedIn,
      product directories — for active unregistered use.
- [ ] **Trademark attorney consult** — 30-minute initial review,
      written opinion on clearance of "NeuroGrim" for this use case.
- [ ] **Domain ownership** — any `.com`/`.io`/`.dev`/`.ai` domains
      claimed for the final mark.
- [ ] **Social handles** — GitHub org (if separate from personal),
      Twitter/X, Bluesky, etc. reserved.

---

## 2. Name availability on package registries ✅ (snapshot)

Snapshot taken 2026-04-17. All names were AVAILABLE as of that date.
**Re-check immediately before any real publish** — squatters can
claim names at any time.

### crates.io

| Crate | Status (2026-04-17) |
|-------|---------------------|
| `neurogrim` | AVAILABLE |
| `neurogrim-core` | AVAILABLE |
| `neurogrim-cli` | AVAILABLE |
| `neurogrim-a2a` | AVAILABLE |
| `neurogrim-sensory` | AVAILABLE |
| `neurogrim-mcp` | AVAILABLE |
| `neurogrim-ecosystem` | AVAILABLE |

The CLI binary is named `neurogrim`; the crate package name is
`neurogrim-cli`. Reserving plain `neurogrim` as an empty re-export
crate is a decision tracked in the publish-day runbook.

### PyPI

| Package | Status (2026-04-17) |
|---------|---------------------|
| `lsp-brains` | AVAILABLE |
| `lsp_brains` | AVAILABLE |
| `neurogrim` | AVAILABLE |

- [ ] **Re-check the day of publish** — names can be claimed at
      any time.

---

## 3. Cargo build + dry-run gate 🟡

**Build + test verified (current):**
- Workspace version bumped to `3.0.0` (previously `0.1.0`; the
  intermediate `3.0.0-rc.1` plan was paused so the supply-chain
  master gate could ship first, then consolidated into stable
  `3.0.0`).
- Intra-workspace dependencies now carry both `path` AND
  `version = "3.0.0"` — required for `cargo publish` to
  resolve deps from crates.io.
- `cargo check --workspace` clean; test suite green.

**Automated runner:** `scripts/prepublish-check.sh` runs the
mechanical pre-flight gates (version consistency, CHANGELOG entry,
LICENSE files, adoption surface, metadata completeness, cargo
check + test + dry-run + audit). Operator runs this before
`cargo publish`.

### Checkboxes (filled in when operator runs the sweep)

- [ ] `neurogrim-core` — dry-run exit 0, log attached
- [ ] `neurogrim-a2a` — dry-run exit 0, log attached
- [ ] `neurogrim-sensory` — dry-run exit 0, log attached
- [ ] `neurogrim-mcp` — dry-run exit 0, log attached
- [ ] `neurogrim-ecosystem` — dry-run exit 0, log attached
- [ ] `neurogrim-cli` — dry-run exit 0, log attached

---

## 4. Metadata gate 🟡 → mostly green

Closed by the Tier 2d pass (2026-04-24):

- [x] `homepage` added at workspace level
- [x] `documentation` added at workspace level
- [x] `readme = "../README.md"` at workspace level; each crate
      inherits via `readme.workspace = true`
- [x] `keywords` added: `["lsp-brains", "agent", "mcp", "devops",
      "observability"]`
- [x] `categories` added: `["development-tools",
      "command-line-utilities"]`
- [x] `rust-version = "1.75"` declared at workspace level
- [x] `repository` already pointed to canonical URL
- [x] `license = "MIT"` declared at workspace level
- [x] `LICENSE` file at repo root (MIT text)

Still pending:

- [ ] **Per-crate `README.md` files OR symlinks** — the workspace
      uses `readme = "../README.md"` and cargo packages it per
      crate. Verify via `cargo package --list -p <crate>` on each
      crate that the README is included. If cargo complains, add a
      short per-crate README.
- [ ] **`cargo package --list` inspection** — confirm each published
      tarball contains LICENSE + README.

---

## 5. Security gate 🔴

Operator-controlled. `cargo audit` integration is in
`scripts/prepublish-check.sh` and in the disabled CI matrix
(`.github/workflows/ci-matrix.yml.disabled`).

- [ ] **`cargo audit`** — zero vulnerabilities, or each finding
      triaged and documented.
- [ ] **`cargo deny check`** — license compatibility + ban-list +
      advisory check, zero failures.
- [ ] **Secret scanner sweep** — `trufflehog` or `gitleaks` against
      the full repo history, zero findings.
- [ ] **Git history review** — any `.env`, `*.pem`, `credentials*`,
      or API-key-looking commits audited; if found, history rewritten
      before public push.
- [ ] **Dependency surface review** — transitive-dep list reviewed,
      no abandoned or single-maintainer deps in hot paths.
- [ ] **`unsafe` audit** — grep for `unsafe` blocks, document each
      or eliminate.
- [ ] **Public-facing defaults** — no debug endpoints, no test
      creds, no localhost assumptions in CLI defaults.

---

## 6. Documentation gate 🟡 → mostly green

Closed by the Tier 1 / Tier 3 pass (2026-04-23/24):

- [x] **`CHANGELOG.md`** — keep-a-changelog format, entry for
      `3.0.0`.
- [x] **`docs/getting-started.md`** — ~20-minute path from clone to
      working Brain.
- [x] **`examples/hello-brain/`** — minimal standalone demo
      (`brain-registry.json`, `src/main.py`, `tests/test_main.py`,
      `README.md`).
- [x] **Whitepaper refresh** — `whitepaper/WHITEPAPER.md` §11
      Evidence Posture added; Appendix C references updated.
- [x] **Release notes** — `docs/release-notes/v3.0.0.md`.
- [x] **Root `README.md`** — `🚀 Getting started in ~20 minutes`
      entry-ramp above the fold.
- [x] **LSP-Brains `README.md`** — entry-ramp pointing back at
      NeuroGrim getting-started.
- [x] **Ecosystem `README.md`** — entry-ramp + whitepaper link +
      release status.
- [x] **Python starter `README.md`** — SDK-from-source framing
      (PyPI deferred per B-20).
- [x] **`INTRO.md` cross-link** — already in place (2026-04-17).
- [x] **Removed references** — all "Sparq", "Motherbrain",
      internal-hostname references swept.

Still pending:

- [ ] **Root `README.md`** install instructions verified from a
      clean machine (part of "someone outside can actually use
      this" — S5-TP-3 reframed to post-publication but this
      sub-check is still a belt-and-suspenders).
- [ ] **Per-crate docs** — `cargo doc` builds cleanly, no broken
      intra-doc links, all public items have at least a one-line
      rustdoc.
- [ ] **`CONTRIBUTING.md`** — file does not currently exist.
      Needed for any public repo accepting contributions.
- [ ] **`docs/adopting.md`** — "how do I actually use this in my
      project?" walkthrough beyond getting-started (the Python
      starter is the fork-target).

---

## 7. PyPI gate ⚪ **No current plan to publish**

**Status update 2026-04-24:** Re-framed from "deferred post-
incident-review" to **"no current plan to publish."** The
ecosystem's canonical SDK for downstream extension is
Rust — `neurogrim-core` + `neurogrim-sensory`. See
[`docs/sdk.md`](docs/sdk.md) for the full framing.

The Python SDK (`lsp_brains` under `sdk-python/`) remains in-repo
as dogfood / internal example / adopter convenience. Install is
source-only: `pip install -e NeuroGrim/sdk-python/`. The package
name is reserved but not published.

BACKLOG **B-20** is now dormant, not active. It has
**reactivation triggers** rather than "when this gate reopens"
conditions — see B-20 in `roadmap/BACKLOG.md`.

Summary of what B-20 activation would require (abbreviated; full
list in BACKLOG):

1. Concrete user demand not servable by the Rust SDK + source-
   install Python SDK.
2. PyPI's trusted-publishing / attestation / SBOM story matures.
3. Our native-Python SCA (E-SC-3) reaches Layer 2+3 parity with
   Layer 1 + demonstrated calibration.
4. An operator-led decision to reverse the Rust-is-canonical
   choice.

None of the above are expected in the current v3.0 release track.

`scripts/prepublish-check.sh` skips the Python build step with an
explanatory message. `CHECK_PYTHON=0` is the steady-state value.

---

## 8. CI gate 🟡

Current state:

- [x] **`.github/workflows/ci.yml`** exists (ubuntu-only baseline:
      cargo fmt + clippy, cargo test --workspace, python pytest,
      docker compose smoke).
- [x] **`.github/workflows/ci-matrix.yml.disabled`** prepared
      (ubuntu × macos × windows, rust stable × 1.75, cargo audit
      weekly + on push/PR). Operator enables by renaming.

Still pending:

- [ ] **MSRV declared and tested** — `rust-version = "1.75"` now
      declared at workspace; CI matrix pins `1.75` as a test target
      when enabled.
- [ ] **Matrix CI on public repo** — operator decides when the
      GitHub repo visibility flips; CI matrix enables when it does.
- [ ] **Release workflow** — a manual-dispatch workflow that runs
      `cargo publish --dry-run` on every crate, ordered correctly.
      Optional but catches drift between local and CI envs. Current
      surrogate: `scripts/prepublish-check.sh` runs the same checks
      locally.

---

## 9. Ecosystem submodule posture 🟢

The parent ecosystem repo and Python starter submodule are
currently private. Public-release order matters:

- [x] **Visibility matrix documented** — ecosystem README notes
      `git clone --recursive` may require auth for private children.
- [ ] **Re-check submodule visibility at publish time.** If
      ecosystem goes public before children, operator updates
      `README.md` accordingly (or flips children to public first).

---

## 10. Publish-day runbook

Moved to its own document: **[`docs/publish-day-runbook.md`](docs/publish-day-runbook.md)**.
Do not run publish commands from memory; follow the runbook.

---

## Summary

**Gate counts (2026-04-24, post-SCA-adoption):**

| Gate | Status |
|---|---|
| 1. Legal / trademark | 🟡 (operator-controlled) |
| 2. Name availability snapshot | ✅ (re-check on publish day) |
| 3. Cargo dry-run | 🟡 (automated via prepublish-check.sh) |
| 4. Metadata completeness | 🟡 → mostly green (per-crate README verification remains) |
| 5. Security (scanner-free SCA posture) | 🟢 — supply-chain detail rolled into gate 11 below (Layers 1+2+3 + spec + calibration + container docs all shipped). Gate 5 narrowly covers the orthogonal `cargo audit` / `cargo deny` / secret-scan / history-review subset; those run via `prepublish-check.sh` and will be re-verified on publish day. The "scanner-free" posture (no Trivy / cargo-audit binary in primary scoring path; native-Rust SCA per spec §16.2) is closed. |
| 6. Documentation | 🟡 → mostly green (per-crate rustdoc, CONTRIBUTING remain) |
| 7. PyPI publish | ⚪ **No current plan** (Python SDK is dogfood-only per 2026-04-24 reframe; see B-20) |
| 8. CI matrix | 🟡 (draft ready; operator enables) |
| 9. Ecosystem submodule posture | 🟢 |
| 10. Publish-day runbook | ✓ written (`docs/publish-day-runbook.md`) |
| **11. Supply-chain security (MASTER GATE)** | 🟢 **E-SC-0..10 GREEN.** All 11 epics in the supply-chain scaffolding (`~/.claude/plans/parallel-hugging-eich.md`) shipped. **Layer 1** (mechanical SCA across Rust + Python + Node ecosystems) + **Layer 2** (deep-signal vigilance, 7 sub-sensors) + **Layer 3** (agent-assisted review framework: hat + decision-ledger + review-tickets + auto-create bridge) + **spec normative** (LSP-Brains v2.6 §16 + METH-EV §15 + 2 new schemas + A2A enum extensions) + **calibration framework** (fixture library + `sca-calibrate` CLI + `--check-promotion-ready` gate) + **container docs** + **publish-gate ratification** (E-SC-10): all 4 Brains declare the three supply-chain domains at advisory weight 0.0; `prepublish-check.sh` extended with strict-with-bypass for L2 + L3 + LiteLLM-equivalent fresh-OSV-rerun; `publish-day-runbook.md` documents the supply-chain rollback window between tag and publish. **`cargo publish` is now operator-decision** — the supply-chain side is closed. v1 calibration: pass-with-sample-size-warning across all three layers; promotion-not-ready (gaps documented). v2 candidates documented in `docs/publish-day-runbook.md`: cross-Brain A2A `supply-chain-signal` wire-up, fixture-library growth toward ≥30/layer, L3 human-agreement data collection, calibration-report schema spec-promotion to LSP-Brains v2.7. |
| **12. Brains-2.0 self-observability (MASTER GATE — v3.0 publish only)** | 🟢 **E-B2-0..E-B2-8 GREEN (2026-04-27).** Nine epics in the Brains-2.0 scaffolding shipped: charter (E-B2-0), confidence as first-class envelope §3.8 (E-B2-1), self-coherence + domain-calibration ledgers §17 (E-B2-2), hat-as-formal-contract §5.4.1 (E-B2-3), trust-budget primitive §16.8 (E-B2-4), METH-EV §16 multi-round assessment (E-B2-5), operator-calibration §17.12 (E-B2-6), federated patterns A2A §16.6.1 (E-B2-7), and the dogfooding + spec v3.0 stability-marker close (E-B2-8). Spec promoted v2.12 → v3.0 (stability marker; no section-content changes — all 5 normative sections were already at MUST/SHOULD/MAY in v2.7-v2.10). All 4 Brains dogfood green: each declares the 4 new Brains-2.0 advisory domains (`domain-calibration`, `trust-budget`, `operator-calibration`, `federated-patterns`) at weight 0.0; CMDBs present + parseable; cross-Brain federated-pattern integration test compiles + passes; hat-contract migration applied to LSP-Brains (2 hats) + python-starter (2 hats) extending NeuroGrim + ecosystem (8 hats each). `prepublish-check.sh` extended with strict gate-12 checks (CMDB-presence + advisory-weight invariant + cross-Brain-integration-test). v1 calibration: structural surface in place; the ≥30-day self-coherence + ≥50 operator-calibration record windows reframed by Charter Amendment 2026-04-27 as post-publish observation feeding a v3.1 calibration-report gate (mirrors gate-11 supply-chain "pass-with-sample-size-warning" precedent). Per-Brain dogfood-green checklist: see below. Charter: `audit/BRAINS-2-0-CHARTER.md`. Retrospective: `audit/BRAINS-2-0-RETROSPECTIVE-2026-04-27.md`. Rollback: `docs/v3-rollback.md`. |

### Gate 12 — Per-Brain dogfood-green checklist

| Brain | 4 Brains-2.0 CMDBs present | Advisory weight 0.0 | Schema-valid | Hat-contract conformance |
|---|---|---|---|---|
| Ecosystem (`D:\Brains\.claude\`) | ✓ | ✓ | ✓ | ✓ (8 hats) |
| NeuroGrim (`D:\Brains\NeuroGrim\.claude\`) | ✓ | ✓ | ✓ | ✓ (8 hats) |
| LSP-Brains (`D:\Brains\LSP-Brains\.claude\`) | ✓ | ✓ | ✓ | ✓ (2 hats: spec-editor, rubber-duck) |
| python-starter (`D:\Brains\NeuroGrim\NeuroGrim-python-starter\.claude\`) | ✓ | ✓ | ✓ | ✓ (2 hats: adopter, rubber-duck) |

The 4 Brains-2.0 CMDBs are: `domain-calibration-cmdb.json`, `trust-budget-cmdb.json`, `operator-calibration-cmdb.json`, `federated-patterns-cmdb.json`. The 3 supply-chain CMDBs (`supply-chain-sca`, `supply-chain-vigilance`, `supply-chain-review`) are gate-11's domain. The `rubber-duck.md` hat is byte-identical (md5 `6d1fb223ce6ac0d85d6b4d8b41c899d4`) across all 4 Brains, preserving the cross-repo invariant. Confidence-as-first-class (§3.8), hat-contract (§5.4.1), and METH-EV §16 are envelope/spec/methodology deliverables — no separate CMDB. `prepublish-check.sh`'s 3 new gate-12 functions enforce this checklist mechanically.

**Legend:** 🟢 closed · 🟡 partial / operator-action-pending · 🔴 open / blocking · 🔵 planned (active campaign in flight) · ⚪ dormant (no current plan) · ✅ closed via snapshot · ✓ closed via deliverable

This document is the readiness source-of-truth. Every change to our
publish posture should update a checkbox here.
