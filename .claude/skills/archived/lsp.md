# Use LSP-Powered Symbol Search

Static analysis and symbol navigation for PowerShell, Terraform, TypeScript, Python, Skills, Gates, Workflows, and Topology — beyond what grep can do.

Role: diagnostic · reference
Governs: scripts/dev/Find-SkillSymbol.ps1, scripts/dev/Find-GateSymbol.ps1, scripts/dev/Find-WorkflowSymbol.ps1, scripts/dev/Find-TopoSymbol.ps1, scripts/dev/Find-SessionContext.ps1, scripts/dev/Find-ShellSymbol.ps1, scripts/dev/Find-TFStateSymbol.ps1

Trigger phrases: "find all callers of function", "where is this function defined", "find references",
Domain: brain
Methodology-step: skills
"go to definition", "who calls this", "type errors in TypeScript", "type errors in Python",
"analyze PowerShell script", "find terraform variable references", "PSScriptAnalyzer", "symbol search",
"find all usages", "cross-file symbol", "lsp", "language server", "pyright", "python type check",
"find python function", "fastapi type errors", "what links to this skill", "skill cross-references",
"what gates block deploy", "critical path gates", "find workflow that uses script", "topology query",
"what depends on this resource", "gates for this file", "ci pipeline dependencies"

---

## Why LSP Instead of Grep

Grep finds string literals. LSP understands the **language structure**.

| Task | Grep can do it? | LSP does it better because... |
|------|----------------|-------------------------------|
| Find all calls to `Update-Gate` | Partially — misses aliases, splat calls | AST knows call sites vs string literals |
| Find where `$ProjectID` is declared | Messy regex | Scope-aware; finds `param()` block vs local assign |
| Find all `var.project_id` references in TF | Yes, but noisy | Understands `var.` prefix semantics |
| Surface type errors in TypeScript | No | `tsc --noEmit` gives compiler-level errors |
| Rename a function safely | No | AST rename handles all call sites |
| Find callers across modules | Tedious | Single command, structured output |

**Use LSP when:** renaming symbols, auditing all callers before changing a function signature, finding dead code, or diagnosing type errors.

**Use grep when:** searching for a specific string value, log message, error code, or comment text. See the Quick Reference below.

---

## Quick Reference

