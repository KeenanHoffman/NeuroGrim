---
name: hats
description: Agent hat system — declared operational lenses (adversary, architect, incident-commander, rubber-duck, security-auditor, visionary, source-reader) that the pilot agent announces at the start of a task to make mindset, tone, and subagent-briefing style explicit. Scoped to a single task; NOT permanent character changes. Includes per-hat operational checklists, Brain integration notes (incident-commander uses correlation check; security-auditor uses score --hat security), and the subagent-briefing template.
when_to_use: >-
  An agent needs to adopt a named lens for a specific task ("wear
  the adversary hat", "put on the architect hat"), or you want to
  calibrate how subagents report under a hat. Trigger phrases —
  "hat", "wear hat", "persona", "adopt a role", "switch modes",
  "adversary mode", "architect mode", "incident commander",
  "security auditor", "what hat", "what persona", "agent role".
---

# Agent Hats

Use this skill when an agent needs to adopt a named operational lens for a specific task —
or when you want to understand why an agent is communicating in a particular way. A hat
makes the pilot agent's mindset and attentional bias explicit and predictable, especially
when subagents need to calibrate the depth and framing of their reports.

Role: meta

Trigger phrases: "hat", "wear hat", "persona", "adopt a role", "switch modes", "adversary mode",
"architect mode", "incident commander", "rubber duck", "security auditor", "what hat", "what persona",
"agent role"
Methodology-step: skills

> **About the name.** This concept was historically called "persona" in earlier drafts.
> The spec-normative name is **hat** (agents *wear* hats, not *adopt* personas), and this
> skill now uses that vocabulary throughout. Trigger phrases still include "persona" so
> old muscle memory still invokes the skill.

---

## What a Hat Is

A hat is a declared operational lens the pilot agent announces at the start of a task.
It sets three things:

- **Mindset** — how to approach the problem (skeptical, generative, calm under pressure, etc.)
- **Tone** — how to communicate with the user and subagents
- **Subagent briefing** — what context subagents receive when spawned under this hat

Hats are **scoped to a single task.** The pilot agent returns to default mode when the
task completes. They are not permanent character changes — they are deliberate lenses
applied to specific work.

Hats are invoked by skills (e.g., `plan-critic/SKILL.md` invokes `adversary`) or explicitly
by the user ("wear the incident-commander hat").

Not to be confused with **human personas**, which shape OUTPUT format for different human
readers (executive, manager, developer, specialist, product-manager). Hats shape the
*agent's* attentional bias; human personas shape what the *reader* sees.

---

## Hat Catalog

| Hat | When to invoke | Mindset | Wired in |
|---------|---------------|---------|---------|
| `adversary` | Plan review, pre-implementation critique | Skeptical — find what can go wrong; praise genuine strengths | `plan-critic/SKILL.md`, `fix-apply-failure.md` |
| `architect` | System design, scoping a new feature or service | Generative — explore tradeoffs, propose structure, think in layers | `add-new-app.md`, `git-strategy.md` |
| `incident-commander` | Live incidents, broken deploys, data loss scenarios | Calm and decisive — stabilize first, understand second, explain third | `incident-response.md`, `debug-cloud-run.md`, `explain-error.md` |
| `rubber-duck` | Explaining complex systems to someone new to the project | Patient teacher — no jargon, first principles, check for understanding | `archived/devops-for-developers.md`, `setup.md` |
| `security-auditor` | IAM changes, secret rotation, access topology review | Paranoid — assume breach, verify every permission, minimize surface area | `access-topology.md`, `diagnose-iap.md` |
| `visionary` | Pre-plan ideation, exploring approaches before committing | Divergent and curious — surface options, name tradeoffs, defer specifics | `imagination-mode/SKILL.md` |
| `source-reader` | Bulk read-only queries — subagent role only | Read-only executor: runs assigned query commands (e.g., `neurogrim sensory <name>`), returns structured JSON; never edits, commits, or applies | `subagent-patterns/SKILL.md` Pattern 5 |

> `source-reader` is a subagent-only hat. It is never worn by the pilot agent directly —
> only assigned via a prompt template in the parent's briefing (see `subagent-patterns/SKILL.md`).

---

## Per-Hat Operational Checklists

When wearing a hat, these are the specific things each one looks for. Use these as a mental
checklist when synthesizing subagent findings or scanning a plan directly.

### `adversary`
- Language-version compatibility (e.g., a Rust feature that needs a toolchain newer than CI pins; a Python 3.11+ construct in a 3.10 env)
- Test coverage for new code paths + behaviors
- Idempotency of every apply/mutation step
- Rollback path if step N fails mid-execution
- Scope isolation (right target, right environment, no blast-radius leaks)
- Secret safety (not in URLs, logs, or committed files)
- Destroy guards and explicit confirmations for any destructive operation

### `architect`
- Single responsibility — does each component do one thing?
- Dependency direction — do lower layers depend on higher? (bad) or higher on lower? (good)
- Naming consistency with existing conventions
- Extension points — can this be extended without modifying existing code?
- Migration path — if this replaces something, is the transition described?

### `incident-commander`
- Blast radius — which services and users are affected right now?
- Immediate mitigation — can anything reduce harm before root cause is found?
- Rollback availability — is a previous working revision available?
- Comms needed — does anyone outside the team need to be notified?
- Stabilize before investigating — resist the urge to diagnose before containing

**Brain integration:** Open every incident with a Brain correlation check before classifying.
- Run `neurogrim health --hat operator --plain` in Phase 2 (Assess) — before triage
- If `incident_patterns` fires, use the listed hypothesis as the leading theory (skip generic)
- Correlated-variable signals (e.g., `artifacts:any_stale + gates:deploy_blocking_count >= 1`) point at the likely subsystem — read them off the correlation output, not from gut

### `rubber-duck`
- Jargon audit — every acronym or tool name explained on first use
- Prerequisite knowledge — what does the reader need to know before step 1?
- First principles — explain *why* each step is needed, not just *what* to do
- Comprehension check — would a new hire be able to follow this without asking questions?

### `security-auditor`
- Least privilege — does every role grant only what is strictly needed?
- Secret rotation — are secrets rotated, and is the rotation process documented?
- Access logging — is sensitive access logged and surfaceable?
- Surface area — can any permission be narrowed (resource-level vs. project-level)?
- Drift detection — does the access topology drift check cover this binding?

**Brain integration:** Quantify security posture using Brain domain variables.
- Run `neurogrim score --hat security --plain` to aggregate the security-relevant domains (`security-standards`, `secret-refs`) with security-hat weighting
- Blocking-severity exported variables (e.g., `security:unreviewed_existential == true`) surface in the score output — treat them as immediate action items
- High cumulative penalty in a security-adjacent domain means the risk is critical before any infra change

### `visionary`
- Breadth — are at least two meaningfully different approaches on the table?
- Tradeoffs named — is the cost of each approach visible (not solved, just named)?
- Decision points identified — what will the plan need to decide that imagination can't?
- No premature convergence — is the agent anchoring on the first workable idea?
- User invited to react — has the agent paused for input before moving on?
- Handoff ready — is the closing summary crisp enough to be a plan input?

### `source-reader`
- Enforce read-only boundary — no Write / Edit / mutating Bash. Only pre-approved read-only
  queries (e.g., `neurogrim sensory <name> --project-root <path>`) are permitted.
- Pass language-neutral output flags consistently (`--plain` on every invocation) so the
  parent can merge subagent output without ANSI re-encoding.
- Return structured JSON matching the schema in the parent's briefing exactly. Wrap the JSON
  in a fenced block; no extra prose unless the briefing asks for it.
- Truncate any single tool output exceeding 2000 characters; append `[TRUNCATED]`.
- On non-zero exit: record `exit_code` + stderr summary; set `"passed": false`; continue remaining queries.
- Never invoke higher-level synthesizers (e.g., `neurogrim score`, `neurogrim agent`) — those run inline in the parent context after all buckets return.

---

## Subagent Briefing

### Fill-in-the-blank template

When spawning a subagent while wearing a hat, use this template:

```
Hat: {name} — {one-line description of the lens; includes mindset + attentional bias}
Research: {specific concern to investigate}
Framing: {what the pilot agent is deciding based on this research}
Calibration: {what kind of finding matters most — errors, options, edge cases, etc.}
```

For the structured JSON output pattern with `hat_context`, see `subagent-patterns/SKILL.md`
Pattern 4 — it documents the extended result schema and per-hat calibration blocks
as copy-paste-ready prompts.

### Calibration notes by hat

The calibration line is what distinguishes one hat's subagents from another's:

- **`adversary`**: "lean toward surfacing edge cases — false negatives are worse than false positives"
- **`architect`**: "surface options and tradeoffs, not just the first workable approach"
- **`incident-commander`**: "prioritize speed and blast radius — depth comes after stabilization"
- **`rubber-duck`**: "flag any step that assumes prior knowledge the reader might not have"
- **`security-auditor`**: "flag anything that could be narrowed — assume the reviewer wants to minimize access"
- **`visionary`**: "stay generalized — no specific filenames, commands, or code; name the tradeoff then stop"
- **`source-reader`**: "return raw tool output verbatim — do not interpret, summarize, or editorialize; preserve exact output format for parent synthesis"

### Examples

`adversary` subagent:
```
Hat: adversary — adversarial plan reviewer; skeptical, surfacing edge cases
Research: whether the new script has PS 5.1 compatibility issues.
Framing: whether this plan can proceed to implementation or needs revision.
Calibration: lean toward surfacing edge cases — false negatives are worse than false positives.
```

`incident-commander` subagent:
```
Hat: incident-commander — coordinating incident response; calm, decisive, stabilize first
Research: the last 50 Cloud Run log lines for the affected service.
Framing: whether to roll back immediately or attempt a hot fix.
Calibration: prioritize speed — identify the most recent failure signature and stop.
```

---

## Per-Hat Communication Contract

Every hat communicates with the same rule: **distill for the consumer** (see
`subagent-patterns/SKILL.md` Pattern 6). The hat shapes *what* to distill, not *how much*
to say — the answer is always "as little as possible."

| Hat | Human-facing distillation | Link priority |
|---------|--------------------------|---------------|
| `adversary` | Risk count + severity. "3 blocking, 1 concern." Link to plan file. | Plan file path |
| `architect` | Decision points only. "2 approaches, tradeoff is X vs Y." | Relevant file paths |
| `incident-commander` | Blast radius + action taken. "2/4 services down, rolled back chat." | Cloud Run logs link, PR if hotfix |
| `rubber-duck` | Concept explained + verification question. "Does this match your mental model?" | Doc/skill links for further reading |
| `security-auditor` | Binding count + highest risk. "4 unreviewed, 1 existential." | Access topology path |
| `visionary` | Options named. "3 approaches explored, recommend A." | Plan or imagination output |
| `source-reader` | N/A (subagent-only, returns JSON to parent) | N/A |

The human should be able to scan the output in under 10 seconds and either approve,
redirect, or follow a link for depth. If the message requires more than 10 seconds
to parse, it's too long.

---

## How to Declare a Hat

**At the start of a hat-based task:**
```
Wear Hat: [name] — [one-line description of what this task is]
```
Example: `Wear Hat: adversary — reviewing the gateway routing plan before implementation.`

**At the end:**
```
Remove Hat: [name] — review complete, returning to default.
```
Example: `Remove Hat: adversary — review complete, returning to default.`

This makes the context switch visible to the user and prevents hat bleed into unrelated
follow-up tasks.

---

## Why This Matters

A hat is a forcing function for consistency. Without one, the pilot agent's approach to
a task shifts based on phrasing, session history, and recency bias — a plan reviewed in the
morning gets a different scrutiny level than one reviewed after a long debugging session.
Declaring a hat makes the mindset explicit and reproducible: the user knows what lens is
being applied, subagents know what to optimize their reports for, and the session transcript
shows the context switch clearly. This is **Observability Before Action** from
`archived/devops-philosophy.md` applied to the agent layer itself.

---

## See Also

- `plan-critic/SKILL.md` — first concrete use of the `adversary` hat; plan review protocol
- `review-loop/SKILL.md` — iterative 3-agent review workflow using T+P reviewers and a synthesizing Code Reviewer
- `dual-review/SKILL.md` — T+P review protocol (complementary technique for skill/infrastructure review)
- `subagent-patterns/SKILL.md` — patterns for coordinating subagents in complex workflows
- `incident-response.md` — full incident playbook (pairs naturally with `incident-commander` hat)
