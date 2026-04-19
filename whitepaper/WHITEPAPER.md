# LSP Brains: A Nervous System for AI Agents

## NeuroGrim — Reference Implementation Whitepaper

**Version:** 2.0
**Date:** April 2026
**Specification:** https://github.com/KeenanHoffman/LSP-Brains

---

## Table of Contents

1. [The Problem](#1-the-problem)
2. [The LSP Brains Approach](#2-the-lsp-brains-approach)
3. [Architecture Overview](#3-architecture-overview)
4. [How It Works](#4-how-it-works)
5. [Built-In Domains](#5-built-in-domains)
6. [Advanced Capabilities](#6-advanced-capabilities)
7. [Governance](#7-governance)
8. [Fractal Composition](#8-fractal-composition)
9. [Adopting LSP Brains](#9-adopting-lsp-brains)
10. [What Success Looks Like](#10-what-success-looks-like)

---

## 1. The Problem

AI coding agents are increasingly capable — they can write tests, refactor modules, analyze logs, and propose architecture changes. But they operate mostly blind. They read source code well. They do not read project health.

Consider what an experienced engineer knows before touching a codebase:

- Are the tests green? What is the test coverage ratio?
- Is the CI pipeline healthy? When did it last break?
- Are there unresolved security advisories in dependencies?
- Is the branch stale? Has anyone reviewed recent commits?
- Is there a documented deployment process? Is it being followed?

This knowledge is not in source files. It is distributed across config files, CI configurations, git metadata, lock files, audit outputs, and the informal conventions your team has accumulated over time. A human engineer learns to read these signals. Most AI agents cannot.

The result is predictable: agents that can't read project state make decisions without context. They write new tests without knowing the test-to-source ratio is already 0.3. They suggest architectural changes while the main branch is failing. They fix a linting issue while ignoring a CVE in a transitive dependency. They are locally intelligent and globally unaware.

This is not a model capability problem. It is an information architecture problem.

The health signals exist. The project declares them, in files, all the time. The gap is a missing layer between "the files exist" and "the agent can reason over them." That layer is what LSP Brains provides.

### The Core Insight

**Everything as Code is the contract between human intent and agent capability.**

If you declare your system state in versioned files rather than dashboards, an agent can reason about it. Once an agent can reason about it, it can score it, correlate it, recommend against it, and eventually act autonomously within defined boundaries.

Dashboards display. Declarations enable.

---

## 2. The LSP Brains Approach

LSP Brains is a language-agnostic specification for building agent nervous systems. Any software project that declares its state in versioned files can have an agent that reasons about it: score it, correlate across domains, recommend actions, and act autonomously within defined governance boundaries.

The methodology has four components that always appear together:

| Component | Role |
|-----------|------|
| **Sensory tools** | Detect and snapshot state from one domain into a CMDB JSON file |
| **Central scoring** | Aggregate domain signals into a unified health score with honest confidence weighting |
| **Declared governance** | Gates that block commit/merge/deploy until conditions are met |
| **Reflexive hooks** | Automatic triggers that run sensory tools and Brain scoring on defined events |

These four components form a feedback loop. Sensory tools write state. The Brain reads state and scores it. Gates enforce minimums on that score. Hooks trigger the loop automatically so the Brain's knowledge stays current.

### Methodology vs. Product

**LSP Brains** is the methodology — a language-agnostic specification that defines WHAT a Brain must do. The specification covers: sensory tool protocol, scoring contracts, governance model, interface contract, fractal composition protocol, and trajectory intelligence. A Python team can implement it without touching a line of Rust.

**NeuroGrim** is the product — the first reference implementation, written in Rust. It proves the methodology works and provides built-in sensory tools, a scoring engine, CLI, and MCP server integration. The product accelerates adoption; the methodology transfers independently.

### The Nervous System Analogy

The architecture maps cleanly onto biological nervous system concepts:

| Biological | LSP Brains | Role |
|------------|------------|------|
| Sensory neurons | Sensory tools | Detect state in one domain |
| Central nervous system | Brain Engine | Integrate signals, produce unified score |
| World model | CMDB JSON files | Declared state the Brain reasons over |
| Motor neurons | Skills | Know how to act on what the Brain perceives |
| Reflexes | Hooks | Automatic responses to specific stimuli |
| Consciousness | Agent + hats | Attentional bias — same signals, different salience |
| Memory | Proposal ledger | Learn from past recommendations |
| Peripheral nervous system | Child Brains | Extend sensing to the ecosystem |
| Proprioceptive trend | Score history + trajectory | "Am I getting better or worse?" |

This framing is not decorative. It guides design decisions. A biological nervous system does not understand the world by looking at a dashboard — it maintains a continuously updated model of the body's state from distributed sensory signals. LSP Brains builds the same infrastructure for software projects.

---

## 3. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        SENSORY LAYER                            │
│                                                                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  test-health │  │ code-quality │  │  deploy-readiness    │  │
│  │  sensory     │  │  sensory     │  │  sensory             │  │
│  │  tool        │  │  tool        │  │  tool                │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │
│         │                 │                      │              │
└─────────┼─────────────────┼──────────────────────┼─────────────┘
          │                 │                      │
          ▼                 ▼                      ▼
┌─────────────────────────────────────────────────────────────────┐
│                       CMDB LAYER                                │
│                                                                 │
│  test-health-cmdb.json   code-quality-cmdb.json   ...          │
│  (raw signals + timestamp + confidence metadata)               │
│                                                                 │
└─────────────────────────┬───────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│                      BRAIN ENGINE                               │
│                                                                 │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────────┐  │
│  │   Confidence   │  │   Trajectory   │  │   Correlation    │  │
│  │   Decay        │  │   Intelligence │  │   Engine         │  │
│  └────────┬───────┘  └────────┬───────┘  └────────┬─────────┘  │
│           │                   │                   │            │
│           └───────────────────▼───────────────────┘            │
│                               │                                │
│                    ┌──────────▼───────────┐                    │
│                    │   Unified Score      │                    │
│                    │   + Floor Constraints│                    │
│                    └──────────┬───────────┘                    │
│                               │                                │
└───────────────────────────────┼────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                        OUTPUT LAYER                             │
│                                                                 │
│  Unified Score    Recommendations    Gate Status    Trajectory  │
│  (0-100)          (attention-budget  (pass/fail)    (velocity + │
│                    limited)          (per gate)      class)     │
│                                                                 │
│  Persona-adapted output: executive / manager / developer / PM  │
└─────────────────────────────────────────────────────────────────┘
          │                                    │
          ▼                                    ▼
    Human Consumer                       Agent Consumer
    (CLI, Slack)                   (parent Brain, subagent,
                                    Claude Code session)
```

### Key Design Principles

The architecture is governed by seventeen principles documented in the LSP Brains specification (VISION.md). Four of them shape every structural decision:

**Declarations over dashboards.** If a human would check a dashboard, declare the state in a file instead. Agents read files; they cannot read dashboards.

**Scoring must be honest.** Unknown is not good. Confidence must weight the score, not just annotate it. A partially-observed system at 72 should outscore a fully-observed system at 65 — but a reassuring number built on missing data is worse than no number.

**The pattern is the product.** The specific domains are implementation details. The architecture — sensory tools, central scoring, declared governance, reflexive hooks — is what transfers. LSP Brains is not a DevOps tool. It is a pattern for building agent nervous systems.

**Trajectories reveal more than snapshots.** A score of 72 means nothing without context. 72 and rising means momentum is positive. 72 and falling means intervention is needed. The Brain answers not just "how healthy am I?" but "am I getting healthier or sicker?"

---

## 4. How It Works

### Step 1: Sensory Tools Write CMDB Files

A sensory tool is any process that detects state in one domain and writes a CMDB JSON file. The file format is standardized: it contains raw signals, a timestamp, and enough metadata for the Brain to compute confidence.

Example: `test-health-cmdb.json` after a sensory tool run:

```json
{
  "domain": "test-health",
  "captured_at": "2026-04-13T14:22:00Z",
  "signals": {
    "test_files_detected": true,
    "test_to_source_ratio": 0.42,
    "failing_tests": 0,
    "test_framework": "cargo-test"
  },
  "raw_score": 87
}
```

Sensory tools can be built-in (Rust, included with NeuroGrim), custom (any language that writes the correct JSON schema), or Python SDK-based (a wrapper that simplifies custom tool authoring). The only contract is the output format.

### Step 2: Confidence Decay

CMDB files have timestamps. The Brain uses exponential decay to reduce the confidence weight of stale data automatically:

```
confidence = 100 * e^(-λ * age_days)
```

Where `λ` is a decay constant configured per domain. A `test-health` CMDB file that is 3 days old might carry 60% confidence. A `deploy-readiness` CMDB that is 14 days old might carry 15% confidence.

This matters because **stale data that appears fresh is dangerous**. Confidence decay makes staleness visible in the score itself, not just in metadata annotations that consumers may ignore.

### Step 3: The Brain Engine Scores

The Brain reads all CMDB files, applies confidence decay, and computes domain scores using one of two models:

**Multiplier model:** `effective_score = floor(raw * confidence / 100)`

A domain with a raw score of 90 but 50% confidence contributes 45 to the unified calculation.

**Floor model:** When confidence drops below a threshold, the domain score is capped at a ceiling value regardless of how high the raw score is. Used for domains where staleness is particularly dangerous (e.g., security audits).

The unified score is a weighted sum of domain effective scores:

```
unified = floor(clamp(0, 100, sum(effective_score[d] * weight[d] for weighted domains)))
```

Advisory domains (weight = 0.0) do not contribute to the unified score but appear in the output with their own scores and trajectories.

### Step 4: Floor Constraints

Some domains are critical enough that a low score should not be diluted by strong performance elsewhere. Floor constraints enforce this:

```json
{
  "domain": "deploy-readiness",
  "floor": {
    "min_score": 40,
    "unified_cap": 60,
    "message": "Deploy readiness is critically low; unified score capped at 60"
  }
}
```

If `deploy-readiness` drops below 40, the unified score cannot exceed 60 — regardless of how well `test-health` and `code-quality` are performing. A system with excellent tests but a broken CI pipeline is not deploy-ready, and the score should say so.

### Step 5: Output

The Brain produces structured output in a versioned JSON schema. The output includes:

- **Unified score** (0-100) with confidence context
- **Domain breakdown** — raw score, effective score, confidence, trajectory per domain
- **Recommendations** — prioritized, attention-budget-limited, hat-aware
- **Gate status** — pass/fail per defined gate with blocking reason
- **Trajectory** — velocity, acceleration, classification (improving / stable / degrading / volatile)
- **Active hat** — which attentional lens shaped the recommendations
- **Floor applications** — which domains are capping the unified score and why

This output is consumed by Claude Code sessions, parent Brain instances, hooks, or persona-formatted for human stakeholders.

---

## 5. Built-In Domains

NeuroGrim ships with ten built-in domains organized into two tiers.

### Weighted Core Domains

These three domains contribute directly to the unified score. Their weights sum to 1.0.

| Domain | Weight | What It Measures |
|--------|--------|-----------------|
| `test-health` | 0.40 | Test file detection, test-to-source ratio, failing test count |
| `code-quality` | 0.35 | Lint configuration presence, formatting standards, code smell indicators |
| `deploy-readiness` | 0.25 | CI configuration, README presence, absence of secrets in tracked files |

**test-health (0.40)** carries the highest weight because test coverage is the most direct proxy for code confidence. The sensory tool detects test files, computes the test-to-source ratio, and checks for failing tests. A ratio below 0.2 triggers a low raw score. Failing tests floor the score at a low ceiling.

**code-quality (0.35)** examines the project's commitment to code standards. The tool looks for lint configuration files (`.eslintrc`, `clippy.toml`, `.rubocop.yml`, etc.), formatting configurations (`.editorconfig`, `rustfmt.toml`, `prettier.config.js`), and common smell indicators. It does not run linters — it detects whether the project has declared its standards.

**deploy-readiness (0.25)** checks the minimum conditions for safe deployment. CI configuration presence, a readable README, and the absence of committed secrets. This domain has a floor constraint: a critically low score caps the unified score at 60, preventing a project with a broken deployment pipeline from appearing healthy.

### Advisory Domains

These seven domains do not affect the unified score but produce their own scores, trajectories, and recommendations. They extend the Brain's awareness without penalizing projects that have not yet absorbed them.

| Domain | What It Measures |
|--------|-----------------|
| `git-health` | Uncommitted changes, branch freshness, stash count |
| `rust-health` | Clippy lint count, cargo audit CVEs, unused dependencies |
| `subagent-health` | Multi-agent task tracking, completion rates, coordination health |
| `security-standards` | SECURITY.md presence, SAST workflow configuration, secret scanning |
| `coherence` | Cross-domain relationship health; the "association cortex" |
| `human-comms` | Persistent human communication model; how agents adapt to individuals |
| `secret-refs` | Safe secret reference catalog; reference patterns only, never values |

**git-health** monitors the raw git state of the working directory. Stale branches, uncommitted changes, and excessive stash entries are signals that the codebase may not be in the state the developer thinks it is. Advisory because git state is highly contextual — a long-running feature branch is not inherently unhealthy.

**rust-health** is language-specific. Clippy lint counts, CVEs from `cargo audit`, and unused dependency detection. A domain like this demonstrates how easy it is to add language-specific sensory capability — the pattern is identical to universal domains, only the signals differ.

**subagent-health** tracks multi-agent coordination. When multiple agents operate on a project simultaneously, task conflicts, incomplete handoffs, and orphaned work items appear as health signals. This domain keeps the coordination layer visible.

**security-standards** looks at the project's security posture declarations: SECURITY.md, static analysis workflow configuration, secret scanning enablement. It scores intent and process, not the absence of vulnerabilities (that is `rust-health`'s job for dependency CVEs).

**coherence** is the meta-domain — it scores how well the other domains relate to each other. A project with strong `test-health` but weak `deploy-readiness` has a coherence deficit: the work is happening but the delivery path is broken. Coherence is described further in Section 6.

**human-comms** tracks how a specific human wants agents to communicate with them. Preferred verbosity, format, cadence, and escalation thresholds are persistent state. Agents that remember how their human communicates are more effective than agents that adapt through repeated prompting.

**secret-refs** is described in detail in Section 6.

---

## 6. Advanced Capabilities

### Trajectory Intelligence

A score without history is a photograph. LSP Brains tracks score history and computes trajectory metrics that turn the photograph into a film:

| Metric | Definition | Signal |
|--------|-----------|--------|
| **Velocity** | Average change per observation window | Which direction am I moving? |
| **Acceleration** | Rate of change of velocity | Is my rate of change changing? |
| **Classification** | Pattern over N observations | improving / stable / degrading / volatile |

Trajectory is computed on raw scores, not confidence-weighted scores. This prevents phantom trends caused by staleness changes rather than actual score changes.

The classification system distinguishes four states:

- **Improving** — positive velocity sustained across the window
- **Stable** — velocity near zero, low variance
- **Degrading** — negative velocity sustained across the window
- **Volatile** — high variance regardless of direction (churn signal)

This matters for recommendations. The Brain does not just report "code-quality is 65." It reports "code-quality is 65, degrading at -3.2 points per day for 5 observations." That context changes the recommended action from "consider improving code quality" to "investigate root cause of declining code quality trend."

Trajectories are first-class outputs. Every domain has one. The unified score has one. Parent Brain instances consume child Brain trajectories to detect ecosystem-level degradation before it becomes a crisis.

### Cross-Domain Correlation

Domains do not exist in isolation. A degrading `test-health` score combined with an improving `code-quality` score is a coherence signal: the team is cleaning up the codebase but not maintaining test coverage. The Brain can detect and surface this kind of compound pattern.

The correlation engine supports four pattern types:

| Pattern Type | Description | Example |
|-------------|-------------|---------|
| `compound_risk` | Multiple weak signals that together indicate elevated risk | Low test-health AND low deploy-readiness = elevated shipping risk |
| `dependency` | One domain's health is structurally dependent on another | Git health affects the freshness of every other domain's CMDB data |
| `reinforcing` | Improvement in one domain accelerates improvement in another | Code quality improvement reinforces test-health improvement |
| `blocking` | One domain's low score prevents another from being meaningful | Deploy-readiness blocks meaningful git-health interpretation pre-deploy |

Correlation patterns are declared in `brain-registry.json` — they are not inferred from statistical history. This makes them auditable, predictable, and controllable. You declare the relationships you know to be true; the engine fires patterns when conditions are met.

### The Coherence Domain

Coherence is the association cortex of the Brain. It does not measure any single domain's health — it measures how well the domains relate to each other as a system.

A project where `test-health` is high, `deploy-readiness` is low, and `git-health` is volatile has a coherence deficit. The project is doing good local work (testing) but the delivery pathway is broken and the working state is unstable. No single domain captures this — coherence does.

The coherence score reflects:
- Whether domain scores are directionally consistent (correlated improvement)
- Whether correlated risk patterns are active
- Whether blocking relationships are resolved or unresolved
- Whether trajectory directions are aligned

A high coherence score means the domains are moving together in a healthy direction. A low coherence score is a signal that the project's health signals are contradicting each other — worth investigating before making major changes.

### The Human Model Domain

The `human-comms` domain implements a persistent model of how a specific human wants to interact with agents. This is distinct from user personas (Section output formatting) — it captures the actual observed preferences of the individual.

The human model tracks:
- Preferred output verbosity (terse / standard / detailed)
- Preferred format (prose / structured / mixed)
- Cadence preferences (immediate / batched / on-request)
- Escalation thresholds (when to interrupt vs. handle autonomously)
- Correction history (recurring misunderstandings to avoid)

This state is persisted in the CMDB layer, versioned, and updated by the sensory tool when feedback is observed. An agent working with the same human over time should become increasingly efficient — not through model fine-tuning but through declared, readable state.

The human model is advisory: it does not affect the unified score. But it affects every output the Brain produces. It is the reason "communication is an interface, not a side effect" is a first-class design principle.

### Secret Reference Safety

The `secret-refs` domain solves a specific, common problem: agents need to know WHERE secrets live (to write code that accesses them correctly) but must never know WHAT the secrets are (to prevent exfiltration).

The domain maintains a `secret_catalog` containing reference patterns — rendered access code, not values:

```json
{
  "secret_catalog": [
    {
      "name": "DATABASE_URL",
      "environment": "production",
      "access_pattern": "os.environ['DATABASE_URL']",
      "location": "environment variable",
      "last_verified": "2026-04-01"
    },
    {
      "name": "STRIPE_API_KEY",
      "environment": "production",
      "access_pattern": "vault.read('secret/stripe/api_key')",
      "location": "HashiCorp Vault at path secret/stripe/api_key",
      "last_verified": "2026-03-28"
    }
  ]
}
```

The safety guarantee is **positive containment**: if a secret is not in the manifest, the agent does not know it exists. Agents write correct access code using reference patterns. They never see values. The catalog is committed to the repository (reference patterns are not secrets) and reviewed like any other code.

This pattern eliminates the common failure mode where agents either cannot access secrets at all or are given credentials directly in context.

### Attention Budget

Every agent recommendation system risks overwhelming its consumer with too many recommendations. The Brain enforces an attention budget: a configurable maximum number of recommendations per output, prioritized by:

1. Active gate violations (blocking)
2. Active floor constraints (score-capping)
3. Hat-weighted domain priorities (context-appropriate)
4. Trajectory classification (degrading domains first)

Recommendations beyond the budget are suppressed. This is not a user experience polish decision — it is a design principle. An agent that surfaces 23 recommendations is not helping the human prioritize; it is delegating prioritization back to the human. The Brain should surface the N most important actions, not everything it notices.

Hat context shapes which recommendations surface within the budget. The same Brain output with an "engineer" hat versus a "security" hat will recommend different things — not because the signals differ, but because the attentional lens changes which signals are relevant to the current task.

---

## 7. Governance

### Gates

Gates block actions until conditions are met. A gate is a declared condition attached to a lifecycle event: commit, merge, deploy, or a custom trigger.

```json
{
  "gate": "pre-deploy",
  "trigger": "deploy",
  "conditions": {
    "unified_score_min": 75,
    "domains": {
      "deploy-readiness": { "min": 80 },
      "test-health": { "min": 70 }
    },
    "no_floor_constraints_active": true
  },
  "blocking": true,
  "message": "Project must reach score 75 with deploy-readiness >= 80 before deploying"
}
```

Gates are declared in `brain-registry.json`. They are not enforced by the CI system or a separate service — they are enforced by hooks that run the Brain and check gate status before allowing the action to proceed.

A gate passes or fails. When a gate fails, the Brain output includes the failing conditions, the current values, and the gap to passing. The agent knows exactly what to work on to unblock the deploy.

### The Autonomy Gradient

LSP Brains defines a spectrum of agent autonomy levels, configurable per domain and per hat:

| Level | Description |
|-------|-------------|
| 0 — Observe | Sense and report. No recommendations, no actions. |
| 1 — Advise | Produce recommendations. Human executes. |
| 2 — Propose | Propose specific actions with detail. Human approves. |
| 3 — Execute with approval | Auto-execute after presenting plan. Human can reject. |
| 4 — Execute autonomously | Auto-execute within defined boundaries. Audit trail only. |

The autonomy level governs what the Brain's motor neurons (skills and hooks) can do without explicit human approval. A domain configured at level 2 will propose CMDB refresh actions for human approval. The same domain at level 4 will auto-execute them and write to the proposal ledger.

The gradient is critical for trust development. New domains start at level 1. As the team validates the Brain's recommendations over time, they can promote domains to higher autonomy. The proposal ledger records outcomes — over time, the Brain's track record justifies higher autonomy.

### Prescriptive Autonomy

At full deployment, the Brain operates in a prescriptive autonomy mode:

- **Safe proposals auto-execute** — CMDB refreshes, sensory tool runs, formatting fixes within declared scope
- **Risky proposals require approval** — Architecture changes, dependency upgrades, gate configuration changes
- **Destructive actions are never auto-executed** — Regardless of confidence or autonomy level

This preserves the human's role as the reviewer of decisions, not checklists. The Brain handles routine maintenance. The human approves changes with meaningful consequences. The audit trail (proposal ledger) makes every autonomous action reviewable.

### Hats

Hats are the Brain's attentional bias system. They do not change what the Brain knows — they change which signals the Brain emphasizes when generating recommendations.

Five standard hats ship with NeuroGrim:

| Hat | Emphasis | When to Wear |
|-----|----------|-------------|
| **engineer** | Test health, code quality, git cleanliness | Active development |
| **reviewer** | Code quality, coherence, test coverage | Code review preparation |
| **operator** | Deploy readiness, gate status, CI health | Pre-deploy checklist |
| **security** | Security standards, secret refs, audit CVEs | Security review or incident |
| **visionary** | Coherence, trajectory, ecosystem health | Architecture or planning sessions |

Hats are declared in `brain-registry.json`. They are not personas — the agent does not pretend to be someone else. A hat is "reading the room" made mechanical: the current task context shapes which signals get attention first.

When the Brain produces output under a hat, the active hat is included in the output schema. Every consumer — human, subagent, parent Brain — knows which attentional bias shaped the recommendations.

### Human User Personas

Separate from hats (which are agent attentional lenses), personas adapt output format for human stakeholders:

| Persona | Receives | Format |
|---------|----------|--------|
| **Executive** | Score + trajectory + top risk | One number, one direction, one action |
| **Engineering Manager** | Domain breakdown, gate status, team velocity | Dashboard summary with trends |
| **Developer** | Full detail — domains, gates, recommendations, hat context | Complete Brain output |
| **Specialist** (security, data, etc.) | Filtered view emphasizing relevant domains | Domain-specific deep dive |
| **Product Manager** | Delivery risk, blockers, timeline impact | Risk-oriented summary |

The same Brain output data produces five different communications. The specification describes these as output transformations on the agent-output-schema JSON, not as different scoring runs. The data is identical; the interpretation layer adapts it to the consumer.

---

## 7.5 Two Protocols, Two Roles

LSP Brains uses two distinct wire protocols, not one. This split is deliberate, and it is the single largest architectural evolution since v2.0.

**MCP (Model Context Protocol)** is used for tool invocation. It fits two roles cleanly:

1. **Sensory tool discovery.** The Brain acts as an MCP client, invoking `check_<domain>` tools on sensory servers to pull observations of the external world into CMDBs. A Python script, Go binary, or remote HTTP service implementing the MCP contract is a valid sensory tool — the language is an implementation detail.
2. **Brain-as-tool to an LLM agent.** The Brain acts as an MCP server, exposing `get_health_score`, `get_recommendations`, `propose`, and similar tools that an LLM agent (Claude Code, Cursor, VS Code extensions) can invoke while the human is in the loop.

**A2A (Agent2Agent Protocol)** is used for peer communication. It fits two other roles cleanly:

1. **Fractal composition.** A parent Brain invokes a child Brain as a peer. The parent fetches the child's Agent Card at `/.well-known/agent-card.json`, creates an A2A task requesting a fresh score, and receives a `score.updated` message whose payload is the child's full Interface Contract output.
2. **Dual brain coordination.** A local Brain (developer terminal) and an external Brain (cloud, CI, issue tracker hub) exchange 10 canonical message types — `score.updated`, `gate.changed`, `ecosystem.scored`, `incident.detected`, and others — wrapped in A2A envelopes that carry idempotency keys and task correlation IDs.

Why the split? MCP was designed for "an LLM calls a tool." Peer Brains are not tools — they are autonomous agents that run tasks, stream progress, and negotiate over capability declarations. Forcing one protocol into both roles either reinvents task lifecycle on top of tool-calling or strips context from peer coordination. The hybrid keeps each protocol inside its role and lets each do what it's designed for.

```
┌────────────────┐                 ┌────────────────────┐                    ┌────────────────┐
│ Sensory tools  │  MCP tools/call │                    │  A2A tasks + msgs  │  Peer Brain    │
│ (lint, test,   │ ───────────────►│    Your Brain      │◄─────────────────► │ (parent, child,│
│  git, custom)  │                 │                    │                    │  external dual)│
└────────────────┘                 │                    │                    └────────────────┘
                                   │                    │
┌────────────────┐  MCP tools/call │                    │
│ LLM agent      │ ◄──────────────►│                    │
│ (Claude Code)  │                 └────────────────────┘
└────────────────┘
```

**Validation discipline.** Three JSON Schemas govern the wire:
- `cmdb-envelope-v1.schema.json` — what sensory tools produce (MCP boundary).
- `agent-output-v1.schema.json` — what the Brain produces (MCP output or A2A message payload).
- `a2a-envelope-v1.schema.json` — how peer Brains exchange messages (A2A boundary).

Plus Agent Cards (`agent-card-v1.schema.json`) — static self-description published once by each A2A peer. See spec §13 and Appendix G for the full A2A contract.

---

## 8. Fractal Composition

LSP Brains is designed to work at project scale AND ecosystem scale. The same pattern repeats without modification.

### Project Scale

A single project has one `brain-registry.json`, a set of domain CMDB files, and a Brain that scores it. The output is the `agent-output-schema.json` — a versioned contract that any consumer can read.

### Ecosystem Scale

A parent Brain registers child projects and consumes their `agent-output-schema.json` outputs as domain inputs. The child's unified score becomes a domain score in the parent. Cross-project incident patterns can fire when conditions across multiple children are met simultaneously.

```
Ecosystem Brain (parent)
├── Project Brain A (child)
│     ├── test-health
│     ├── code-quality
│     ├── deploy-readiness
│     └── LSP tools (sensory neurons)
│
├── Project Brain B (child)
│     └── (same structure, different domain configuration)
│
└── Project Brain C (child)
      └── (same structure, different domain configuration)
```

Each child is self-describing. The parent does not understand child internals — it consumes structured `agent-output-schema.json` outputs. Adding a child is a registry declaration. The dependency graph is code.

### Recursive Confidence

Confidence flows through the hierarchy. A parent Brain scoring a child at 82 checks the child's output confidence before trusting the 82. A stale or low-confidence child score is confidence-decayed at the parent level, just as a stale CMDB file is confidence-decayed at the project level.

This prevents a confident parent score from being built on uncertain child foundations. Honesty is fractal.

### Cross-Project Incident Patterns

At ecosystem scale, the correlation engine fires across project boundaries:

```json
{
  "pattern": "ecosystem_test_regression",
  "type": "compound_risk",
  "condition": {
    "projects": ["service-a", "service-b"],
    "signal": "test-health.velocity < -5.0",
    "window_days": 3
  },
  "escalation": "engineering-manager",
  "recommendation": "Investigate shared test infrastructure change"
}
```

When two services both show test-health declining at more than 5 points per day over three days, this is more likely a shared infrastructure issue than two independent problems. The ecosystem Brain surfaces this; individual project Brains cannot.

---

## 9. Adopting LSP Brains

### The Six-Step Adoption Ramp

LSP Brains is designed for absorption — you bring the methodology to an existing project, not the other way around. The ramp progresses in six steps:

**Step 1: Declare domains in brain-registry.json**

Start with the three universal weighted domains: `test-health`, `code-quality`, `deploy-readiness`. Add advisory domains as needed. No code changes to your project — just a new JSON file.

```json
{
  "project": "my-service",
  "version": "1.0",
  "domains": {
    "test-health": { "weight": 0.40 },
    "code-quality": { "weight": 0.35 },
    "deploy-readiness": { "weight": 0.25 }
  }
}
```

**Step 2: Run sensory tools**

The built-in sensory tools detect state automatically from your existing files. No instrumentation required. Run them against your project:

```bash
neurogrim sense --all
```

This writes CMDB files. Inspect them. The signals are transparent JSON — you can see exactly what the Brain knows.

**Step 3: Score health**

```bash
neurogrim score
```

You now have a unified score with domain breakdown and confidence context. If the score looks wrong, check the CMDB files — the Brain's reasoning is fully visible.

**Step 4: Define gates**

Add gates to `brain-registry.json` for the lifecycle events that matter. Start with one gate: `pre-commit` with a minimum unified score. Add more as you gain confidence.

**Step 5: Wire hooks**

Configure hooks in your repository's hook system (`.claude/settings.json` for Claude Code, pre-commit hooks, CI steps). Hooks run the sensory tools and Brain automatically on relevant events.

**Step 6: Add hats**

Declare hats that match how your team works. The five standard hats cover most teams. Add custom hats for project-specific attentional patterns (e.g., a "data-migration" hat that amplifies schema health and test coverage for migration-adjacent code).

### Adoption Guide

The LSP Brains repository includes a language-agnostic adoption guide
(`adoption-guide/WHAT-IS-A-STARTER-KIT.md`) that walks through building a starter kit
for any stack. It describes the minimum: three domains, a registry, CMDB files, and the
"declare → score → hook" workflow that completes in under an afternoon.

Use the adoption guide as a reference pattern, not a dependency. The reference
implementation (NeuroGrim, Rust) demonstrates one conformant realization; your
own starter kit can target Python, Go, TypeScript, or any language whose output can
match the CMDB envelope schema.

### The Python SDK

Custom sensory tools can be written in any language that produces the correct CMDB JSON schema. A Python SDK is provided for teams that prefer not to write raw JSON:

```python
from lsp_brains import SensoryTool, CMDBWriter

class MyCustomTool(SensoryTool):
    domain = "my-domain"

    def sense(self, project_root: str) -> dict:
        # Detect signals from project_root
        return {
            "signal_a": True,
            "signal_b": 42,
            "raw_score": 78
        }

# Run and write CMDB automatically
writer = CMDBWriter(project_root="/path/to/project")
writer.run(MyCustomTool())
```

The SDK handles CMDB file format, timestamping, and schema validation. Custom tools integrate into the same scoring pipeline as built-in tools without modification.

### The MCP Server

NeuroGrim includes an MCP server that exposes Brain operations as tools for AI agents:

```bash
neurogrim serve --mcp
```

With the MCP server running, agents can invoke Brain scoring, query domain status, check gate conditions, and retrieve recommendations as structured tool outputs. This is how Claude Code sessions integrate with the Brain natively — the agent calls MCP tools rather than parsing CLI output.

### Progressive Adoption

You do not need all ten domains on day one. A useful adoption sequence:

| Phase | Domains | Value delivered |
|-------|---------|-----------------|
| Week 1 | `test-health`, `code-quality`, `deploy-readiness` | Baseline unified score, CI gate |
| Week 2-3 | `git-health`, `security-standards` | Operational visibility, security posture |
| Month 2 | `secret-refs`, `coherence` | Safe credential references, meta-health |
| Month 3+ | `human-comms`, `subagent-health`, `rust-health` | Adaptive communication, multi-agent coordination |

The Brain is useful at Phase 1. It becomes comprehensive over time. New domains do not require changes to existing domains or to the Brain engine — they are purely additive.

---

## 10. What Success Looks Like

### For an Individual Developer

You run `neurogrim score` before a commit. The output tells you the unified score, which gates pass, which fail, and what to fix. You fix the two blocking items. The gate passes. You commit with confidence.

The Brain is not a checklist you fill out manually. It reads your project state automatically. You do not maintain it — you work, and it watches.

### For a Team

Gates enforce minimums before merges and deploys. The team stops debating whether "this is good enough to ship" on every PR. The Brain has a declared standard. The standard is in the repository. It applies to everyone equally.

The proposal ledger records what the Brain recommended and what the outcomes were. Over time, the team promotes effective recommendations to higher autonomy. Routine maintenance happens without interruption; consequential decisions still come to humans.

### For an Engineering Manager

The Brain produces manager-persona output: domain trends, gate health, team velocity. You can see which projects are improving, which are degrading, which have active floor constraints. You get signal, not noise — the attention budget ensures the most important items surface.

Trajectory intelligence means you can catch a project in early decline before it becomes a crisis. A score of 72 is fine. A score of 72 degrading at -4 per day for a week is not.

### For the Ecosystem

At ecosystem scale, the parent Brain surfaces cross-project patterns that are invisible at the project level. A dependency upgrade that degrades test health across three services simultaneously. A CI infrastructure change that causes correlated deploy-readiness drops. These patterns fire recommendations at the ecosystem level — the kind of signal that traditionally requires a post-incident review to notice.

### The North Star

The ultimate measure of success: an engineer reads this whitepaper, follows the adoption guide to build a starter kit for their stack, declares three domains, writes one custom sensory tool, wires a pre-commit hook, and — within an afternoon — has an agent that can answer three questions:

1. How healthy is my system?
2. What should I fix first?
3. What should I focus on right now?

Those three questions — health, priority, focus — are what LSP Brains is built to answer. The Brain is not a deployment pipeline, a test runner, or a code reviewer. It is the persistent, honest, self-updating model of project health that gives agents the context they need to act well.

---

## Appendix A: Core Concepts Glossary

| Term | Definition |
|------|-----------|
| **Brain** | The scoring engine that aggregates domain signals into a unified score with recommendations and governance |
| **Brain Engine** | The core Rust library implementing the LSP Brains specification; produces the unified score |
| **CMDB** | Configuration Management Database; a domain-specific JSON file written by a sensory tool, read by the Brain |
| **Coherence** | The meta-domain that scores how well other domains relate to each other |
| **Confidence decay** | Exponential reduction in CMDB signal weight as a function of age |
| **Domain** | A named area of project health with its own sensory tool, CMDB, scoring rules, and optional gate conditions |
| **Floor constraint** | A rule that caps the unified score when a domain score falls below a minimum |
| **Gate** | A declared condition that blocks a lifecycle event (commit, merge, deploy) until health conditions are met |
| **Hat** | An attentional lens that biases the Brain's recommendation priority toward a specific task context |
| **LSP Brains** | The language-agnostic specification for building agent nervous systems |
| **NeuroGrim** | The reference implementation of LSP Brains, written in Rust |
| **Persona** | An output formatting profile that adapts Brain output for a specific human stakeholder type |
| **Proposal ledger** | A persistent record of Brain recommendations and their outcomes; the learning substrate |
| **Sensory tool** | A process that detects state in one domain and writes a CMDB JSON file |
| **Trajectory** | Velocity, acceleration, and classification computed from score history |
| **Unified score** | The weighted aggregate of domain effective scores (0-100); the Brain's primary output |

---

## Appendix B: The agent-output-schema

The Brain's primary output is a versioned JSON document. Key fields:

```json
{
  "schema_version": "2.0",
  "project": "my-service",
  "scored_at": "2026-04-13T14:22:00Z",
  "active_hat": "operator",
  "unified_score": 78,
  "unified_confidence": 85,
  "floor_constraint": null,
  "trajectory": {
    "velocity": 2.1,
    "acceleration": 0.3,
    "classification": "improving",
    "samples": 12
  },
  "domains": {
    "test-health": {
      "raw_score": 87,
      "effective_score": 74,
      "confidence": 85,
      "weight": 0.40,
      "trajectory": { "velocity": 3.2, "classification": "improving", "samples": 12 }
    },
    "deploy-readiness": {
      "raw_score": 91,
      "effective_score": 91,
      "confidence": 100,
      "weight": 0.25,
      "trajectory": { "velocity": 0.0, "classification": "stable", "samples": 12 }
    }
  },
  "gates": {
    "pre-deploy": { "status": "pass", "conditions_checked": 3, "conditions_failed": 0 }
  },
  "recommendations": [
    {
      "priority": 1,
      "domain": "test-health",
      "action": "Increase test-to-source ratio above 0.3",
      "context": "Current ratio is 0.28; test-health velocity is improving, maintain momentum",
      "autonomy_level": 1
    }
  ]
}
```

This schema is stable across Brain versions within a major version number. Consumers validate against it. Breaking changes require a major version bump.

---

## Appendix C: Further Reading

- **LSP Brains Specification** — https://github.com/KeenanHoffman/LSP-Brains
- **NeuroGrim Repository** — reference implementation source
- `roadmap/VISION.md` — seventeen guiding principles (1–17) and the north star
- `roadmap/ROADMAP.md` — stage progression and current implementation status
- `.claude/skills/brain.md` — operational Brain usage guide
- `.claude/skills/hats.md` — hat system documentation and hat-persona pairing
- `.claude/skills/gate-system-overview.md` — gate state machine architecture
- `domains/laas/` — archived first-customer domain implementation (read-only reference)
