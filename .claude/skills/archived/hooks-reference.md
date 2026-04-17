# Hooks Reference

Complete reference for all 31 hooks registered in `.claude/settings.json` and
`.git/hooks/`. Use this when a hook fires unexpectedly, you want to understand what
a hook emits, or you need to know which hooks fire for a given action.

Role: reference

Trigger phrases: "what does this hook do", "hook fired unexpectedly", "hooks reference",
Methodology-step: skills
"why did this hook run", "list all hooks", "hook behavior", "what fires when I edit",
"hooks documentation", "hook output explained"

---

## Quick Reference

| Hook | When it fires | Blocking | What it emits |
|------|--------------|----------|---------------|
| `pre-apply-lsp.sh` | Before `apply-*.ps1` Bash commands | No (advisory) | `[lsp:pre-apply]` deploy readiness: dirty gate count + topology at-risk count |
| `check-sa-liveness.sh` | Before any Bash command | **Yes** (exits non-zero if expired) | `[SA] token valid` or error + block |
| `philosophy-check.sh` | Before any Bash command | No (always exits 0) | `[philosophy] <principle>: <nudge>` |
| `smoke-test.sh` | After editing `scripts/**/*.ps1` | No (async) | Runs Pester; results in terminal |
| `tf-validate.sh` | After editing `terraform/**/*.tf` | No (async) | `terraform validate` output |
| `workflow-lint.sh` | After editing `.github/**/*.yml` | No (async) | `actionlint` or YAML parse result |
| `validate-gates-json.sh` | After editing `test-gates.json` | No (sync, advisory) | `[validate-gates] OK` or schema errors |
| `assess-skill-on-edit.sh` | After editing `.claude/skills/*.md` | No (sync, advisory) | 10-check structural quality report + two-pass T/P review |
| `lsp-on-edit.sh` | After editing `.ps1`/`.tf`/`.ts`/`.tsx` | No (async) | PSScriptAnalyzer / tsc / tf-validate |
| `skill-context-on-read.sh` | After reading `.claude/skills/*.md` | No (sync) | `[skill]` governed script names + live gate status for each |
| `apply-failure.sh` | After any Bash command exits non-zero | No (async) | Writes to `.claude/apply-failure.log` |
| `gate-completion.sh` | After gate script exits | No (sync) | `[gate]` updated gate health summary + nudge to run SessionContext |
| `e2e-gate-update.sh` | After `run-e2e.ps1` | No (sync) | `[e2e-gate] e2e:<module>: CLEAN/DIRTY` |
| `watch-cert-after-gateway.sh` | After successful gateway apply | No (async) | SSL cert status; starts watcher |
| `health-check-after-apply.sh` | After successful apply commands | No (async) | Cloud Run Ready status poll |
| `annotate-topology.sh` | After any Bash command | No (sync) | `[topology] needs_review` warnings |
| `annotate-access-topology.sh` | After any Bash command | No (sync) | `[access-topology] needs_review` warnings |
| `pre-destroy-confirmation.sh` | Before `destroy-*.ps1` | **Yes** (blocks without env var) | Blocks destroy unless `LAAS_ALLOW_DESTROY=yes` |
| `check-staleness-on-dispatch.sh` | Before `gh workflow run deploy-dev.yml`/`sandbox-deploy.yml` | No (advisory) | Warns if services are stale/never-deployed |
| `check-new-branch-base.sh` | Before `git checkout -b`/`git switch -c` | No (advisory) | Warns if base is not `laas/main`; lists stale branches |
| `check-upstream-freshness.sh` | Before `git commit` Bash command | No (advisory) | Warns if branch is >3 commits behind `laas/main` |
| `pre-pr-create.sh` | Before `gh pr create` | **Yes** (exit 1) | Blocks PR creation if branch not pushed to remote |
| `critique-plan.sh` | After editing `.claude/plans/*.md` | No (sync) | Reminds to run plan-critic before implementing |
| `suggest-architect.sh` | After editing `.claude/plans/*.md` | No (sync) | Nudges architect persona for architectural keywords |
| `suggest-pr-on-push.sh` | After `git push laas <branch>` | No (async) | Checks for open PR; prints `gh pr create` reminder |
| `render-topology-views.sh` | After `update-network-topology.ps1` | No (async) | Renders dev-scoped topology SVG in background |
| `suggest-security-auditor.sh` | After IAM-related Bash commands | No (async) | Nudges security-auditor persona for IAM operations |
| `suggest-lsp-for-grep.sh` | After grep-like Bash commands in src dirs | No (async) | Suggests LSP equivalents for code-navigation grep patterns |
| Bash logger (inline) | After every Bash command | No (sync) | Appends `[timestamp] exit=N cmd=...` to `.claude/deploy.log` |
| `pre-commit.sh` | `git commit` | **Yes** (blocks if gates dirty) | Gate status summary; blocks commit |
| `pre-push.sh` | `git push` to main | **Yes** (blocks if merge gates dirty) | Gate summary; blocks push |

