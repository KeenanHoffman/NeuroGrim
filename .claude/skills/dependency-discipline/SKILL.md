---
name: dependency-discipline
description: >-
  You are about to install or add a software dependency — `npm install`, `npm
  add`, `pnpm add`, `yarn add`, `cargo add`, `cargo install`, `pip install`,
  `poetry add`, `uv add`, `go get`, `gem install`, or any equivalent. STOP
  and read this skill first. The "wild west" of 2026 package ecosystems
  (especially npm) makes reflexive installation a real supply-chain risk —
  this skill captures the discipline NeuroGrim's methodology requires
  before any dep enters the project's trust boundary.
when_to_use: >-
  Any time you (the agent) are about to add a new dependency or run an
  installer that mutates a lockfile. Trigger phrases — "npm install", "npm
  add", "pnpm add", "yarn add", "cargo add", "cargo install", "pip install",
  "poetry add", "uv add", "go get", "gem install", "add a dependency",
  "new package", "install package", "pull in", "added to package.json",
  "added to Cargo.toml", "yarn lockfile", "cargo lockfile", "package-lock",
  "vulnerable dep", "audit deps".
---

# Skill: Dependency Discipline

**When to use this skill:** You are about to install, add, or upgrade a
software dependency in a NeuroGrim-aware project. STOP. Do not run the
installer reflexively. Work through the discipline below.

## The pattern

NeuroGrim's cultural substrate — *integrity, honesty, critical-but-kind* —
applies to dependency choices as much as to code. A dependency is a
declaration of trust in another author's judgment, security posture, and
supply chain. Every transitive dep inherits that trust. Reflexive
installation in 2026 — especially in npm — is how typosquats land,
exfil-payload packages get pulled, and CVE-laden artifacts ship.

This skill captures the four disciplines: **justify, audit, pin, document.**

## The discipline

### 1. Justify the new dep (pre-install)

Before running ANY installer, write down (in chat to the operator, or in
the changelog draft):

- **What it does** in one sentence
- **Why we need it** (what would we have to write ourselves otherwise)
- **Who maintains it** (single individual? org? RustSec / OpenSSF tracked?)
- **License** (MIT/Apache-2 OK; GPL/AGPL needs explicit operator approval)
- **Last release age** (abandoned packages > 18 months untouched are red flags)

If you can't answer these in <60 seconds, you don't know enough about
the dep. Read its README, scan the most recent issues, check the
maintainer's other work.

### 2. Audit the resolved tree (during install)

```bash
# Cargo (Rust): use --locked + cargo audit
cargo add <dep>            # writes to Cargo.toml; doesn't install yet
cargo update --dry-run     # see what would resolve
cargo audit                # check known CVEs
# only then:
cargo build --locked       # honor the lockfile

# npm: audit BEFORE running anything from the new tree
npm install <dep> --package-lock-only   # writes lockfile, doesn't run scripts
npm audit --audit-level=moderate
# review findings; if clean OR documented:
npm ci                     # reproducible install (FAILS on lockfile drift)

# pip: use --dry-run then pip-audit
pip install --dry-run <dep>
pip-audit
```

**`npm install` (without --package-lock-only) runs install scripts.**
A malicious package executes arbitrary code at install time. Treat
the lockfile as the audit artifact and run `npm ci` only after audit.

### 3. Pin appropriately

- **Cargo**: pin major versions in `Cargo.toml`; `Cargo.lock` is the
  canonical record for binary projects. Library crates use `^x.y` for
  minor flexibility.
- **npm**: `^x.y.z` (caret) is the npm default but accepts minor/patch
  upgrades on next `npm install`. **For build-tooling deps with a
  history of CVEs (esbuild, vite, webpack), prefer `~x.y.z` (tilde) or
  exact pins.** The lockfile pins the actual resolved version; the
  `package.json` semver decides what `npm install` will accept on
  upgrade.
- **Always commit lockfiles** (`Cargo.lock` for binary crates;
  `package-lock.json` always; `yarn.lock` always).

### 4. Document acceptance (post-install)

If the audit surfaced findings you're accepting (because the fix is
breaking, or the blast radius is dev-only, or the upstream hasn't
released a fix), document the acceptance in:

- `CHANGELOG.md` under the next release's `### Security` section, OR
- A short comment in the relevant `Cargo.toml` / `package.json` block, OR
- A `audit/dep-accepted-<date>.md` file for non-trivial accepted risk

The acceptance MUST include:
- The CVE / advisory ID
- Why the fix isn't being applied now
- The blast-radius assessment (dev-only? prod? exposed network surface?)
- The trigger for revisiting (e.g., "when vite 8 + plugin-react 5 ship together")

## Run NeuroGrim's own check

NeuroGrim ships an xtask that wraps `cargo audit` + `npm audit` for
the workspace:

```bash
cargo xtask sca-check
```

Run it after every dep change. CI gates on it. Findings above
`moderate` block merge unless documented per discipline 4.

## What this skill does NOT do

- **Not a substitute for human judgment on novel deps.** When you
  encounter a dep nobody on the team has used, escalate to the
  operator. Don't assume "it has lots of stars" implies safety.
- **Not a replacement for NeuroGrim's `supply-chain-vigilance`
  domain.** This skill is the AGENT's pre-flight discipline; vigilance
  is the BRAIN's continuous deep-signal scan (typosquatting, exfil
  indicators, binary reproducibility).
- **Not a license-compliance review.** Touch on it (point 1) but the
  full review is operator territory.

## Cultural substrate

- **Integrity** — commit acknowledged risk explicitly. Don't paper
  over `npm audit` findings. If you accept, document the acceptance.
- **Honesty** — call out the discipline you skipped, even if
  unintentionally. The cleanup is cheaper than the blast.
- **Critical-but-kind** — when an operator (or another agent) skipped
  the discipline, surface the gap and the fix path; don't lecture.
- **Respect** — the dependency authors are also operators. Treat
  "needs review" as a neutral state, not a judgment.

## See also

- `.claude/skills/supply-chain-auditor/` — the hat for explicit SCA review
- NeuroGrim domain `supply-chain-sca` (Layer 1 — mechanical)
- NeuroGrim domain `supply-chain-vigilance` (Layer 2 — deep signals)
- NeuroGrim domain `supply-chain-review` (Layer 3 — agent-assisted human review)
- `cargo xtask sca-check` — the consolidated pre-merge gate
- LSP Brains spec §16.x — supply-chain protocol layer
