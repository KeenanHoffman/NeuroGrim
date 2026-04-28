<!-- topic: domain — bundled in neurogrim-cli v3.2 -->
# Domains — the unit of project concern

A **domain** is a declared unit of project concern: "test health,"
"code quality," "secret-refs hygiene," "deploy readiness." Each
domain measures one thing well. Most projects need 3–10 domains;
the ecosystem itself runs 17.

This document covers the anatomy of a domain, how weights work,
and how to add one. To see this Brain's current domain set, run
`neurogrim agent --prose`. To author a new domain, run
`neurogrim domain new <name>`.

## Anatomy

A registered domain has three parts in `brain-registry.json`:

```json
"config": {
  "domain_weights": {
    "test-health": 0.35
  },
  "principle_map": {
    "test-health": "Test Coverage & Health"
  },
  "domain_definitions": {
    "test-health": {
      "scoring_source": {
        "type": "cmdb",
        "path": ".claude/test-health-cmdb.json"
      }
    }
  }
}
```

- **`domain_weights`** — numeric weight in the unified score.
  Weights of all *weighted* domains sum to 1.0 (advisory domains
  at weight 0.0 don't contribute).
- **`principle_map`** — humanized display name for prose output
  and recommendations.
- **`domain_definitions`** — where this domain's data comes from.
  `scoring_source.type` is one of:
  - `"cmdb"` — read a JSON file at `path` (most common)
  - `"a2a"` — query a peer Brain at `endpoint` (federation)
  - `"function"` — implementation-specific scoring function

## Weight tiers

There are three postures a domain can take:

| Posture       | Weight | Meaning                                      |
|---------------|--------|----------------------------------------------|
| Weighted      | > 0.0  | Contributes to the unified score             |
| Advisory      | 0.0    | Visible but does not affect the score        |
| Stub          | 0.0    | Declared intent; sensor not yet authored     |

Stub and advisory have the same wire shape (weight 0.0); the
distinction is intent. New domains should land *advisory* — the
discipline is "observe before promoting." Promote to weighted
only after the sensor has accumulated enough operator-judgment
data to calibrate its findings against reality.

## When to add a domain

Add a domain when:
- A class of project concern is currently unmeasured
- You can imagine a sensor that would produce a reproducible
  0–100 score for it (if you can't, you have a discipline, not a
  domain — author a skill instead via `neurogrim skill new`)
- The signal would actually change agent behavior — score-by-itself
  isn't the goal; *acted-upon* score is

Don't add a domain when:
- It duplicates an existing domain's coverage
- The "score" is binary (it's a gate, not a domain)
- The signal is already covered by `correlations` (cross-domain
  patterns) or `incident_patterns` (recurrence detection)

## How to add one

The automated path:

```bash
neurogrim domain new my-coverage --description "My measurement" --type stub
```

This mutates `brain-registry.json` (adds entries to all three
sections atomically), generates a stub CMDB at
`.claude/my-coverage-cmdb.json` (score 50, low_confidence true),
and prints next steps. Run `neurogrim doctor` to verify.

To author the actual sensor:
- **Python**: `neurogrim domain new my-coverage --type python`
  scaffolds `sensory/check_my_coverage.py`. Edit the `analyze()`
  method. Run via `py -3 sensory/check_my_coverage.py > .claude/my-coverage-cmdb.json`.
- **Built-in (NeuroGrim contributor)**: see CONTRIBUTING.md for
  the Rust sensor pattern. Most adopters don't need this.

## What a healthy domain looks like

- **Score**: realistic. A domain that always reads 100 isn't a
  signal — it's a placebo. A domain that always reads 0 isn't a
  signal either — it's been broken or was never authored.
- **Confidence**: high when the data is fresh. CMDB updated_at
  more than `cmdb_very_stale_days` (default 7) old → confidence
  decays to 0; agents stop trusting the signal.
- **Findings**: itemized. A domain that says "score: 60" with no
  findings is unactionable. Each finding should have a name, a
  status, and ideally a detail string explaining what the agent
  should do.
- **Trajectory**: tracked. After ~5 score samples, the trajectory
  classifier reports improving / degrading / stable / volatile.
  Agents should treat degrading-trajectory domains as priority.

## Cross-references

- `neurogrim explain sensor` — authoring the program that produces a domain's score
- `neurogrim explain scoring` — how per-domain scores aggregate to a unified score
- `neurogrim doctor` — catches advisory orphans and weighted orphans
- Spec §3 — sensory protocol; §4 — scoring contract
