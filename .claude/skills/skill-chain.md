# Skill Chains

Common multi-skill sequences for complex tasks. Each chain shows which skills to read
and in what order, with the handoff point between them.

> **Honesty note (2026-04-17):** Some skill names referenced in chains below are
> aspirational patterns rather than authored files — specifically: `session-recap.md`,
> `start-feature.md`, `sandbox.md`, `pr-checklist.md`, `preflight.md`, `apply-infra.md`.
> Those names describe operator workflows that are still valid patterns; agents reading
> a chain should apply the pattern using currently-authored skills where a direct
> mapping isn't available. See `skill-index.md` for the authored inventory.

Role: meta

Trigger phrases: "what order do I use these skills", "multi-step workflow", "skill sequence",
Methodology-step: skills
"how do I do a full deploy", "morning workflow", "what's the full process for X"

---

## How to Use This Skill

Find the chain that matches your task. Read the listed skills in order. Each skill's
"See Also" section will also point you forward when you're done with it.

---

## Chain 0 — Start New Feature (after a PR merges)

*Starting the next piece of work after a PR merges. Goal: clean branch, correct base.*

```
session-recap.md            (confirm gate state is clean, no dirty leftovers)
  → start-feature.md        (fetch laas/main, create branch, clean up old branch)
  → sandbox.md              (if the change touches terraform/ — deploy personal sandbox)
  → pr-checklist.md         (when ready to open the PR)
```

**When to use:** Every time you begin a new feature, fix, or chore after a previous PR merges.
**Time cost:** <2 min for a code-only branch; +sandbox deploy time for terraform changes
**Automated:** `check-new-branch-base.sh` warns if you forget `laas/main` as base;
`suggest-pr-on-push.sh` reminds you to open a PR after the first push.

---

## Chain 1 — Morning Startup

*Starting a new session after a break. Goal: establish current state before doing any work.*

```
session-recap.md
  → gate-status.md          (if any DIRTY gates found in recap)
  → what-next.md            (once state is known — to prioritize the session)
```

**Hat:** Check Brain `suggested_hat` during recap. If operator or security hat is suggested,
wear it for the session's focused work.
**When to use:** First thing in any session, or after returning from a break of >2 hours.
**Time cost:** ~5 min (mostly waiting for gcloud commands)
**Key output:** Gate advisor verdict + list of what's safe to work on today

---

## Chain 2 — Pre-Commit

*About to run `git commit`. Goal: ensure code gates are clean.*

```
gate-status.md              (run gate-advisor -Action commit)
  → test.md                 (if pester:* gates are dirty — run the failing suites)
  → gate-status.md          (re-check after fixing)
```

**When to use:** Before every `git commit`.
**Time cost:** <2 min (Pester suites run in 10–40s)
**Automated:** The `pre-commit.sh` hook runs this check automatically. You only need this
chain if the hook fires and you want to understand why.

---

## Chain 3 — Pre-Deploy Safety Check

*About to run an apply script or trigger CI deploy. Goal: confirm all blockers are clear.*

```
weigh-time-risk.md          (decide which tier of testing is needed)
  → gate-status.md          (run gate-advisor -Action deploy)
  → preflight.md            (if gates PROCEED — verify SA token, locks, etc.)
  → review-plan.md          (run terraform plan, review output)
[parallel: preflight.md and review-plan.md are independent — see Chain 16]
  → apply-infra.md          (the actual deploy)
  → post-deploy-verify.md   (after apply completes)
```

**Hat:** Wear `operator` hat throughout the deploy chain. Brain recommendations re-sort to
prioritize deploy-blocking gates and stale artifacts.
**When to use:** Before any `apply-*.ps1` or CI `workflow_dispatch`.
**Time cost:** 5–30 min depending on tier
**Key decision point:** `weigh-time-risk.md` determines whether you need Tier 2 (validate),
Tier 3.5 (smoke), or just Tier 3 (preflight) before applying.

---

## Chain 4 — Terraform Change

*Made edits to `.tf` files. Goal: get them deployed safely.*

