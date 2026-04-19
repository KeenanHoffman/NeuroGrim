# coherence

**Purpose:** Surface cross-domain relationships that individual domains cannot see alone.

## When to Use This Skill

- Health score seems inconsistent with observed risk
- Two domains are trending in divergent directions
- You want to define a named compound risk for your project
- Preparing to promote a domain from advisory (0.0) to weighted

---

## What Coherence Scores

Coherence evaluates the `correlations` array in `brain-registry.json` against live domain
CMDBs. Each correlation has a `condition_tree`; if it fires, the coherence score decreases.

```
score = 100
for each correlation:
  if condition_tree fires:
    severity "critical" → −35
    severity "warning"  → −20
    severity "info"     → −5
score = max(score, 0)
```

**Score interpretation:**

| Score | Meaning |
|-------|---------|
| 100 | All correlations healthy (or no correlations defined yet) |
| 80–99 | One warning-severity relationship active |
| 60–79 | Multiple warning or one critical active |
| 40–59 | Compound risk pattern present |
| 0–39 | Multiple critical cross-domain failures |

A score of 100 with 0 correlations defined means you haven't encoded your project's
relationship knowledge yet. The value of coherence grows with your registry.

---

## Running the Tool

```bash
# Regenerate coherence CMDB
neurogrim sensory coherence --project-root . > .claude/coherence-cmdb.json

# Full health with coherence row
neurogrim health

# See only correlation firings
neurogrim health --plain | grep -A5 "Correlations:"
```

---

## Correlation Taxonomy

Four types describe the relationship between domains. The type is metadata — it
does not change condition evaluation, but guides remediation framing.

| Type | What It Means | Example |
|------|---------------|---------|
| `compound_risk` | A + B together are worse than either alone | Low test coverage + high deploy-readiness |
| `dependency` | B's health requires A first | Deploy pipeline present but no security baseline |
| `reinforcing` | Two signals point to the same root cause | High uncommitted changes + low code quality |
| `blocking` | A domain gate prevents another from advancing | Test health gate blocks deploy gate |

---

## Authoring a Correlation

Correlations live in `brain-registry.json` under `config.correlations`. Each entry is read
by two consumers: the coherence sensory tool (scores it) and the core engine (fires a
`CorrelationFired` event visible in health output). One definition, two consumers.

**Full schema:**

```json
{
  "id":             "my-correlation-id",
  "name":           "my-correlation-id",
  "type":           "compound_risk",
  "severity":       "critical",
  "domains":        ["domain-a", "domain-b"],
  "description":    "One sentence: what risk does this capture?",
  "insight":        "One sentence: what should the team do about it?",
  "condition_tree": {
    "and": [
      { "<": ["domain-a:score", 40] },
      { ">": ["domain-b:score", 60] }
    ]
  }
}
```

**Required fields:**
- `id` / `name` — unique identifier (kebab-case)
- `condition_tree` — evaluated against domain variables (see below)

**Optional fields used by coherence:**
- `type` — one of `compound_risk`, `dependency`, `reinforcing`, `blocking`
- `severity` — `critical`, `warning`, `info` (default: `info`)
- `domains` — list of domain names involved (for documentation and CMDB output)
- `insight` — actionable remediation text shown in coherence findings

---

## Condition Tree Reference

Conditions are evaluated against **domain variables** — all numeric and boolean top-level
fields from each domain's CMDB, keyed as `domain-name:field-name`.

**Comparison operators** (always `[variable, value]`):
```json
{ ">":  ["test-health:score",        40] }
{ "<":  ["git-health:uncommitted_changes", 10] }
{ ">=": ["code-quality:score",       60] }
{ "==": ["security-standards:has_security_policy", true] }
```

**Branch operators:**
```json
{ "and": [ ...conditions... ] }
{ "or":  [ ...conditions... ] }
{ "not": { ...condition... } }
```

