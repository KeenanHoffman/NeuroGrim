# DevOps Philosophy

Platform-agnostic principles that govern all decisions on this project. Read this when a
technical choice feels uncertain, when you're evaluating a shortcut, or when the "why" behind
a rule isn't obvious from the implementation.

Role: philosophy

Trigger phrases: "why are we doing this", "what's the right approach", "big picture",
Methodology-step: skills
"platform-agnostic", "first principles", "philosophy", "what really matters in devops",
"why does this exist", "is this the right way", "explain the reasoning"

---

## The Core Principles

These eight principles are the foundation. Every technical decision on this project traces back
to at least one of them. When the right technical move isn't obvious, pick the principle first.

---

### 1. Immutable Infrastructure

**Statement:** Replace, don't patch. Systems are rebuilt from source, not modified in place.

**Why it matters:** Mutable systems accumulate invisible state. After enough in-place changes,
no one knows what the running system actually contains. Drift between environments becomes
impossible to debug. Immutable infrastructure makes the deployed state deterministic and
reproducible.

**How it manifests here:**
- Cloud Run services are never SSH'd into and modified. New code = new container image = new revision.
- Terraform resources are destroyed and recreated (via `replace`) rather than mutated when
  the mutation would leave ambiguous state.
- No one runs `gcloud run services update --set-env-vars` directly in production; that change
  must be in source and re-deployed.

---

### 2. GitOps / Single Source of Truth

**Statement:** The repository is the authoritative description of the system. If it's not in
git, it doesn't exist.

**Why it matters:** Console changes, one-off `gcloud` commands, and "quick fixes" applied
directly to the cloud produce systems that cannot be reproduced, reviewed, or rolled back.
The repo is the only place where history is preserved, changes are reviewed, and the state
is auditable.

**How it manifests here:**
- All infrastructure is Terraform-managed. No IAM binding, URL map rule, or Cloud Run config
  is authoritative unless it exists in `terraform/`.
- Network and access topology snapshots (`network-topology.json`, `access-topology.json`) are
  committed so the current state is always in version control.
- The gate system tracks test and infra state in `test-gates.json`, also committed.
- CI deploys from `main`. Manual deploys go to sandbox first, then merge to trigger CI for dev.

---

### 3. Fail Fast / Shift Left

**Statement:** Find problems as early as possible. The cost of a bug rises with every step
it travels toward production.

**Why it matters:** A bug caught in a unit test costs minutes to fix. The same bug caught
after a production deploy costs hours, affects users, and may require a rollback under
pressure. Every gate, test, and pre-commit check is an investment that pays back many times
over.

**How it manifests here:**
- Pre-commit hooks block commits with dirty code gates.
- Pester tests run automatically after script edits (via the `smoke-test.sh` hook).
- `terraform validate` fires after every `.tf` edit before any apply.
- CI runs lint → tests → plan before any apply job.
- The gate system enforces that tests are clean before a deploy is allowed.
- Sandbox is the first environment where changes land. Dev is only touched after sandbox passes.

---

### 4. Defense in Depth

**Statement:** No single control is sufficient. Layer multiple independent safeguards so that
one failure doesn't produce a breach or an outage.

**Why it matters:** Any individual control can fail: a test can have a bug, a reviewer can
miss something, a hook can be bypassed. Layered defenses mean that two independent failures
must align for a bad change to reach production.

**How it manifests here:**
- IAP authentication sits in front of all services, independent of application-level auth.
- SA impersonation uses short-lived tokens, not long-lived keys.
- Destructive resource checks fire at the plan stage AND at the apply stage.
- The four-layer CI testing confidence system (local lint → static validate → plan → full
  integration) provides independent checkpoints for pipeline changes.
- Topology drift checks are a second opinion after every apply.

---

### 5. Observability Before Action

**Statement:** Before acting on a broken or uncertain system, establish what is actually true.
Assumptions based on what should be true cause most operational errors.

**Why it matters:** Acting on a misdiagnosis makes things worse. The impulse to "just fix it
quickly" under incident pressure leads to applying changes to the wrong service, rolling back
to a version that also has the bug, or making a second failure while the first is unresolved.

**How it manifests here:**
- Establishing true current state at session start is the first operator move before any action.
- `incident-response.md` begins with a detection and assessment phase before any remediation.
- Topology snapshots are read before applying changes that affect the network.
- Gate advisor (`gate-advisor.ps1`) is checked before deploying, not after something breaks.
- `post-deploy-verify.md` runs after every apply to confirm the intended state was reached.

---

### 6. Least Privilege

**Statement:** Every identity — human, service account, or CI runner — should have exactly
the permissions required to perform its function. No more.

**Why it matters:** Broad permissions are a blast radius amplifier. When a credential is
compromised or misused, least-privilege access limits what an attacker (or an accidental
command) can affect.

**How it manifests here:**
- Terraform SA impersonation: humans authenticate as themselves; terraform operations run as
  a service account with only the necessary roles.
