# Demo: Agent-Human Skill Interactions

Worked examples showing how agents respond to common human requests using the skill library.
Use this skill for onboarding, walkthroughs, or to understand the agent's full capability.

Role: meta · teaching

Trigger phrases: "show me an example", "demo", "how would you handle", "walk me through",
Methodology-step: skills
"what would happen if I said", "example conversation", "demonstrate"

---

## How to Read These Examples

Each scenario shows:
1. **Human says:** the natural language input
2. **Agent reads:** which skill(s) guide the response
3. **Agent does:** the actual actions taken
4. **Agent responds:** the output the human sees

These are representative — actual responses vary based on current gate state and live GCP output.

---

## Scenario 1 — Morning Startup

**Human says:** `"Morning, let's get started"`

**Agent reads:** `session-recap.md` → follows steps in order

**Agent does:**
```powershell
# Step 0 — Gate check
pwsh -File scripts/utility/gate-advisor.ps1 -Action commit

# Step 2 — Service status
gcloud run services list --project=laas-489115 --region=us-central1 ...

# Step 4 — CI status
gh run list --workflow=deploy-dev.yml --limit=3
```

**Agent responds:**
> Good morning! Here's the current state:
>
> **Gates:** All code gates CLEAN. `smoke:cloud-run` is DIRTY (needs re-run before deploy).
> `e2e:*` gates are stale (>24h) — run `run-e2e.ps1` before any deploy.
>
> **Services:** All 4 Cloud Run services Ready ✓
>
> **CI:** Last run (2026-04-06) passed. storybook nginx fix deployed successfully.
> All 3 e2e tests green in CI.
>
> **Suggested first step:** Run the e2e gates locally to mark them clean before the
> next deploy. Start with: `pwsh -File scripts/verify/run-e2e.ps1 -Module chat`

---

## Scenario 2 — Something Is Down

**Human says:** `"Storybook is broken, users can't reach it"`

**Agent reads:** `incident-response.md` → phase 2 (assess) → `debug-cloud-run.md`

**Agent does:**
```bash
# Assess impact
curl -s -o /dev/null -w "%{http_code}" https://34-120-21-175.sslip.io/storybook

# Check Cloud Run status
gcloud run services describe storybook --project=laas-489115 --region=us-central1 \
  --format="value(status.conditions[0].status,status.latestReadyRevisionName)"

# Check recent logs
gcloud run services logs read storybook --project=laas-489115 --region=us-central1 --limit=30
```

**Agent responds:**
> Found the issue. `/storybook` is returning **404**.
>
> Cloud Run service is **Ready** and healthy — the problem is nginx, not the container.
>
> Logs show: nginx is receiving `GET /storybook` (no trailing slash) and returning 404.
> Root cause: `nginx.conf` is missing `location = /storybook { return 301 .../storybook/; }`
>
> **Fix:** Add the redirect block to `apps/storybook-web/nginx.conf` and push.
> Expected CI time: ~3 min (only storybook image will rebuild via change detection).
>
> Want me to apply the fix now?

---

## Scenario 3 — Deploying a Change

**Human says:** `"I've updated the chat UI. Deploy it to dev."`

**Agent reads:** `skill-chain.md` (Chain 3) → `weigh-time-risk.md` → `gate-status.md` → `apply-infra.md`

**Agent does:**
```powershell
# Weigh risk: app source change only — Tier 0 (Pester) + gate check sufficient
pwsh -File scripts/utility/gate-advisor.ps1 -Action deploy
```

**Agent responds:**
> Gate advisor says: **BLOCKED** — `e2e:chat` is stale (expired >24h).
>
> Before deploying, I need to clear this gate. Since you're about to deploy a UI change,
> it makes sense to run the e2e test against the *current* dev first to confirm the
> baseline is clean before your change goes out:
>
> ```powershell
> pwsh -File scripts/verify/run-e2e.ps1 -Module chat
> ```
>
> After that passes, we can push to main and CI will:
> 1. Detect only `app_chat` changed
> 2. Build only the chat image (~3 min)
> 3. Skip all TF jobs
> 4. Run `gcloud run services update chat --image=...chat:latest`
> 5. Run all 3 e2e tests to confirm
>
> Ready to run the gate check?

---

## Scenario 4 — What Skills Are Available?

**Human says:** `"What can you help me with on this project?"`

**Agent reads:** `skill-index.md`

**Agent responds:**
> The skill library has **45 skills** covering everything from daily operations to
> incident response. Here's a quick map:
>
> **Daily work:**
> - Starting a session: `session-recap.md`
> - Knowing what to do next: `what-next.md`
> - Deploying: `apply-infra.md` (manual) or CI via `ci-workflows.md`
>
> **When things break:**
> - Active outage: `incident-response.md`
> - Service misbehaving: `debug-cloud-run.md`
> - Roll back a bad deploy: `rollback-deployment.md`
>
> **Platform changes:**
> - Add a new app: `add-new-app.md`
> - Change secrets/API keys: `secrets-management.md`
> - Modify routing: `gateway-routing.md`
>
> **CI/CD:**
> - Pipeline structure: `ci-workflows.md`
> - Playwright smoke tests: `playwright-e2e.md`
>
> For the full categorized list, see `skill-index.md`.
> What are you trying to do?

