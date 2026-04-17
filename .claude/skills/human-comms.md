# human-comms

**Purpose:** Persistent human model — track how a specific human wants to receive
information from agents across projects and sessions.

The `human-comms` domain closes the gap between the Brain (world model) and the agent's
knowledge of the human it works with. Preferences are explicit, versioned, and visible
in `motherbrain health` — not guessed fresh each session.

---

## Two-File Architecture

```
~/.claude/human-comms.yaml          ← user-scoped defaults (NEVER committed)
    ↓ base layer
.claude/human-comms.yaml            ← project-scoped overrides (committed to repo)
    ↓ override layer
.claude/human-comms-cmdb.json       ← merged, scored, exported as domain variables
```

**Merge rule:** Project-scoped fields win over user-scoped fields. Anything not in the
project file falls through to the user file. Anything not in either file uses the
built-in default.

`per_hat` entries are merged one level deeper: per hat name, project wins per hat.

---

## Preference Schema Reference

Both files share the same YAML shape. Project-scoped files typically only contain
the fields they need to override.

```yaml
communication:
  include_urls: true          # Include relevant URLs in responses
  verbosity: standard         # brief | standard | full
  lead_with: answer           # answer | reasoning

format:
  code_blocks: always         # always | executable_only | inline_when_short
  lists_vs_prose: lists       # lists | prose | tables_when_comparing
  emoji: never                # never | contextual | encouraged

signals:
  proactive_hat_suggestions: true   # Suggest hat changes when appropriate
  alert_on_correlation_fire: true   # Surface correlation firings unprompted
  include_why_context: true         # Add "why this matters" to recommendations

interaction:
  ask_one_question: true            # When uncertain, ask one question not many
  confirm_completed_steps: false    # Silently execute vs narrate each step
  acknowledge_hat_announcements: true

per_hat:                            # Optional per-hat overrides
  visionary:
    verbosity: full
    lists_vs_prose: prose
  engineer:
    verbosity: standard
    lead_with: answer
  reviewer:
    verbosity: full
    include_why_context: true
```

---

## Scoring Model

Score = preference completeness. No preference is "wrong" — the score rewards having
an explicit contract over leaving the agent to guess.

```
+25  communication block has ≥1 key defined
+25  format block has ≥1 key defined
+25  signals block has ≥1 key defined
+25  interaction block has ≥1 key defined
```

| Score | Meaning |
|-------|---------|
| 0 | No preferences defined — agent is guessing |
| 25–75 | Partial contract — some categories undefined |
| 100 | Complete contract — all preference categories explicit |

---

## Running the Tool

```bash
# Refresh both layers and write CMDB
motherbrain sensory human-comms --project-root . > .claude/human-comms-cmdb.json

# Inspect merged preferences
cat .claude/human-comms-cmdb.json | jq '{score, include_urls, verbosity, lead_with, per_hat}'

# Full health with human-comms row
motherbrain health
```

---

## Domain Variables

All bool and number fields are auto-extracted. String fields need explicit
`exported_variables` in the registry (already configured).

| Variable | Type | Example |
|----------|------|---------|
| `human-comms:include_urls` | bool | `true` |
| `human-comms:verbosity` | string | `"standard"` |
| `human-comms:lead_with` | string | `"answer"` |
| `human-comms:code_blocks` | string | `"always"` |
| `human-comms:lists_vs_prose` | string | `"lists"` |
| `human-comms:emoji` | string | `"never"` |
| `human-comms:proactive_hat_suggestions` | bool | `true` |
| `human-comms:alert_on_correlation_fire` | bool | `true` |
| `human-comms:include_why_context` | bool | `true` |
| `human-comms:ask_one_question` | bool | `true` |
| `human-comms:confirm_completed_steps` | bool | `false` |
| `human-comms:acknowledge_hat_announcements` | bool | `true` |
| `human-comms:has_user_defaults` | bool | `true` |
| `human-comms:has_project_overrides` | bool | `true` |
| `human-comms:preferences_complete` | bool | `true` |

---

## How Agents Consume Preferences

Agents read preferences through the Brain context output like any other domain variable.
The pattern is: check before crafting output.

**Example — honoring `include_urls`:**
When `human-comms:include_urls = true`, include links to relevant documentation,
specifications, PRs, and external references whenever a topic has a canonical URL.

**Example — honoring `verbosity` per hat:**
Before responding, check whether the active hat has a `per_hat` override. If wearing
the `visionary` hat and `per_hat.visionary.verbosity = full`, use long-form prose
exploration rather than brief bullets.

**Example — honoring `lead_with`:**
When `lead_with = answer`, give the direct answer or result first, then reasoning.
When `lead_with = reasoning`, walk through the reasoning before the conclusion.

---

## Using Preferences in Correlation Rules

Human-comms domain variables are first-class citizens in `condition_tree` expressions:

```json
{
  "id": "verbose-mode-without-full-brain",
  "type": "compound_risk",
  "severity": "info",
  "domains": ["human-comms", "test-health"],
  "description": "Full verbosity requested but brain health is low — responses will be detailed but signal is thin.",
  "condition_tree": {
    "and": [
      { "==": ["human-comms:verbosity", "full"] },
      { "<":  ["test-health:score", 40] }
    ]
  }
}
```

---

## `per_hat` Authoring Guide

Per-hat entries override specific preference fields when that hat is active. Only
include fields you want to change — unspecified fields use the base preferences.

Supported per-hat fields:
- `verbosity` — brief | standard | full
- `lead_with` — answer | reasoning
- `lists_vs_prose` — lists | prose | tables_when_comparing
- `include_why_context` — true | false

Example project-scoped overrides (`.claude/human-comms.yaml`):
```yaml
per_hat:
  visionary:
    verbosity: full
    lists_vs_prose: prose   # Architectural thinking flows better as prose
  engineer:
    verbosity: standard     # Stay concise in active development
    lead_with: answer       # Code/answer first, explanation after
  reviewer:
    verbosity: full         # Thorough during code review
```

---

## Domain Promotion Guide

`human-comms` starts at advisory weight `0.0`. It should stay advisory for most
teams — a low score (incomplete preferences) shouldn't penalize the unified health
score, it should just be visible as a gap.

Consider promoting to a small weight (0.05) if your team wants to enforce that all
contributors have explicit communication contracts before their work is reviewed.

```json
"human-comms": 0.05
```

Adjust the other weights to maintain a sum of 1.000.

---

## File Ownership

| File | Owner | Committed? |
|------|-------|-----------|
| `~/.claude/human-comms.yaml` | Individual human | Never |
| `.claude/human-comms.yaml` | Project / team | Yes |
| `.claude/human-comms-cmdb.json` | Generated (sensory tool) | Yes |
