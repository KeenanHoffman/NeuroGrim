# Before Public Release — Pre-Publish Readiness Checklist

**Status: 🔴 NOT READY TO PUBLISH.** This document tracks the gates
that must close before running `cargo publish` or `twine upload` on
any artifact in this repository.

**Posture:** `cargo publish` is *irrevocable* — published crate names
cannot be reused, yanking a version does not free the name, and
third-party tooling caches versions long after yank. This document
exists so that when we do publish, every gate below has been closed
intentionally.

**Principle #2** of this project says scoring must be honest — that
includes scoring our own publish-readiness. Every unchecked box below
is a real gate, not a formality.

---

## 1. Legal / naming gate 🟡

**Historical context (closed):** The original name for this project
was "Motherbrain," which surfaced two prior-use conflicts —
**EQT Motherbrain** (Swedish PE firm's AI deal-sourcing platform,
same software/AI class) and **Mother Brain / Motherbrain** (Nintendo
*Metroid* character, entertainment class). The plain-text form and
space-variants were indistinguishable in trademark law, so the
project rebranded to **NeuroGrim** on 2026-04-19. Every code/docs
reference was swept in the same session — see the rebrand commits.

**Remaining work (open):** An informal Google/GitHub search on
"NeuroGrim" came up empty, but informal is not dispositive. A formal
check is still required before public release.

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

**Cross-reference:** The rebrand is documented in
`LSP-Brains/INTRO.md` — the "About the name" footnote reflects the
decided name.

---

## 2. Name availability on package registries ✅ (snapshot)

Snapshot taken 2026-04-17. All names are AVAILABLE as of this date —
this can change at any time; **re-check immediately before any real
publish.**

### crates.io

| Crate | Status |
|-------|--------|
| `neurogrim` | AVAILABLE |
| `neurogrim-core` | AVAILABLE |
| `neurogrim-cli` | AVAILABLE |
| `neurogrim-a2a` | AVAILABLE |
| `neurogrim-sensory` | AVAILABLE |
| `neurogrim-mcp` | AVAILABLE |
| `neurogrim-ecosystem` | AVAILABLE |

The CLI binary is named `neurogrim` (see
`crates/neurogrim-cli/Cargo.toml`). The crate package name is
`neurogrim-cli`. Reserving the plain `neurogrim` crate name (e.g.
as an empty placeholder that re-exports from `neurogrim-cli`) is a
decision to make in the publish-day runbook.

### PyPI

| Package | Status |
|---------|--------|
| `lsp-brains` | AVAILABLE |
| `lsp_brains` | AVAILABLE |
| `neurogrim` | AVAILABLE |

- [ ] **Re-check the day of publish** — names can be claimed by
      squatters at any time. A free account + `twine upload` of an
      empty package is all it takes.

---

## 3. Cargo build + dry-run gate 🟡

**Build + test verified (2026-04-19, post-rebrand):** Using
`stable-x86_64-pc-windows-gnu` toolchain with mingw-w64 at
`D:/mingw64/`, the renamed workspace compiles clean and the full test
suite passes:

- `cargo build --workspace` → zero errors, 10 warnings (pre-existing
  unused-import / dead-code, not caused by the rebrand)
- `cargo test --workspace --all-targets` → **201 tests passed, 0
  failed** across all suites (unit, CLI smoke, dual-brain pair,
  three-way brain, schema conformance, sensor behavior, ecosystem
  contract)
- `./target/debug/neurogrim.exe --version` → `neurogrim 0.1.0`
- `./target/debug/neurogrim.exe --help` → all commands listed under
  the new binary name

