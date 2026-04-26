# NeuroGrim

> *a book of spells for AI agents*

**A language-agnostic nervous system for AI-assisted software projects.**

NeuroGrim implements the [LSP Brains Specification](spec/LSP-BRAINS-SPEC.md) — a
methodology for giving AI agents continuous, honest awareness of project health through
MCP-based sensory tools, A2A-based peer coordination, cross-domain correlation,
trajectory intelligence, and gated governance. Sensory tools are small spells cast
against the project; the Brain keeps a grimoire of their readings and tells you what
has changed.

**Current version:** `3.0.0-rc.1` — first public release candidate.
See [CHANGELOG.md](CHANGELOG.md) for what shipped + what's open.

## 🚀 Getting started in ~20 minutes

New here? Start with **[docs/getting-started.md](docs/getting-started.md)** —
a clone → build → first-score walkthrough with a working example at
[`examples/hello-brain/`](examples/hello-brain/). It will land you on a
real Brain score for a real project in under half an hour.

Already comfortable with Rust workspaces? Jump to [Quick Start](#quick-start)
below.

## What's Here

| Directory | Contents |
|-----------|----------|
| `neurogrim/` | Rust Brain engine (workspace: core, sensory, mcp, a2a [Stage 6], cli crates) |
| `spec/` | The LSP Brains Specification v2.1 (13 sections + Appendix G), dual-brain design, methodology evolution |
| `sdk-python/` | Python SDK for writing custom sensory tools (`lsp-brains` package) |
| `docs/` | Domain catalog, architecture guides |
| `whitepaper/` | LSP Brains methodology whitepaper (Markdown; prior HTML build archived 2026-04-17) |
| `starter-kit/` | **Archived 2026-04-17** — PowerShell reference; moved to `D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\` |
| `NeuroGrim-python-starter/` | **Child submodule** — Python adoption template. Declared as NeuroGrim's A2A child Brain (port 8423); exercises recursive peer communication (ecosystem → NeuroGrim → starter). |
| `domains/laas/` | Archived first-customer domain: LaaS (16 domains, 26 gates, 3 hats) — read-only historical reference |
| `roadmap/` | Vision, roadmap, data architecture, dependency map, stage epics (including new S6 "Dual Brain via A2A") |
| `.claude/skills/` | Agent skills for methodology work, brain operations, and domain authoring |

## Quick Start

### Run the Brain

```bash
cd neurogrim
cargo build

# Run a sensory tool
./target/debug/neurogrim sensory test-health --project-root ..

# Write its output to CMDB
./target/debug/neurogrim sensory test-health --project-root .. > ../.claude/test-health-cmdb.json

# Get your health score
./target/debug/neurogrim health --project-root ..

# Validate registry
./target/debug/neurogrim validate -r ../.claude/brain-registry.json
```

### Run Tests

```bash
# Rust Brain engine tests
cd neurogrim
cargo test

# Python SDK tests
cd sdk-python
python -m pytest tests/ -v
```

## Command aliases

Every NeuroGrim primary command also accepts a themed alias — same behavior, same
flags, different feel. Aliases are additive; primary names remain canonical.

| Command        | Alias     | Purpose |
|----------------|-----------|---------|
| `score`        | `scry`    | Quick unified health read |
| `agent`        | `divine`  | Full agent-mode JSON output |
| `trend`        | `drift`   | Trajectory analysis (velocity, acceleration) |
| `validate`     | `seal`    | Validate `brain-registry.json` |
| `serve`        | `summon`  | Start as MCP server |
| `sensory`      | `cast`    | Run a built-in sensory tool |
| `init`         | `conjure` | Scaffold a new `brain-registry.json` |
| `a2a-serve`    | `beacon`  | Publish Agent Card + accept peer invocations |
| `a2a-invoke`   | `commune` | Call a peer Brain over A2A |
| `a2a-discover` | `behold`  | Fetch a peer's Agent Card |

Run `neurogrim --help` to verify the live list.

## Tool Invocation Mode (MCP vs CLI)

NeuroGrim can be reached two ways from a Claude Code session:

| Mode | Context cost at session start | When to choose |
|------|-------------------------------|----------------|
| **MCP** (default) | ~983 tokens (7 BrainServer tool schemas) | Discovery + typed error shapes matter; newcomers. |
| **CLI** (opt-in) | 0 tokens | Power users comfortable with `neurogrim` subcommands; long sessions under context pressure. |

**CLI mode:** omit NeuroGrim from `.claude/.mcp.json`; invoke via
Bash using the existing `score` / `trend` / `health` / `validate` /
`awareness` / `agent` subcommands. Full docs:
[`docs/cli-mode.md`](docs/cli-mode.md) +
[`docs/cli-sensory-surface.md`](docs/cli-sensory-surface.md). Benchmark
methodology: `roadmap/data/b09-bench-<date>.json` (regenerate via
`cargo test -p neurogrim-cli --test context_overhead -- --nocapture`).

## Architecture

```
                    ┌──── MCP ────┐                                  ┌──── A2A ────┐
 Sensory Tools  ───►│             │  Brain Engine  ──► Unified Score │             │  Peer Brains
 (LSP, lint, git,   │  tool-call  │  ├─ Trajectory                   │  peer-agent │  (parent/child,
  test results, ...)│             │  ├─ Correlation + Coherence      │             │   local/external)
                    │             │  ├─ Incident detection           │             │
 LLM Agent      ───►│             │  ├─ Gated governance             │             │
 (Claude Code,      │             │  ├─ Human comms model            │             │
  Cursor, ...)      │             │  └─ Secret-ref catalog           │             │
                    └─────────────┘                                  └─────────────┘
```

The Brain reads pre-computed scores from CMDB files written by sensory tools (delivered
via MCP). It applies confidence decay based on data freshness, computes domain weights
and floor constraints, evaluates cross-domain correlations, fires incident patterns, and
surfaces recommendations bounded by an attention budget. Peer Brains (fractal composition
children, or an external dual-brain counterpart) exchange messages via A2A (Stage 6).

## Protocols

Two distinct protocols carry traffic across the Brain's boundary. They are orthogonal
and must not be conflated.

| Protocol | Role | Crate | Spec |
|----------|------|-------|------|
| **MCP** (Model Context Protocol) | Sensory tool invocation (Brain-as-MCP-client) + Brain exposure to LLM agents (Brain-as-MCP-server) | `neurogrim-mcp` | §3.7, Appendix F |
| **A2A** (Agent2Agent Protocol) | Brain-to-Brain peer communication: fractal composition + dual brain | `neurogrim-a2a` (Stage 6) | §9, §10, §13, Appendix G |

**When in doubt:** if the other end is a sensor or an LLM, use MCP. If the other end is
another Brain, use A2A. See `spec/METHODOLOGY-EVOLUTION.md` §6 for the rationale behind
the split.

## Containers + claude-proxy (opt-in)

NeuroGrim's day-one usage runs natively on the host (`cargo build`
+ invoke via CLI/MCP). **Containers and the companion
`claude-proxy` are opt-in capabilities** for deployments that
need them — multi-host A2A peer topologies, multi-agent
credential isolation, sealed CI runtimes. You don't need any of
this to use NeuroGrim.

When you DO want them:

- [`Dockerfile`](Dockerfile) + [`docs/EXTERNAL-BRAIN-DEPLOYMENT.md`](docs/EXTERNAL-BRAIN-DEPLOYMENT.md)
  — package `neurogrim a2a-serve` for any Docker host.
- [`claude-proxy/README.md`](../claude-proxy/README.md) — host-
  side credential mediator: containers get per-scope tokens
  (`nb_sct_…`); the real Anthropic API key never leaves the host;
  audit metadata only (no prompts on disk); instant per-token
  revocation.
- [`docs/container-brain.md`](docs/container-brain.md) —
  decision matrix + threat-model + cross-references.

## Built-In Domains

Ten domains ship with NeuroGrim, organized in two tiers:

### Core (Weighted — contribute to unified score)

| Domain | Weight | What It Measures |
|--------|--------|-----------------|
| `test-health` | 0.40 | Test file detection, test-to-source ratio, failing test count |
| `code-quality` | 0.35 | Lint configs, formatting standards, quality tooling |
| `deploy-readiness` | 0.25 | CI config, README, no secrets in tracked files |

### Advisory (Weight 0.0 — visible in health output; promote when signal is trusted)

| Domain | What It Measures |
|--------|-----------------|
| `git-health` | Uncommitted changes, branch freshness, stash count |
| `rust-health` | Clippy lint count, cargo audit CVEs, MSRV compliance |
| `subagent-health` | Multi-agent task completion rate, agent protocol compliance |
| `security-standards` | SECURITY.md, SAST workflows, secret scanning |
| `coherence` | Cross-domain relationship health — the "association cortex" |
| `human-comms` | Persistent human communication model (preferences, per-hat overrides) |
| `secret-refs` | Safe credential reference catalog — references only, never values |

See [docs/DOMAINS.md](docs/DOMAINS.md) for full descriptions, scoring models, and a catalog
of potential domains you can build.

## Key Concepts

- **Sensory Tools** — run against a project, write a CMDB JSON file with score and findings
- **Domains** — named health dimensions; each backed by a CMDB file
- **Confidence Decay** — `confidence = 100 × e^(−λ × age_days)` — stale data loses weight automatically
- **Floor Constraints** — a critically low domain score caps the unified score
- **Trajectory** — velocity and acceleration computed from raw score history
- **Correlations** — named cross-domain patterns (compound_risk, dependency, reinforcing, blocking)
- **Coherence** — meta-domain that scores how well all other domains relate to each other
- **Human Model** — domain that tracks how a specific human wants agents to communicate
- **Secret-Refs** — safe catalog of credential locations; agents generate access code without seeing values
- **Gates** — checkpoints that block commit, merge, or deploy until conditions are met
- **Hats** — operational lenses that amplify different domains (engineer, reviewer, operator, security)
- **Human personas** — adapted output for different human readers (executive, manager, developer, specialist, PM). Distinct from agent *hats*, which shape what the Brain itself emphasizes.
- **Attention Budget** — limits displayed recommendations to prevent overload

## Python SDK

Write custom sensory tools in Python using the `lsp-brains` package:

```bash
pip install -e sdk-python/
```

```python
from lsp_brains import SensoryTool, Finding, run_server

class MyTool(SensoryTool):
    name = "my-domain"
    domain = "my-domain"

    async def analyze(self, project_root: str) -> dict:
        return self.build_cmdb(
            score=75,
            findings=[Finding("All checks passed")],
        )

if __name__ == "__main__":
    run_server(MyTool())
```

Register custom secret providers:

```python
from lsp_brains import SecretProvider, SecretProviderSpec

class MyVaultProvider(SecretProvider):
    spec = SecretProviderSpec(
        name="my-vault",
        description="Internal HashiCorp Vault with AppRole auth",
        reference_template=(
            "import hvac, os\n"
            "client = hvac.Client(url=\"{vault_url}\", token=os.environ[\"VAULT_TOKEN\"])\n"
            "{env_var} = client.secrets.kv.v2.read_secret_version(path=\"{secret_path}\")[\"data\"][\"data\"][\"value\"]"
        ),
    )

MyVaultProvider.register(project_root=".")
```

## Explore Further

- **[Whitepaper](whitepaper/WHITEPAPER.md)** — Full methodology, architecture, and design principles
- **[Domain Catalog](docs/DOMAINS.md)** — All 10 built-in domains + potential domains to inspire adopters
- **[LSP Brains Spec](spec/LSP-BRAINS-SPEC.md)** — The formal specification
- **[Vision](roadmap/VISION.md)** — Design principles and north star
- **[Roadmap](roadmap/ROADMAP.md)** — Stage progression and current status
- **[LaaS Reference](domains/laas/)** — Complete archived implementation (16 domains, 26 gates, 3 hats)

## Repository

- **Source:** https://github.com/KeenanHoffman/NeuroGrim
- **Spec Repo:** https://github.com/KeenanHoffman/LSP-Brains
- **Origin:** Extracted from [Lies-as-a-Service](https://github.com/sparq-doug/lies-as-a-service)

## License

See individual files for licensing terms.
