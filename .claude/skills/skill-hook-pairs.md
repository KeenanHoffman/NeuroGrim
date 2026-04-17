# Skill+Hook Pair Catalog

Catalog of all skill↔hook pairs in the system. Use this when writing a new skill (to check
whether a companion hook should be proposed), when a hook fires unexpectedly (to understand
the skill it automates), or when auditing the gap between documented guidance and automated
enforcement.

Role: meta

Trigger phrases: "skill hook pair", "companion hook", "should this have a hook",
Methodology-step: skills
"hook for this skill", "automate this skill", "skill+hook", "pair a hook with a skill",
"hook catalog", "which skills have hooks"

---

## Hook Pair Taxonomy

Four types of hooks complement skills. Understanding the type determines the right trigger,
blocking behavior, and exit code.

| Type | What it does | Blocking? | Typical trigger |
|------|-------------|-----------|----------------|
| **Enforcement** | Blocks a harmful or premature action | **Yes** — exits non-zero | `PreToolUse` |
| **Detection** | Surfaces a condition that requires human awareness | No — exits 0 | `PostToolUse` |
| **Verification** | Validates correctness after an action | No — exits 0 (async) | `PostToolUse` |
| **Automation** | Triggers downstream work automatically | No — exits 0 (async) | `PostToolUse` / `PostEditFile` |

---

## Implemented Pairs

All documented skill↔hook relationships in the system.

| Skill | Hook | Type | What the hook automates |
|-------|------|------|------------------------|
| `write-skill.md` | `assess-skill-on-edit.sh` | Automation | Runs 10 quality checks + two-pass T/P review on every skill edit; emits incoming ref count via `Find-SkillSymbol.ps1 -Name` when checks pass |
| `gate-status.md` | `pre-commit.sh` | Enforcement | Blocks commits when code gates are dirty |
| `gate-status.md` | `pre-push.sh` | Enforcement | Blocks pushes when merge gates are dirty |
| `apply-infra.md` | `check-sa-liveness.sh` | Enforcement | Blocks apply/destroy if SA token expired |
| `apply-infra.md` | `apply-failure.sh` | Detection | Captures failure to log, dirties gate, prompts fix |
| `apply-infra.md` | `health-check-after-apply.sh` | Verification | Polls Cloud Run Ready status after apply |
| `apply-infra.md` | `pre-destroy-confirmation.sh` | Enforcement | Blocks destroy without `LAAS_ALLOW_DESTROY=yes` |
| `apply-infra.md` | `philosophy-check.sh` | Detection | Soft nudge to consider DevOps principles before apply/destroy |
| `network-topology.md` | `annotate-topology.sh` | Detection | Surfaces `needs_review` network resources after any Bash command |
| `network-topology.md` | `tf-validate.sh` | Verification | Auto-validates TF syntax after `.tf` edit |
| `network-topology.md` | `watch-cert-after-gateway.sh` | Automation | Monitors SSL cert provisioning after gateway apply |
| `access-topology.md` | `annotate-access-topology.sh` | Detection | Surfaces `needs_review` IAM bindings after any Bash command |
| `ci-workflows.md` | `workflow-lint.sh` | Verification | Lints CI YAML on edit via actionlint |
| `publish.md` | `smoke-test.sh` | Automation | Runs Pester suite automatically on script edit |
| `rollback-deployment.md` | `e2e-gate-update.sh` | Detection | Surfaces e2e gate result after `run-e2e.ps1` completes |
| `secrets-management.md` | `validate-gates-json.sh` | Verification | Validates gate JSON syntax + schema on direct edit |
| `lsp.md` | `lsp-on-edit.sh` | Automation | Runs PSScriptAnalyzer / tsc / tf-validate / pyright / shellcheck after source file edit |
| `lsp.md`, `add-lsp-for-language.md` | `suggest-lsp-for-grep.sh` | Detection | After any Bash command, detects code-navigation grep patterns targeting source dirs and suggests LSP equivalents |
| `lsp-subagent-queries.md`, `lsp-grounded.md` | `suggest-lsp-subagents.sh` | Detection | After 3 direct Find-*Symbol.ps1 calls in a session, nudges agent to delegate bulk LSP queries to subagents (Pattern 5) |
| `start-feature.md` | `check-new-branch-base.sh` | Detection | Warns when `git checkout -b` is run without `laas/main` as base; lists stale local branches |
| `start-feature.md` | `suggest-pr-on-push.sh` | Detection | After `git push laas <branch>`, checks for open PR and prints `gh pr create` reminder if none exists |
| `deployment-staleness.md` | `check-staleness-on-dispatch.sh` | Detection | Before `gh workflow run deploy-dev.yml` or `sandbox-deploy.yml`, warns if services are stale/never-deployed; recommends `force_services` flag |
| `topology-views.md` | `render-topology-views.sh` | Automation | After `update-network-topology.ps1` succeeds, renders dev-scoped view (foundation+gateway+app) in background |
| *(whitepaper diagrams)* | `render-whitepaper-diagrams.sh` | Automation | After any `.mmd` file in `apps/devops-whitepaper/diagrams/` is edited, re-renders that diagram to SVG |
| `plan-critic.md` | `critique-plan.sh` | Detection | After any plan file in `.claude/plans/*.md` is written or edited, prints reminder to run plan-critic before implementing |
| `add-new-app.md` | `suggest-architect.sh` | Detection | After any plan file is written/edited, checks for architectural keywords and nudges adoption of the architect persona |
| `incident-response.md`, `debug-cloud-run.md` | `apply-failure.sh` (extended) | Detection | After any apply/destroy/e2e/smoke failure exits non-zero, nudges adoption of the incident-commander persona |
| `access-topology.md`, `diagnose-iap.md` | `suggest-security-auditor.sh` | Detection | After any Bash command matching IAM operations, nudges adoption of the security-auditor persona |
| `devops-for-developers.md`, `setup.md` | *(none)* | — | rubber-duck is triggered by explicit user request or skill invocation; no reliable machine-detectable signal exists for onboarding context. No companion hook needed (evaluated 2026-04-07). |
| `git-strategy.md` | `check-upstream-freshness.sh` | Detection | Before any `git commit` Bash command, fetches laas/main and warns if branch is >3 commits behind; highlights auto-generated file conflict risk |
| `review-loop.md` | *(none)* | — | Meta workflow skill — no automatable trigger; loop is invoked explicitly by the orchestrating agent. No companion hook needed (evaluated 2026-04-07). |
| `lsp-grounded.md`, `gate-status.md` | `skill-context-on-read.sh` | Detection | After agent reads any `.claude/skills/*.md` with a `Governs:` field, emits live gate status for the governed scripts — connects skill reads to real gate state |
| `lsp-grounded.md`, `smoke-infra.md`, `test.md` | `gate-completion.sh` | Detection | After any gate script (run-tests.ps1, smoke-infra.ps1, etc.) exits, emits updated gate health summary and nudges agent to run Find-SessionContext for next steps |
| `lsp-grounded.md`, `apply-infra.md`, `sandbox.md` | `pre-apply-lsp.sh` | Detection | Before any apply-*.ps1 script, emits deploy readiness from gates.json and topology — dirty gate count, at-risk resources, estimated clear time; advisory only (exit 0) |
| `imagination-mode.md` | *(none)* | — | Conversation-initiated ideation; there is no machine-detectable signal for "user wants to imagine". No companion hook needed (evaluated 2026-04-07). |
| `artifact-cmdb.md` | `prewarm-artifacts.yml` | Automation | Daily schedule + `workflow_dispatch` rebuilds stale container images, updates CMDB hashes, commits to main — closes the build-skip gate circuit for the next deploy |
| `brain.md` | `gate-completion.sh` | Detection | After every gate script completes, appends confidence-aware Moth(er):Br+AI+n score line (`Moth(er):Br+AI+n: N/100 [domain:score(confidence%) ...]`) to gate summary — surfaces cross-domain health with data completeness transparency |
| `start-feature.md` | `pre-pr-create.sh` | Enforcement | Before `gh pr create`, checks if branch is pushed to remote; blocks with guidance if not |
| `sca.md` | *(none)* | — | Audits are slow and on-demand; no reliable machine-detectable trigger exists. No companion hook needed (evaluated 2026-04-08). |
| `git-tree-cmdb.md` | `rebuild-git-tree.sh` | Automation | After any edit to gates/settings/skills/workflows/registry, rebuilds cross-reference CMDB (async) |
| `watch-pr.md` | *(none)* | — | Uses CronCreate for polling; no hook needed — automation lives in the session-scoped cron prompt. No companion hook needed (evaluated 2026-04-08). |

