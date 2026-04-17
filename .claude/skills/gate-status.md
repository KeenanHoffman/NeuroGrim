# Gate Status Skill

Use this skill to inspect the current clean/dirty status of all test gates, identify
which gates need re-running, and get the exact commands to clear them.

Role: diagnostic · planning
Governs: .claude/test-gates.json, scripts/utility/gate-advisor.ps1

Trigger phrases: "gate status", "are my tests clean", "which tests are stale",
Domain: gates
Methodology-step: skills
"check test gates", "what gates are dirty", "do I need to re-run any tests"

---

## Check All Gates

```powershell
pwsh -NonInteractive -File scripts/utility/check-gates.ps1
```

Output shows each gate with a status column:
- **CLEAN** — last clean run is still valid; watch_paths unchanged
- **STALE** — files changed since last clean run, or infra gate time-expired
- **DIRTY** — gate was run and failed
- **NEEDS-RUN** — never run in this repo clone

## Check Gates by Action (preferred)

```powershell
# Only gates that block commits
pwsh -NonInteractive -File scripts/utility/check-gates.ps1 -BlocksAction commit

# Only gates that block deploys
pwsh -NonInteractive -File scripts/utility/check-gates.ps1 -BlocksAction deploy
```

## Check Gates by Scope (legacy)

```powershell
pwsh -NonInteractive -File scripts/utility/check-gates.ps1 -Scope code
pwsh -NonInteractive -File scripts/utility/check-gates.ps1 -Scope infra
```

## Full Decision Verdict (recommended before any significant action)

```powershell
pwsh -NonInteractive -File scripts/utility/gate-advisor.ps1 -Action commit
pwsh -NonInteractive -File scripts/utility/gate-advisor.ps1 -Action deploy
pwsh -NonInteractive -File scripts/utility/gate-advisor.ps1 -Action deploy -Budget 10
```

Gate advisor shows PROCEED / WARN / BLOCKED with specific run commands, estimated times,
and flags hard blockers (`critical-guardrail`, `critical-security`) with `[!]`.

**Run `gate-advisor -Action deploy` before starting any deploy sequence.** A `BLOCKED` result
means one or more critical gates are stale or dirty — the advisor prints the exact commands to
clear them. See `apply-infra.md` for the full deploy workflow, which starts with this check.

---

## Time-Budget Clearance (deploy in a limited window)

When you need to deploy but have limited time, use `-Budget` and `-CriticalPath` to find
the minimum viable gate set:

```powershell
# Critical-path gates — minimum set required for a valid deploy
pwsh -NonInteractive -File scripts/dev/Find-GateSymbol.ps1 -CriticalPath -Plain

# Gates clearable within 10 minutes — sorted by estimated_minutes ascending
pwsh -NonInteractive -File scripts/dev/Find-GateSymbol.ps1 -Budget 10 -Plain
```

**The 10-minute deploy-unblocking pattern:**
1. `Find-GateSymbol -CriticalPath` — identify mandatory gates
2. `Find-GateSymbol -Budget 10` — confirm all critical-path gates fit in the window
3. Run each gate's printed `run_command`
4. `gate-advisor -Action deploy` — confirm PROCEED verdict before applying

If a critical-path gate's `estimated_minutes` exceeds your budget, defer the deploy.
Never skip a gate tagged `critical-guardrail` — these prevent catastrophic operations.

---

## Re-running Dirty Gates

Each gate entry in `.claude/test-gates.json` has a `run_command` field with the exact
command to re-run that gate. Read the file and present `run_command` for every gate
that is DIRTY, STALE, or NEEDS-RUN.

Example output format:
```
Gates needing attention:

  pester:deploy       STALE   → pwsh -NonInteractive -File scripts/verify/run-tests.ps1 -Target deploy
  terraform:test:app  DIRTY   → pwsh -NonInteractive -File scripts/deploy/verify-terraform.ps1 -Target app
  preflight           STALE   → pwsh -NonInteractive -File scripts/utility/check-preflight.ps1 -ProjectID $env:LAAS_PROJECT_ID
```

