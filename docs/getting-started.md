# Getting Started with NeuroGrim + LSP Brains

**Goal:** in ~20 minutes, go from cloning the repo to seeing your
own project produce a Brain health score.

**Audience:** developers who want to try LSP Brains on a real
codebase. No prior familiarity with the spec or the methodology
required — links to deeper reading appear where you need them.

**What you'll have at the end:**
- A working `neurogrim` binary in your PATH.
- A demo output score from the built-in `examples/hello-brain/`.
- A scaffolded `.claude/brain-registry.json` in your own project
  with 3-4 domains enabled.
- A first score of your project + pointers to next steps.

---

## 0. Before you start

### What LSP Brains is

An LSP Brain is a persistent project-awareness layer that
accumulates context across sessions the way a language server
accumulates code awareness in an IDE. Sensors inspect the repo
(git health, test baseline, deploy readiness, secret management,
etc.) and produce structured health signals; a scoring engine
aggregates them; agents can consult the Brain through MCP, A2A,
or the CLI.

**Primary value:** cumulative awareness over a project's
lifetime — consistency, caution, security posture — that
individual agent sessions' memory doesn't preserve.
(See [ROADMAP.md § Evidence + Hypothesis Posture](../roadmap/ROADMAP.md)
for the nuance.)

**Not primary value:** one-shot response uplift on a single
prompt. The Brain's scope is longitudinal; individual turns are
a secondary use surface. (See
[METHODOLOGY-EVOLUTION §14](../../LSP-Brains/spec/METHODOLOGY-EVOLUTION.md)
for why we bound this claim honestly.)

### Prerequisites

- **Rust toolchain** (1.75+) — https://rustup.rs/
- **Python 3.11+** (for the optional agent-behavior harness)
- **Git** (for submodule + repo operations)
- A project to try it on (or use the built-in example below)

Windows, macOS, Linux all supported; commands below use Bash
syntax. Windows users run the same commands in Git Bash (the
shell ships with Git for Windows).

---

## 1. Clone + build (5 minutes)

```bash
# Clone the ecosystem (includes NeuroGrim + LSP-Brains as submodules)
git clone --recursive https://github.com/KeenanHoffman/Brains-ecosystem.git brains
cd brains

# Or: clone just NeuroGrim standalone
# git clone https://github.com/KeenanHoffman/NeuroGrim.git
# cd NeuroGrim

cd NeuroGrim/neurogrim  # the Rust workspace
cargo build --release    # ~5 min first time; faster on subsequent builds
```

Verify the binary works:

```bash
./target/release/neurogrim --help
# Expected: usage + list of subcommands (score, sensory, health, ...)
```

Optional: add the binary to your PATH so the rest of this guide
uses `neurogrim` instead of the full path:

```bash
# Bash / Zsh
export PATH="$PWD/target/release:$PATH"

# Or copy it somewhere in your existing PATH:
# cp target/release/neurogrim ~/bin/
```

---

## 2. Run the built-in demo (3 minutes)

The `examples/hello-brain/` directory is a minimal standalone
project with a pre-configured Brain registry. Run the Brain
against it:

```bash
cd ../examples/hello-brain
neurogrim score --project-root .
```

Expected output: a unified health score, per-domain breakdown, and
a list of findings. Sample shape:

```
✦ Casting score…
NeuroGrim Score: 48/100  (confidence: 71%)
  + git-health raw:60 eff:30
  + test-health raw:40 eff:12
  + code-quality raw:45 eff:6
  - deploy-readiness raw:0 eff:0
  ...
Trajectory: no-data (velocity: +0.0, samples: 0)

Findings:
  ! No CI configuration found (deploy-readiness)
  ! Single test file; consider expanding coverage (test-health)
  ...
```

Your exact numbers will differ — scoring depends on the repo
state. What matters: you got a score + findings + a trajectory
placeholder.

If this worked, you have a functioning Brain. Next step: point it
at your own project.

**Troubleshooting:**
- *"No such file or directory: brain-registry.json"* — the
  command runs from inside `examples/hello-brain/` where that
  file lives. `cd` into it first.
- *"Unable to find cargo"* — the Rust toolchain isn't on PATH.
  Re-run `rustup-init.exe` (Windows) or source your shell config.

---

## 3. Scaffold a Brain in your own project (10 minutes)

### 3a. Pick your project + enable the .claude directory

```bash
cd /path/to/your/project
mkdir -p .claude
```

### 3b. Copy the starter registry

Copy `examples/hello-brain/brain-registry.json` as your starting
point, then trim to domains relevant to your project:

```bash
cp /path/to/NeuroGrim/examples/hello-brain/brain-registry.json \
   .claude/brain-registry.json
```