| What you want | Command |
|--------------|---------|
| All callers + definition of a PS function | `pwsh -File scripts/dev/Find-Symbol.ps1 -Name <FunctionName>` |
| Where a TF variable/resource is defined | `pwsh -File scripts/dev/Find-TFSymbol.ps1 -Name <name>` |
| TypeScript type errors in all apps | `pwsh -File scripts/dev/Find-TSSymbol.ps1` |
| TypeScript errors in chat only | `pwsh -File scripts/dev/Find-TSSymbol.ps1 -App chat` |
| Find a TS component/function definition | `pwsh -File scripts/dev/Find-TSSymbol.ps1 -App all -Symbol <Name>` |
| All pyright errors in apps/api/ | `pwsh -File scripts/dev/Find-PySymbol.ps1 -Check` |
| Find a Python function/class/variable | `pwsh -File scripts/dev/Find-PySymbol.ps1 -Name <name>` |
| Check a single .py file | `pwsh -File scripts/dev/Find-PySymbol.ps1 -File apps/api/app/routers/generate.py` |
| Lint a single PS1 file | `pwsh -Command "Invoke-ScriptAnalyzer -Path <file>"` |
| Lint all scripts/ recursively | `pwsh -Command "Invoke-ScriptAnalyzer -Path scripts/ -Recurse"` |
| Validate a TF module (must be init'd) | `cd terraform/<module> && terraform validate` |
| What skills link to a given skill? | `pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Name lsp.md` |
| Corpus-wide skill health check | `pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Check` |
| Skills with a given role or persona | `pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Role operational` |
| All artifacts for a Brain domain | `pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Domain gates` |
| Artifacts by methodology step | `pwsh -File scripts/dev/Find-SkillSymbol.ps1 -MethodologyStep hooks` |
| Compile tag index (skills+hooks+scripts) | `pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Compile` |
| Deploy-blocking gates + time estimate | `pwsh -File scripts/dev/Find-GateSymbol.ps1 -CriticalPath` |
| Gates watching a path | `pwsh -File scripts/dev/Find-GateSymbol.ps1 -WatchPath "scripts/deploy/"` |
| Full gate health: dirty, blockers, budget | `pwsh -File scripts/dev/Find-GateSymbol.ps1 -Check` |
| Which workflows call a script? | `pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Script update-network-topology.ps1` |
| Validate all workflow script/uses refs | `pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Check` |
| Workflow dependency graph (JSON) | `pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -DAG` |
| What depends on a topology resource? | `pwsh -File scripts/dev/Find-TopoSymbol.ps1 -DependsOn "db:firestore"` |
| Resources needing annotation review | `pwsh -File scripts/dev/Find-TopoSymbol.ps1 -NeedsReview` |
| All resources by criticality | `pwsh -File scripts/dev/Find-TopoSymbol.ps1 -Criticality existential` |

### Session Context Modes (Find-SessionContext)

| When | Command |
|------|---------|
| Session start / explore | `pwsh -File scripts/dev/Find-SessionContext.ps1` |
| Before `git commit` | `pwsh -File scripts/dev/Find-SessionContext.ps1 -Action commit` |
| Before `apply-*.ps1` | `pwsh -File scripts/dev/Find-SessionContext.ps1 -Action deploy` |
| Before opening a PR / merging | `pwsh -File scripts/dev/Find-SessionContext.ps1 -Action review` |
| Debugging a dirty gate / incident | `pwsh -File scripts/dev/Find-SessionContext.ps1 -Action debug` |

Each mode is a filter lens over the same 4-domain join. Choose the mode matching your
current workflow phase. See `lsp-grounded.md` for the full grounding loop pattern.

---

## Step 1 — PowerShell: PSScriptAnalyzer + AST Symbol Search

### Install PSScriptAnalyzer (one-time, per machine)

```powershell
Install-Module PSScriptAnalyzer -Scope CurrentUser -Force
```

Verify:

```powershell
Get-Module -ListAvailable PSScriptAnalyzer | Select-Object Name, Version
```

### Lint a file or directory

```powershell
# Single file
Invoke-ScriptAnalyzer -Path scripts/deploy/apply-app.ps1

# All scripts, recursive
Invoke-ScriptAnalyzer -Path scripts/ -Recurse

# Only errors (suppress informational)
Invoke-ScriptAnalyzer -Path scripts/ -Recurse -Severity Error, Warning
```

### Find all definitions and call sites for a symbol

Use the `Find-Symbol.ps1` wrapper (understands function definitions, calls, variable assignments, and parameter declarations):

```powershell
# Find all definitions and call sites for Update-Gate
pwsh -File scripts/dev/Find-Symbol.ps1 -Name "Update-Gate"

# Only find function definitions (not calls)
pwsh -File scripts/dev/Find-Symbol.ps1 -Name "Update-Gate" -Type Function

# Search under a specific subtree
pwsh -File scripts/dev/Find-Symbol.ps1 -Name "ProjectID" -Type Variable -Path scripts/deploy/
```

Output format:

```
scripts/utility/update-gate.ps1:12   [definition]  function Update-Gate {
scripts/deploy/apply-app.ps1:45      [call]        & $UpdateGate -Gate $gateKey -Status ...
scripts/verify/run-tests.ps1:117     [call]        & $UpdateGate -Gate $gateKey -Status ...
```

### What the AST finds that grep misses

- Function definitions declared inside other functions
- Call sites using `&` or `.` dot-source invocation
- Splat calls (`Update-Gate @params`)
- Variable assignments vs parameter names with the same text

---

## Step 2 — Terraform: terraform-ls and Module Inspection

### Check if terraform-ls is available

```bash
terraform-ls --version 2>/dev/null || echo "not installed"
```

If not installed, download from https://releases.hashicorp.com/terraform-ls/ and place on PATH.
The binary is a single executable — no install wizard needed.

### Inspect a module's symbol table

```bash
# List all variables, resources, outputs, and data sources in a module
terraform-ls inspect-module terraform/app/

# List only outputs
terraform-ls inspect-module terraform/app/ | grep -A3 '"outputs"'
```

### Find where a TF symbol is defined and referenced

Use the `Find-TFSymbol.ps1` wrapper (no terraform-ls required — uses pattern matching across HCL):

```powershell
# Find all references to variable "project_id" across terraform/
pwsh -File scripts/dev/Find-TFSymbol.ps1 -Name "project_id" -Type variable

# Find all uses of a module named "web-app"
pwsh -File scripts/dev/Find-TFSymbol.ps1 -Name "web-app" -Type module

# Find all definitions and references for a resource
pwsh -File scripts/dev/Find-TFSymbol.ps1 -Name "google_cloud_run_v2_service" -Type resource

# Search everything named "laas-api"
pwsh -File scripts/dev/Find-TFSymbol.ps1 -Name "laas-api"
```

Output format:

```
terraform/app/main.tf:3          [resource-def]    resource "google_cloud_run_v2_service" "laas-api" {
terraform/app/outputs.tf:5       [output-def]      output "service_url" {
terraform/apps/gateway.tf:14     [reference]       module.web-app.service_url
```

### Validate a module (static check, no plan needed)

```bash
cd terraform/app && terraform validate
# or
cd terraform/foundation && terraform validate
```

Note: `terraform validate` requires the module to have been initialized (`terraform init`).
The `tf-validate.sh` hook runs this automatically after each `.tf` edit if `.terraform/` exists.

---

## Step 3 — TypeScript: tsc Type Checking and Symbol Search

### Find type errors across all apps

```powershell
pwsh -File scripts/dev/Find-TSSymbol.ps1
```

This runs `npx tsc --noEmit` in each Next.js app that has a `tsconfig.json` and reports errors.

### Find type errors in a single app

```powershell
pwsh -File scripts/dev/Find-TSSymbol.ps1 -App chat
pwsh -File scripts/dev/Find-TSSymbol.ps1 -App docs
pwsh -File scripts/dev/Find-TSSymbol.ps1 -App storybook-web
```

### Find all definitions and uses of a TypeScript symbol

```powershell
pwsh -File scripts/dev/Find-TSSymbol.ps1 -App all -Symbol "fetchCompletion"
pwsh -File scripts/dev/Find-TSSymbol.ps1 -App chat -Symbol "ChatMessage"
```

This uses `grep` with structural patterns to find:
- `function fetchCompletion` / `const fetchCompletion =`
- All import sites: `import { fetchCompletion }`
- All call sites: `fetchCompletion(`

### Direct tsc invocation

```bash
cd apps/chat && npx tsc --noEmit
cd apps/docs && npx tsc --noEmit
```

Requires dependencies installed: `cd apps/chat && npm install` first.

---

## Step 4 — Python: Pyright Type Checking and Symbol Search

The FastAPI backend (`apps/api/`) uses [pyright](https://github.com/microsoft/pyright) for static
type analysis. Pyright understands Pydantic models, FastAPI dependency injection types, and Python
3.11 syntax.

### Check all type errors in apps/api/

```powershell
pwsh -File scripts/dev/Find-PySymbol.ps1 -Check
```

Output:

```
[lsp:python] Running pyright on apps/api/ ...

pyright summary: 3 error(s), 1 warning(s)
  [error] generate.py:42: Cannot access attribute "items" for class "None"
  [warning] models.py:15: Type of "data" is partially unknown
  [error] routers/chat.py:88: Argument of type "str" cannot be assigned to parameter ...
```

### Find a Python symbol (function, class, variable)

```powershell
# Find definition + all import sites + call sites
pwsh -File scripts/dev/Find-PySymbol.ps1 -Name "generate_data"

# Filter to class definitions only
pwsh -File scripts/dev/Find-PySymbol.ps1 -Name "ChatMessage" -Type class

# Find only function definitions
pwsh -File scripts/dev/Find-PySymbol.ps1 -Name "get_db" -Type function
```

Output:

```
  apps/api/app/models.py:8              [class-def]  class ChatMessage(BaseModel):
  apps/api/app/routers/generate.py:12   [import]     from app.models import ChatMessage
  apps/api/app/routers/generate.py:44   [call]       msg = ChatMessage(role="user", content=prompt)
```

### Check a single file

```powershell
pwsh -File scripts/dev/Find-PySymbol.ps1 -File apps/api/app/routers/generate.py
```

### Configuration

The `apps/api/pyrightconfig.json` uses `typeCheckingMode: "basic"` (not `strict`) — the codebase
currently has no type annotations, and strict mode would produce hundreds of errors. Basic catches
high-value errors: undefined names, missing imports, obvious type mismatches in FastAPI handler
signatures and Pydantic models.

### Known limitations

Pyright's CLI doesn't expose go-to-definition or hover type info — those are IDE features. For
cross-file call site analysis, `Find-PySymbol.ps1 -Name` uses regex-based parsing; it catches the
common patterns (imports, direct calls) but may miss dynamic dispatch or late-bound attribute access.

---

## Step 5 — Skills: Cross-Reference Graph and Corpus Queries

The `Find-SkillSymbol.ps1` wrapper queries the `.claude/skills/` corpus — cross-references,
personas, roles, orphaned skills, and the full adjacency graph.

### Corpus health check

```powershell
# Dead cross-references, missing H1/Role tags, invalid personas, skill-index.md sync
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Check
```

### Find all incoming references for a skill

```powershell
# Which skills link to lsp.md?
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Name "lsp.md"
```

Output includes: incoming cross-references with file and line, hook pairs from `skill-hook-pairs.md`.

### Filter by role or persona

```powershell
# All operational skills
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Role operational

# All skills with the adversary persona
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Persona adversary

# Skills with zero incoming cross-references (potential dead skills)
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Orphaned

# Full adjacency list as JSON (pipe to file for processing)
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Graph

# All artifacts (skills, hooks, scripts) tagged with a Brain domain
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Domain gates

# Artifacts by LSP Brains step
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -MethodologyStep sensory-tools

# Artifacts with a free-form tag
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Tag deploy-pipeline

# Compile tag index (.claude/tag-index.json) from all skill/hook/script metadata
pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Compile
```

---

## Step 6 — Gates: Status, Blockers, and Critical Path

The `Find-GateSymbol.ps1` wrapper queries `.claude/test-gates.json` — gate status, blocking
actions, critical path to deploy, and time budget estimates.

> **Note on stale status:** `test-gates.json` stores `clean`, `dirty`, and `needs-run` only.
> Stale is computed at runtime by `gate-advisor.ps1`. `-Status stale` always returns 0 results.

### Gate health overview

```powershell
# Status summary: dirty gates, deploy blockers, time to clear
pwsh -File scripts/dev/Find-GateSymbol.ps1 -Check
```

### Critical path to deploy

```powershell
# All deploy-blocking gates sorted by tier with estimated minutes
pwsh -File scripts/dev/Find-GateSymbol.ps1 -CriticalPath

# Only deploy blockers clearable in 10 minutes
pwsh -File scripts/dev/Find-GateSymbol.ps1 -Budget 10
```

### Query gates by path, action, or status

```powershell
# Which gates watch files in scripts/deploy/?
pwsh -File scripts/dev/Find-GateSymbol.ps1 -WatchPath "scripts/deploy/"

# All gates that block commit
pwsh -File scripts/dev/Find-GateSymbol.ps1 -Blocks commit

# Full detail for a specific gate
pwsh -File scripts/dev/Find-GateSymbol.ps1 -Name "pester:deploy"

# All gates with needs-run status
pwsh -File scripts/dev/Find-GateSymbol.ps1 -Status needs-run
```

---

## Step 7 — Workflows: CI/CD Cross-Reference Validation

The `Find-WorkflowSymbol.ps1` wrapper queries `.github/workflows/` — validates script
references, `uses:` calls, detect-changes output consumption, and secrets usage.

> **YAML parsing:** Uses `python3 -c yaml.safe_load` when PyYAML is available; falls back
> to regex for common patterns when it isn't.

### Validate all workflow references

```powershell
# Check script refs, uses: refs, action refs, detect-changes output consumption
pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Check
```

### Cross-reference queries

```powershell
# Which workflows call update-network-topology.ps1?
pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Script "update-network-topology.ps1"

# Which jobs consume the 'api_changed' detect-changes output?
pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Output "api_changed"

# Which workflows use deploy-dev.yml as a reusable workflow?
pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Workflow "deploy-dev.yml"

# Where is the WIF_PROVIDER secret referenced?
pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Secret "WIF_PROVIDER"

# Which workflows deploy the 'laas-api' Cloud Run service?
pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Service "laas-api"

# Full workflow dependency graph as JSON
pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -DAG
```

---

## Step 8 — Topology: CMDB Queries for Network and Access

The `Find-TopoSymbol.ps1` wrapper queries `network-topology.json` and `access-topology.json`
as a configuration management database — dependencies, criticality, annotation review state.

### Topology health check

```powershell
# Resources with needs_review:true + bindings missing criticality
pwsh -File scripts/dev/Find-TopoSymbol.ps1 -Check
```

### Resource dependency queries

```powershell
# What breaks if db:firestore is removed?
pwsh -File scripts/dev/Find-TopoSymbol.ps1 -DependsOn "db:firestore"

# Full detail for a named resource (network + access bindings joined)
pwsh -File scripts/dev/Find-TopoSymbol.ps1 -Name "lb:https-proxy"

# All existential-criticality resources
pwsh -File scripts/dev/Find-TopoSymbol.ps1 -Criticality existential
```

> **Note:** `high`, `medium`, and `low` criticality have no entries in the current topology.
> Use `existential` for critical infrastructure queries.

### Filter and annotation queries

```powershell
# Resources in the gateway scope
pwsh -File scripts/dev/Find-TopoSymbol.ps1 -Scope gateway

# Resources managed by Terraform
pwsh -File scripts/dev/Find-TopoSymbol.ps1 -ManagedBy terraform

# All bindings for a principal
pwsh -File scripts/dev/Find-TopoSymbol.ps1 -Principal "sa:laas-app-sa"

# All resources/bindings needing annotation review
pwsh -File scripts/dev/Find-TopoSymbol.ps1 -NeedsReview
```

---

## Step 9 — Shell/Bash: Shellcheck Linting and Function Search

The `Find-ShellSymbol.ps1` wrapper provides inventory, symbol search, and linting for the
32 shell hooks in `.claude/hooks/` and any scripts in `scripts/**/*.sh`.

### Shell inventory and health check

```powershell
# File count, function count, shellcheck availability
pwsh -File scripts/dev/Find-ShellSymbol.ps1 -Check -Plain
```

### Find a shell function definition and call sites

```powershell
pwsh -File scripts/dev/Find-ShellSymbol.ps1 -Name "run_psscriptanalyzer" -Plain
```

### List functions in a specific file

```powershell
pwsh -File scripts/dev/Find-ShellSymbol.ps1 -File '.claude/hooks/lsp-on-edit.sh' -Plain
```

### Lint with shellcheck

```powershell
# Lint all hooks and scripts
pwsh -File scripts/dev/Find-ShellSymbol.ps1 -Lint -Plain

# Lint a specific file
pwsh -File scripts/dev/Find-ShellSymbol.ps1 -Lint '.claude/hooks/lsp-on-edit.sh' -Plain
```

---

## Step 10 — Terraform State: Deployed Output Queries

The `Find-TFStateSymbol.ps1` wrapper bridges HCL definitions (what is declared) and live
Terraform state (what is deployed). It reads from `.claude/tf-state/tf-state-cmdb.json`.

### State CMDB health check

```powershell
# Per-module init status, output counts, CMDB age
pwsh -File scripts/dev/Find-TFStateSymbol.ps1 -Check -Plain
```

### Query a specific module's outputs

```powershell
pwsh -File scripts/dev/Find-TFStateSymbol.ps1 -Module foundation -Plain
```

### Find which module exposes an output

```powershell
pwsh -File scripts/dev/Find-TFStateSymbol.ps1 -Output laas_app_sa_email -Plain
```

### Find HCL consumers of a state output

```powershell
# Who reads this output via data.terraform_remote_state?
pwsh -File scripts/dev/Find-TFStateSymbol.ps1 -Consumers laas_app_sa_email -Plain
```

### Refresh the CMDB (after deploy)

```powershell
pwsh -File scripts/utility/update-tf-state-cmdb.ps1 -Module all -Environment dev
```

---

## Step 11 — Mermaid Diagrams

The `Find-MermaidSymbol.ps1` tool validates `.mmd` Mermaid diagram files. It uses
`npx @mermaid-js/mermaid-cli` for full syntax validation, falling back to a regex
structural check if mermaid-cli is not available.

### Health check (all diagrams)

```powershell
# Validates all .mmd files in repo (excludes .claude/worktrees/)
pwsh -File scripts/dev/Find-MermaidSymbol.ps1 -Check -Plain
```

### Validate a single diagram

```powershell
pwsh -File scripts/dev/Find-MermaidSymbol.ps1 -File terraform/architecture.mmd -Plain
pwsh -File scripts/dev/Find-MermaidSymbol.ps1 -File apps/devops-whitepaper/diagrams/ci-cd-pipeline.mmd -Plain
```

**Coverage:** `terraform/*.mmd`, `.claude/network/*.mmd`, `.claude/access/*.mmd`,
`apps/devops-whitepaper/diagrams/*.mmd`

**Graceful degradation:** if `npx` or `mermaid-cli` is unavailable, falls back to regex
structural check (verifies a known diagram type declaration exists) and emits `[FALLBACK]`.

---

## Step 12 — Dockerfiles

The `Find-DockerSymbol.ps1` tool lints Dockerfile files using PowerShell regex rules —
no `hadolint` binary required on Windows.

### Health check (all app Dockerfiles)

```powershell
pwsh -File scripts/dev/Find-DockerSymbol.ps1 -Check -Plain
```

### Lint a single Dockerfile

```powershell
pwsh -File scripts/dev/Find-DockerSymbol.ps1 -File apps/api/Dockerfile -Plain
```

**Coverage:** `apps/chat/`, `apps/docs/`, `apps/api/`, `apps/devops-whitepaper/`,
`apps/storybook-web/`, `apps/swagger/` (all `apps/*/Dockerfile`).

**Detected issues:**

| Severity | Rule |
|----------|------|
| high | `FROM :latest` tag |
| high | `USER root` |
| high | `curl \| bash` install pattern |
| medium | `ADD` instead of `COPY` |
| low | Missing `HEALTHCHECK` |
| low | Long `RUN` chain (>4 `&&`) |

Exit code 1 if any high-severity issues found.

---

## Step 13 — Nginx Configuration

The `Find-NginxSymbol.ps1` tool parses nginx.conf files using PowerShell regex —
no `nginx` binary required on Windows dev machines.

### Health check (all app nginx.conf files)

```powershell
pwsh -File scripts/dev/Find-NginxSymbol.ps1 -Check -Plain
```

### Parse a single conf file

```powershell
pwsh -File scripts/dev/Find-NginxSymbol.ps1 -File apps/swagger/nginx.conf -Plain
```

**Coverage:** `apps/storybook-web/nginx.conf`, `apps/devops-whitepaper/nginx.conf`,
`apps/swagger/nginx.conf`.

**Parsed fields:** `server_name`, `listen` ports, `location` blocks, `upstream` definitions.

**Detected issues:**

| Severity | Rule |
|----------|------|
| high | Duplicate `listen` port within a file |
| medium | Missing `server_name` directive |
| medium | `location` block without `proxy_pass`, `root`, `alias`, `return`, or `rewrite` |

---

## Step 14 — EaC Symbol: Governs Path Validation

The `Find-EaCSymbol.ps1` tool validates the `Governs:` fields across all skill files and
provides bidirectional lookup between files and their governing skills.

### Validate all Governs: paths

```powershell
pwsh -File scripts/dev/Find-EaCSymbol.ps1 -Check -Plain
```

Scans all skills, resolves every path in every `Governs:` field, reports `[valid]` or
`[stale]` per path. Exits 1 if any stale (not-found) paths are detected.

**When to run:** After adding a new `Governs:` field, after renaming scripts, or when the
`everything-is-code` Brain domain score drops unexpectedly.

### Which skills govern a specific file?

```powershell
pwsh -File scripts/dev/Find-EaCSymbol.ps1 -File scripts/deploy/apply-foundation.ps1 -Plain
```

### What does a specific skill govern?

```powershell
pwsh -File scripts/dev/Find-EaCSymbol.ps1 -Skill everything-as-code.md -Plain
```

### Which tracked files have no governing skill?

```powershell
pwsh -File scripts/dev/Find-EaCSymbol.ps1 -Ungoverned -Plain
```

Cross-references `Governs:` paths against the git-tree CMDB. Falls back to coverage ratio
if CMDB is absent.

**lsp-on-edit:** Not wired — `.md` skill files dispatch to `Find-SkillSymbol.ps1`.
Run `-Check` manually after editing `Governs:` fields.

---

## `Find-TerminologySymbol.ps1` — Language Governance

The `Find-TerminologySymbol.ps1` tool validates canonical terminology usage across the
codebase. It reads `.claude/terminology-catalog.json` and scans for drift variants,
capitalization errors, and hat-persona pairing gaps.

```bash
pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Check -Plain
```

Full compliance scan. Writes `.claude/terminology-cmdb.json` as side-effect. Exits 1 if
drift-error violations found. Scans error+warning severity by default (info is documentation).

```bash
pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Name hat -Plain
```

Look up a term by canonical name or drift variant. Shows definition, drift variants, pairings.

```bash
pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -File .claude/hooks/suggest-architect.sh -Plain
```

Scan a single file for terminology drift and pairing gaps.

```bash
pwsh -File scripts/dev/Find-TerminologySymbol.ps1 -Pairings -Plain
```

Show hat-persona pairing table with validation status across all hook files.

**lsp-on-edit:** Not wired — terminology scanning is too slow for per-edit feedback.
Run `-Check` on demand or via Brain integration. See `terminology-governance.md`.

---

## The lsp-on-edit Hook (Automatic)

The `lsp-on-edit.sh` hook fires automatically after any `Edit` or `Write` to source files.

- **PS1:** runs `Invoke-ScriptAnalyzer` on the edited file, emits warnings/errors inline
- **TF:** runs `terraform validate` on the parent module (delegates to `tf-validate.sh` pattern)
- **TS/TSX:** runs `npx tsc --noEmit` in the relevant app directory
- **PY:** runs `npx pyright --outputjson` in the nearest `pyrightconfig.json` ancestor directory
- **SH:** runs `shellcheck --format=json1` on the edited file, emits errors/warnings inline
- **MMD:** runs `Find-MermaidSymbol.ps1 -File` on the edited diagram
- **Dockerfile:** runs `Find-DockerSymbol.ps1 -File` on the edited Dockerfile
- **nginx.conf:** runs `Find-NginxSymbol.ps1 -File` on edited conf files under `apps/`

Output appears in the terminal out-of-band:

```
[lsp] apply-app.ps1: 0 warnings, 0 errors
[lsp] main.tf: OK (delegated to tf-validate)
[lsp] chat/app/page.tsx: 1 warning, 0 errors
  TS6133: 'unused' is declared but its value is never read. [page.tsx:5]
[lsp:python] generate.py: 2 error(s), 0 warning(s)
  [error] generate.py:42: Cannot access attribute "items" for class "None"
  [error] generate.py:88: Argument of type "str" cannot be assigned to parameter ...
```

The hook is async — it never blocks editing. Errors do not gate commits (that is the Pester suite's job).

Results are cached in `.claude/lsp-on-edit.log`. Check the log before re-running manually — the hook
may have already computed current findings for the file you're reading.

---

## File Read Protocol

When reading a source file, pair it with LSP context for the full picture:

| File type | Read | Also run |
|-----------|------|----------|
| `.ps1` | Read tool | `Invoke-ScriptAnalyzer -Path <file>` or check `lsp-on-edit.log` |
| `.py` | Read tool | `pwsh -File scripts/dev/Find-PySymbol.ps1 -File <path>` or check `lsp-on-edit.log` |
| `.ts` / `.tsx` | Read tool | `npx tsc --noEmit 2>&1 \| grep <filename>` or check `lsp-on-edit.log` |
| `.tf` | Read tool | `terraform validate` in the module directory |
| `.sh` | Read tool | `pwsh -File scripts/dev/Find-ShellSymbol.ps1 -Lint <path>` or check `lsp-on-edit.log` |
| `.claude/skills/*.md` | Read tool | `pwsh -File scripts/dev/Find-SkillSymbol.ps1 -Name <skill.md>` for incoming refs |
| `.claude/test-gates.json` | Read tool | `pwsh -File scripts/dev/Find-GateSymbol.ps1 -Check` for status overview |
| `.github/workflows/*.yml` | Read tool | `pwsh -File scripts/dev/Find-WorkflowSymbol.ps1 -Check` for broken refs |
| `network-topology.json` / `access-topology.json` | Read tool | `pwsh -File scripts/dev/Find-TopoSymbol.ps1 -NeedsReview` for annotation gaps |
| `*.mmd` | Read tool | `pwsh -File scripts/dev/Find-MermaidSymbol.ps1 -File <path>` for syntax validation |
| `Dockerfile` | Read tool | `pwsh -File scripts/dev/Find-DockerSymbol.ps1 -File <path>` for lint issues |
| `nginx.conf` | Read tool | `pwsh -File scripts/dev/Find-NginxSymbol.ps1 -File <path>` for structural issues |
| `Governs:` path | Read tool | `pwsh -File scripts/dev/Find-EaCSymbol.ps1 -File <path>` to see governing skills |

The `lsp-on-edit.sh` hook writes all analysis to `.claude/lsp-on-edit.log` — read it first before
re-running a tool. The log entry is tagged with a timestamp and the filename for easy grepping:

```bash
grep "generate.py" .claude/lsp-on-edit.log | tail -20
```

---

## Why This Matters

This skill implements **Everything is Code** from `devops-philosophy.md`. Static analysis and symbol search treat scripts and infrastructure definitions as first-class software — not deployment artifacts to be manually reviewed. Running PSScriptAnalyzer, `tsc --noEmit`, and `terraform validate` at edit time (via `lsp-on-edit.sh`) catches type errors and undefined references seconds after they're introduced, rather than minutes after a failed apply.

---

## Troubleshooting

**Problem: `Invoke-ScriptAnalyzer` not found**
- Cause: PSScriptAnalyzer not installed or not on the module path.
- Fix: `Install-Module PSScriptAnalyzer -Scope CurrentUser -Force`
- Verify: `Get-Module -ListAvailable PSScriptAnalyzer`

**Problem: `Find-Symbol.ps1` returns no results for a function that clearly exists**
- Cause: The function may use a non-standard declaration style or be defined in a string (dynamic).
- Fix: Fall back to grep: `grep -rn "function Update-Gate" scripts/`
- Also check: is `-Path` set to the right subtree?

**Problem: `terraform-ls inspect-module` fails with "module not initialized"**
- Cause: `terraform init` has not been run in that directory.
- Fix: `cd terraform/<module> && terraform init` (use SA impersonation if needed — see `everything-as-code.md` for the declared infra principles and `secret-refs.md` for credential-access patterns)
- Note: `terraform validate` also needs init; `Find-TFSymbol.ps1` does NOT need init.

**Problem: `npx tsc --noEmit` exits with "Cannot find module 'next'"**
- Cause: `node_modules` not installed in the app directory.
- Fix: `cd apps/chat && npm install`

**Problem: `Find-TSSymbol.ps1` reports no type errors but CI fails**
- Cause: CI may use a stricter `tsconfig.json` or different Node version.
- Fix: check `apps/chat/tsconfig.json` for `strict: true`; run `npx tsc --noEmit --strict` locally.

**Problem: lsp-on-edit hook fires but produces no output**
- Cause: Tool may be running but PSScriptAnalyzer is not installed, or app has no `tsconfig.json`.
- Fix: Check `.claude/lsp-on-edit.log` for the last run output.

---

## See Also

- `lsp-grounded.md` — how to work with LSP as the grounding layer; action mode selection; decision tree
- `ci-testing.md` — confidence ladder for CI workflow changes (separate from LSP)
- `write-smoke-tests.md` — converting topology annotations into Pester assertions
- `gate-status.md` — `pester:dev` gate tracks dev script test results
- `fix-apply-failure.md` — when a terraform apply fails after LSP said validate passed