- Short-lived impersonated tokens, not exported JSON key files.
- IAP gates access to all frontends; Cloud Run services are not publicly exposed.
- The access topology tracks all IAM bindings; `needs_review` bindings are surfaced explicitly.
- Firestore access is scoped to the API service account only.

---

### 7. Progressive Delivery

**Statement:** Deliver changes incrementally, validate each increment, and keep rollback cheap.

**Why it matters:** Large, infrequent deployments concentrate risk. A small change to one
service is easy to understand, easy to test, and easy to roll back. A multi-service "big bang"
deploy makes attribution of failures nearly impossible.

**How it manifests here:**
- The deploy sequence (foundation → infra → app → apps → gateway) applies changes in
  dependency order, one layer at a time.
- Cloud Run preserves previous revisions automatically; rollback is a single command pointing
  traffic to the prior revision.
- CI change detection ensures only the modules that changed are deployed in a given pipeline
  run.
- Sandbox isolation means new infrastructure patterns are tested against a personal environment
  before touching shared dev.

---

### 8. Everything is Code

**Statement:** Infrastructure, access policies, tests, pipelines, runbooks, and operational
procedures all belong in source control, versioned, reviewed, and executable.

**Why it matters:** Documentation that lives outside source control becomes stale. Procedures
stored in someone's head or in a wiki diverge from reality. When operations are code, they
are testable, auditable, and reproducible.

**How it manifests here:**
- Infrastructure: Terraform in `terraform/`.
- Pipelines: GitHub Actions in `.github/workflows/`.
- Tests: Pester suites in `scripts/verify/`.
- Runbooks: Skills in `.claude/skills/`.
- Operational state: Gate status in `test-gates.json`, topology in `network/` and `access/`.
- The hook system: automated behaviors in `.claude/hooks/` are code, not manual checklists.

---

## When Technical Details Are the Wrong Starting Point

Sometimes the right first question is not "how do I do this in GCP" but "what are we actually
trying to achieve?"

Signs you're starting at the wrong layer:

- The command you're about to run would be correct on the current platform but would not
  survive a provider change — and you can't articulate why the approach is right
  independent of the provider.
- The justification for a decision is "that's what the docs say" rather than a statement
  about reliability, security, cost, or maintainability.
- You're solving a symptom (this specific error) rather than the underlying cause (the
  system has no automated check for this class of problem).
- The fix you're considering creates a new manual step that will be forgotten in the future.

**The migration test question:**

Before committing to a significant technical approach, ask:

> "If we migrated this project from GCP to AWS tomorrow, would this decision still apply?"

If yes: the decision is grounded in principle and will age well.
If no: you are making an implementation choice, which may be correct but should be
recognized as such. Document the GCP-specific reasoning explicitly so future agents and
humans know why this isn't more general.

---

## The Platform Migration Test

A thought experiment. Imagine migrating LaaS from GCP to AWS overnight. What survives?

**Survives intact (philosophy):**
- The plan → review → apply workflow
- Testing before deploying
- Topology snapshots as version-controlled observability
- Short-lived credentials and SA impersonation (maps to IAM roles for EC2/ECS)
- Gate system logic (dirty/clean/needs-run state machine)
- Progressive delivery via per-service rollouts
- Pre-commit hooks checking test state
- The `[philosophy] GitOps principle: ...` nudges from the hook system

**Does not survive (GCP implementation):**
- `gcloud` commands and flags
- Cloud Run revision management
- Terraform GCP provider resources (`google_cloud_run_service`, etc.)
- IAP (replaced by AWS Cognito/ALB auth, or similar)
- Secret Manager secret paths
- Firestore (replaced by DynamoDB or similar)
- Specific project IDs and region identifiers

**The lesson:** The philosophy layer is the durable investment. Every `.claude/skills/*.md`
file embeds GCP specifics, but the reasoning behind when to use each skill and why the
workflow is structured the way it is — that reasoning transfers.

---

## Philosophy → Technical Mapping

| Philosophy principle | GCP implementation | Cloud-agnostic alternative |
|---------------------|-------------------|---------------------------|
| Immutable Infrastructure | Cloud Run revision-per-deploy; `terraform apply` for all changes | ECS task definition per deploy; CDK/Pulumi apply |
| GitOps / Single Source of Truth | Terraform in git; `main` branch triggers CI | Same pattern on any provider |
| Fail Fast / Shift Left | Pre-commit hooks, Pester, `terraform validate`, gate system | Any test framework + pre-commit hooks + gate-equivalent |
| Defense in Depth | IAP + SA impersonation + plan safety checks + topology drift | Auth proxy + role-based SA equiv + plan gates + drift checks |
| Observability Before Action | Topology snapshots, gate advisor, session recap | Config snapshot + state advisor + session init |
| Least Privilege | SA impersonation with short-lived tokens; scoped IAM roles | IAM roles for EC2/ECS; assume-role with session tokens |
| Progressive Delivery | Foundation→infra→app→apps→gateway sequence; Cloud Run traffic | CDK stacks in order; ECS blue/green |
| Everything is Code | Terraform, GitHub Actions, Pester, `.claude/skills/*.md` | Same pattern on any provider |