```
weigh-time-risk.md          (assess risk of the specific change)
  → review-plan.md          (run plan, check for destructive changes)
  → network-topology.md     (if plan touches network resources — run diff-network-change.ps1)
  → apply-infra.md          (apply in correct module order)
  → post-deploy-verify.md   (verify services healthy after apply)
  → network-topology.md     (update topology snapshot)
  → access-topology.md      (if IAM resources changed)
[parallel: network-topology.md and access-topology.md are independent — see Chain 14]
```

**When to use:** After editing any `terraform/**/*.tf` file.
**Time cost:** 15–45 min
**Key gate:** `network:topology` and `access:topology` must be updated after apply

---

## Chain 5 — Something Is Down (Incident)

*A service is returning errors or users can't access the app. Goal: restore service ASAP.*

```
incident-response.md        (phase 1: detect + assess + classify)
[parallel: Phase 2 diagnosis and rollback preflight can overlap — see Chain 15]
  → debug-cloud-run.md      (if Cloud Run service issue — get logs + revision status)
  → rollback-deployment.md  (if deploy regression — roll back immediately)
  → post-deploy-verify.md   (after recovery — confirm all services healthy)
  → fix-apply-failure.md    (after recovery — add a test to prevent recurrence)
```

**Hat:** Wear `operator` hat; pair with `incident-commander` persona. Brain health check
with `-Hat operator` prioritizes deploy-blocking signals during triage.
**When to use:** Any time a deployed service is behaving unexpectedly.
**Time cost:** 5–30 min (rollback is ~3 min; diagnosis can take longer)
**Key principle:** Roll back first, investigate second. See `incident-response.md` for
the roll-back vs fix-forward decision criteria.

---

## Chain 6 — CI Pipeline Failure

*A GitHub Actions run failed. Goal: understand what failed and fix it.*

```
session-recap.md            (step 4: check CI pipeline status)
  → ci-workflows.md         (understand which job failed and why)
  → explain-error.md        (classify the error type)
  → fix-apply-failure.md    (if terraform apply failed — write regression test)
  → ci-testing.md           (if workflow config changed — validate before re-push)
```

**When to use:** After `gh run list` shows a failed deploy-dev.yml run.
**Time cost:** 5–20 min
**Key command:** `gh run view <run-id> --log-failed` to see the specific error

---

## Chain 7 — Adding a New Sub-App

*Want to add a 6th frontend to the platform. Goal: wire everything correctly.*

```
add-new-app.md              (full recipe — follow the checklist in order)
  → docker-builds.md        (if Docker build issues arise)
  → playwright-e2e.md       (wire up the smoke test and gate)
  → ci-workflows.md         (add the app to detect-changes + build-all-images + deploy-dev)
  → gateway-routing.md      (add URL map entry)
  → post-deploy-verify.md   (verify after first deploy)
```

**When to use:** Adding a new Cloud Run sub-app to the platform.
**Time cost:** 2–4 hours (most time in Dockerfile + terraform iteration)
**Key lesson:** Add the nginx trailing-slash redirect early — see `add-new-app.md` for why.

---

## Chain 8 — Sandbox Development Cycle

*Developing a change in sandbox before merging to dev. Goal: test safely without risk to dev.*

```
sandbox.md                  (create/access sandbox environment)
  → apply-infra.md          (apply changes to sandbox state)
  → local-proxy.md          (access sandbox services via gcloud proxy)
  → playwright-e2e.md       (run smoke tests against sandbox BASE_URL)
  → ci-testing.md           (validate CI changes in sandbox before merging)
```

**When to use:** Any change that touches terraform or Docker before merging to main.
**Time cost:** 30–90 min per iteration
**Key principle:** sandbox state is isolated — changes don't affect dev

---

## Chain 9 — Bootstrap New Project

*Setting up LaaS from scratch in a new GCP project. Goal: reproduce the full platform.*

```
environments.md             (understand the tier structure first)
  → setup.md                (local tooling + credentials)
  → bootstrap.md            (GCS state bucket + terraform SA + WIF)
  → apply-infra.md          (foundation → infra → app → apps → gateway in order)
  → ci-workflows.md         (configure GitHub Actions secrets + enable CI)
  → playwright-e2e.md       (run smoke tests to confirm everything works)
```

