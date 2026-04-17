# Understand the Gate System

Conceptual guide to how the LaaS gate system works — what gate states mean, how gates are
triggered, and when to respect vs. bypass them. Read this before `gate-status.md` if you're
new to the project or confused by gate output.

Role: teaching
Governs: .claude/test-gates.json

Trigger phrases: "what is a gate", "how do gates work", "gate states explained",
Domain: gates
Methodology-step: skills
"what does needs-run mean", "what does stale mean", "why is my gate dirty",
"gate system overview", "how does watch_paths work", "gate expires"

---

## What Is a Gate?

A gate is a named record in `.claude/test-gates.json` that tracks whether a particular
test or infra check has passed recently. Each gate has:

- A **scope** (`code` or `infra`) — code gates run Pester tests; infra gates run real-GCP checks
- A **tier** (`immediate`, `before-merge`, `pre-deploy`) — when it must be clean
- A **blocks** list — which actions it prevents when not clean (`commit`, `merge`, `deploy`)
- A **watch_paths** list — file paths that reset the gate when changed
- An **expires_hours** field *(infra gates only)* — how long a clean result stays valid

Gates are **not** CI pipeline steps. They are local state, updated by running the gate's
`run_command` from your machine. CI may pass and your local gates may still need re-running.

---

## The Four Gate States

| State | What it means | What caused it |
|-------|--------------|----------------|
| **CLEAN** | Last run passed and is still valid | Gate was run, tests passed, no watched files changed since then |
| **DIRTY** | Last run failed | Tests failed, or `update-gate.ps1` was called with `-Status dirty` |
| **STALE** | Passed before but is no longer valid | A file in `watch_paths` changed, OR `expires_hours` elapsed since last clean (infra gates) |
| **NEEDS-RUN** | Never been run in this repo clone | No recorded clean run exists |

**STALE vs. DIRTY:** STALE means the test was passing but context has changed (code changed
or time expired) — the test just needs to be re-run. DIRTY means the test actively failed.
Both block the same actions; only the recovery path differs.

---

## How `watch_paths` Works

When you edit a file that matches a gate's `watch_paths` glob, the gate transitions
to **STALE** automatically (via `check-gates.ps1`). This ensures gates don't claim
"clean" after code changes that could affect what they test.

```
edit: scripts/deploy/apply-infra.ps1
  → matches watch_paths: ["scripts/deploy/"]
  → pester:deploy gate → STALE
  → you must re-run: pwsh -File scripts/verify/run-tests.ps1 -Target deploy
```

`watch_paths` uses prefix matching — `"scripts/deploy/"` matches any file under that directory.
It does **not** use glob patterns like `**/*.ps1`.

---

## How `expires_hours` Works

Infra gates (scope: `infra`) expire after a fixed wall-clock duration, regardless of file
changes. This models the reality that GCP infrastructure drifts over time — a topology
snapshot taken 25 hours ago may not reflect the current state.

```
expires_hours: 24   →  gate is CLEAN for 24h after last clean run
                         then transitions to STALE automatically
```

Code gates (scope: `code`) do **not** expire — they only go STALE on file changes.

---

## The Gate Advisor Verdict Flow

`gate-advisor.ps1` reads all gates and produces one of three verdicts:

```
PROCEED    All blocking gates are CLEAN
WARN       Some non-blocking gates are STALE/DIRTY (advisory only)
BLOCKED    One or more blocking gates are STALE/DIRTY/NEEDS-RUN
```

The advisor also respects `tags`:
- `critical-guardrail` and `critical-security` tagged gates are always flagged with `[!]`
- `[!]` gates on a BLOCKED result must be cleared; they cannot be bypassed

Run before any significant action:
```powershell
pwsh -File scripts/utility/gate-advisor.ps1 -Action commit   # before git commit
pwsh -File scripts/utility/gate-advisor.ps1 -Action deploy   # before terraform apply
pwsh -File scripts/utility/gate-advisor.ps1 -Action merge    # before merging to main
```

---

## When You Can (and Cannot) Skip a Gate

**Budget-based skip** — advisory (non-critical) infra gates can be skipped if time is the
constraint. The `-Budget` flag tells the advisor your time budget in minutes; it will only
flag gates that can be re-run within that budget:

```powershell
pwsh -File scripts/utility/gate-advisor.ps1 -Action deploy -Budget 5
```

**Hard blockers** — gates tagged `critical-guardrail` or `critical-security` are never
skipped. They represent guardrails that exist precisely for the high-pressure moment when
you're tempted to skip them.

**Rule of thumb:** Skip advisory gates when you have a time constraint and the code change
is low risk. Never skip a gate that explicitly blocks your next action.

See `weigh-time-risk.md` for the full decision framework.

---

## Why This Matters

This skill implements **Fail Fast / Shift Left** from `devops-philosophy.md`. Gates exist to
catch failures at the cheapest possible moment — Pester tests catch script bugs before commit,
infra gates catch topology drift before deploy. A gate system without clear state semantics
creates uncertainty ("should I re-run this?") that leads to either skipping tests or running
unnecessary ones. The four-state model makes the decision deterministic.

---

## See Also

- `gate-status.md` — operational commands: check gates, clear specific gates, re-run dirty gates
- `weigh-time-risk.md` — decision framework for how much testing each change needs
- `what-next.md` — prioritized action list that reads gate state for you