Build-example ordering note: 3 ecosystem contract tests depend on
`examples/stub_child_brain.exe` at a non-hashed path that `cargo test
--all-targets` doesn't populate automatically. Running `cargo build
--examples -p neurogrim-ecosystem` once before test runs is the
known workaround — pre-existing behavior, not a rebrand artifact.

Still pending for a real publish:

### Required commands (bottom-up, dependency order)

```bash
cd D:/Brains/NeuroGrim/neurogrim
mkdir -p .dry-run-logs
cargo publish -p neurogrim-core      --dry-run 2>&1 | tee .dry-run-logs/core.log
cargo publish -p neurogrim-a2a       --dry-run 2>&1 | tee .dry-run-logs/a2a.log
cargo publish -p neurogrim-sensory   --dry-run 2>&1 | tee .dry-run-logs/sensory.log
cargo publish -p neurogrim-mcp       --dry-run 2>&1 | tee .dry-run-logs/mcp.log
cargo publish -p neurogrim-ecosystem --dry-run 2>&1 | tee .dry-run-logs/ecosystem.log
cargo publish -p neurogrim-cli       --dry-run 2>&1 | tee .dry-run-logs/cli.log
```

### 🟡 Expected intra-workspace blocker

`neurogrim-core` has no intra-workspace dependencies and should
dry-run cleanly. The other five crates depend on `neurogrim-core`
(and transitively on each other) via `{ workspace = true }`. In the
workspace `Cargo.toml` the workspace-dep entries look like:

```toml
neurogrim-core = { path = "crates/neurogrim-core" }
```

— they have a `path` but no `version`. `cargo publish` requires
**both** for workspace members that depend on other workspace members,
so that the published crate can resolve its deps from crates.io. Dry
-run will fail on every crate except `neurogrim-core` until this is
fixed.

- [ ] **Add `version = "0.1.0"` to every intra-workspace dep** in the
      root workspace `Cargo.toml` (lines 54–59). Must match the
      `workspace.package.version`.
- [ ] **Re-run dry-run sweep.** All six must exit 0.

### Checkboxes (to be filled in when dry-run actually runs)

- [ ] `neurogrim-core` — dry-run exit 0, log attached
- [ ] `neurogrim-a2a` — dry-run exit 0, log attached
- [ ] `neurogrim-sensory` — dry-run exit 0, log attached
- [ ] `neurogrim-mcp` — dry-run exit 0, log attached
- [ ] `neurogrim-ecosystem` — dry-run exit 0, log attached
- [ ] `neurogrim-cli` — dry-run exit 0, log attached

---

## 4. Metadata gate 🟡

Workspace `Cargo.toml` `repository` now points to the canonical URL:
`https://github.com/KeenanHoffman/NeuroGrim` (updated during the
2026-04-19 rebrand). The stale `keenanHoffmanSparq` references were
swept in the same commit. Remaining metadata work:

- [ ] **Add `homepage`** field — either the repo or a dedicated site.
- [ ] **Add `readme = "README.md"`** at the workspace level (or per
      crate — cargo reads README from the crate dir).
- [ ] **Add `keywords`** — up to 5 per crate, lowercase, hyphenated.
      Draft: `["lsp-brains", "agent", "scoring", "observability",
      "ai"]`.
- [ ] **Add `categories`** — valid crates.io categories only. Draft:
      `["development-tools", "command-line-utilities"]`.
- [ ] **Per-crate `README.md` files** — each published crate needs
      its own README or a symlink to the root. cargo will warn
      otherwise.
- [ ] **License file present.** `license = "MIT"` declared; confirm
      `LICENSE` file lives at the workspace root AND is shipped in
      each crate's package (cargo handles this by default if
      `license` is declared, but verify via `cargo package --list`).

---

## 5. Security gate 🔴

- [ ] **`cargo audit`** — run on the full workspace, zero
      vulnerabilities, or each finding triaged and documented.
- [ ] **`cargo deny check`** — license compatibility + ban-list +
      advisory check, zero failures.
- [ ] **Secret scanner sweep** — `trufflehog` or `gitleaks` against
      the full repo history, zero findings.
- [ ] **Git history review** — any `.env`, `*.pem`, `credentials*`,
      or API-key-looking commits audited and (if found) history
      rewritten before public push.
- [ ] **Dependency surface review** — crate-level
      transitive-dep list reviewed, no abandoned or single-maintainer
      deps in hot paths.
- [ ] **`unsafe` audit** — grep for `unsafe` blocks, document each
      or eliminate.
- [ ] **Public-facing defaults** — no debug endpoints, no test creds,
      no localhost assumptions in CLI defaults.

---

## 6. Documentation gate 🟡

- [ ] **Root `README.md`** — install instructions verified from a
      clean machine.
- [ ] **Per-crate docs** — `cargo doc` builds cleanly, no broken
      intra-doc links, all public items have at least a one-line
      rustdoc.