**When to use:** New GCP project or disaster recovery.
**Time cost:** 2–4 hours
**Key blocker:** WIF setup must happen before any CI workflow can auth to GCP

---

## Chain 10 — Session Handoff (End of Session)

*Wrapping up a work session. Goal: leave the repo in a clean, documented state.*

```
gate-status.md              (run gate-advisor -Action commit — commit any clean gates)
  → session-handoff.md      (produce handoff note + commit uncommitted artifacts)
```

**When to use:** Before ending any productive session.
**Time cost:** ~5 min
**Key output:** A commit (or saved note) that captures what was done and what's pending

---

## Chain 11 — Skill System Maintenance

*Adding, updating, or auditing skills. Goal: keep the skill library accurate and complete.*

```
write-skill.md              (authoring standards before writing a new skill)
  → skill-index.md          (find existing skills to avoid duplication)
  → skill-gap-tracker.md    (check if the gap you're filling is tracked)
  → write-skill.md          (quality checklist before saving)
  → skill-hook-pairs.md     (evaluate whether a companion hook is needed)
  → CLAUDE.md               (add to skills index after writing)
```

**When to use:** Writing a new skill or substantially updating an existing one.
**Time cost:** 30–90 min per skill

---

## Chain 12 — Skill+Hook Pair Design

*Identifying and implementing a companion hook for an existing skill.*

```
skill-hook-pairs.md         (check proposed queue; identify pair type: Enforcement/Detection/Verification/Automation)
  → hooks-reference.md      (choose trigger: PreToolUse/PostToolUse/PostEditFile/git hook)
  → dual-review.md          (T+P review before registering — especially if hook will block)
  → settings.json           (register hook; validate-gates-json.sh fires on save)
  → skill-hook-pairs.md     (move proposed → implemented)
  → hooks-reference.md      (add row to reference table)
```

**When to use:** When `assess-skill-on-edit.sh` check-10 fires, or when you identify a skill
behavior that should be automated or enforced.
**Time cost:** 30–90 min (hook implementation + dual review)
**Key decision:** Enforcement hooks block (exit 1) and need the highest justification. Detection
and Automation hooks advise (exit 0) and are lower risk.

---

## Chain 13 — New Skill with Full Review

*Writing a new skill that introduces a new operational pattern. Goal: ensure it's both
correct and principled before registering it.*

```
write-skill.md              (draft skill using template + checklist)
  → assess-skill-on-edit.sh (auto-fires: checks 1-10 + two-pass T/P review)
  → skill-hook-pairs.md     (if check-10 fires: evaluate companion hook → Chain 12)
  → dual-review.md          (if high-stakes pattern: produce formal T+P review record)
  → skill-index.md          (add to Role Reference table + appropriate category section)
  → philosophy-index.md     (if skill implements a principle: add to principle mapping)
  → skill-chain.md          (if skill creates a new multi-skill workflow: add chain here)
```

**When to use:** Any new skill introducing a pattern that will be repeated — especially
new operational workflows, blocking hooks, or architectural decisions.
**Time cost:** 60–120 min (most time in review and cross-reference updates)
**Key output:** A skill that passes all 10 checks, has a companion hook decision recorded,
and has an approved dual review if the pattern is high-stakes.

---

## Chain 14 — Post-Deploy Parallel Verification

*Steps 1–3 and Step 6 of `post-deploy-verify.md` are independent. Run them as a fan-out
rather than serializing. Steps 5, 7, 8 must remain sequential.*

```
  Parent
   ├── Agent A: Step 1 — Cloud Run Ready   ──┐
   ├── Agent B: Step 2 — Route Smoke Test  ──┼── simultaneous
   ├── Agent C: Step 3 — API Health Check  ──┤
   └── Agent D: Step 6 — Topology Refresh  ──┘
         ↓ converge: surface all failures before Step 5
         Brain: pwsh -File scripts/dev/Find-Brain.ps1 -Mode score -Plain
         (score < 60 after converge: add recommend output to incident report)
   Step 5 (E2E) — inline, sequential (writes test-gates.json)
         ↓
   Step 7 (Gate Confirmation) — inline, sequential (reads gate state)
         ↓
   Step 8 (Commit) — inline, sequential (git write)
```