---

## PreToolUse Hooks (fire BEFORE a tool executes)

### `check-sa-liveness.sh`

**Fires:** Before every Bash tool call
**Blocking:** Yes — exits non-zero to block the command if SA token is invalid
**Purpose:** Validates that GCP Service Account impersonation is active before any command that might call GCP APIs. Prevents confusing "permission denied" errors mid-apply.

**Output when valid:**
```
[SA] token valid for keenan.hoffman@sparq.com → laas-deploy@laas-489115.iam.gserviceaccount.com
```

**Output when expired:**
```
[SA] token EXPIRED — run: gcloud auth application-default login
[SA] BLOCKED: cannot proceed without valid ADC
```

**Fix:** `gcloud auth application-default login` then re-impersonate the SA.

---

### `philosophy-check.sh`

**Fires:** Before every Bash tool call
**Blocking:** No — always exits 0
**Purpose:** Emits a one-line principle reminder for high-impact Bash commands. Helps agents and developers stay grounded in platform-agnostic principles before making infrastructure changes.

**Triggers and output:**

| Command pattern | Emits |
|----------------|-------|
| `apply-*.ps1` | `[philosophy] GitOps principle: verify this change is committed and reviewed before applying.` |
| `destroy-*.ps1` | `[philosophy] Immutable infra principle: confirm this teardown is intentional and documented.` |
| `terraform apply` (direct) | `[philosophy] GitOps principle: verify this change is committed and reviewed before applying.` |
| `gcloud ... set-iam-policy` | `[philosophy] Least Privilege principle: confirm the minimum necessary permissions are being granted.` |
| `git push --force` | `[philosophy] GitOps principle: force push rewrites history — confirm this is intentional.` |

Emits nothing for other commands.

---

### `pre-destroy-confirmation.sh`

**Fires:** Before any `destroy-*.ps1` Bash command
**Blocking:** Yes — exits non-zero unless `LAAS_ALLOW_DESTROY=yes` env var is set
**Purpose:** Safety gate preventing accidental infrastructure teardown. Set `$env:LAAS_ALLOW_DESTROY = 'yes'` before running destroy scripts.

---

### `check-staleness-on-dispatch.sh`

**Fires:** Before `gh workflow run deploy-dev.yml` or `sandbox-deploy.yml`
**Blocking:** No (advisory)
**Purpose:** Warns if services are stale or never-deployed, recommends using `force_services` flag to force rebuild.

---

### `check-new-branch-base.sh`

**Fires:** Before `git checkout -b` or `git switch -c`
**Blocking:** No (advisory)
**Purpose:** Warns when new branch base is not `laas/main`. Lists stale local branches when base is correct.

---

### `check-upstream-freshness.sh`

**Fires:** Before any `git commit` Bash command
**Blocking:** No (advisory)
**Purpose:** Fetches `laas/main` and warns if branch is >3 commits behind. Highlights auto-generated file conflict risk.

---

### `pre-pr-create.sh`

**Fires:** Before any Bash command containing `gh pr create`
**Blocking:** **Yes** (exit 1 if branch not on remote)
**Purpose:** Prevents the cryptic GraphQL errors that occur when `gh pr create` runs before the branch is pushed.

