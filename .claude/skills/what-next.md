# What Next Skill

Use this skill when you need a prioritized action list: "what should I run before committing?",
"I'm about to deploy — what do I need to check first?", "what tests should I run given what I
just changed?"

Role: planning · diagnostic
Governs: scripts/utility/gate-advisor.ps1, scripts/dev/Find-SessionContext.ps1

Trigger phrases: "what should I do next", "what tests do I need to run", "what do I need before
Domain: gates, brain
Methodology-step: skills
deploying", "prioritize my tests", "what's my next step", "am I ready to commit/merge/deploy"

---

## Brain-Weighted Priority

Before listing next actions, run Brain recommend to get priority-ranked, skill-linked actions:

```powershell
pwsh -NonInteractive -File scripts/dev/Find-Brain.ps1 -Mode recommend -Plain
```

The output is ranked by `tierWeight × ageMultiplier × downstreamMultiplier` and names the
owning skill (`skill: test.md`) alongside each action. Lead with the top Brain recommendation.

**If recommend output is empty** (all gates clean):
```powershell
pwsh -NonInteractive -File scripts/dev/Find-Brain.ps1 -Mode health -Plain
```
Check for expired-clean gates or advisory-tier items before declaring the session unblocked.

---

## Step 1 — Identify Intent

Determine which action the user is heading toward. If unclear, ask. Map to one of:
- **commit** — about to `git commit`
- **merge** — about to open or merge a PR
- **deploy** — about to run `apply-*.ps1` or any Terraform apply
- **explore** — no specific action; just want situational awareness

---

## Step 2 — Run the Session Synthesizer

Run `Find-SessionContext.ps1` with the action mode matching your intent:

```powershell
# For commit:
pwsh -NonInteractive -File scripts/dev/Find-SessionContext.ps1 -Action commit

# For merge/PR review:
pwsh -NonInteractive -File scripts/dev/Find-SessionContext.ps1 -Action review

# For deploy/apply:
pwsh -NonInteractive -File scripts/dev/Find-SessionContext.ps1 -Action deploy

# For debugging a dirty gate or incident:
pwsh -NonInteractive -File scripts/dev/Find-SessionContext.ps1 -Action debug

# For general situational awareness (explore):
pwsh -NonInteractive -File scripts/dev/Find-SessionContext.ps1
```

SessionContext joins gate health, governing skills, workflow coverage, and topology impact
into a single view — it replaces the need to run separate gate-advisor and topology queries.

Read the output. Note:
- **"Commit blocked"** / **"Deploy blocked"** — the specific gates to clear
- **"Skills to consult"** — the skill governing the dirty gate; read it for the run procedure
- **"Topology at risk"** — existential/critical resources blocked by dirty deploy gates
- **"Recommended next step"** — the single most important thing to do right now

If BLOCKED and you need the exact list of commands per gate tier, drill down with gate-advisor:

```powershell
pwsh -NonInteractive -File scripts/utility/gate-advisor.ps1 -Action <action>
```

---

## Step 3 — Cross-reference with Recent Changes

Check what changed since the last green commit:

```powershell
git diff --name-only HEAD~1..HEAD
# or, for uncommitted changes:
git diff --name-only HEAD
git diff --name-only --cached
```

Identify which subsystems were touched:
- `scripts/deploy/` → `pester:deploy` gate is the primary concern
- `scripts/dev/` → `pester:dev` gate is the primary concern
- `scripts/network/` → `pester:network` gate is the primary concern
- `terraform/foundation/` → `terraform:test:foundation` is the primary concern
- `terraform/infra/` → run `terraform test` from `terraform/infra/`; deploy target is `apply-infra.ps1`; gateway is downstream (reads lb_ip + cert from infra remote state — re-plan gateway after infra changes)
- `app/` or `terraform/app/` → `e2e:chat` gate (when implemented)
- `terraform/` (any module) → `network:topology` gate watch_paths overlap; run Tier A diff before apply
- Multiple subsystems → run all relevant gates before any commit

Cross-reference against the gate advisor output: if a gate is already CLEAN and its
`watch_paths` don't overlap with recent changes, it stays clean — skip it.

---

## Step 4 — Build the Prioritized Action List

Present the list ordered by:
1. **Absolute blockers first** — `critical-guardrail` / `critical-security` tagged DIRTY gates
2. **Fastest unblocking next** — sort remaining blockers by `estimated_minutes` ascending
3. **Within-budget vs. deferred** — if the user gave a time budget, flag anything that won't fit
4. **Advisory last** — show but don't block on these

Format:

```
WHAT TO DO NEXT — heading toward: deploy
────────────────────────────────────────────────────────────

NOW (clears blockers, ~3 min total):
  1. pwsh -File scripts/verify/run-tests.ps1 -Target deploy      (~1 min)  [critical-guardrail]
  2. pwsh -File scripts/deploy/verify-terraform.ps1 -Target app  (~1 min)
  3. pwsh -File scripts/utility/check-preflight.ps1              (~1 min)  [critical-guardrail]

BEFORE RELEASING (can finish later today):
  4. pwsh -File scripts/verify/smoke-infra.ps1 -Module web-app   (~5 min)

ADVISORY (won't block, run when convenient):
  5. pwsh -File scripts/utility/check-drift.ps1                  (~6 min)

────────────────────────────────────────────────────────────
Once steps 1–3 pass, re-run:
  pwsh -File scripts/utility/gate-advisor.ps1 -Action deploy
to confirm PROCEED verdict before applying.
```