---

## Proposed Pairs

Pairs where a skill describes behavior that could benefit from a companion hook but none
exists yet. When a pair is implemented, move it to the Implemented table and remove it
from this list.

*(No proposed pairs at this time — evaluated 2026-04-07.)*

Add a proposed pair using this format:

```
### <skill-name>.md → <proposed-hook-name>.sh
- **Type:** Enforcement | Detection | Verification | Automation
- **Trigger:** PreToolUse | PostToolUse | PostEditFile | git hook
- **Behavior:** What the hook would detect, enforce, verify, or automate
- **Priority:** High | Medium | Low
- **Status:** Proposed
```

---

## Authoring Guide: Does This Skill Need a Companion Hook?

When writing or reviewing a skill, ask these four questions. If any answer is "yes",
add a proposed pair entry above and track it in `skill-gap-tracker.md`.

1. **Enforcement question:** Does this skill describe a step that must happen before
   another action can be safely taken? Could it be skipped by accident?
   → If yes, propose an **Enforcement** hook.

2. **Detection question:** Does this skill describe state that an agent should automatically
   surface rather than requiring manual inspection?
   → If yes, propose a **Detection** hook.

3. **Verification question:** Does this skill describe a verification step that could
   run automatically after the triggering action completes?
   → If yes, propose a **Verification** hook.

4. **Automation question:** Does this skill trigger downstream work that currently requires
   a separate manual invocation?
   → If yes, propose an **Automation** hook.

**If all four answers are "no":** Add a note to the skill's `## See Also` section:
`No companion hook needed (evaluated YYYY-MM-DD).`

**Meta and philosophy skills are exempt** from hook evaluation — they describe thinking,
not repeatable actions.

---

## Why This Matters

This catalog implements **Everything is Code** from `devops-philosophy.md`. Documentation
without automation drifts: a skill that says "always run this check first" will be skipped
under pressure unless a hook enforces it. Pairing every enforceable guidance step with a
hook closes the gap between what the documentation says and what the system actually does.
The Platform Migration Test applies here too: on any platform, the principle "automate what
you mandate" survives; only the hook trigger mechanism changes.

---

## See Also

- `write-skill.md` — authoring guide with companion hook evaluation checklist
- `hooks-reference.md` — complete reference for all hooks (triggers, output, troubleshooting)
- `skill-gap-tracker.md` — proposed pairs work queue (mirrored for priority ordering)
- `dual-review.md` — T+P review protocol for evaluating whether a new hook is sound