**Pattern:** Parallel Fan-Out (Pattern 1 in `subagent-patterns.md`)
**When to use:** Any post-deploy verification cycle after `apply-*.ps1` or CI deploy
**Expected saving:** ~90 seconds vs. sequential (30 sec parallel vs. ~2 min serialized)

---

## Chain 15 — Incident Parallel Diagnosis

*When deploy regression is a plausible hypothesis, Phase 2 diagnosis and rollback preflight
are independent. Overlap them to compress decision time.*

```
Phase 1 (Detect) — inline
  ↓
  ├── Agent A: Phase 2 — Route probes + Cloud Run status  ──┐  simultaneous
  └── Agent B: Rollback preflight — find previous image   ──┘
        ↓ convergence
        Brain: pwsh -File scripts/dev/Find-Brain.ps1 -Mode health -Plain
        (incident_patterns fires: image_build_failure→4A, iam_drift_cascade→Phase 3)
  if A: category A + B: rollback ready → Phase 4A: rollback immediately
  if A: category B/C/D/E             → Phase 4B: fix-forward
  else                               → Phase 3: classify + decide
        ↓
  Phase 5 (Verify Recovery) — inline, sequential
        ↓
  Phase 6 (Post-Incident) — inline, sequential
```

**Pattern:** Staged Convergence (Pattern 2 in `subagent-patterns.md`)
**When to use:** Any incident where a recent bad deploy is a plausible root cause
**Expected saving:** 3–5 minutes — rollback image lookup overlaps with route probes
**When NOT to use:** When root cause is unknown (run Phase 2 inline first to classify)

---

## Chain 16 — Pre-Deploy Parallel Safety Gate

*Preflight (infrastructure readiness) and plan review (change risk) address different
concerns and are independent. Both feed the deploy/abort decision.*

```
  ├── Agent A: preflight.md — SA token, locks, drift   ──┐  simultaneous
  └── Agent B: review-plan.md — destructive changes    ──┘
        ↓ staged convergence decision
        Brain: pwsh -File scripts/dev/Find-Brain.ps1 -Mode score -Plain
        (score < 50 → BLOCKED: below safe-deploy threshold; score >= 50 → proceed)
  if A: FAIL        → BLOCKED: list failures; do not apply
  if B: risk=critical → BLOCKED: present destructive resources; require confirmation
  if B: risk=high   → WARN: present IAM changes; require user approval
  if A: PASS + B: risk=low|medium → PROCEED to apply-infra.md
```

**Pattern:** Staged Convergence (Pattern 2 in `subagent-patterns.md`)
**When to use:** Any pre-deploy safety check; replaces serial Chain 3 preflight+plan steps
**Prerequisite:** `weigh-time-risk.md` must have determined both checks are needed for this tier

---

## Chain 17 — Day 1 / Local Dev Setup

*First time in this repo, or setting up a new machine. Goal: working local environment with correct credentials and first feature branch.*

```
setup.md                    (install tools + configure gcloud credentials + WIF)
  → local-dev.md            (run apps locally, wire API_URL, verify pnpm commands)
  → start-feature.md        (create first feature branch from laas/main)
```

**When to use:** First time working in this repo, or setting up on a new machine.
**Time cost:** 30–60 min (mostly tool installs + WIF credential setup)
**Key blocker:** WIF setup in `setup.md` must complete before `gcloud auth` works; credentials must work before sandbox deploys are possible.

---

## Chain 18 — Topology Snapshot Cycle

*After terraform changes to networking or IAM. Goal: ensure topology gates reflect current deployed state.*

```
weigh-time-risk.md          (determine whether topology refresh is needed for this change)
  → network-topology.md     (if networking/Cloud Run changed — run update-network-topology.ps1)
  → access-topology.md      (if IAM changed — run update-access-topology.ps1)
  → gate-status.md          (confirm network:topology and access:topology gates are CLEAN)
```