---

## Step 5 — After Clearing

Once the user has run the listed tests:
1. Remind them to commit `.claude/test-gates.json` alongside their code changes
2. Re-run `gate-advisor.ps1` to confirm the PROCEED verdict
3. For deploy actions: run `gate-advisor.ps1 -Action deploy` immediately before the apply,
   since infra gates can expire while code tests were running
4. **After every successful apply**: run `update-network-topology.ps1` and `update-access-topology.ps1`
   to refresh both topology snapshots and mark `network:topology` + `access:topology` clean for the
   next deploy cycle (both are auto-called by `apply-*.ps1` scripts)
5. **If you just opened a PR**: offer to start a merge watcher with `watch-pr.md` so you'll
   be notified when the PR merges and the deploy-dev pipeline completes

---

## Tag Reference (for interpreting gate advisor output)

| Tag | Meaning | Priority |
|-----|---------|----------|
| `critical-guardrail` | Prevents catastrophic ops (destroy, prod writes) | **Absolute blocker** |
| `critical-security` | Auth/IAM/RBAC correctness | **Absolute blocker** |
| `core-functionality` | Basic happy path | High — run before every commit |
| `final-validation` | E2E correctness before release | High — required before deploy |
| `regression` | Catches known past bugs | Medium — run before merge |
| `e2e` | Playwright / browser tests | Medium — run before deploy, skip for refactors |

---

## Quick Reference — Action-to-Gate Mapping

| Changed path | Primary gate |
|-------------|--------------|
| `scripts/deploy/` or `scripts/verify/deploy.Tests.ps1` | `pester:deploy` |
| `scripts/dev/` or `scripts/verify/dev.Tests.ps1` | `pester:dev` |
| `scripts/network/` or `scripts/verify/network.Tests.ps1` | `pester:network` |
| `scripts/bootstrap/` or `scripts/verify/bootstrap.Tests.ps1` | `pester:bootstrap` |
| `scripts/setup/` or `scripts/verify/setup.Tests.ps1` | `pester:setup` |
| `scripts/publish/` or `scripts/verify/publish.Tests.ps1` | `pester:publish` |
| `scripts/utility/` or `scripts/verify/utility.Tests.ps1` | `pester:utility` |
| `scripts/verify/` (other) | `pester:verify` |
| `terraform/foundation/` | `terraform:test:foundation` |
| `terraform/app/` | `terraform:test:app` |
| `terraform/modules/web-app/` | `terraform:test:web-app` |
| `terraform/apps/gateway/` | `terraform:test:gateway` |

| About to… | Minimum gates that must be CLEAN |
|-----------|----------------------------------|
| `git commit` | All `tier: immediate` gates for changed paths |
| Open/merge PR | All `tier: immediate` + `tier: before-merge` gates |
| `apply` (any module) | All of the above + `preflight` + `smoke` for changed module + `network:topology` + `network:drift` + `access:topology` + `access:drift` |
| `apply` (touches network resources) | Run Tier A diff first: `diff-network-change.ps1 -PlanFile .claude/network/last-plan.json` |
| `apply` (touches IAM resources) | Run `check-access-drift.ps1` before apply to verify current IAM state |
| `apply` (prod, >4h since last drift check) | All of the above + `drift:all` + re-run `network:drift` + `access:drift` |
| After successful `apply` | `apply-*.ps1` auto-runs `update-network-topology.ps1` + `update-access-topology.ps1` |

---

## Why This Matters

This skill implements **Observability Before Action** from `devops-philosophy.md`. Starting work without knowing the current state is how teams accumulate invisible debt — a dirty gate left from last session, a stale topology, a CI failure nobody noticed. This skill surfaces that state so the next action is chosen with full context rather than optimistic assumptions.

---

## See Also

- `session-recap.md` — always run before what-next to establish current state
- `gate-status.md` — for understanding and clearing blocking gates
- `start-feature.md` — if what-next determines the next action is starting new work
- `watch-pr.md` — if you just created a PR and want to be notified when it merges
- `weigh-time-risk.md` — for deciding how much testing a pending change needs

---

## Troubleshooting

**Problem: Gate-advisor shows BLOCKED but I'm not sure which change triggered it**
- Cause: A file matching a gate's `watch_paths` was modified since the last clean run
- Fix: `pwsh -File scripts/utility/check-gates.ps1` shows both the status and the `watch_paths` for each gate; compare against your recent `git diff --name-only` to identify the trigger

**Problem: What-next suggests running tests but I haven't changed anything**
- Cause: The gate was never run in this repo clone (`needs-run` status)
- Fix: `needs-run` gates should be run once to establish a baseline; this is normal on a fresh clone; after the first run, only file changes will re-trigger them

**Problem: The action list is long — not sure which item to tackle first**
- Cause: Multiple things are pending from previous sessions
- Fix: Commit-blocking gates (`blocks: ["commit"]`) must be cleared before any code commit; deploy-blocking gates (`blocks: ["deploy"]`) before any apply; everything else is advisory
