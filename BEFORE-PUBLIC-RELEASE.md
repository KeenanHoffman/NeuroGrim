# Before Public Release — Pre-Publish Readiness Checklist

**Status: 🟡 v3.0-rc.1 ready pending remaining open gates.** The
adoption surface is in place (Tier 1 complete: getting-started,
examples, whitepaper §11, CHANGELOG, LICENSE, README entry-ramps);
the publication mechanics are prepared (Tier 2 complete:
prepublish-check script, metadata pass, CI-matrix draft); the
publish-day runbook is written (`docs/publish-day-runbook.md`).
Operator-controlled gates remain below.

**Posture:** `cargo publish` is *irrevocable* — published crate
names cannot be reused, yanking a version does not free the name,
and third-party tooling caches versions long after yank. This
document exists so that when we publish, every gate below has been
closed intentionally.

**Last refresh:** 2026-04-24 (pre-publish walkthrough after Tier
1–3 landed, Tier 2 prepared, PyPI gate deferred post-incident).

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

## 7. PyPI gate 🔴 **DEFERRED post-incident-review**

A PyPI supply-chain incident in the 2026-04-23 window led us to
**pause this gate** pending incident review + supply-chain audit.
Tracked as candidate future work at **BACKLOG B-20**.

The Python SDK continues to ship as "install from source" via
`pip install -e NeuroGrim/sdk-python/` — see
`NeuroGrim-python-starter/README.md` and `docs/release-notes/
v3.0-rc.1.md` for the framing.

When this gate reopens (B-20 preconditions met):

- [ ] **PyPI incident post-mortem publicly available and understood.**
- [ ] **Supply-chain audit** covering SDK's transitive dependency
      graph (attestations / SBOM ideal).
- [ ] **2FA / trusted-publishing** in place for the publish
      credential.
- [ ] **`python -m build`** clean — wheel + sdist.
- [ ] **`twine check dist/*`** clean — metadata + long-description
      render.
- [ ] **PyPI name re-check on publish day.**
- [ ] **Decide: `lsp-brains` or `lsp_brains`** — PyPI normalizes
      both; import name is `lsp_brains`.
- [ ] **TestPyPI dry-run first** — upload, install, smoke test.
- [ ] **Version alignment** — `sdk-python/pyproject.toml` version
      matches (or consciously diverges from) Rust workspace version.

Until this gate closes, `scripts/prepublish-check.sh` SKIPs the
Python build step with an explanatory message. When re-enabling,
flip `CHECK_PYTHON=1` in that script.

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

**Gate counts (2026-04-24):**

| Gate | Status |
|---|---|
| 1. Legal / trademark | 🟡 (operator-controlled) |
| 2. Name availability snapshot | ✅ (re-check on publish day) |
| 3. Cargo dry-run | 🟡 (automated via prepublish-check.sh) |
| 4. Metadata completeness | 🟡 → mostly green (per-crate README verification remains) |
| 5. Security | 🔴 (operator-controlled) |
| 6. Documentation | 🟡 → mostly green (per-crate rustdoc, CONTRIBUTING remain) |
| 7. PyPI publish | 🔴 **DEFERRED post-incident-review (B-20)** |
| 8. CI matrix | 🟡 (draft ready; operator enables) |
| 9. Ecosystem submodule posture | 🟢 |
| 10. Publish-day runbook | ✓ written (`docs/publish-day-runbook.md`) |

This document is the readiness source-of-truth. Every change to our
publish posture should update a checkbox here.