Open `.claude/brain-registry.json` and read through the domain list.
For a new project, pick 3-4 domains that match your situation:

- **Always useful:** `git-health`, `test-health`, `code-quality`.
- **If you have CI/CD:** `deploy-readiness`.
- **If you handle secrets:** `secret-refs`.
- **If you care about security posture:** `security-standards`.
- **If you use the skill/hat/culture system:** `capability-hygiene`,
  `skill-coherence`.

Delete domains you don't need (to keep the output focused) or
leave them — they'll advisory-score at weight 0.0 by default.

### 3c. Run your first score

```bash
neurogrim score --project-root .
```

This produces a Brain score for your project. The first run is
usually low — most projects don't have every signal in place.
That's the point: the Brain surfaces what's missing.

### 3d. Regenerate per-domain CMDBs (optional)

Some domains use cached data (CMDBs) for incremental scoring.
Regenerate them explicitly:

```bash
neurogrim sensory git-health --project-root . > .claude/git-health-cmdb.json
neurogrim sensory test-health --project-root . > .claude/test-health-cmdb.json
# ... repeat for each domain you've enabled
```

Then re-run `neurogrim score` — the score now reflects the
freshly-regenerated CMDBs.

---

## 4. Next steps (choose your own adventure)

### For developers: integrate the Brain with your agent

- Add NeuroGrim as an MCP server in your `.mcp.json` — the
  `neurogrim serve` command exposes scoring tools to LLM agents.
- Or: load the `cli-mode` skill and let the agent invoke Brain
  queries via Bash subcommands. Context-cost: ~0 tokens vs ~983
  for MCP. See [`docs/cli-mode.md`](cli-mode.md).

### For adopters: customize the framework

- Write your own sensors (a new "domain") — see
  [`docs/write-skill-guide.md`](write-skill-guide.md) and the
  [`neurogrim-sensory` crate source](../neurogrim/crates/neurogrim-sensory/src/).
- Add skills specific to your project — see
  [`.claude/skills/write-skill/SKILL.md`](../.claude/skills/write-skill/SKILL.md).
- Register hats for your operational contexts — see
  [`.claude/skills/hats/SKILL.md`](../.claude/skills/hats/SKILL.md).

### For researchers: run the agent-behavior harness

- The `agent-behavior-runner` (Python, `D:/Brains/agent-behavior-runner/`)
  is a pre-registered experiment harness: judge-based scoring,
  calibration gates, red-sample integrity, multi-judge consensus.
- See [`.claude/experiments/brain-vs-control/`](../../.claude/experiments/brain-vs-control/)
  for a worked example (three-arm comparison across 12 + 22 tasks,
  432-row ledger).

### For spec readers: the normative layer

- [`LSP-BRAINS-SPEC.md`](../../LSP-Brains/spec/LSP-BRAINS-SPEC.md) —
  15 normative sections + 7 appendices. Current version: v2.5.
- [`METHODOLOGY-EVOLUTION.md`](../../LSP-Brains/spec/METHODOLOGY-EVOLUTION.md)
  — 14 discovery-log entries tracking how the spec got here.
- [`VISION.md`](../roadmap/VISION.md) — 19 guiding principles.

### For fractal composition (advanced)

- The Brain can participate in A2A peer topologies (parent↔child
  or local↔external). See the `peer-brain` skill at
  `.claude/skills/peer-brain/SKILL.md`.
- The ecosystem root has its own Brain that aggregates mechanical
  scores from children. See `D:/Brains/.claude/brain-registry.json`
  (if you're running the full ecosystem clone).

---

## What you haven't seen yet (and is fine)

- **Agent integration** — the Brain is useful on its own for
  operator health checks, but its richest value shows when an
  agent has persistent access to it across sessions. That's the
  primary value hypothesis; the getting-started flow doesn't
  require it.
- **Cross-Brain correlation** — if you run multiple related
  projects, the ecosystem-level Brain can correlate across them.
  Advanced usage.
- **Governance flows** — domain promotion, red-mode sweeps,
  judge-integrity triage. These are Stage-10 spec §15.5 concerns
  that matter when you're making trust-bearing scoring decisions,
  not day-one adoption concerns.

---

## Support + troubleshooting

- **Something broke?** Open an issue on the NeuroGrim repo with
  the command you ran + the error output.
- **Want to contribute?** See the repo's CONTRIBUTING.md
  (forthcoming in v3.0 final).
- **Core docs:**
  - [`README.md`](../README.md) — project overview.
  - [`whitepaper/WHITEPAPER.md`](../whitepaper/WHITEPAPER.md) —
    methodology + nervous-system framing.
  - [`CHANGELOG.md`](../CHANGELOG.md) — version history.

**Welcome aboard.**