**When to use:** After any terraform apply touching network or IAM resources; also whenever `network:topology` or `access:topology` gates are STALE before a deploy.
**Time cost:** 2–5 min
**Parallelizable:** Both topology updates are independent — if both are needed, run them concurrently (see Chain 4 for the full terraform change sequence that includes this).
**Automated:** `annotate-topology.sh` and `annotate-access-topology.sh` surface `needs_review` resources automatically after any Bash command.

---

## Chain 19 — Testing / Validation Cycle

*Validate a change before committing or opening a PR. Goal: all relevant gates CLEAN.*

```
weigh-time-risk.md          (decide which tests are required for this change tier)
  → test.md                 (run Pester suite for any changed scripts)
  → smoke-infra.md          (if terraform changed — run smoke tests against real GCP)
  → playwright-e2e.md       (if routing/IAP changed — run e2e against the LB)
  → gate-status.md          (confirm all relevant gates are CLEAN before committing)
```

**When to use:** Any change touching `scripts/`, `terraform/`, or routing config.
`weigh-time-risk.md` tells you which downstream steps are required for your tier —
don't run all of them for a one-line comment fix.
**Time cost:** 2 min (Pester only) to 30 min (full e2e suite)
**Parallelizable:** `smoke-infra.md` and `playwright-e2e.md` are independent once Pester passes (see Chain 14 for parallel verification pattern)

---

## Chain 20 — State Recovery

*Terraform state is stuck, partially applied, or locked. Goal: clean state, services healthy, regression test written.*

```
state-recovery.md           (assess what's stuck: partial apply, lock, orphaned resource)
  → drift-check.md          (after recovery — confirm state matches real GCP resources)
  → fix-apply-failure.md    (write a regression test to prevent recurrence)
  → post-deploy-verify.md   (confirm services are healthy after recovery)
```

**When to use:** After any apply that exits non-zero and leaves Terraform state unknown —
stuck lock, partial resource creation, import needed, or `terraform state` errors.
**Time cost:** 5–30 min depending on how far the apply got before failing
**Critical:** Start with `state-recovery.md` before running any `terraform state` commands —
the wrong recovery sequence can make the state worse.

---

## Chain 21 — Brain-Guided Remediation

*Brain health score is below threshold and you want structured, traceable recovery.
Goal: propose remediation, plan execution waves, execute, and verify with historical context.*

```
brain.md -Mode propose     → structured proposals with writes_to + depends_on
brain.md -Mode plan        → topologically sorted execution waves
  → execute wave 1         → parallel proposals (independent domains)
  → brain.md -Mode score   → verify score improved after wave 1
  → execute wave 2         → dependent proposals (requires wave 1 outputs)
  → brain.md -Mode trend   → compare current trajectory against historical baseline
  → operational-memory.md  → check if this pattern has occurred before
```

**When to use:** When `brain.md -Mode score` returns below 60, or after `gate-completion.sh`
emits a low-health nudge.
**Time cost:** 10-30 min depending on number of dirty domains
**Key output:** Score delta between waves; historical match from `operational-memory.md`
confirms whether this is a recurring pattern or a novel failure.

---

## Troubleshooting

**Problem: Not sure which chain applies to my task**
- Read the one-line description under each chain header. If none fit exactly, use the closest
  chain and skip inapplicable steps. Most chains have explicit "skip if not applicable" notes.
- If no chain covers your task, that's a skill gap — add it to `skill-gap-tracker.md`.

**Problem: A skill referenced in a chain doesn't exist**
- The chain file may be stale. Check that the skill file exists: `ls .claude/skills/<name>.md`
- If it's missing, check `skill-gap-tracker.md` to see if it's a known gap.
- Report the broken chain reference as a suspected inaccuracy in `skill-gap-tracker.md`.

**Problem: Chain step order seems wrong for my specific case**
- Chain orders are guidelines for the common case, not hard rules. The "See Also" section
  at the end of each skill shows natural next steps from that skill's perspective.
- For unusual cases, compose your own sequence and document it as a new chain if it comes
  up more than once.