---

## Applying Philosophy to Common Decisions

### "Should we use spot/preemptible instances?"

**Principle lens:** Progressive Delivery + Observability Before Action.

The question is not whether spot instances are cheaper (they are) but whether the service
can tolerate interruption. For a stateless API or frontend: yes, with a proper health check
and traffic routing. For anything with in-flight state: only if that state is externalized
(Firestore, not in-memory).

Decision: start with standard, migrate to spot/preemptible after confirming the service is
truly stateless and health-checked.

---

### "How should secrets be managed?"

**Principle lens:** Least Privilege + Everything is Code + GitOps.

Secrets must never be in source control (violates confidentiality). But the *binding* between
a service and a secret must be in source control (GitOps). Short-lived token access is
preferred over long-lived key files (Least Privilege).

Decision: secrets in Secret Manager, mounted via Cloud Run env var or volume mounts,
with the binding declared in Terraform. Rotation is a Terraform change, not a console change.

See `secrets-management.md` for the GCP implementation.

---

### "When should we roll back vs fix forward?"

**Principle lens:** Observability Before Action + Progressive Delivery.

Roll back when: the blast radius is active (users affected now), the fix is not obvious within
five minutes, or the change was a configuration/infrastructure change rather than a code change
(infrastructure changes are harder to fix forward safely).

Fix forward when: the issue is a known code bug with a clear fix, rollback would cause data
loss or a worse outage, or the rollback target also has the problem.

Decision tree:
1. Is the outage active? → Rollback first, diagnose second.
2. Is the root cause clear and the fix trivial? → Fix forward.
3. Is the previous revision known good? → Roll back.

See `rollback-deployment.md` for the GCP implementation.

---

### "How granular should our tests be?"

**Principle lens:** Fail Fast / Shift Left + Everything is Code.

Tests should be as close to the change as possible. Unit tests catch logic errors before
integration tests run. Integration tests catch wiring errors before e2e tests run.

The cost pyramid: unit tests are cheap and fast (run on every edit). E2e tests are expensive
and slow (run after deploy). Don't compensate for missing unit tests by relying on e2e.

Decision: each script change → Pester unit tests. Each infrastructure change → terraform
validation tests + smoke tests. Each full deploy → e2e smoke test. The gate system enforces
this pyramid.

---

### "Is it okay to make a quick console change to unblock a prod issue?"

**Principle lens:** GitOps + Immutable Infrastructure.

No, with one exception: as a temporary emergency measure to restore service, followed
immediately by a Terraform change that codifies the same state.

If you make a console change and don't follow up with a Terraform change:
- The next `terraform apply` will undo your console change (Terraform will see drift).
- Future agents and humans will have no record that the change was made.
- The topology drift check will flag the discrepancy.

Decision: manual console changes are permitted for emergency stabilization only. Every such
change must be immediately followed by the corresponding Terraform code change and a
`drift-check.md` verification.

---

## Troubleshooting — When Philosophy is Being Violated

**Symptom: "I just made a quick console change to fix it"**
- Violated principle: GitOps / Single Source of Truth
- Risk: next `terraform apply` undoes the fix; no audit trail; other engineers are unaware
- Remedy: immediately encode the change in Terraform; run drift check; commit

**Symptom: "Let me skip the tests, I'm sure this is fine"**
- Violated principle: Fail Fast / Shift Left
- Risk: defects that tests would have caught reach production
- Remedy: dirty gates are listed by `gate-advisor.ps1`; clear them before deploying

**Symptom: "I'll just export a JSON key file for the SA, it's easier"**
- Violated principle: Least Privilege
- Risk: long-lived credential if leaked grants persistent access; no expiry
- Remedy: use `gcloud auth print-access-token --impersonate-service-account=...` for
  short-lived tokens; see `secret-refs.md` for the declared credential-access patterns

**Symptom: "The topology is out of date, I'll just trust what I remember"**
- Violated principle: Observability Before Action
- Risk: applying changes against a stale mental model of the system causes unexpected
  resource conflicts, security gaps, or outages
- Remedy: run topology refresh before any apply; read `network-topology.md`

**Symptom: "I'll deploy everything at once to save time"**
- Violated principle: Progressive Delivery
- Risk: if something breaks, attribution is hard; rollback is coarser; blast radius is larger
- Remedy: follow the foundation → infra → app → apps → gateway sequence; verify at each step

**Symptom: "This IAM binding is easier to manage at the project level"**
- Violated principle: Least Privilege
- Risk: over-broad permissions; any compromise of that identity has wider impact
- Remedy: review `access-topology.md`; scope bindings to the minimum necessary resource level

---

## See Also

- `philosophy-index.md` — cross-reference: which principle underlies which technical skill
- `devops-philosophy.md` ← (this file) — the master principle set
- `write-skill.md` — skill authoring standards, including the optional "Why This Matters" section