---

## Scenario 5 — Writing a New Skill

**Human says:** `"We need a skill for cost analysis — estimating GCP spend from terraform plan"`

**Agent reads:** `write-skill.md` → `skill-index.md` → `skill-gap-tracker.md`

**Agent does:**
- Checks `skill-index.md` — no cost-analysis skill exists ✓
- Checks `skill-gap-tracker.md` — "Cost estimation from terraform plan" is in the Open Gaps table ✓
- Follows `write-skill.md` template and quality checklist

**Agent responds:**
> Great — this is a tracked gap in `skill-gap-tracker.md`. I'll write cost-analysis.md now.
>
> Following `write-skill.md` standards: I'll include trigger phrases, Cloud Run cost model
> (per-request + min-instances), Artifact Registry storage cost, and an example of reading
> a terraform plan output to estimate monthly spend.
>
> [writes the skill file]
>
> `assess-skill-on-edit.sh` hook fired: ✓ 4/4 structural checks passed.
>
> Done. I've:
> - Created `.claude/skills/cost-analysis.md` (312 lines)
> - Added it to `CLAUDE.md` skills index
> - Added it to `skill-index.md` under CI/CD → Operations
> - Moved it from Open to Closed in `skill-gap-tracker.md`
>
> Shall I commit it?

---

## Scenario 6 — Skill Discovery via Fuzzy Search

**Human says:** `"How do I rotate the Gemini API key?"`

**Agent reads:** `skill-index.md` → "Rotating GEMINI_API_KEY" matches `secrets-management.md`

**Agent responds:**
> That's covered in `secrets-management.md`. Here's the rotation procedure:
>
> Rotation = adding a new version to the existing secret. The old version stays accessible
> until you explicitly disable it. Cloud Run picks up `version = "latest"` automatically
> on the next container start.
>
> ```bash
> # 1. Add the new key value
> echo -n "NEW_API_KEY" | gcloud secrets versions add GEMINI_API_KEY \
>   --data-file=- --project=laas-489115
>
> # 2. Force a new revision to pick it up immediately
> gcloud run services update laas-api --region=us-central1 --project=laas-489115 \
>   --set-env-vars=ROTATION_MARKER=$(date +%s)
> # (then remove the marker)
>
> # 3. Verify the API still works
> curl https://34-120-21-175.sslip.io/api/health
>
> # 4. Disable the old version once confirmed healthy
> gcloud secrets versions disable <old-version> --secret=GEMINI_API_KEY --project=laas-489115
> ```
>
> Do NOT delete the old version until you're confident the new key works.

---

## Scenario 7 — Pre-Deploy Readiness Check

**Human says:** `"Am I ready to deploy?"`

**Agent reads:** `what-next.md` → runs gate advisor

**Agent does:**
```powershell
pwsh -File scripts/utility/gate-advisor.ps1 -Action deploy
```

**Agent responds:**
> Gate advisor verdict: **BLOCKED**
>
> Blockers (must clear before deploy):
> ```
> 1. preflight          NEEDS-RUN  (~1 min)  [critical-guardrail]
>    → pwsh -File scripts/utility/check-preflight.ps1 -ProjectID laas-489115
>
> 2. network:drift      NEEDS-RUN  (~2 min)  [critical-guardrail]
>    → pwsh -File scripts/network/check-network-drift.ps1 -ProjectID laas-489115
>
> 3. e2e:chat           STALE      (~2 min)  [final-validation]
>    → pwsh -File scripts/verify/run-e2e.ps1 -Module chat
> ```
>
> Total estimated time to unblock: ~5 min
>
> Run them in this order (preflight first — it validates the SA token we need for drift check).
> Want me to run them now?

---

## Scenario 8 — End of Session

**Human says:** `"I'm done for today, let's wrap up"`

**Agent reads:** `session-handoff.md`

**Agent does:**
```powershell
pwsh -File scripts/utility/gate-advisor.ps1 -Action commit
git status
git diff --stat HEAD
```

**Agent responds:**
> Here's the session summary for the handoff:
>
> **What was done this session:**
> - Fixed storybook nginx trailing-slash routing (`apps/storybook-web/nginx.conf`)
> - All 3 e2e tests passing in CI ✓
>
> **Current state:**
> - Gate advisor: PROCEED for commit
> - Last CI run `24018794927`: SUCCESS — all jobs green
> - No open apply failures
>
> **Nothing pending** — the session is clean.
>
> Committing gate state and pushing:
> ```bash
> git add .claude/test-gates.json
> git commit -m "chore: post-session gate state after storybook nginx fix"
> git push laas devops
> ```
>
> Done. Next session can start fresh with `session-recap.md`.

---

## Scenario 9 — Unknown Request (No Matching Skill)

**Human says:** `"Can you set up a load balancer in a different region?"`

**Agent reads:** `skill-index.md` → no matching skill found → checks `skill-gap-tracker.md`

