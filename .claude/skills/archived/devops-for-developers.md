# DevOps for Developers

Bridge the gap between application development and operations for developers new to DevOps
concepts. Use this skill when someone asks "what is Terraform?", "what is a service account?",
"what does CI/CD do?", "why can't I just deploy directly?", "what is IaC?", "why does this
pipeline exist?", or any foundational DevOps question.

Role: teaching

Trigger phrases: "what is terraform", "what is a service account", "what is IaC",
Methodology-step: skills
"explain CI/CD", "why use containers", "what is infrastructure as code",
"devops concepts", "explain the pipeline", "what is a service mesh",
"why does this process exist", "operations basics"

## Persona

This skill invokes the `rubber-duck` persona. Declare it at the start of the explanation:

> Persona: rubber-duck — explaining [concept or system] to [who's asking]

See `personas.md` for the full persona system, subagent briefing format,
and rubber-duck checklist.

---

## The Core Insight: Infrastructure is Just Code

Before cloud infrastructure, deploying software meant buying servers, racking them, installing
software manually, and hoping nothing went wrong. If it broke, no one knew exactly what was
running or how to reproduce it.

The DevOps revolution: **treat infrastructure the same way you treat application code.**
Version control it. Test it. Review it. Deploy it automatically.

This repo embodies that philosophy. Every GCP resource — load balancers, Cloud Run services,
IAM permissions — is declared in Terraform files and managed like code. The same discipline
you apply to a PR for a bug fix applies to "let's add a new Cloud Run service."

---

## Key Concepts

### Infrastructure as Code (IaC)

**What it is:** Describing infrastructure resources (servers, databases, networks, permissions)
in text files that can be versioned, reviewed, and applied automatically.

**Why it matters:** If you delete a cloud resource accidentally, you can recreate it exactly
from the code. If you want to know why a firewall rule exists, `git log` tells you. If you
want a second copy of the environment, you run the code again.

**In this project:** The `terraform/` directory declares everything. `terraform apply` reads
those declarations and creates/modifies/deletes real GCP resources to match.

**The alternative:** Clicking through a web console. Impossible to review, impossible to audit,
impossible to reproduce. Fine for exploration; never for production.

---

### Terraform

**What it is:** The IaC tool this project uses. Reads `.tf` files, talks to GCP APIs, and
converges real infrastructure to match what the files describe.

**Key concepts:**

| Term | Meaning |
|------|---------|
| `plan` | Preview what would change — reads current state + desired state, shows diff |
| `apply` | Make changes real — calls GCP APIs |
| `state` | File that tracks what Terraform "knows" exists; stored in GCS |
| `module` | Reusable block of Terraform code (like a function) |
| `variable` | Parameter you pass into a module or root config |
| `output` | Value a module exports (e.g., a Cloud Run service URL) |

**Why plan before apply?** Plan is read-only and free. Apply touches real infrastructure.
Reviewing the plan is the equivalent of code review for infrastructure — you see exactly
what will change before it happens.

**In this project:** Four module directories (foundation, infra, app, apps) applied in order
because each reads outputs from the previous one. See `deployment-flow.md` for the full DAG.

---

### Service Accounts

**What they are:** Robot identities in GCP. Unlike a human user account (`you@email.com`),
a service account is a machine identity (`laas-tf-sa@laas-489115.iam.gserviceaccount.com`).

**Why they exist:** Your human account has broad permissions. Terraform and Cloud Run services
should only have the minimum permissions they need — not yours. Service accounts enforce
**least privilege**: a service can only do what it's explicitly granted.

**In this project:**
- `laas-tf-sa` — the Terraform service account; has write access to GCP resources
- Cloud Run services run as their own service accounts with only the permissions they need
- You **impersonate** the Terraform SA when running deploys — you don't give it your credentials

**The "impersonation" concept:** Instead of giving the SA a long-lived key (dangerous), you
temporarily use your own identity to act as the SA. This is auditable, revocable, and expires.

---

### Containers and Docker

**What they are:** Lightweight, portable application packages. A container includes your
application code, runtime (Python 3.11, Node 22), and all dependencies — packaged so it
runs identically everywhere.

**Why they matter:** "Works on my machine" is a real problem. Containers eliminate it.
The exact same image that passes your local tests is what runs in Cloud Run.

**Key terms:**

| Term | Meaning |
|------|---------|
| `Dockerfile` | Recipe for building an image |
| `image` | A built artifact (frozen snapshot of an app + dependencies) |
| `container` | A running instance of an image |
| `registry` | Storage for images (Artifact Registry in this project) |
| `tag` | A label on an image (e.g., `latest`, `sha-abc1234`) |

**In this project:** `apps/*/Dockerfile` defines each app. CI builds images and pushes to
Artifact Registry. Cloud Run pulls and runs them. See `docker-builds.md`.

---

### CI/CD (Continuous Integration / Continuous Deployment)

**What it is:** Automating the steps between "developer pushes code" and "code runs in
production." CI = automated testing on every push. CD = automated deployment when tests pass.

**Why it matters:** Manual deploys are slow, error-prone, and poorly documented. CI/CD makes
deploys fast, consistent, and auditable. Every deploy is a pipeline run with a log.

**In this project:** Push to `main` triggers `deploy-dev.yml`. That workflow:
1. Detects which files changed
2. Builds only the images that need rebuilding
3. Runs Pester tests
4. Applies Terraform modules in the correct order
5. Runs e2e smoke tests
6. Refreshes topology snapshots

A human doesn't manually run any of these steps — the pipeline does. See `ci-workflows.md`.

---

### IAP (Identity-Aware Proxy)

**What it is:** A GCP service that sits in front of your application and requires Google OAuth
authentication before any request gets through. No auth token = 401 before your code runs.

**Why it matters:** Lets you put internal tools on the public internet without exposing them.
GCP handles auth entirely — your app doesn't need to implement login.

**In this project:** The `laas-api` backend is protected by IAP. Only users in the GCP project
can access it. The frontend apps (chat, swagger, storybook) are public. See `diagnose-iap.md`.

---

### Load Balancer

**What it is:** A component that receives all incoming traffic and routes it to the right
backend service based on URL path rules.

**In this project:** One GCP HTTPS load balancer handles all traffic:
- `/` → chat app
- `/swagger/*` → docs app
- `/storybook/*` → storybook app
- `/api/*` → laas-api (behind IAP)

This is why all sub-apps appear on the same domain. The LB is declared in `terraform/apps/gateway/`.

---

### Terraform State

**What it is:** A JSON file that records what Terraform has created. Without state, Terraform
can't know whether a resource already exists or needs to be created.

**Where it lives:** In a GCS bucket (not on your laptop). This means multiple team members
can all run Terraform against the same environment safely.

**Why it matters for developers:** State drift = Terraform thinks X exists but it doesn't
(or vice versa). If you delete a resource manually in the console, Terraform's state is now
wrong. Always change infrastructure through Terraform, not the console.

---

### Gates and the Confidence Ladder

**What they are:** Status flags that track whether specific tests have passed recently. Before
deploying, gates must be "clean" — meaning the tests they represent have passed since the
last relevant code change.

**Why they exist:** Deploying with dirty gates is like merging with failing tests. The gates
enforce discipline: you can't accidentally deploy when something is known to be broken.

**In this project:** See `gate-status.md` for the full gate system. Key principle: the gate
checks are faster than a full deploy — catching problems early is always cheaper.

---

## The Developer Mental Model Shift

As a developer, you're accustomed to: write code → test → commit → done.

In a DevOps-integrated workflow, add: commit → CI runs → infra converges → monitors verify.

| Developer habit | DevOps equivalent |
|----------------|------------------|
| Unit test | `terraform test` (mock) |
| Integration test | `smoke-infra.ps1` (real Cloud Run) |
| Code review | Terraform `plan` review |
| Merging a PR | Triggering a deploy pipeline |
| Reading logs | Cloud Run logs + structured observability |
| Debugging locally | Cloud Run direct URL + `gcloud logs tail` |

The key insight: **infrastructure failures are just bugs in a different language.** They
deserve the same rigor as application bugs — reproduce, diagnose, fix, verify.

---

## What "Zero Trust" Means Here

The platform is described as "zero trust." For developers:

- Every service assumes other services may be compromised
- No service has implicit trust because it's "on the same network"
- Authentication is required at every boundary (IAP for users, service accounts for services)
- The network topology documents every trust relationship explicitly

This isn't paranoia — it's defense in depth. If one service is compromised, the blast radius
is limited by the permissions that service was explicitly granted.

---

## Further Reading

| If you want to understand... | Read |
|------------------------------|------|
| The full infrastructure pipeline | `deployment-flow.md` |
| Platform-agnostic DevOps principles | `devops-philosophy.md` |
| How to actually deploy | `apply-infra.md` |
| The confidence-before-deploy ladder | `weigh-time-risk.md` |
| Why this architecture was chosen | `devops-philosophy.md` (Platform Migration Test) |

---

## Troubleshooting — Common Conceptual Confusions

**"I changed a variable but nothing happened"**
- Terraform only applies changes when you run `apply`. Changing `.tf` files is just editing
  text until you run the pipeline or a local apply.

**"I deleted a resource in the console and now Terraform is confused"**
- The state file still thinks the resource exists. Run `terraform plan` — it will show
  "resource not found, will create." Apply to recreate it, or `terraform state rm` to
  remove the orphan reference. See `state-recovery.md`.

**"Why do I need service account tokens? I'm already logged into gcloud"**
- Your personal `gcloud auth` credentials are for you. Terraform runs as the Terraform SA
  to enforce least privilege. The SA token is generated via impersonation from your creds.
  See `preflight.md` for how token expiry is handled.

**"The pipeline ran but my change isn't live"**
- Check the deploy pipeline: did all jobs pass? Did `detect-changes` determine your path
  wasn't affected? Did `build-images` skip because app source didn't change?
  Read `ci-workflows.md` for the full change-detection logic.

---

## Why This Matters

This skill implements **You Build It, You Run It** from `devops-philosophy.md`. Developers who understand the deployment pipeline make better code decisions — they know why immutable containers matter, why gate checks exist, and why rollback speed is a design constraint. This shared mental model reduces friction between writing code and operating it in production.
