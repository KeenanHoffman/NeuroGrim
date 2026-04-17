# Philosophy Index

Cross-reference between the eight DevOps principles in `devops-philosophy.md` and the
technical skills that implement them. Read this when you want to understand why a skill
works the way it does, or when you want to know which principle should guide a decision.

Role: philosophy · reference

Trigger phrases: "which principle applies here", "why does this skill work this way",
Methodology-step: skills
"philosophy behind X", "what principle governs this", "why is this rule here",
"connect the skill to the principle", "what's the philosophy behind"

---

## How to Use This Index

**Before diving into a technical skill:** Check the Skills → Principle section below.
If the principle isn't obvious from the skill name, reading `devops-philosophy.md` first
will make the technical steps make sense rather than feel arbitrary.

**When evaluating a shortcut:** Look up the relevant principle in the Principle → Skills
section. The "Why Not" section at the bottom directly addresses common shortcuts and
why the philosophy argues against each one.

**During an incident:** The Principle → Skills section maps Observability Before Action
and Progressive Delivery to the skills most relevant in a crisis. Read the philosophy note,
then read the skill.

**When writing a new skill:** Identify which principle the skill embodies. Add a
`## Why This Matters (Philosophy)` section linking back to `devops-philosophy.md`.

---

## Principle → Skills Mapping

### Immutable Infrastructure

*Replace, don't patch. Systems are rebuilt from source, not modified in place.*

| Skill | How the principle manifests |
|-------|-----------------------------|
| `apply-infra.md` | Deploy sequence rebuilds services from Terraform state, never patches in place |
| `rollback-deployment.md` | Rollback = point traffic to a prior immutable revision, not revert a mutation |
| `docker-builds.md` | New image per change; no in-container mutations |
| `retire.md` | Full teardown and rebuild rather than accumulated mutations |
| `state-recovery.md` | Recover from partial applies by restoring to a known-good declared state |

---

### GitOps / Single Source of Truth

*The repository is the authoritative description of the system.*

| Skill | How the principle manifests |
|-------|-----------------------------|
| `apply-infra.md` | Plan → review → apply ensures git state is applied, not console state |
| `network-topology.md` | Topology snapshot committed; drift from committed state is a finding |
| `access-topology.md` | IAM bindings committed; `needs_review` bindings are surfaced |
| `drift-check.md` | Detects divergence between git state and real cloud state |
| `ci-workflows.md` | CI deploys from `main`; human pushes to main to trigger authoritative deploy |
| `secrets-management.md` | Secret bindings are Terraform; only secret values are outside git |
| `session-handoff.md` | Committed gates and topology are the handoff artifact, not verbal state |

---

### Fail Fast / Shift Left

*Find problems as early as possible.*

| Skill | How the principle manifests |
|-------|-----------------------------|
| `gate-status.md` | Gates enforce that tests are clean before deploying |
| `what-next.md` | Surfaces blocked gates before the agent proposes a deploy |
| `test.md` | Unit tests run on every edit, before any integration or deploy step |
| `smoke-infra.md` | Smoke tests run against real GCP before gateway changes |
| `ci-testing.md` | Four-layer confidence ladder: local lint is first, full integration is last |
| `preflight.md` | 8-item check runs before any apply command |
| `review-plan.md` | Plan review catches infrastructure mistakes before apply |
| `write-smoke-tests.md` | Topology annotations drive test assertions; tests are written before issues occur |

---

### Defense in Depth

*Layer multiple independent safeguards.*

| Skill | How the principle manifests |
|-------|-----------------------------|
| `apply-infra.md` | SA liveness check + plan safety check + topology diff + gate check = 4 independent gates |
| `diagnose-iap.md` | IAP is an independent auth layer in front of application auth |
| `preflight.md` | Preflight checks are a second opinion independent of individual apply scripts |
| `ci-testing.md` | Four independent checkpoints for pipeline changes |
| `secrets-management.md` | Secret Manager + SA scoping + short-lived tokens = three independent barriers |
| `access-topology.md` | Access topology is an independent audit of IAM state after every change |

---

### Observability Before Action

