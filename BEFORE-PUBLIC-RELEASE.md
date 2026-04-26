# Before Public Release — Pre-Publish Readiness Checklist

**Status: 🔴 v3.0-rc.1 blocked on supply-chain security work.**
The adoption surface is in place (getting-started, examples,
whitepaper §11, CHANGELOG, LICENSE, README entry-ramps); the
publication mechanics are prepared (prepublish-check script,
metadata pass, CI-matrix draft); the publish-day runbook is
written. **What changed on 2026-04-24:** a new master gate — the
eleven-epic supply-chain security scaffolding (E-SC-0 through
E-SC-10) — must close before any `cargo publish` runs. Phase 0
(self-audit baseline, E-SC-0) has already shipped and is green;
Layers 1-3 remain.

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
- Workspace version bumped to `3.0.0-rc.1` (previously `0.1.0`).
- Intra-workspace dependencies now carry both `path` AND
  `version = "3.0.0-rc.1"` — required for `cargo publish` to
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
      `3.0.0-rc.1`.
- [x] **`docs/getting-started.md`** — ~20-minute path from clone to
      working Brain.
- [x] **`examples/hello-brain/`** — minimal standalone demo
      (`brain-registry.json`, `src/main.py`, `tests/test_main.py`,
      `README.md`).
- [x] **Whitepaper refresh** — `whitepaper/WHITEPAPER.md` §11
      Evidence Posture added; Appendix C references updated.
- [x] **Release notes** — `docs/release-notes/v3.0-rc.1.md`.
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
| 5. Security (scanner-free SCA posture) | 🟡 — rolled into gate 11 below; gate 5 is now the narrower `cargo audit` / `cargo deny` / secret-scan / history-review subset |
| 6. Documentation | 🟡 → mostly green (per-crate rustdoc, CONTRIBUTING remain) |
| 7. PyPI publish | ⚪ **No current plan** (Python SDK is dogfood-only per 2026-04-24 reframe; see B-20) |
| 8. CI matrix | 🟡 (draft ready; operator enables) |
| 9. Ecosystem submodule posture | 🟢 |
| 10. Publish-day runbook | ✓ written (`docs/publish-day-runbook.md`) |
| **11. Supply-chain security (MASTER GATE)** | 🔴 **E-SC-0/1/2/3/4/5/6/7 GREEN; E-SC-8/9/10 pending.** No `cargo publish` until this closes. 11-epic scaffolding at `~/.claude/plans/parallel-hugging-eich.md`. Phase 0 self-audit baseline + Rust SDK reframe + native Layer-1 SCA sensor covering all three ecosystems (Rust crates.io + Python PyPI + Node npm/yarn/pnpm). **Layer 2 vigilance shipped** (E-SC-5): all 7 sub-sensors with 7-day registry cache + opt-in tarball-fetch + dogfood-tuned FP suppression. **Layer 3 review framework shipped** (E-SC-6): supply-chain-auditor hat + append-only decision ledger + review-ticket file format + supply-chain-review CMDB sensor + auto-create bridge from Layer 2 + sca-review create/list/resolve CLI. v1 has NO automated LLM invocation — agent_findings are operator-edited (LLM-as-judge piece is post-v1 follow-on). **Spec normative work also shipped** (LSP-Brains v2.6 §16 + METH-EV §15 + 2 new schemas + A2A enum extensions). All three layers normatively conformant per §16.2/.3/.4. Remaining: E-SC-8 (calibration), E-SC-9 (container docs), E-SC-10 (final dogfooding + publish gate). |

**Legend:** 🟢 closed · 🟡 partial / operator-action-pending · 🔴 open / blocking · ⚪ dormant (no current plan) · ✅ closed via snapshot · ✓ closed via deliverable

This document is the readiness source-of-truth. Every change to our
publish posture should update a checkbox here.
