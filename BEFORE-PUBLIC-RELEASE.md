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

## 1. Legal / naming gate 🔴

The plain-text name "Motherbrain" has at least two known prior uses:

- **EQT Motherbrain** — Swedish private-equity firm EQT's internal
  AI-driven deal-sourcing platform. Actively used. Tech sector.
  Highest-risk conflict: same class (software / AI).
- **Mother Brain (Nintendo)** — character in *Super Metroid* (1994)
  and the *Metroid* franchise. Different class (entertainment), but
  the mark has been policed by Nintendo.

The space vs. no-space distinction ("Mother Brain" vs. "Motherbrain")
is **not** meaningful in trademark law — both resolve to the same
word mark.

- [ ] **USPTO TESS search** run on "Motherbrain" across classes 9
      (software), 42 (SaaS), 41 (entertainment), and adjacent.
- [ ] **Trademark attorney consult** — 30-minute initial review,
      written opinion on risk of "Motherbrain" vs. alternatives.
- [ ] **Decision recorded** on one of: (a) proceed with stylized
      "Moth(er):Br+AI+n" as the mark, (b) rebrand before public
      release, (c) proceed with unstylized "Motherbrain" on attorney
      advice.
- [ ] **If rebrand:** all crate names, repo URLs, PyPI package names,
      README/docs, and `spec` cross-references updated. LSP-Brains
      INTRO.md "About the name" footnote updated.
- [ ] **domain ownership:** any `.com`/`.io`/`.dev` domains claimed
      match the final mark decision.

**Cross-reference:** `LSP-Brains/INTRO.md` "About the name" footnote
already acknowledges this review is in progress.

---

## 2. Name availability on package registries ✅ (snapshot)

Snapshot taken 2026-04-17. All names are AVAILABLE as of this date —
this can change at any time; **re-check immediately before any real
publish.**

### crates.io

| Crate | Status |
|-------|--------|
| `motherbrain` | AVAILABLE |
| `motherbrain-core` | AVAILABLE |
| `motherbrain-cli` | AVAILABLE |
| `motherbrain-a2a` | AVAILABLE |
| `motherbrain-sensory` | AVAILABLE |
| `motherbrain-mcp` | AVAILABLE |
| `motherbrain-ecosystem` | AVAILABLE |

The CLI binary is named `motherbrain` (see
`crates/motherbrain-cli/Cargo.toml`). The crate package name is
`motherbrain-cli`. Reserving the plain `motherbrain` crate name (e.g.
as an empty placeholder that re-exports from `motherbrain-cli`) is a
decision to make in the publish-day runbook.

### PyPI

| Package | Status |
|---------|--------|
| `lsp-brains` | AVAILABLE |
| `lsp_brains` | AVAILABLE |
| `motherbrain` | AVAILABLE |

- [ ] **Re-check the day of publish** — names can be claimed by
      squatters at any time. A free account + `twine upload` of an
      empty package is all it takes.

---

## 3. Cargo dry-run gate 🔴

**Status on this machine:** the Rust toolchain is not installed on
the current development workstation. Dry-runs must be performed from
a machine with Rust + cargo available. This is not a blocker for the
session that documented this gate, but it IS a blocker for closing
this gate — dry-run logs must be captured and attached below before
publish.

### Required commands (bottom-up, dependency order)

```bash
cd D:/Brains/Moth-er-Br-AI-n/motherbrain
mkdir -p .dry-run-logs
cargo publish -p motherbrain-core      --dry-run 2>&1 | tee .dry-run-logs/core.log
cargo publish -p motherbrain-a2a       --dry-run 2>&1 | tee .dry-run-logs/a2a.log
cargo publish -p motherbrain-sensory   --dry-run 2>&1 | tee .dry-run-logs/sensory.log
cargo publish -p motherbrain-mcp       --dry-run 2>&1 | tee .dry-run-logs/mcp.log
cargo publish -p motherbrain-ecosystem --dry-run 2>&1 | tee .dry-run-logs/ecosystem.log
cargo publish -p motherbrain-cli       --dry-run 2>&1 | tee .dry-run-logs/cli.log
```

### 🟡 Expected intra-workspace blocker

`motherbrain-core` has no intra-workspace dependencies and should
dry-run cleanly. The other five crates depend on `motherbrain-core`
(and transitively on each other) via `{ workspace = true }`. In the
workspace `Cargo.toml` the workspace-dep entries look like:

```toml
motherbrain-core = { path = "crates/motherbrain-core" }
```

— they have a `path` but no `version`. `cargo publish` requires
**both** for workspace members that depend on other workspace members,
so that the published crate can resolve its deps from crates.io. Dry
-run will fail on every crate except `motherbrain-core` until this is
fixed.

- [ ] **Add `version = "0.1.0"` to every intra-workspace dep** in the
      root workspace `Cargo.toml` (lines 54–59). Must match the
      `workspace.package.version`.
- [ ] **Re-run dry-run sweep.** All six must exit 0.

### Checkboxes (to be filled in when dry-run actually runs)

- [ ] `motherbrain-core` — dry-run exit 0, log attached
- [ ] `motherbrain-a2a` — dry-run exit 0, log attached
- [ ] `motherbrain-sensory` — dry-run exit 0, log attached
- [ ] `motherbrain-mcp` — dry-run exit 0, log attached
- [ ] `motherbrain-ecosystem` — dry-run exit 0, log attached
- [ ] `motherbrain-cli` — dry-run exit 0, log attached

---

## 4. Metadata gate 🟡

### Repository URL mismatch

Current workspace `Cargo.toml` (line 18):
```
repository = "https://github.com/keenanHoffmanSparq/Moth-er-Br-AI-n"
```

The repo actually lives at
`https://github.com/KeenanHoffman/Moth-er-Br-AI-n`. The current value
may be a stale personal-account reference. **Pointing to the wrong
repo on a published crate is a bad first impression.**

- [ ] **Decide which GitHub account owns the public project.**
- [ ] **Update `workspace.package.repository`** to the canonical URL.
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
      are private, at the moment of MotherBrain's public release.
- [ ] **Recursive clone warnings** — ecosystem README notes that
      `git clone --recursive` against a public ecosystem will fail
      on private children without auth.

---

## 10. Publish-day runbook (when every gate above is closed)

Do not run any of these commands until every `[ ]` above is `[x]`.

```bash
# 0. Confirm clean worktree + tag ready
cd D:/Brains/Moth-er-Br-AI-n/motherbrain
git status
git tag v0.1.0
git push origin v0.1.0

# 1. Final dry-run sweep (one more time, from a clean checkout)
for crate in motherbrain-core motherbrain-a2a motherbrain-sensory \
             motherbrain-mcp motherbrain-ecosystem motherbrain-cli; do
  cargo publish -p "$crate" --dry-run || exit 1
done

# 2. Real publish, bottom-up, with a pause between each
cargo publish -p motherbrain-core
sleep 30   # give crates.io index a moment to propagate
cargo publish -p motherbrain-a2a
sleep 30
cargo publish -p motherbrain-sensory
sleep 30
cargo publish -p motherbrain-mcp
sleep 30
cargo publish -p motherbrain-ecosystem
sleep 30
cargo publish -p motherbrain-cli

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