*Before acting, establish what is actually true.*

| Skill | How the principle manifests |
|-------|-----------------------------|
| `session-recap.md` | Always establishes true current state before any work begins |
| `incident-response.md` | Phase 1 (Detect) and Phase 2 (Assess) precede all remediation |
| `network-topology.md` | Read topology before applying changes that touch the network |
| `access-topology.md` | Read access topology before any IAM changes |
| `gate-status.md` | Check gate state before acting on test results you haven't verified |
| `post-deploy-verify.md` | Confirm the apply actually achieved the intended state |
| `drift-check.md` | Establish the actual state of the cloud before planning further changes |
| `debug-cloud-run.md` | Read logs and revision state before guessing at a fix |
| `explain-error.md` | Classify the error before attempting to fix it |

---

### Least Privilege

*Every identity should have exactly the permissions required. No more.*

| Skill | How the principle manifests |
|-------|-----------------------------|
| `apply-infra.md` | SA impersonation uses short-lived tokens, not key files |
| `preflight.md` | SA liveness check validates token, not a persistent credential |
| `access-topology.md` | All IAM bindings tracked; `needs_review` surfaces over-broad grants |
| `diagnose-iap.md` | IAP troubleshooting starts from minimum required roles |
| `secrets-management.md` | Secret access scoped to specific SA; rotation via Terraform |
| `bootstrap.md` | SA creation assigns only the Terraform-required roles, no owner/editor |

---

### Progressive Delivery

*Deliver changes incrementally, validate each increment, keep rollback cheap.*

| Skill | How the principle manifests |
|-------|-----------------------------|
| `apply-infra.md` | Module sequence (foundation→infra→app→apps→gateway) is ordered by dependency |
| `deployment-flow.md` | Sandbox → dev → prod promotion ladder, one environment at a time |
| `rollback-deployment.md` | Cloud Run revisions make rollback a one-command operation |
| `sandbox.md` | Personal ephemeral environment validates changes before shared dev |
| `weigh-time-risk.md` | Risk assessment determines how much validation is required per change |
| `ci-workflows.md` | Change detection deploys only what changed; full pipeline for full confidence |

---

### Everything is Code

*Infrastructure, policies, tests, and procedures all belong in source control.*

| Skill | How the principle manifests |
|-------|-----------------------------|
| `write-skill.md` | Runbooks are code: versioned, reviewed, triggerable |
| `ci-workflows.md` | Pipelines are YAML in git, not click-ops in the CI UI |
| `test.md` | Operational tests are Pester scripts, not manual checklists |
| `gateway-routing.md` | URL map rules are Terraform resources, not console configurations |
| `terraform-migration.md` | State migrations are scripted and committed, not manual state edits |
| `network-topology.md` | Topology snapshot is a committed JSON artifact, not tribal knowledge |
| `publish.md` | Image builds are scripted and version-controlled, not manual `docker build` commands |

---

## Skills → Principle Mapping

A quick lookup by skill area to see which principle is the primary driver.

| Skill area | Primary principle | Supporting principles |
|-----------|-------------------|----------------------|
| Gate system (`gate-status.md`, `what-next.md`) | Fail Fast / Shift Left | Everything is Code |
| Topology snapshots (`network-topology.md`, `access-topology.md`) | Observability Before Action | Everything is Code, GitOps |
| Terraform state (`apply-infra.md`, `state-recovery.md`) | Immutable Infrastructure | GitOps |
| Pre-commit hooks (`gate-status.md`, `preflight.md`) | Shift Left | Defense in Depth |
| Rollback (`rollback-deployment.md`, `incident-response.md`) | Progressive Delivery | Observability Before Action |
| IAM / access (`access-topology.md`, `diagnose-iap.md`) | Least Privilege | Defense in Depth |
| CI pipelines (`ci-workflows.md`, `ci-testing.md`) | Everything is Code | Fail Fast, Progressive Delivery |
| Secrets (`secrets-management.md`) | Least Privilege | GitOps, Defense in Depth |
| Deploy sequence (`deployment-flow.md`, `environments.md`) | Progressive Delivery | Immutable Infrastructure |
| Session management (`session-recap.md`, `session-handoff.md`) | Observability Before Action | Everything is Code |
| Testing (`test.md`, `smoke-infra.md`, `playwright-e2e.md`) | Fail Fast / Shift Left | Everything is Code |