**Output (blocked):**
```
BLOCKED: Branch 'feat/my-feature' has not been pushed to remote 'laas'.
  Run: git push -u laas feat/my-feature
  Then retry: gh pr create
```

**Detection logic:** Parses Bash command via jq, checks `git ls-remote --heads` for the current branch on the configured remote (defaults to `laas`, then `origin`).

---

## PostToolUse (Edit/Write) Hooks

### `smoke-test.sh`

**Fires:** After editing any `scripts/**/*.ps1`
**Blocking:** No (async)
**Purpose:** Auto-runs the relevant Pester test suite after a script edit. Determines which test target to run based on the edited file's path prefix.

**Output:** Pester results appear in terminal (may arrive after the next tool call due to async).

---

### `tf-validate.sh`

**Fires:** After editing any `terraform/**/*.tf`
**Blocking:** No (async)
**Purpose:** Runs `terraform validate` in the affected module directory to catch syntax errors immediately. Does not run `terraform plan` — only validates HCL syntax and references.

**Output:**
```
[tf-validate] terraform/app: Success! The configuration is valid.
```
or
```
[tf-validate] terraform/app: Error: Invalid argument
```

---

### `workflow-lint.sh`

**Fires:** After editing any `.github/**/*.yml`
**Blocking:** No (async)
**Purpose:** Runs `actionlint` if installed, otherwise falls back to YAML syntax parse. Catches workflow YAML errors before they fail in CI.

**Output:**
```
[workflow-lint] .github/workflows/deploy-dev.yml: OK
```

---

### `validate-gates-json.sh`

**Fires:** After editing `.claude/test-gates.json` directly
**Blocking:** No (sync, advisory — exits 0 even on error to preserve edit history)
**Purpose:** Validates JSON syntax and gate schema (valid `status` values: `clean`, `dirty`, `needs-run`; valid `tier` values: `immediate`, `before-merge`, `pre-deploy`, `advisory`).

**Output when valid:**
```
[validate-gates] test-gates.json: OK (22 gates, JSON valid)
```

**Output when invalid:**
```
[validate-gates] test-gates.json: INVALID — fix before committing
ERROR: Gate pester:deploy: invalid status "passed" (must be: {'clean', 'dirty', 'needs-run'})
```

---

### `assess-skill-on-edit.sh`

**Fires:** After editing any `.claude/skills/*.md`
**Blocking:** No (sync, advisory)
**Purpose:** Runs 10 deterministic structural quality checks on the edited skill file,
followed by a two-pass Technical + Philosophy review prompt:

1. H1 title present
2. Role tag present and valid
3. Trigger phrases section present
4. At least one runnable code block
5. At least 3 H2 sections
6. Length in acceptable range
7. Cross-referenced skill files exist on disk
8. DEPRECATED marker check
9. `## Why This Matters` section present (or exempt status)
10. Companion hook documented in `skill-hook-pairs.md` (advisory only — for operational/
    diagnostic/recovery/validation skills without an existing pair entry)

After deterministic checks, emits two structured review passes (T1–T5 Technical Lens,
P1–P4 Philosophy Lens) and a synthesis rule reminder.

**Output:**
```
[assess-skill] apply-infra.md — structural check: 9 pass, 1 issue(s)
  ...
  ⚠ Why This Matters section missing (exempt: no)
[assess-skill] ADVISORY (check-10): 'apply-infra.md' is an operational/... skill but has
  no companion hook entry in skill-hook-pairs.md. ...

[assess-skill] REVIEW PASS 1 — TECHNICAL LENS
  T1: Are all code blocks syntactically valid and runnable as-written?
  ...

[assess-skill] REVIEW PASS 2 — PHILOSOPHY LENS
  P1: Does '## Why This Matters' give a genuine reason ...?
  ...

[assess-skill] SYNTHESIS RULE: If T and P lenses produce conflicting recommendations,
  philosophy takes precedence. ...
```

---

### `critique-plan.sh`

**Fires:** After editing or writing any `.claude/plans/*.md` file
**Blocking:** No (sync)
**Purpose:** Reminds the agent to run plan-critic before implementing. Surfaces the `plan-critic.md` skill workflow.

