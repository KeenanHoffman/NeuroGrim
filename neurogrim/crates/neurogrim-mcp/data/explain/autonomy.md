<!-- topic: autonomy — bundled in neurogrim-cli v3.5 -->
# Autonomy — declared action types and safety invariants

The `autonomy` block in `brain-registry.json` declares **what an agent
may do without asking**, **what requires approval**, and **what is
blocked outright**. This is the safety surface that makes "agent acts
on the Brain's signals" tractable: the Brain can recommend a fix; the
autonomy block decides whether the agent can apply it.

To add or modify autonomy declarations, edit `brain-registry.json`
directly. The CLI does not yet ship a declarative subcommand
(`autonomy add-invariant <rule>`); see also `neurogrim doctor` for
schema validation as of v3.3.

<!-- anchor: three-pieces -->
## The three pieces

```json
"autonomy": {
  "levels":            { ... },   // 1. the autonomy ladder
  "action_types":      { ... },   // 2. classes of action with default levels
  "safety_invariants": [ ... ]    // 3. invariant overrides (raise the floor)
}
```

<!-- anchor: levels -->
### 1. Levels

A closed set of **four** autonomy levels, ordered from least to most
restrictive:

| Level | Requires approval | Notifies after | When to use |
|-------|-------------------|-----------------|-------------|
| `auto` | no | no | Reversible, low-blast-radius actions (refresh CMDB, recompute score) |
| `notify` | no | yes | Reversible but visible to humans (write a finding, log a recommendation) |
| `approve` | yes | n/a | Irreversible or operator-identity-bearing (submit application, send outreach) |
| `blocked` | yes | n/a | Hard-stop regardless of confidence (delete data, destructive ops) |

The `levels` block in the registry SHOULD declare all four with
`description` + `requires_approval`. They're foundational; agents
inspect this when reasoning about whether to act.

<!-- anchor: action-types -->
### 2. Action types

A vocabulary of action classes the Brain reasons about. Each entry:

```json
"action_types": {
  "refresh-snapshot": {
    "default_level": "auto",
    "blast_radius": "low",
    "reversible": true,
    "description": "Re-run a sensor and write a fresh CMDB."
  },
  "submit-application": {
    "default_level": "approve",
    "blast_radius": "high",
    "reversible": false,
    "description": "Submit a job application on the operator's behalf."
  }
}
```

| Field | Required | Notes |
|-------|----------|-------|
| `default_level` | yes | One of the four `levels` keys; the action's autonomy floor by default |
| `blast_radius` | recommended | `low` / `medium` / `high` — read by reasoning surfaces |
| `reversible` | recommended | `true` / `false` — used by autonomy heuristics |
| `description` | recommended | One sentence explaining what this action class covers |

`default_level` is the floor that applies UNLESS a `safety_invariant`
raises it.

<!-- anchor: safety-invariants -->
### 3. Safety invariants

An invariant **raises the autonomy floor** for matching action types.
Each entry:

```json
"safety_invariants": [
  {
    "rule": "agents-must-not-submit-applications-without-explicit-operator-approval",
    "minimum_level": "approve",
    "description": "Submitting a job application is non-reversible; require explicit approval."
  },
  {
    "rule": "destroy-always-blocked",
    "enforced_level": "blocked",
    "description": "Destructive actions MUST stay blocked regardless of confidence."
  }
]
```

| Field | When to use | Notes |
|-------|-------------|-------|
| `rule` | always | Stable identifier — typically a descriptive sentence in kebab-case |
| `minimum_level` | floor-raise | Action's level can be ≥ this; agents raise to this when default is lower |
| `enforced_level` | hard-pin | Action's level MUST be exactly this; overrides any autonomy heuristic |
| `description` | always | Operator-readable rationale — auditors WILL ask "why is this here" |

Use `minimum_level` for "this action is OK to do, just always check first."
Use `enforced_level` for "this action MUST never auto-execute, period."

<!-- anchor: examples -->
## Worked examples

### Adding a project-specific safety invariant

A job-hunt project wants to block agents from auto-submitting
applications or auto-sending networking outreach. Edit
`.claude/brain-registry.json`:

```json
"autonomy": {
  "action_types": {
    "submit-application": {
      "default_level": "approve",
      "blast_radius": "high",
      "reversible": false,
      "description": "Submitting a job application on the operator's behalf."
    },
    "send-outreach": {
      "default_level": "approve",
      "blast_radius": "medium",
      "reversible": false,
      "description": "Sending networking outreach (LinkedIn DM, email)."
    }
  },
  "safety_invariants": [
    {
      "rule": "agents-must-not-submit-applications-without-explicit-operator-approval",
      "minimum_level": "approve",
      "description": "Job applications carry the operator's identity; require explicit approval."
    },
    {
      "rule": "agents-must-not-send-outreach-without-explicit-operator-approval",
      "minimum_level": "approve",
      "description": "Outreach messages bind professional identity; require explicit approval."
    }
  ]
}
```

After editing, run `neurogrim doctor` to validate the autonomy block
shape (v3.3+).

### Verifying

```bash
neurogrim doctor             # checks levels/action_types/safety_invariants are well-formed
neurogrim agent --prose      # current declared posture is summarized
neurogrim validate           # registry-shape check
```

<!-- anchor: mistakes -->
## Common mistakes

- **Forgetting `description`** — this field is recommended on all three pieces (action_types, safety_invariants). Operators reading the registry six months later need to know **why** a rule exists. `doctor` warns when missing.
- **Mixing `minimum_level` and `enforced_level`** — pick one. `minimum_level` raises the floor; `enforced_level` pins the level. Putting both is undefined.
- **Referencing an unknown level** — `minimum_level: "supervisor"` would be ignored silently in v3.2; `doctor` flags this in v3.3+.
- **Inventing fields** — the schema is closed. Adding `autonomy_bias`, `policy`, etc. is silently ignored at runtime; `doctor` flags it in v3.3+.

## Cross-references

- `neurogrim explain methodology` — the larger context (the overlay model)
- `neurogrim explain hat` — hats narrow attention but cannot loosen autonomy
- `neurogrim doctor` — validates the autonomy block (v3.3+)
- `LSP-Brains/spec/LSP-BRAINS-SPEC.md` §5 — formal autonomy contract