---

## Committing Gate Status

After tests pass, commit `.claude/test-gates.json` along with your code changes:

```powershell
git add .claude/test-gates.json
git commit -m "mark pester:deploy clean after fixing X"
```

The pre-commit hook will block commits that stage code files with dirty code-scoped
gates. Run the relevant tests first, then commit both together.

---

## Install the Pre-commit Hook (first-time setup)

If you cloned the repo without running `setup-local-env.ps1`:

```powershell
pwsh -File scripts/utility/install-hooks.ps1
```

This installs `.claude/hooks/pre-commit.sh` into `.git/hooks/pre-commit`. The hook
runs `check-gates.ps1 -Scope code -CheckStaged` before every commit and blocks if
any code-scoped gate is dirty for the staged files.

---

## Gate Definitions Reference

All gates are defined in `.claude/test-gates.json`. Fields per gate:

| Field | Meaning |
|-------|---------|
| `scope` | `code` (logic gates) or `infra` (real GCP gates) |
| `tier` | `immediate` / `before-merge` / `pre-deploy` / `advisory` |
| `tags` | Semantic labels: `core-functionality`, `critical-guardrail`, `critical-security`, `final-validation`, `regression`, `e2e` |
| `blocks` | Actions blocked when not CLEAN: `["commit", "merge"]`, `["deploy"]`, etc. |
| `watch_paths` | Paths that trigger staleness when changed |
| `expires_hours` | Infra gates only: hours before automatic staleness |
| `estimated_minutes` | Approximate run time; used by gate-advisor budget mode |
| `run_command` | Exact command to re-run this gate |
| `status` | Persisted: `clean`, `dirty`, `needs-run` |
| `last_clean_commit` | Git SHA of last clean run |
| `last_clean_at` | UTC timestamp of last clean run |
| `last_failures` | Up to 10 specific test names that failed in the last dirty run |

**Code gates** become STALE when their `watch_paths` have changed since `last_clean_commit`.
**Infra gates** become STALE when `expires_hours` has elapsed or `watch_paths` changed.
STALE is computed at evaluation time — the JSON always reflects the last known state.

---

## Troubleshooting

**Problem: Gate shows STALE immediately after running it**
- `expires_hours` uses UTC; clock drift or a very short expiry value can cause this
- Fix: Re-run the gate's `run_command` to generate a fresh clean timestamp

**Problem: Gate is DIRTY but Pester tests pass locally**
- Gate was dirtied by a file change matching `watch_paths`, not by a test failure
- Fix: Confirm tests pass, then `pwsh -File scripts/utility/update-gate.ps1 -Gate pester:<target> -Status clean`

**Problem: BLOCKED on a gate I thought was advisory-only**
- The gate has `"blocks": ["commit"]` or `"blocks": ["deploy"]` in `test-gates.json`
- Fix: Check the gate entry in `.claude/test-gates.json`; advisory gates have `"blocks": []`; run the `run_command` to clear it

**Problem: Gate shows `needs-run` forever and never advances**
- The test file referenced by `run_command` may not exist on disk
- Fix: Check that the `.Tests.ps1` file exists at the path in `run_command`; if it doesn't exist, remove the gate entry from `test-gates.json` until the file is created

---

## Why This Matters

This skill implements **Fail Fast / Shift Left** from `devops-philosophy.md`. Gates encode the minimum verification threshold for each action type — commit, deploy, merge. Checking gate state before acting prevents state rot where "we forgot to run the tests" compounds across sessions until a dirty gate silently fails in CI or mid-apply. The gate advisor exists because a blocked deploy is always cheaper than a failed one.

---

## See Also

- `session-recap.md` — run at session start to surface current gate state
- `test.md` — if pester:* gates are dirty, run the failing suites here
- `what-next.md` — after clearing gates, use to prioritize the session
- `weigh-time-risk.md` — determines which gates must be clean for your change tier