- [ ] **`CHANGELOG.md`** — file does not currently exist at the
      workspace root. Create and populate for v0.1.0.
- [ ] **`CONTRIBUTING.md`** — file does not currently exist. Needed
      for any public repo accepting contributions.
- [x] **`INTRO.md` cross-link** — LSP-Brains/INTRO.md landed and
      README cross-referenced (Session 7a, 2026-04-17).
- [ ] **`docs/adopting.md`** — "how do I actually use this in my
      project?" walkthrough. Python starter repo is the fork-target,
      but the walkthrough needs prose.
- [ ] **Removed references** — grep for "Sparq", internal hostnames,
      internal project codenames, and redact or replace.

---

## 7. PyPI gate 🔴

The Python SDK lives at `sdk-python/` and is not yet on PyPI.

- [ ] **`python -m build`** clean — wheel and sdist build with no
      warnings.
- [ ] **`twine check dist/*`** clean — long description renders, no
      metadata errors.
- [ ] **PyPI name re-check on publish day** — `lsp-brains` was
      available 2026-04-17.
- [ ] **Decide: `lsp-brains` or `lsp_brains`?** — PyPI normalizes
      both to `lsp-brains` for resolution, but the import name is
      `lsp_brains`. Current convention in the codebase uses
      `lsp_brains` as the import path.
- [ ] **TestPyPI dry-run first.** Upload to
      https://test.pypi.org/, install from it, run a smoke test.
      Never skip straight to real PyPI.
- [ ] **Version alignment** — `sdk-python/pyproject.toml` version
      matches (or consciously diverges from) the Rust workspace
      version.

---

## 8. CI gate 🟡

- [ ] **GitHub Actions** — workspace tests green on push to main,
      on a matrix of at least `ubuntu-latest` + `windows-latest`.
- [ ] **MSRV declared and tested** — `rust-version` field in
      `workspace.package`, and a CI job pinning to that toolchain.
- [ ] **Release workflow** — a manual-dispatch workflow that runs
      `cargo publish --dry-run` on every crate, ordered correctly.
      Optional but catches drift between local and CI envs.

---

## 9. Ecosystem submodule posture 🟢

The parent ecosystem repo is private. The Python starter submodule
(added in Session 7d) is also private. If the ecosystem root is made
public before the starter:

- [ ] **Visibility matrix documented** — which repos are public, which
      are private, at the moment of NeuroGrim's public release.
- [ ] **Recursive clone warnings** — ecosystem README notes that
      `git clone --recursive` against a public ecosystem will fail
      on private children without auth.

---

## 10. Publish-day runbook (when every gate above is closed)

Do not run any of these commands until every `[ ]` above is `[x]`.

```bash
# 0. Confirm clean worktree + tag ready
cd D:/Brains/NeuroGrim/neurogrim
git status
git tag v0.1.0
git push origin v0.1.0

# 1. Final dry-run sweep (one more time, from a clean checkout)
for crate in neurogrim-core neurogrim-a2a neurogrim-sensory \
             neurogrim-mcp neurogrim-ecosystem neurogrim-cli; do
  cargo publish -p "$crate" --dry-run || exit 1
done

# 2. Real publish, bottom-up, with a pause between each
cargo publish -p neurogrim-core
sleep 30   # give crates.io index a moment to propagate
cargo publish -p neurogrim-a2a
sleep 30
cargo publish -p neurogrim-sensory
sleep 30
cargo publish -p neurogrim-mcp
sleep 30
cargo publish -p neurogrim-ecosystem
sleep 30
cargo publish -p neurogrim-cli

# 3. Python SDK (only after TestPyPI smoke test)
cd ../sdk-python
python -m build
twine upload --repository testpypi dist/*   # test first
# verify from clean venv:  pip install -i https://test.pypi.org/simple/ lsp-brains
twine upload dist/*                          # real

# 4. Post-publish
# - create a GitHub Release on v0.1.0 tag
# - update ecosystem README install paths from "source" to "pip install" / "cargo install"
# - announce
```

---

## Summary

Open gates: **9** (legal, dry-run, metadata, security, docs, PyPI,
CI — each with sub-items). Closed gates: **1** (name availability
snapshot + INTRO.md cross-link).

This document is the readiness source-of-truth. Every change to our
publish posture should update a checkbox here.