---

### `suggest-architect.sh`

**Fires:** After editing or writing any `.claude/plans/*.md` file
**Blocking:** No (sync)
**Purpose:** Checks plan content for architectural keywords and nudges adoption of the architect persona when designing new infrastructure or multi-service changes.

---

### `lsp-on-edit.sh`

**Fires:** After editing `.ps1`, `.tf`, `.ts`, or `.tsx` files in `scripts/` or `apps/`
**Blocking:** No (async)
**Purpose:** Runs language-specific static analysis. For `.ps1`: PSScriptAnalyzer. For `.tf`: `terraform validate`. For `.ts`/`.tsx`: `tsc --noEmit`.

**Note:** `tf-validate.sh` and `lsp-on-edit.sh` both fire on `.tf` edits. `tf-validate.sh`
fires synchronously at the module level; `lsp-on-edit.sh` fires asynchronously and may
add additional diagnostics.

---

## PostToolUse (Bash) Hooks

### `apply-failure.sh`

**Fires:** After every Bash tool call (checks exit code internally)
**Blocking:** No (sync)
**Purpose:** When a Bash command exits non-zero, appends the failed command and timestamp to `.claude/apply-failure.log`. This log is the source of truth for failed applies.

**Output (only on failure):**
```
[apply-failure] Command failed (exit 1) — logged to .claude/apply-failure.log
Run: /fix-apply-failure to analyze and recover
```

---

### `e2e-gate-update.sh`

**Fires:** After any Bash command — internally checks if `run-e2e.ps1` was called
**Blocking:** No (sync)
**Purpose:** Surfaces the e2e gate state change to Claude's context after running `run-e2e.ps1`. `run-e2e.ps1` already calls `update-gate.ps1` internally; this hook ensures the agent sees the result even when called via a direct Bash invocation.

**Output (only for `run-e2e.ps1` commands):**
```
[e2e-gate] e2e:chat: CLEAN — smoke test passed
```
or
```
[e2e-gate] e2e:chat: DIRTY — smoke test failed (exit 1)
  Re-run: pwsh -File scripts/verify/run-e2e.ps1 -Module chat
  Logs:   npx playwright show-report (in apps/chat)
```

---

### `watch-cert-after-gateway.sh`

**Fires:** After any Bash command — internally checks if a gateway apply succeeded
**Blocking:** No (async)
**Purpose:** After a successful gateway apply, checks the current SSL cert status and starts `watch-cert.ps1` in the background if the cert is not yet ACTIVE. SSL provisioning takes 10–30 minutes; this hook ensures you're notified when it completes.

**Trigger pattern:** Command must match `apply-apps.*gateway` or `apply-apps.*-Target\s+gateway` and exit code must be 0.

**Output:**
```
SSL cert status: PROVISIONING
SSL cert watcher started in background (cert is PROVISIONING).
You will be notified when laas-lb-cert-dots becomes ACTIVE (typically 10-30 min).
```

**Note:** Falls back to `$LAAS_PROJECT_ID` env var if `-ProjectID` not in the command; set this var to avoid the hardcoded fallback.

---

### `health-check-after-apply.sh`

**Fires:** After successful `apply-app` or `apply-apps` commands (async)
**Purpose:** Polls Cloud Run service Ready status after a deploy. Ensures the new revision becomes healthy before the next tool call proceeds.

**Output:** Cloud Run revision status; alerts if Ready never reaches `True` within timeout.

---

### `annotate-topology.sh`

**Fires:** After every Bash command
**Blocking:** No (sync)
**Purpose:** Reads `network-topology.json` and surfaces any resources flagged `needs_review: true`. These are resources added to the topology snapshot that haven't been reviewed and annotated yet.

**Output (only when `needs_review` resources exist):**
```
[topology] 2 network resource(s) need review:
  - google_compute_forwarding_rule.laas-http-redirect (needs_review: true)
  - google_compute_global_address.laas-ip (needs_review: true)
Annotate these in .claude/network/network-topology.json before deploying.
```

---

### `annotate-access-topology.sh`

