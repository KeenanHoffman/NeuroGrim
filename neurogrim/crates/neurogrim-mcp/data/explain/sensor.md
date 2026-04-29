<!-- topic: sensor — bundled in neurogrim-cli v3.5 -->
# Sensors — the program that measures a domain

A **sensor** is a small program that reads on-disk artifacts (or
peer-Brain state, or external signals) and emits a CMDB JSON
envelope summarizing what it observed. NeuroGrim ships 20 built-in
Rust sensors; adopters typically author Python sensors for
project-specific concerns.

This document covers the sensor authoring contract. To scaffold a
new domain with a stub Python sensor, run
`neurogrim domain new <name> --type python`.

## The contract

A sensor has one job: read project state, return a CMDB envelope.

Required output fields:

```json
{
  "meta": {
    "schema_version": "1",
    "updated_by": "check-my-coverage",
    "updated_at": "2026-04-28T08:00:00Z"
  },
  "score": 87,
  "updated_at": "2026-04-28T08:00:00Z",
  "findings": [
    { "name": "no_uncovered_modules", "status": "pass", "points": 0 },
    { "name": "stale_test_cache",     "status": "warn", "points": -3,
      "detail": "tests/.cache/ is 14d old; rerun `pytest --no-cache`" }
  ]
}
```

Optional fields:

- `confidence: 0..100` — sensor-supplied freshness signal. When
  absent, the aggregator falls back to age-based decay against
  `updated_at`.
- Domain-specific exported variables (any extra fields). Define
  them in `domain_definitions.<domain>.exported_variables` so
  correlations can reference them.

Schema: `LSP-Brains/schemas/cmdb-envelope-v1.schema.json`.

## Score formula patterns

Most sensors follow one of two patterns:

```python
# Pattern 1: subtractive — start at 100, subtract per finding
score = 100
for f in findings:
    if f.severity == "warn":
        score -= 2
    elif f.severity == "error":
        score -= 10
score = max(0, min(100, score))

# Pattern 2: ratio — fraction of healthy artifacts
score = round(100 * healthy_count / total_count) if total_count else 100
```

Avoid:
- **Score 100 unconditionally** (your sensor isn't measuring anything)
- **Score 0 on absence of data** (use `no_file_score` in the
  registry's `scoring_source`, or pair with confidence < 50)
- **Floating-point scores** (the contract is `u8`, 0..=100 inclusive)

## Python sensor skeleton

```python
"""Sensor: check-my-coverage. Measures my-coverage domain."""
import json
from datetime import datetime, timezone
from pathlib import Path

def analyze(project_root: str) -> dict:
    findings = []
    score = 100
    # ... read files, populate findings, adjust score ...
    return {
        "meta": {
            "schema_version": "1",
            "updated_by": "check-my-coverage",
            "updated_at": _now(),
        },
        "score": score,
        "updated_at": _now(),
        "findings": findings,
    }

def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")

if __name__ == "__main__":
    import sys
    project_root = sys.argv[1] if len(sys.argv) > 1 else "."
    print(json.dumps(analyze(project_root), indent=2))
```

Refresh the CMDB:

```bash
py -3 sensory/check_my_coverage.py . > .claude/my-coverage-cmdb.json
```

`neurogrim domain new <name> --type python` produces this skeleton
automatically.

## Built-in Rust sensors (NeuroGrim contributors only)

NeuroGrim's Rust sensors live in
`neurogrim/crates/neurogrim-sensory/src/<name>.rs`. They expose
an async function `analyze_<name>(project_root) -> Value` and are
registered in `crates/neurogrim-cli/src/main.rs` under
`run_sensory()`. They use the shared `build_cmdb()` helper from
`crates/neurogrim-sensory/src/cmdb.rs`.

This is contributor work, not adopter work. If you're authoring a
sensor for *your* project, prefer Python — you can iterate without
recompiling NeuroGrim, and the bundled `domain new --type python`
handles the scaffolding.

## What makes a good sensor

- **Static-only signals.** A good sensor reads files; it doesn't
  run tests, doesn't fork processes (except for cheap CLI calls
  like `git status`), doesn't make network requests. Static =
  reproducible = scoreable.
- **Deterministic.** Same project state → same score. If your
  sensor's score drifts run-to-run with no project change, it has
  a non-determinism bug.
- **Fast.** Run-time should be < 5 seconds. Sensors are invoked
  during scoring; slow sensors block agent responsiveness.
- **Itemized findings.** "Score 73" is not actionable; "Score 73,
  3 findings: stale lockfile, missing CHANGELOG entry, deprecated
  API call" *is*. The agent acts on findings, not on the score.
- **Honest about absence.** No data → low confidence + `no_file_score`
  default, not a fabricated score.

## Cross-references

- `neurogrim explain domain` — registering the domain that your sensor measures
- `neurogrim explain scoring` — how per-sensor scores aggregate to a unified score
- Spec §3 — full sensory protocol; §3.8 — confidence semantics
- `LSP-Brains/schemas/cmdb-envelope-v1.schema.json` — output schema
- `D:/Brains/sensory/check_terminology_coherence.py` — canonical Python example
