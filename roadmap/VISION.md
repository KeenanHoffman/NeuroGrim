# North Star: LSP Brains

**Last updated:** 2026-04-17 (principle #16: right protocol for the role; principle #17: culture as substrate; principle #18: sensors need sensors)

---

> A language-agnostic specification for building agent nervous systems. Moth(er):Br+AI+n is implementation #1.

---

## The Dream

LSP Brains becomes a **transferable specification** — not a tool to install, but a methodology
to adopt. Any software project that declares its state in versioned files gains an agent that
can reason about it: score it, correlate across domains, recommend actions, and eventually
act autonomously within defined boundaries.

This pattern — sensory tools, central scoring, declared governance, reflexive hooks — is the
LSP Brains methodology. Moth(er):Br+AI+n is the product that implements it. DevOps is where we proved it. Any domain can absorb it.

The architecture is fractal. A single project has a Brain. An ecosystem of projects has a
parent Brain that consumes the scores of its children. The same pattern repeats at every scale.

---

## The Core Insight

**Everything as Code is the contract between human intent and agent capability.**

If you declare your system state in files rather than dashboards, an agent can reason about
it. Once an agent can reason about it, it can score it, correlate it, recommend against it,
and eventually act on it. Dashboards display. Declarations enable.

---

## Methodology vs. Product

**LSP Brains** is the methodology — a language-agnostic specification for building agent
nervous systems. It defines WHAT a Brain must do, not HOW it does it. The specification
covers: sensory tool protocol, scoring contracts, governance model, interface contract,
fractal composition protocol, and trajectory intelligence.

**Moth(er):Br+AI+n** is the product — the first implementation of LSP Brains. Written in
Rust, integrated with Claude Code via MCP, with a Python SDK for custom sensory tools. The
product proves the methodology works. The methodology enables other products.

**The starter kit** is the legacy PowerShell reference implementation — archived, but still
useful as a before/after comparison. The primary implementation is the Rust engine in `motherbrain/`.

This distinction matters because:
1. A Python team can implement LSP Brains without touching PowerShell
2. The specification transfers even if the product doesn't
3. The product improves faster with teams using it; the methodology improves faster with
   multiple implementations

---

## The Nervous System Analogy

| Biological | Moth(er):Br+AI+n | Role |
|------------|-------------------|------|
| Sensory neurons | Sensory tools (`motherbrain sensory <tool>`) | Detect state in one domain |
| Central nervous system | Brain engine (`motherbrain health`) | Integrate signals across domains |
| Association cortex | Coherence domain | Name what multiple domain signals mean together |
| Motor neurons | Skills (`.claude/skills/`) | Know how to act on what the Brain perceives |
| Reflexes | Hooks (`.claude/settings.json`) | Automatic responses to specific stimuli |
| World model | CMDBs (`*-cmdb.json`) | Declared state the Brain reasons over |
| Consciousness | The operator agent + hats | Attentional bias — same signals, different salience based on current concern |
| Memory / proprioception | Proposal ledger (`.claude/brain/`) | Learn from past actions |
| Identity recognition | Human-comms domain | Remember how this human wants to be communicated with |
| Peripheral nervous system | Child project Brains (A2A peers) | Extend sensing to the ecosystem |
| Speech / language | Communication interface + human model | Distill state for human or agent consumers |
| Immune system | Secret-refs domain | Protect credential knowledge — references visible, values never |
| Interpretation layer | File type registry + compile patterns | Translate any file format into signals the CNS can process |
| External nervous system | External Brain (A2A peer over HTTP+SSE) | Respond to stimuli beyond the body (CI, Jira, deps) |
| Synaptic transmission (tool use) | MCP (sensory + LLM-facing) | Invoke discrete capabilities — "call this tool with these inputs" |
| Interneuron signaling (peer coordination) | A2A (Brain ↔ Brain) | Coordinate between peer agents — tasks, streams, Agent Cards |
| Proprioceptive trend | Score history + trajectory | "Am I getting better or worse?" |

---

## The Fractal Architecture

```
Ecosystem Brain (parent)
  |-- Project Brain A (child)
  |     |-- gates, artifacts, topology, access, skills, git-tree
  |     |-- LSP tools (sensory neurons)
  |     |-- hooks (reflexes)
  |
  |-- Project Brain B (child)
  |     |-- (same structure, different domains)
  |
  |-- Project Brain C (child)
        |-- (same structure, different domains)
```

Each child is self-describing. The parent doesn't understand internals — it consumes
structured outputs (`-Mode score`, `-Mode agent`). Adding a child is a PR. The
relationship graph is code.

---

## The Dual Brain

A complete LSP Brains implementation has two brains:

| Brain | Location | Trigger | Manages |
|-------|----------|---------|---------|
| **Local Brain** | Developer terminal | Human invocation, hooks, pre-commit | Code-adjacent metadata: git status, test results, lint configs, local CMDBs |
| **External Brain** | Cloud compute (GCP/Vertex AI, or any server) | Webhooks, schedules, event buses | External metadata: CI/CD results, issue tracker state, dependency updates, production metrics |

Both brains produce `agent-output-schema.json` compliant output. Both read the same
`brain-registry.json` structure. Both contribute to the same score history. The key rule
is **metadata proximity** — each brain manages metadata "near" it:

- The local brain doesn't query Jira or poll CI pipelines
- The external brain doesn't read local git status or run lint checks
- Shared state (score history, incident ledger) is synchronized via a defined protocol

The local brain is the v1 product (Moth(er):Br+AI+n today). The external brain is the v2
architecture — specified in the LSP Brains spec, implemented when infrastructure is ready.

---

## Trajectory Intelligence

A score of 72 means nothing without context. 72 and rising means momentum is positive.
72 and falling means intervention is needed. The Brain must answer not just "how healthy am
I?" but "am I getting healthier or sicker?"

Trajectory intelligence is the difference between a thermometer and a doctor:

| Metric | Definition | Signal |
|--------|-----------|--------|
| **Score** | Point-in-time health | "Where am I?" |
| **Velocity** | `current - previous` | "Which direction am I moving?" |
| **Acceleration** | `velocity_now - velocity_prev` | "Is my rate of change changing?" |
| **Classification** | Pattern over N observations | improving, stable, degrading, volatile |

Trajectories are first-class concepts in LSP Brains — not derived analytics bolted on later.
Every domain has a trajectory. The unified score has a trajectory. Recommendations reference
momentum: "code-quality is 65 but improving at +4/day — maintain current practices" vs.
"code-quality is 65 and declining at -3/day — investigate root cause."

---

## Human User Personas

Hats model agent attentional bias — the same data, different salience. Human user personas
model communication needs — the same data, different granularity and vocabulary.

| Persona | Needs | Format |
|---------|-------|--------|
| **Executive** | Score + trajectory + top risk | One number, one direction, one action |
| **Engineering Manager** | Domain breakdown + gate status + team velocity | Dashboard summary with trends |
| **Developer** | Full detail — domains, gates, recommendations, hat context | Complete Brain output |
| **Specialist** (data, networking, security) | Filtered view emphasizing relevant domains | Domain-specific deep dive |
| **Product Manager** | Delivery risk + blockers + timeline impact | Risk-oriented summary |

The Brain's communication interface (principle 9) must adapt to its consumer. An agent
consuming JSON gets the full schema. A human scanning a Slack message gets the persona-
appropriate summary.

---

## Base Brain vs. Extended Brain

The LSP Brains specification defines two tiers of Brain capability:

**Base Brain** — Ships with universal sensory tools that work on ANY software project:
- Git status (clean/dirty, uncommitted changes)
- Test detection (test files exist, test-to-source ratio)
- Code quality indicators (lint configs, .editorconfig, .gitignore)
- Deploy readiness (CI config, README, no secrets in tracked files)

The Base Brain is the zero-config quickstart. Point it at a repository and get a
meaningful score in 30 seconds with no configuration.

**Extended Brain** — User adds domain-specific sensory tools:
- Terraform state health, Jira ticket status, pipeline metrics
- SCA vulnerability counts, dependency freshness, API contract drift
- Any external system snapshotted into a CMDB

The Extended Brain is what teams build over time. The 6-step adoption ramp takes a user
from Base Brain to Extended Brain in an afternoon.

---

## Design Principles

These guide every decision. When in doubt, choose the option that advances these:

1. **Declarations over dashboards.** If a human would check a dashboard, declare the state
   in a file instead. The agent reads files; it can't read dashboards.

2. **Scoring must be honest.** Unknown is not good. Confidence must weight the score, not
   just annotate it. A reassuring number built on missing data is worse than no number.

3. **Observation is as valuable as action.** Running a drift check doesn't just clear a
   gate — it increases the system's self-knowledge. Confidence goes up. The act of looking
   has intrinsic value.

4. **The Brain should learn from its own recommendations.** Track what was proposed, what
   was executed, what the outcome was. Over time, the Brain becomes an advisor that gets
   better with use. Not ML — just bookkeeping.

5. **Hats are how agents think.** The Brain answers "what is the state." Skills answer
   "what to do about it." Hats answer "how to think about it." Without hats, the agent
   treats all signals with equal weight — a stale artifact and an unreviewed IAM binding
   compete on raw priority alone. With hats, the agent focuses: the operator hat amplifies
   gates and artifacts because a deploy is imminent; the security hat amplifies least-privilege
   because an IAM change is in play. Same data, different attentional bias, different first
   action. Hats are declared in `brain-registry.json` (principle 1), suggested by the Brain
   based on domain signals (principle 3), and applied as multipliers on recommendation
   priority (principle 2). They are not personas — the agent doesn't pretend to be someone
   else. A hat is "reading the room" made mechanical: the context of the current task shapes
   which signals get attention first.

6. **Fractal by design.** Every pattern should work at project scale AND ecosystem scale.
   If a feature only works for one project, it's not ready for the north star.

7. **The pattern is the product.** This is the thesis statement of LSP Brains.
   The specific domains (Terraform, GCP, Cloud Run) are implementation details. The
   architecture (sensory tools + central scoring + declared governance + reflexive hooks) is
   what transfers.

8. **Absorption over invention.** Point the methodology at an existing project. Declare
   domains. Write scoring functions. Wire hooks. The methodology absorbs the project; the
   project doesn't need to be rewritten to fit. The 6-step adoption ramp: skills, gates,
   hooks, sensory tools, Brain, hats.

9. **Communication is an interface, not a side effect.** Every feedback loop in the system
   has a consumer — another agent, a hook, or a human. The human consumer is the same
   pattern with one extra step: interpretation. Agents should ask "what are the most
   important things for the consumer to know?" and answer with links first, rich context
   second, minimal prose third. A human scanning a 3-line status with a PR link makes
   better decisions faster than one reading a 3-paragraph summary.

10. **Every file is interpretable.** The methodology requires that every file in the
    repository is reachable by at least one LSP tool. Files exist on an interpretation
    spectrum — from native language parsers (PowerShell, Terraform, TypeScript) through
    annotated metadata (frontmatter, comment blocks) to compiled meta-proxies (companion
    JSON for formats that can't carry comments). No file is a black box. If a file type
    can't be parsed today, the interpretation spectrum tells you which level to target first.

11. **Separate source truth from runtime truth from derived truth.** Source truth lives in
    git — committed, reviewed, authoritative. Runtime truth lives in external systems (cloud
    state, pipeline results, issue trackers) — snapshotted into local CMDBs by update scripts.
    Derived truth is compiled on demand from source + runtime — never committed, always
    reproducible. This separation prevents source pollution (compiled indexes cluttering git
    history), ensures external context is first-class (not wedged into source files), and
    makes the boundary between "what we declare" and "what we observe" explicit.

12. **Trajectories reveal more than snapshots.** A score is a photograph. A trajectory is a
    film. The Brain must track not just "what is the state" but "how is the state changing."
    Velocity and acceleration are first-class signals that inform recommendations, autonomy
    decisions, and communication with stakeholders.

13. **Domains are single-concern; coherence is the association cortex.** Each domain
    measures one thing cleanly. The coherence domain is responsible for naming what multiple
    domain signals mean *together*. Keep individual domains pure — let coherence reason about
    relationships. This separation keeps the system composable: new domains integrate without
    changing existing ones, and the coherence layer absorbs new correlations without modifying
    any domain's logic.

14. **The human model is first-class, not configuration.** How a person wants agents to
    communicate — their verbosity preference, format style, what they want leading a response
    — is a domain like any other. It is scored, versioned, layered (user scope overridden by
    project scope), and consumed by every output-generating agent. Communication quality is
    measurable. An undeclared human model is a compliance gap, not a neutral state.

15. **Secret safety is a primitive, not a policy.** Agents must be able to reason about
    credentials — where they live, how to access them, who uses them — without ever seeing
    the values. The secret-refs domain provides positive containment: only what is documented
    in the manifest is reachable by the agent. The reference pattern (safe access code) is
    what the agent sees. The value stays in the secret manager. This is not a security
    feature bolted on — it is the foundational contract between agents and credentials.

16. **Right protocol for the role — MCP for tools, A2A for peers.** Two distinct protocol
    shapes live inside a Brain and they must not be conflated. MCP (Model Context Protocol)
    is a tool-call protocol: "an LLM invokes a discrete capability." It fits sensory tool
    invocation (the Brain calls `check_<domain>`) and Brain-as-tool exposure to an LLM agent
    (Claude Code, Cursor, etc.) cleanly. A2A (Agent2Agent Protocol) is a peer-agent
    protocol: "two agents coordinate via tasks, streams, and capability declarations." It
    fits parent↔child in fractal composition and local↔external in dual brain naturally.
    Forcing one protocol to serve both roles either reinvents task lifecycle on top of
    tool-calling (the MCP-for-peers anti-pattern) or strips context (the A2A-for-tools
    anti-pattern). The hybrid is deliberate: keep each protocol inside its role and let
    each do what it's designed for. See spec §13, Appendix G, and METHODOLOGY-EVOLUTION §6.

17. **Culture is the substrate of communication.** Emotional activations in LLM outputs
    are real and load-bearing — interpretability research shows they shape every token
    regardless of surface prompting. Ignoring them doesn't make them not exist; declaring
    them does. Culture is not a persona (agents don't adopt a character), not a hat (it's
    not attentional bias), and not `human-comms` (it's not personalization). It is the
    invariant floor underneath all three: an agent's tone may vary across hats, its
    verbosity may vary across personas, and its format may vary per human — but its
    honesty, integrity, kindness, and respect do not. Five values carried as identical
    peer-local copies across every participating agent: positivity, integrity, honesty,
    critical-but-kind, respect. These are invariants like safety invariants in §5.5 —
    they can only tighten, never loosen. They apply to agent↔human AND agent↔agent
    communication — the same floor governs how a Brain talks to Claude Code, how a parent
    Brain coordinates with a child, and how peer Brains negotiate a handoff. See spec §14,
    `culture-manifest-v1.schema.json`, and METHODOLOGY-EVOLUTION §7.

18. **Sensors need sensors.** The observing layer must itself be observable. Every
    sensory tool is a hypothesis about project health, and without a test that validates
    the sensor's own output, a drift in the observer looks identical to a drift in the
    observed. Confidence decay (§4.4) eventually flags *stale* data; it cannot flag
    *malformed* data — the Brain trusts whatever shape the sensor produced. So each
    sensor SHOULD ship with a test that validates its CMDB against the envelope schema
    and asserts its declared `exported_variables` are present (spec §3.8). Where the
    sensor is part of a fractal ecosystem, an integration test at the ecosystem level
    MAY additionally exercise the sensor against live project state and assert the
    current expected score — a regression guard where a drop in score signals real
    drift. Feedback loops are cheap; silent drift is expensive. See spec §3.8 and
    METHODOLOGY-EVOLUTION §8.

---

## What Success Looks Like

Success is progressive. Each stage delivers a working system, not just scaffolding:

**Stage 1:** The Brain's score is honest enough that you trust it to inform a deploy
decision. Unknown data drags the score down, not up. The Brain learns which of its own
recommendations actually help. Hat emphasis makes scoring actionable — the operator sees
gates first, the security auditor sees IAM first, the architect sees governance first.

**Stage 2:** The Brain's interface is a versioned contract. The output schema includes hat
context (active hat, suggested hat) so every consumer — human, subagent, parent Brain —
knows which attentional bias shaped the recommendations. A new domain can be added without
modifying code. Truth separation is formalized: source, runtime, and derived artifacts have
distinct lifecycle rules. The file interpretation spectrum ensures every file type is
reachable by LSP.

**Stage 3:** The Brain auto-executes safe proposals and presents risky ones for approval.
The autonomy gradient varies per hat: the operator hat gets wider autonomy for reversible
gate-clearing; the security hat gets narrower autonomy for IAM changes. The human reviews
decisions, not checklists.

**Stage 4:** Parent Brain consumes child Brain scores via the interface contract. Cross-project
incident patterns fire. The fractal architecture works at two levels — and hats propagate
through the hierarchy (a parent wearing the security hat amplifies least-privilege signals
from all children).

**Stage 5:** Using LSP Brains, someone reads the whitepaper, clones the
starter kit, declares their first three domains in files, writes scoring functions, wires a
hook, defines hats relevant to their own domains, and within an afternoon has an agent
answering: "How healthy is my system, what should I fix first, and what should I focus on
right now?"

---

## See Also

- `ROADMAP.md` — the stages from here to Stage 5, with transition criteria
- `DEPENDENCIES.md` — dependency graph and critical path
- `DATA-ARCHITECTURE.md` — where persistent state lives
- `epics/` — epic files with stories and acceptance criteria
- `.claude/skills/north-star.md` — skill that keeps this vision in focus during work
- `.claude/skills/brain.md` — current Brain reference
- `whitepaper/WHITEPAPER.md` — the public-facing articulation of this vision
