# Skill Index

Full discovery map of all skills in the library. Use this when:
- You're not sure which skill covers a task
- You want to know if a skill exists before writing a new one
- You want to understand how skills relate to each other
- A human asks "what can you do?" or "what skills are available?"

Role: meta · reference

Trigger phrases: "what skills are available", "do you have a skill for", "list all skills",
Methodology-step: skills
"what can you do", "skill directory", "find a skill", "skill library", "what do you know about"

**Quick navigation:** [Role Reference](#role-reference) · [Getting Started](#getting-started) · [Deployment](#deployment--infrastructure) · [Planning](#planning--safety) · [Testing](#testing--validation) · [Operations](#operations--monitoring) · [Incident Response](#incident-response) · [Configuration](#infrastructure-configuration) · [CI/CD](#cicd-pipelines) · [Philosophy](#philosophy--principles) · [Meta](#meta-skills-system)

---

## Role Reference

*Scan this table first. Each skill is tagged with its primary role so you know what kind of help you're getting before reading the full file.*

**Role key:** `philosophy` · `teaching` · `operational` · `diagnostic` · `recovery` · `planning` · `validation` · `reference` · `configuration` · `ci-cd` · `meta`

| Skill | Role |
|-------|------|
| `a2a.md` | `reference · configuration` |
| `peer-brain.md` | `operational · reference` |
| `access-topology.md` | `diagnostic · reference` |
| `add-new-app.md` | `operational · configuration` |
| `apply-infra.md` | `operational` |
| `bootstrap.md` | `configuration` |
| `ci-testing.md` | `validation · ci-cd` |
| `ci-workflows.md` | `reference · ci-cd` |
| `debug-cloud-run.md` | `diagnostic · recovery` |
| `demo.md` | `meta · teaching` |
| `deployment-flow.md` | `reference · teaching` |
| `devops-for-developers.md` | `teaching` |
| `devops-philosophy.md` | `philosophy` |
| `diagnose-iap.md` | `diagnostic · recovery` |
| `deployment-staleness.md` | `diagnostic · ci-cd` |
| `docker-builds.md` | `operational · ci-cd` |
| `drift-check.md` | `diagnostic · validation` |
| `environments.md` | `reference · teaching` |
| `explain-error.md` | `diagnostic` |
| `fix-apply-failure.md` | `validation · recovery` |
| `gate-status.md` | `diagnostic · planning` |
| `gate-system-overview.md` | `teaching` |
| `gateway-routing.md` | `reference · configuration` |
| `git-strategy.md` | `planning · operational` |
| `hats.md` | `meta` |
| `hooks-reference.md` | `reference` |
| `incident-response.md` | `recovery` |
| `local-dev.md` | `configuration` |
| `local-proxy.md` | `configuration` |
| `lsp.md` | `diagnostic · reference` |
| `lsp-subagent-queries.md` | `operational · reference` |
| `add-lsp-for-language.md` | `operational · planning` |
| `artifact-cmdb.md` | `operational · ci-cd` |
| `brain.md` | `diagnostic · reference · meta` |
| `operational-memory.md` | `diagnostic · planning · retrospective` |
| `sca.md` | `diagnostic · security` |
| `git-tree-cmdb.md` | `diagnostic · reference` |
| `terminology-governance.md` | `diagnostic · governance` |
| `network-topology.md` | `diagnostic · reference` |
| `north-star.md` | `meta · guiding` |
| `personas.md` | `meta` |
| `philosophy-index.md` | `philosophy · reference` |
| `playwright-e2e.md` | `validation` |
| `post-deploy-verify.md` | `validation · operational` |
| `pr-checklist.md` | `planning` |
| `preflight.md` | `planning · validation` |
| `publish.md` | `operational · ci-cd` |
| `retire.md` | `operational` |
| `review-plan.md` | `planning · validation` |
| `rollback-deployment.md` | `recovery` |
| `start-feature.md` | `operational · planning` |
| `summarize-skills.md` | `meta` |
| `sandbox.md` | `configuration · operational` |
| `secrets-management.md` | `configuration · reference` |
| `session-handoff.md` | `meta` |
| `session-recap.md` | `diagnostic · reference` |
| `setup.md` | `configuration` |
| `skill-chain.md` | `meta` |
| `skill-deprecation.md` | `meta` |
| `skill-gap-tracker.md` | `meta` |
| `skill-hook-pairs.md` | `meta` |
| `skill-index.md` | `meta · reference` |
| `dual-review.md` | `meta` |
| `imagination-mode.md` | `planning · meta` |
| `smoke-infra.md` | `validation` |
| `subagent-patterns.md` | `operational · reference` |
| `state-recovery.md` | `recovery · diagnostic` |
| `terraform-migration.md` | `operational · configuration` |
| `test.md` | `validation` |
| `verify.md` | `validation · operational` *(deprecated → `post-deploy-verify.md`)* |
| `weigh-time-risk.md` | `planning` |
| `watch-pr.md` | `operational` |
| `what-next.md` | `planning · diagnostic` |
| `write-skill.md` | `meta` |
| `write-smoke-tests.md` | `operational · validation` |

---

## Getting Started

*New to the project? Start here.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `setup.md` | Install required tools, GCP auth, env vars, git hooks | "set up my environment", "onboard me", "install dependencies" |
| `local-dev.md` | Run API and frontends locally with hot reload | "run locally", "local dev server", "hot reload", "develop without deploying" |
| `devops-for-developers.md` | Teaching guide: IaC, Terraform, containers, CI/CD explained for devs | "what is terraform", "explain CI/CD", "what is a service account", "devops basics" |

**Reading order for new team members:** `setup.md` → `local-dev.md` → `devops-for-developers.md` → `deployment-flow.md`

---

## Deployment & Infrastructure

*Getting code and config into GCP.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `apply-infra.md` | Full manual deploy sequence: foundation → infra → app → apps → gateway | "deploy to dev", "run apply", "terraform apply" |
| `sandbox.md` | Ephemeral personal environment: create, use, destroy | "sandbox", "test in isolation", "my own environment" |
| `deployment-flow.md` | How code gets from local → sandbox → dev → prod | "how does deployment work", "promotion flow", "environments" |
| `environments.md` | Four tiers: local, sandbox, dev, prod — state paths, access, cost | "what is dev vs sandbox", "which environment", "terraform state path" |
| `bootstrap.md` | One-time project setup from scratch | "new GCP project", "start from scratch", "bootstrap" |
| `retire.md` | Teardown sequence for decommissioning an environment | "destroy environment", "teardown", "decommission" |
| `post-deploy-verify.md` | Unified checklist after any apply | "verify deploy", "did the deploy work", "post-deploy" |

**Typical reading order:** `environments.md` → `deployment-flow.md` → `apply-infra.md` → `post-deploy-verify.md`

---

## Planning & Safety

*Deciding what to do and how much testing it needs.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `weigh-time-risk.md` | Choose the right testing tier based on change risk | "how much testing", "is this safe to deploy", "risk vs time" |
| `what-next.md` | Prioritized action list for commit/merge/deploy intent | "what should I do next", "am I ready to commit", "prioritize" |
| `gate-status.md` | Inspect gate state, re-run dirty gates, gate reference | "gate status", "are my tests clean", "which gates are dirty" |
| `gate-system-overview.md` | Conceptual model: gate states, watch_paths, expires_hours, skip rules | "how do gates work", "what is a gate", "gate states explained", "what does stale mean" |
| `preflight.md` | 8-item readiness check before any apply | "preflight", "check before deploy", "is the SA token valid" |
| `pr-checklist.md` | Pre-PR readiness: gate requirements, CI coverage, terraform plan review | "ready for PR", "pre-PR checklist", "before I merge", "open a pull request" |
| `review-plan.md` | Read a terraform plan output for risk and correctness | "review the plan", "check terraform plan", "is this plan safe" |
| `start-feature.md` | Create a clean branch from laas/main; branch naming; post-merge cleanup | "start new work", "new branch", "after PR merges", "branch naming", "post-merge cleanup" |
| `refocus.md` | Quick re-orient: brain pulse + top blockers + next action | "refocus", "what should I do next", "I'm lost", "session drift", "get back on track" |
| `watch-pr.md` | Watch a PR for merge, monitor deploy pipeline, suggest next actions | "watch this PR", "notify me when PR merges", "merge watcher" |

**Typical reading order:** `weigh-time-risk.md` → `gate-status.md` → `preflight.md` → `review-plan.md`

---

## Testing & Validation

*Verifying that code, scripts, and infrastructure are correct.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `test.md` | Run Pester unit tests (all targets or specific module) | "run tests", "pester", "unit tests", "test the scripts" |
| `smoke-infra.md` | Real-GCP smoke tests for terraform modules | "smoke test", "test against real GCP", "smoke-infra" |
| `playwright-e2e.md` | Playwright smoke tests for all three frontends | "e2e test", "playwright", "smoke test chat/swagger/storybook" |
| `write-smoke-tests.md` | Convert topology annotations into Pester test assertions | "write smoke tests", "generate tests from topology" |
| `ci-testing.md` | Four-layer confidence system for CI changes | "test CI changes", "validate workflow", "confidence ladder" |
| `post-deploy-verify.md` | Verify route health, SSL, e2e, gate state after deploy | "verify after deploy", "check everything is up" |
| `lsp.md` | LSP-style symbol search and static analysis for PS1, TF, TS, Python, Shell/Bash, TF State, Skills, Gates, Workflows, and Topology | "find all callers", "go to definition", "type errors", "PSScriptAnalyzer", "Find-Symbol", "pyright", "shellcheck", "terraform state", "what links to this skill", "critical path gates", "workflow dependencies", "topology query" |
| `lsp-subagent-queries.md` | Delegate 3+ independent Find-*Symbol.ps1 queries to subagents (max 5 concurrent) for faster bulk health checks | "lsp subagent", "parallelize lsp", "bulk health check", "delegate lsp queries", "fan-out lsp", "parallel lsp" |
| `add-lsp-for-language.md` | Recipe for wiring LSP support for a new language (config + Find-Symbol + hook + docs) | "add lsp for", "wire language server", "add type checking", "lsp for go", "lsp for rust" |

---

## Operations & Monitoring

*Understanding what's running and keeping it healthy.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `session-recap.md` | Synthesize current state of infra at session start | "session recap", "morning recap", "what's the current state" |
| `session-handoff.md` | Clean session end: commit gates, write handoff note | "end session", "wrap up", "session handoff" |
| `hooks-reference.md` | Complete reference for all 16 hooks: triggers, output, troubleshooting | "what does this hook do", "hook fired unexpectedly", "list all hooks" |
| `debug-cloud-run.md` | Logs, revision status, startup failures, OOM, cold starts | "cloud run logs", "service not starting", "container crashed" |
| `verify.md` | Quick smoke check of deployed services via check-all.ps1 | "check services", "verify services are up", "health check" |
| `drift-check.md` | Detect infrastructure drift (manual changes vs state) | "drift check", "has anything changed", "infra drift" |
| `network-topology.md` | Network snapshot inspection and update | "network topology", "show the network", "what's deployed" |
| `access-topology.md` | IAM snapshot inspection and access path validation | "access topology", "IAM", "who has access", "permissions" |
| `artifact-cmdb.md` | Container image freshness, build-skip gate, pre-warm schedule | "artifact stale", "skip build", "prewarm", "docker layer cache", "artifact cmdb" |
| `brain.md` | NeuroGrim: unified health score, cross-domain correlation, tool registry | "brain", "neurogrim", "system health", "unified health", "cross-domain", "tool registry", "domain score" |
| `operational-memory.md` | Query historical operational data: Brain scores, gate ledger, incident log, deploy outcomes | "operational memory", "historical scores", "past incidents", "what happened before", "gate history", "deploy outcomes" |
| `sca.md` | Software composition analysis: pip-audit + pnpm audit vulnerability scanning | "sca", "vulnerability", "dependency audit", "pip-audit", "pnpm audit", "supply chain", "CVE" |
| `git-tree-cmdb.md` | Git tree cross-references: file impact, xref lookup, secret scan | "cross-reference", "file impact", "which gates watch this", "git tree", "xref", "what does this file affect" |
| `terminology-governance.md` | Terminology governance: drift scan, term lookup, hat-persona pairings | "terminology", "language alignment", "canonical term", "drift term", "hat persona pairing", "naming convention" |

---

## Incident Response

*When something breaks in production.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `incident-response.md` | Full incident playbook: detect → assess → respond → verify | "something is down", "production broken", "emergency", "outage" |
| `rollback-deployment.md` | Find previous image, update Cloud Run, verify recovery | "roll back", "revert deploy", "undo deploy", "previous version" |
| `debug-cloud-run.md` | Diagnose the broken service | "service not starting", "check logs", "crash loop" |
| `fix-apply-failure.md` | Write a regression test after a terraform apply failure | "apply failed", "fix failure", "prevent this from happening again" |
| `state-recovery.md` | Recover from partial applies and stuck state locks | "stuck lock", "partial apply", "state corruption", "terraform state" |
| `explain-error.md` | Classify and explain error messages | "what does this error mean", "403", "409", "timeout" |

**Reading order for a live incident:** `incident-response.md` → `debug-cloud-run.md` → `rollback-deployment.md`

---

## Infrastructure Configuration

*Configuring GCP resources, networking, and access.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `gateway-routing.md` | URL map structure, adding routes, inter-service URLs | "add a route", "URL map", "gateway", "path matcher" |
| `secrets-management.md` | Secret Manager integration, rotation, adding new secrets | "rotate API key", "secret manager", "GEMINI_API_KEY", "add a secret" |
| `terraform-migration.md` | State migration pitfalls and verification checklist | "migrate state", "terraform state mv", "state import" |
| `diagnose-iap.md` | IAP troubleshooting: brand → client → backend → IAM → LB | "IAP", "401 unauthorized", "403 forbidden", "access denied" |
| `local-proxy.md` | Access sandbox services via gcloud proxy | "sandbox proxy", "access sandbox", "local proxy" |
| `network-topology.md` | Inspect and update network topology snapshot | "network topology", "what's the network look like" |

---

## CI/CD Pipelines

*GitHub Actions workflows, image builds, deployment automation.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `ci-workflows.md` | Full workflow inventory, DAG, dispatch commands, skip flags | "CI pipeline", "github actions", "dispatch workflow", "skip build" |
| `ci-testing.md` | Confidence ladder for safely changing CI itself | "test CI changes", "validate workflow", "deploy-sandbox" |
| `docker-builds.md` | Docker build config per app, pnpm patterns, build failures | "docker build", "dockerfile", "build image", "cloudbuild" |
| `publish.md` | Build + push Docker images: which command for which scenario | "build and push", "publish image", "update service image" |
| `add-new-app.md` | Complete recipe for adding a new sub-app to the platform | "add new app", "new service", "new frontend", "6th app" |
| `deployment-staleness.md` | Detect stale/never-deployed services; when and how to use force_services | "force services", "jobs skipped", "never deployed", "brand new sandbox", "stale image", "detect-changes skipped" |
| `artifact-cmdb.md` | Pre-warm stale container images before deploy; CMDB-gated build skip | "prewarm artifacts", "rebuild stale images", "artifact freshness", "pipeline caching" |
| `hotfix-deploy.md` | Surgical single-service hotfix rebuild and deploy via dedicated workflow | "hotfix", "emergency deploy", "deploy just one service", "quick fix deploy" |

---

## Philosophy & Principles

*Platform-agnostic DevOps thinking.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `devops-philosophy.md` | 8 DevOps principles with Platform Migration Test and project manifestations | "devops principles", "why do we do this", "platform migration", "what if we moved to AWS" |
| `philosophy-index.md` | Cross-reference: which principles apply to which skills | "which principle applies", "philosophy map", "principle-to-skill" |
| `devops-for-developers.md` | Teaching guide: IaC, Terraform, containers, CI/CD explained for devs | "what is terraform", "what is a service account", "explain CI/CD" |
| `everything-as-code.md` | Everything as Code: IaC, CaC, PaC, DaC, SaC pillars and decision framework | "everything as code", "EaC", "config as code", "policy as code", "why is this in a file" |

---

## Meta (Skills System)

*Skills about managing skills.*

| Skill | One-line description | Key trigger phrases |
|-------|---------------------|-------------------|
| `skill-index.md` ← (this file) | Full skill discovery map by category | "what skills are available", "find a skill", "skill directory" |
| `write-skill.md` | Authoring guide: structure, style, template, quality checklist | "write a skill", "create a skill", "how do I write a skill" |
| `skill-chain.md` | Common multi-skill sequences for complex tasks | "what order do I use these", "full deploy workflow", "skill sequence" |
| `skill-hook-pairs.md` | Catalog of skill↔hook pairs + proposed pairs work queue | "skill hook pair", "should this have a hook", "companion hook", "hook catalog" |
| `dual-review.md` | Two-pass T+P review protocol for skills, hooks, and decisions | "dual review", "technical and philosophy review", "T+P review", "review this skill" |
| `imagination-mode.md` | Conversational pre-plan ideation — surface approaches, name tradeoffs, no code or files | "imagine", "brainstorm", "what are the options", "think through this", "explore approaches", "before we plan" |
| `subagent-patterns.md` | Three spawn patterns for parallel agent coordination with JSON result conventions | "spawn a subagent", "run in parallel", "fan-out pattern", "parallelize this workflow" |
| `personas.md` | Operator agent operational modes: adversary, architect, incident-commander, rubber-duck, security-auditor, visionary, lsp-reader | "persona", "adopt persona", "adversary mode", "incident commander" |
| `hats.md` | Context-aware focus lenses: operator, security, architect hats with domain emphasis multipliers | "hat", "wear hat", "operator hat", "security focus", "architect focus" |
| `north-star.md` | Lightweight alignment check: does this advance toward Stage 5? | "north star", "vision check", "are we on track", "which stage" |
| `session-handoff.md` | Clean session end and handoff note | "end session", "wrap up", "handoff" |
| `skill-gap-tracker.md` | Living record of missing and stale skills | "what skills are missing", "skill gap", "undocumented task" |
| `summarize-skills.md` | Quick summary of all skills with role, hook, and chain annotations | "summarize skills", "skill overview", "what does each skill do" |
| `skill-deprecation.md` | Process for retiring and removing outdated skills | "skill is outdated", "retire a skill", "stale skill" |
| `demo.md` | Worked examples of agent-human skill interactions | "demo", "show me an example", "how would this work" |
| `lsp-grounded.md` | LSP-as-grounding-layer philosophy: action modes, Governs: circuit, decision tree | "lsp grounded", "how do I use lsp", "lsp workflow", "lsp as basis", "how the lsp tools work together" |

---

## Skill Relationships

### Prerequisites (read A before B)

| If you're reading... | Read first... |
|---------------------|--------------|
| `apply-infra.md` | `gate-status.md`, `weigh-time-risk.md` |
| `review-plan.md` | `network-topology.md` (for network changes) |
| `rollback-deployment.md` | `incident-response.md` (for context) |
| `add-new-app.md` | `docker-builds.md`, `gateway-routing.md`, `playwright-e2e.md` |
| `ci-testing.md` | `ci-workflows.md` |
| `write-skill.md` | `skill-index.md` (to check existing skills first) |

### Often read together

| These skills are commonly used in the same session |
|---------------------------------------------------|
| `session-recap.md` + `gate-status.md` + `what-next.md` |
| `weigh-time-risk.md` + `preflight.md` + `review-plan.md` |
| `incident-response.md` + `debug-cloud-run.md` + `rollback-deployment.md` |
| `apply-infra.md` + `post-deploy-verify.md` + `network-topology.md` |
| `write-skill.md` + `skill-gap-tracker.md` + `CLAUDE.md` |

---

## "Which Skill Do I Need?" Quick Guide

| You want to... | Read this |
|---------------|-----------|
| Start the day / return from break | `session-recap.md` |
| Know what to do next | `what-next.md` |
| Set up my machine for the first time | `setup.md` |
| Run apps locally without deploying | `local-dev.md` |
| Understand what Terraform/IaC is | `devops-for-developers.md` |
| Understand the "why" behind a practice | `devops-philosophy.md` |
| Deploy something | `apply-infra.md` (manual) or `ci-workflows.md` (CI) |
| Something is broken | `incident-response.md` |
| A service is misbehaving | `debug-cloud-run.md` |
| Roll back a bad deploy | `rollback-deployment.md` |
| Check if it's safe to deploy | `gate-status.md` + `weigh-time-risk.md` |
| Understand what tests to run | `weigh-time-risk.md` |
| Verify after deploy | `post-deploy-verify.md` |
| Add a new app | `add-new-app.md` |
| Something in IAP/auth is broken | `diagnose-iap.md` |
| Find symbol definition / all callers | `lsp.md` |
| Write a new skill | `write-skill.md` |
| Find a skill for X | This file (scroll up) |
| End a session cleanly | `session-handoff.md` |

---

## Troubleshooting — Index Health Check

Run this to verify all skills referenced in this index actually exist on disk:

```bash
grep -oE '`[a-z-]+\.md`' .claude/skills/skill-index.md | tr -d '`' | sort -u | \
  while read f; do
    [ -f ".claude/skills/$f" ] || echo "MISSING IN INDEX: $f"
  done
echo "Index health check complete."
```

Run this to find skills on disk that are NOT in the index (potential undocumented skills):

```bash
ls .claude/skills/*.md | xargs -I{} basename {} | \
  while read f; do
    grep -q "$f" .claude/skills/skill-index.md || echo "NOT IN INDEX: $f"
  done
```