**Available variables per domain** (examples):

| Variable | Source Domain | Type |
|----------|--------------|------|
| `test-health:score` | test-health CMDB | number |
| `test-health:has_test_directory` | test-health CMDB | bool |
| `code-quality:score` | code-quality CMDB | number |
| `code-quality:has_lint_config` | code-quality CMDB | bool |
| `deploy-readiness:score` | deploy-readiness CMDB | number |
| `deploy-readiness:has_ci` | deploy-readiness CMDB | bool |
| `git-health:score` | git-health CMDB | number |
| `git-health:uncommitted_changes` | git-health CMDB | number |
| `security-standards:score` | security-standards CMDB | number |
| `security-standards:controls_evidenced` | security-standards CMDB | number |
| `coherence:correlations_fired` | coherence CMDB | number |
| `coherence:highest_severity` | coherence CMDB | string* |

*String variables require `==` / `!=` only.

---

## Starter Correlations (Built-In)

Three correlations ship with this repo:

### `deploy-without-test-baseline` — compound_risk / critical
```
test-health:score < 40 AND deploy-readiness:score > 50
```
You can ship, but you can't verify what you're shipping. Neither domain flags this alone.

### `commit-debt-with-quality-pressure` — reinforcing / warning
```
git-health:uncommitted_changes > 10 AND code-quality:score < 40
```
Both signals trace to the same root cause: development moving faster than hygiene.

### `security-gate-missing-from-ci` — dependency / warning
```
security-standards:score < 20 AND deploy-readiness:score > 30
```
The pipeline can ship, but no security validation exists. A dependency is skipped.

---

## Domain Promotion Guide

Coherence is advisory (weight `0.0`) by default. Promote it when:
- ≥ 3 correlations are defined and tuned for your project
- The score has been stable above 80 for ≥ 2 weeks
- The team trusts the fired correlations as actionable signals

To promote to 10% weight:
```json
"domain_weights": {
  "test-health":        0.35,
  "code-quality":       0.30,
  "deploy-readiness":   0.20,
  "git-health":         0.0,
  "coherence":          0.10,
  ...
}
```

At 10% weight, a coherence score of 60 (two warnings firing simultaneously) reduces the
unified health score by ~4 points — enough to appear in trend data but not dominate.

---

## CMDB Field Reference

| Field | Type | Description |
|-------|------|-------------|
| `score` | 0–100 | Relationship health score |
| `correlations_evaluated` | number | Count of correlations checked |
| `correlations_fired` | number | Count that fired this run |
| `highest_severity` | string | Worst active severity: `none`, `info`, `warning`, `critical` |
| `correlation_details` | array | Full detail for every correlation (fired and healthy) |
| `findings` | array | Only fired correlations with deduction points |

`correlation_details` entry shape:
```json
{
  "id":          "deploy-without-test-baseline",
  "type":        "compound_risk",
  "severity":    "critical",
  "fired":       false,
  "domains":     ["test-health", "deploy-readiness"],
  "description": "...",
  "insight":     "..."
}
```

---

## Health Constellations (Advanced)

A **constellation** is a named pattern of domain states with a known outcome. Once you
have a few correlations firing consistently, name the pattern:

```json
{
  "id":          "pre-incident-constellation",
  "name":        "pre-incident-constellation",
  "type":        "compound_risk",
  "severity":    "critical",
  "domains":     ["git-health", "test-health", "deploy-readiness"],
  "description": "Historical pattern preceding incidents: git debt + low test coverage + active pipeline.",
  "insight":     "Freeze deploy gates and schedule a quality sprint.",
  "condition_tree": {
    "and": [
      { ">":  ["git-health:uncommitted_changes", 15] },
      { "<":  ["test-health:score",              30] },
      { ">":  ["deploy-readiness:score",         50] }
    ]
  }
}
```

Teams that encode incident post-mortems as correlation conditions prevent recurrence.