**Fires:** After every Bash command
**Blocking:** No (sync)
**Purpose:** Same as `annotate-topology.sh` but for `access-topology.json`. Surfaces IAM bindings flagged `needs_review: true`.

---

### `suggest-pr-on-push.sh`

**Fires:** After `git push laas <branch>` (non-main branches)
**Blocking:** No (async)
**Purpose:** Checks for an open PR for the pushed branch. If none exists, prints a `gh pr create` reminder with suggested title.

---

### `render-topology-views.sh`

**Fires:** After `update-network-topology.ps1` succeeds
**Blocking:** No (async)
**Purpose:** Renders the dev-scoped topology view (foundation+gateway+app) as SVG in background. The rendered view is committed as `.claude/network/network-topology-dev.svg`.

---

### `suggest-security-auditor.sh`

**Fires:** After any Bash command matching IAM operations (e.g., `set-iam-policy`, `add-iam-policy-binding`)
**Blocking:** No (async)
**Purpose:** Nudges adoption of the security-auditor persona when IAM changes are detected.

---

### `suggest-lsp-for-grep.sh`

**Fires:** After any Bash command that looks like code-navigation grep patterns targeting source directories
**Blocking:** No (async)
**Purpose:** Detects grep patterns that could be replaced by LSP tool equivalents (e.g., `grep -r "function Foo"` → `Find-Symbol.ps1 -Name Foo`). Suggests the appropriate `Find-*.ps1` tool.

---

### Bash Logger (inline)

**Fires:** After every Bash command
**Blocking:** No (sync)
**Purpose:** Appends a timestamped entry to `.claude/deploy.log` for audit trail. Format: `[timestamp] exit=N cmd=<first 200 chars>`. Configured as inline bash in settings.json, not a standalone script.

---

## Git Hooks (installed via `install-hooks.ps1`)

### `pre-commit.sh`

**Fires:** On every `git commit`
**Blocking:** Yes — exits non-zero to abort the commit
**Purpose:** Checks all gates with `blocks: ["commit"]`. If any are DIRTY or STALE, blocks the commit and prints the gate status + commands to clear them.

Install: `pwsh -File scripts/utility/install-hooks.ps1`

---

### `pre-push.sh`

**Fires:** On `git push` to `main` or `devops` branch
**Blocking:** Yes — exits non-zero to abort the push
**Purpose:** Checks all gates with `blocks: ["merge"]`. Stricter than pre-commit — includes terraform test gates and before-merge tier gates.

---

## When a Hook Fires Unexpectedly

**Hook output appears but you didn't expect it:**
- Check the trigger pattern: every PostToolUse Bash hook fires on every Bash call; most filter internally based on the command content
- `annotate-topology.sh` and `annotate-access-topology.sh` fire on every Bash call and may surface stale `needs_review` flags from a previous topology update

**Hook output is blank / nothing appears:**
- Async hooks (smoke-test, tf-validate, etc.) deliver output after a delay — check the terminal after the next few seconds
- Some hooks only emit output when there's something to report (e.g., `apply-failure.sh` only emits on non-zero exit)

**`check-sa-liveness.sh` blocks a command you expected to work:**
- ADC token has expired (typical after >1h idle)
- Fix: `gcloud auth application-default login` then re-impersonate the SA

**`assess-skill-on-edit.sh` reports philosophy warning:**
- The skill is missing `## Why This Matters` and is not on the exempt list
- Fix: Add a 2-3 sentence `## Why This Matters` section, or check if the skill qualifies for the exempt list in `write-skill.md`

---

## Why This Matters

This skill implements **Everything is Code** from `devops-philosophy.md`. Hooks are the
automated enforcement layer between human intent and system state — they replace the "remember
to run X after editing Y" mental checklist with a deterministic system. Understanding what each
hook does (and doesn't do) is prerequisite to trusting the system's feedback rather than
second-guessing it.

---

## See Also

- The project's `CLAUDE.md` Hook Automation table — canonical one-line summary of all registered hooks (project file, not a skill)
- `write-skill.md` — `assess-skill-on-edit.sh` quality check details
- `gate-status.md` — gate states and how to clear them
- `gate-system-overview.md` — conceptual model for how gates work