---

## The "Why Not" Section

Common shortcuts and what the philosophy says about each.

---

**"Why not just edit in the console?"**

Violated principle: **GitOps / Single Source of Truth**

The console change is real and immediate. But it has no git history, no review, no way to
replay it in another environment, and no way to know it happened unless someone was watching.
The next `terraform apply` will treat the console change as drift and undo it. The fix
requires encoding the change in Terraform and committing it.

See `drift-check.md` for how to detect console changes that preceded a Terraform apply.

---

**"Why not skip the tests when I'm in a hurry?"**

Violated principle: **Fail Fast / Shift Left**

The urgency is real. But tests exist because real failures have already been observed and
encoded as assertions. Skipping tests doesn't save time — it defers finding the failure to
production, where the cost is higher. The gate system exists precisely for this moment.

If the tests themselves are wrong, fix the tests. If a test must be temporarily bypassed for
an emergency, document the bypass and create a follow-up gate to re-enable it.

---

**"Why not use a long-lived service account key file?"**

Violated principle: **Least Privilege**

Key files are static credentials. If leaked (in a commit, a log, a screenshot), they grant
persistent access until explicitly rotated. Short-lived impersonated tokens expire automatically.
The `check-sa-liveness.sh` hook validates the short-lived token before every apply; this is
the intended workflow.

---

**"Why not deploy directly to dev without going through sandbox first?"**

Violated principle: **Progressive Delivery**

Sandbox is a personal, isolated copy of the full environment. Any change that breaks sandbox
breaks it for you only, with no user impact. The same change breaking dev affects everyone.
The sandbox step is cheap insurance. Merge to `main` after sandbox passes; CI handles dev.

---

**"Why not keep the topology docs in a wiki instead of in git?"**

Violated principle: **Everything is Code + GitOps**

A wiki can be edited without review, doesn't track who changed what, can't be diffed, and
drifts from reality without anyone noticing. Committed JSON topology artifacts are versioned,
diffable, and tied to the git history of the changes that produced them.

---

**"Why not grant editor/owner role to the SA so it can do everything it needs?"**

Violated principle: **Least Privilege**

Broad roles are a blast radius. If the SA is compromised, an attacker has editor or owner
access to the entire project. Terraform operations only need specific roles. See `bootstrap.md`
for the minimum required role set.

---

**"Why not write one big end-to-end test instead of unit + integration + e2e?"**

Violated principle: **Fail Fast / Shift Left**

A single e2e test catches failures late (after a full deploy), runs slowly, and produces
ambiguous failures (which layer broke?). The cost pyramid: unit tests are cheap and run on
every edit. The e2e test is expensive and runs once after deploy. Skipping the lower layers
means every defect survives until the most expensive layer.

---

## Troubleshooting

**Problem: "I can't tell which principle applies to a decision I'm making"**
- Read `devops-philosophy.md` section "Applying Philosophy to Common Decisions" for worked
  examples that model the reasoning process.
- Most decisions involve multiple principles. Pick the most constraining one (the one that
  rules out the most options) as your guide.

**Problem: "A skill tells me to do X, but X seems to violate a principle"**
- The skill is describing the GCP implementation. The principle is the intent. If they appear
  to conflict, the principle takes precedence — either the skill has a bug, or you're
  misreading the implementation detail.
- File it as a skill gap in `skill-gap-tracker.md`.

**Problem: "I'm in an incident and don't have time to read philosophy"**
- You're right, don't read philosophy during an active incident. Go straight to
  `incident-response.md`.
- Use the philosophy index post-incident during a retrospective to understand why the incident
  happened (which principle was violated) and what the remediation codified.

---

## See Also

- `devops-philosophy.md` — the eight principles in full with examples and troubleshooting
- `skill-index.md` — full skill directory by technical area
- `write-skill.md` — includes the optional "Why This Matters (Philosophy)" section template