**Agent responds:**
> There's no skill covering multi-region load balancer setup. The current platform uses a
> single-region setup (`us-central1`) with a global HTTPS LB fronting it.
>
> For that kind of change you'd be modifying `terraform/infra/` (the LB module) and
> potentially `terraform/foundation/` — I can help navigate that with `review-plan.md`
> and `apply-infra.md`, but there's no dedicated skill for multi-region topology.
>
> I'll add "multi-region LB setup" to `skill-gap-tracker.md` so it's tracked for a
> future skill. In the meantime, want me to look at the existing infra terraform to
> understand what would need to change?

---

## Scenario 10 — Assessing a Skill (Meta)

**Human says:** `"Is the ci-workflows skill accurate and up to date?"`

**Agent reads:** `ci-workflows.md` → cross-checks against actual `.github/workflows/deploy-dev.yml`

**Agent does:**
```bash
# Verify the DAG in the skill matches the actual workflow
grep "needs:" .github/workflows/deploy-dev.yml
grep "detect-changes" .github/workflows/deploy-dev.yml
```

**Agent responds:**
> I reviewed `ci-workflows.md` against the current `deploy-dev.yml`. Assessment:
>
> ✓ DAG is accurate — includes `detect-changes`, `update-services`, `run-e2e`
> ✓ Skip flags table is correct — `skip_e2e`, `skip_topology`, per-image flags documented
> ✓ Change detection path groups table is accurate
>
> ⚠ One stale item: the "Adding a New App" section still references the old 4-step process.
> It should now reference `add-new-app.md` for the full recipe (that skill was added 2026-04-06).
>
> Shall I update that section now?

---

## Using Demo Scenarios for Skill Validation

The demo scenarios above are not just documentation — they are test cases. An agent can
role-play as the human user to validate that each skill produces the expected response.

### How to Run a Skill Validation Pass

For each scenario in this file:

1. **Identify the trigger phrase** — the `Human says:` line
2. **Read the referenced skill(s)** — the `Agent reads:` line
3. **Execute the `Agent does:` commands** to gather live data
4. **Compare actual output against `Agent responds:`** — does the skill lead to the same
   conclusion, or has reality diverged?

If the actual output matches: skill is current ✓
If the actual output diverges: skill is stale → update it or log in `skill-gap-tracker.md`

### Validation Pass Example

**Scenario under test:** Scenario 7 (Pre-Deploy Readiness Check)

**Agent plays user:** "Am I ready to deploy?"

**Agent runs:**
```powershell
pwsh -File scripts/utility/gate-advisor.ps1 -Action deploy
```

**Agent compares:**
- Expected: gate advisor shows BLOCKED with specific gates listed
- Actual output: all gates CLEAN → `PROCEED`
- **Assessment:** Scenario 7 expected output is a snapshot, not a contract. The skill
  (`what-next.md`) is correct; the scenario example just shows what a blocked state looks like.
  ✓ No update needed.

### When Actual vs Expected Diverge

| Divergence type | Action |
|----------------|--------|
| Command no longer exists or has different flags | Update skill commands immediately |
| Expected output format changed (e.g. new gate names) | Update scenario + skill |
| Response leads to wrong next action | Flag in `skill-gap-tracker.md` as suspected inaccuracy |
| Skill trigger phrase matches wrong skill | Update trigger phrases or create disambiguation note |

### Full Validation Checklist (run after major refactors)

```bash
# For each scenario, manually verify the agent does block still runs cleanly
# Scenarios that depend on live infra: 1, 2, 3, 7, 8
# Scenarios that are structural/meta: 4, 5, 6, 9, 10
```

1. Run Scenario 1 trigger → check session-recap.md steps still match CI + gate output
2. Run Scenario 3 trigger → confirm gate-advisor.ps1 output format matches documented blockers
3. Run Scenario 7 trigger → confirm gate-advisor.ps1 -Action deploy output format unchanged
4. Run Scenario 8 trigger → confirm session-handoff.md steps still match git workflow
5. Run Scenario 5 trigger → confirm write-skill.md template + quality checklist still accurate
6. Check Scenario 2 commands → curl URL and gcloud commands still reference correct project/region

If all pass: add a review note at the bottom of this file with the date.
If any fail: update the scenario AND the underlying skill — they drift together.

---

## Troubleshooting

**Problem: A scenario's "Agent does" commands fail or produce unexpected output**
- This is expected — scenarios are snapshots, not contracts. The command itself may still be
  correct; the output has changed.
- Check whether the underlying skill's expected output section matches current reality.
  If not, update the skill first, then update the scenario to match.

**Problem: An agent picks a different skill than the one listed in "Agent reads"**
- The trigger phrase may now match a different skill more strongly.
- Run the trigger phrase in a new session and observe which skill is actually loaded.
- Either update the trigger phrases in the target skill, or add a disambiguation note.

**Problem: A scenario references a feature or script that no longer exists**
- Flag it as a suspected inaccuracy in `skill-gap-tracker.md`.
- Update the scenario to reflect current reality — outdated scenarios train agents
  to expect the wrong behavior.
