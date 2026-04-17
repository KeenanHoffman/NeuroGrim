# Coordinate Subagents

> **Note:** Some examples below reference archived PowerShell starter-kit
> scripts (`Find-*.ps1`, `pwsh -File scripts/...`). The methodology is
> current; the specific commands are not — swap them for `motherbrain`
> CLI equivalents in practice.

When a workflow has multiple independent concerns, spawn subagents to run them in parallel
rather than serializing. This skill documents the three spawn patterns used in this project,
when to use them vs. running inline, and how to handle convergence.

Role: operational · reference
Protocol: lsp-brains/agent/1.0 (see agent-protocol.md for full envelope reference)

Trigger phrases: "spawn a subagent", "run in parallel", "parallelize this workflow",
Domain: deploy
Methodology-step: skills
"subagent coordination", "fan-out pattern", "multiple agents", "staged agents",
"parallel verification", "how do I use the Agent tool", "coordinate agents",
"independent concerns", "run these simultaneously"

---

## When to Spawn vs. Run Inline

| Condition | Decision |
|-----------|---------|
| ≥2 concerns are genuinely independent (no shared write target, no dependency on each other's output) AND wall-clock saving >60 seconds AND each concern's scope fits in a precise 3–5 sentence prompt | **Spawn** |
| B requires A's output as input | **Run inline sequentially** |
| Overhead of writing precise subagent prompts exceeds the time saved | **Run inline** |
| Total task time <90 seconds | **Run inline** |
| Concerns write to shared state (git commit, gate update, topology JSON write) | **Never spawn — serialize** |

**The overhead rule:** Spawning a subagent costs approximately 5–15 seconds (model load + context transfer). Parallelism only pays when the work being parallelized takes longer than that threshold per concern.

**The shared-state rule:** Two agents writing to the same file simultaneously will corrupt it. Gates, git commits, and topology JSON are always written by the parent agent after all subagents have returned. Never delegate a write to a subagent.

---

## Pattern 1 — Parallel Fan-Out

N independent subagents start simultaneously. The parent waits for all N to complete, then converges results.

```
  Parent
   ├── Agent A: concern-1  ──┐
   ├── Agent B: concern-2  ──┼── simultaneous
   └── Agent C: concern-3  ──┘
         ↓ (converge: surface all failures before proceeding)
   Parent synthesizes + decides
```

**LaaS example: Post-Deploy Parallel Verification**

Steps 1–3 and Step 6 (topology refresh) in `post-deploy-verify.md` are independent.
Spawn them simultaneously rather than serializing:

```
Agent A prompt (Cloud Run Ready — Step 1):
"Run this bash command and return ONLY a JSON result:
 gcloud run services list --project=laas-489115 --region=us-central1 \
   --format='table(metadata.name,status.conditions[0].type,status.conditions[0].status)'
Return: {\"passed\": bool, \"error\": null | \"<summary of not-ready services>\",
\"services_not_ready\": [\"<name>\", ...]}"

Agent B prompt (Route Smoke Test — Step 2):
"Run these curl probes and return ONLY a JSON result:
 for path in /api/health /chat /swagger /storybook; do
   curl -s -o /dev/null -w \"%{http_code}\" -L --max-time 10 https://34-120-21-175.sslip.io$path
 done
Return: {\"passed\": bool, \"error\": null | \"<paths that returned non-2xx>\",
\"results\": {\"/path\": <http_code>, ...}}"

Agent C prompt (API Health Check — Step 3):
"Run: curl -s https://34-120-21-175.sslip.io/api/health
Return: {\"passed\": bool, \"error\": null | \"<status code or connection error>\",
\"status_code\": <int>, \"body_preview\": \"<first 200 chars>\"}"

Agent D prompt (Topology Refresh — Step 6):
"Read .claude/skills/post-deploy-verify.md Step 6 only. Run the network topology
update script for project laas-489115. Do NOT run access topology. Do NOT commit.
Return: {\"passed\": bool, \"error\": null | \"<error summary>\",
\"needs_review_count\": <int>}"
```

**Convergence logic:**
```
results = [agent_a, agent_b, agent_c, agent_d]
failures = [r for r in results if not r["passed"]]

if failures:
  → surface all failures in a single summary
  → do NOT proceed to Step 5 (E2E) or Step 7 (Gate Confirmation)
else:
  → proceed to Step 5 inline (E2E writes gate state — must stay sequential)
```

**Time saving:** ~90 seconds vs. sequential verification (Steps 1–3 + topology refresh complete in ~30 seconds in parallel vs. ~2 minutes serialized).

---

## Pattern 2 — Staged Convergence

Stage 1 agents run in parallel. Their outputs are collected by the parent and passed as
context to a Stage 2 inline decision (or Stage 2 agents if those are also independent).

```
  Stage 1 (parallel)
   Agent A: concern-1 ──┐
   Agent B: concern-2 ──┴──> Parent collects both outputs
                               ↓
                          Stage 2: parent decides based on combined results
```

**LaaS example: Incident Response — Parallel Diagnosis + Rollback Preflight**

After Phase 1 (detect) and before Phase 3 (classify), two concerns are independent:
diagnosing what's broken AND checking whether a rollback is ready to execute.

```
Agent A prompt (Diagnosis):
"Run these route probes and Cloud Run status checks for project laas-489115.
 1) curl each of /api/health, /chat, /swagger, /storybook on https://34-120-21-175.sslip.io
 2) gcloud run services list --project=laas-489115 --region=us-central1
 3) gh run list --workflow=deploy-dev.yml --limit=3
Return: {\"passed\": false (always — this is a diagnostic, not a pass/fail check),
\"error\": null, \"failing_paths\": [\"/path\",...],
\"cloud_run_not_ready\": [\"svc\",...], \"recent_ci_run_status\": \"success|failure|unknown\",
\"likely_category\": \"A|B|C|D|E\"}"

Agent B prompt (Rollback Preflight):
"Read .claude/skills/rollback-deployment.md Step 1 only.
List the last 3 sha-tagged images for the 'chat' service in project laas-489115.
Return: {\"passed\": bool, \"error\": null | \"<error summary>\",
\"current_image\": \"<tag>\", \"previous_image\": \"<tag>\", \"rollback_ready\": bool}"
```

**Convergence (Stage 2 — inline decision):**
```
if agent_a["likely_category"] == "A" and agent_b["rollback_ready"]:
  → Phase 4A: execute rollback using agent_b["previous_image"] immediately
elif agent_a["likely_category"] in ["B", "C"]:
  → Phase 4B: fix-forward (rollback preflight result is irrelevant)
else:
  → Phase 3: classify using agent_a data; decide action
```

**Time saving:** 3–5 minutes — rollback image lookup (~30 sec) overlaps with route probes
(~60 sec) and CI history check (~30 sec). Under incident pressure, those minutes are significant.

**When NOT to use staged convergence for incidents:** If it's unclear whether a deploy
regression is involved (Category B/C/D/E), spawn Agent B is wasteful. Use staged convergence
only when deploy regression is a plausible hypothesis after Phase 1.

---

**LaaS example: Pre-Deploy Parallel Safety Gate**

Before any apply, preflight (infrastructure readiness) and plan review (change risk) are
independent. Both feed the deploy/abort decision.

```
Agent A prompt (Preflight):
"Read .claude/skills/preflight.md. Run check-preflight.ps1 for project laas-489115.
Return: {\"passed\": bool, \"error\": null | \"<summary of failures>\",
\"failures\": [{\"item\": \"<check name>\", \"message\": \"<detail>\"}]}"

Agent B prompt (Plan Review):
"Read .claude/skills/review-plan.md. Parse the terraform plan output for the module
being applied. Return: {\"passed\": bool, \"error\": null,
\"risk_level\": \"low|medium|high|critical\",
\"destructive_resources\": [\"<address>\"], \"iam_changes\": [\"<description>\"],
\"summary\": \"<1-sentence summary of what changes\"}"
```

**Convergence (Stage 2 — inline decision):**
```
if not agent_a["passed"]:
  → BLOCKED: list agent_a["failures"]; do not apply

if agent_b["risk_level"] == "critical":
  → BLOCKED: present agent_b["destructive_resources"]; require explicit confirmation

if agent_b["risk_level"] == "high":
  → WARN: present agent_b["iam_changes"] to user; require approval before apply

if agent_a["passed"] and agent_b["risk_level"] in ["low", "medium"]:
  → PROCEED to apply-infra.md
```

---

## Pattern 3 — Sequential Hand-Off

Agent A runs a bounded task; its complete output is passed explicitly as context to Agent B.
This differs from a plain sequential prompt sequence because each agent has a tightly
limited scope and the hand-off is structured rather than implicit.

```
  Agent A: first-concern ──> structured JSON output
                                 ↓ (parent passes A's output to B's prompt)
                             Agent B: second-concern (reads A's output)
                                 ↓
                             Parent synthesizes
```

**LaaS example: Dual-Review as Staged Agents**

The T and P passes in `dual-review.md` can be literal subagents rather than successive
prompts in one conversation. See `dual-review.md` Staged Agent Path section for the
complete prompt templates.

**Why hand-off over a single sequential prompt:** When T and P are separate agents, any
conflict between their outputs is structurally visible in the parent context and must be
resolved in an explicit synthesis step. A single agent playing both roles has a
cognitive bias toward resolving conflicts mentally before surfacing them.

---

## Pattern 4 — Persona-Calibrated Briefing

The operator agent's current persona shapes the subagent's briefing: priority order, output
depth, and format. This is NOT persona inheritance — subagents don't become the persona.
It's explicit calibration embedded in the subagent's prompt.

```
  Parent (in persona X)
   ├── Agent A: concern-1, briefed with X's calibration block ──┐
   ├── Agent B: concern-2, briefed with X's calibration block ──┼── simultaneous
   └── Agent C: concern-3, briefed with X's calibration block ──┘
         ↓ (converge: results already shaped for persona X's decision model)
   Parent synthesizes in persona X's voice
```

**When to use:** The parent agent has declared a persona AND is spawning a research or
diagnostic subagent. Without this pattern, all subagent results arrive with generic depth
and format — the parent must re-interpret before synthesizing. With calibration, subagents
return pre-shaped output the parent can directly incorporate.

**Key rule:** Never assume the subagent infers persona from context. Always paste the
calibration block explicitly into the prompt.

**Extended result schema:** Add `persona_context` and `hat_context` to the standard JSON:
```json
{
  "passed": true | false,
  "error": null | "<summary>",
  "finding": "<one sentence>",
  "detail": "<expansion if needed>",
  "severity": "blocking | concern | suggestion | strength",
  "persona_context": "<persona name>: <one phrase describing how output was calibrated>",
  "hat_context": "<hat name or 'none'>: <domain priority applied>"
}
```

`hat_context` captures which domain emphasis shaped the subagent's output. If the parent
is wearing a hat, subagents receive hat calibration alongside persona calibration. If no
hat is active, set to `"none: default domain emphasis"`.

### Per-Persona Calibration Blocks

Copy the relevant block verbatim into subagent prompts when the parent is in that persona.

**`incident-commander` calibration block:**
```
REPORTING TO: incident-commander persona.
Priority order: blast radius → immediate mitigation → root cause.
Output calibration: Keep findings to ≤2 sentences per item. Mark severity:blocking for
anything requiring immediate action. Skip background explanations.
```

**`adversary` calibration block:**
```
REPORTING TO: adversary persona.
Priority order: worst-case risks → plausible concerns → minor issues.
Output calibration: Lean toward false positives over false negatives. Flag every risk
you can plausibly argue, even weak ones. Return all findings regardless of likelihood.
```

**`security-auditor` calibration block:**
```
REPORTING TO: security-auditor persona.
Priority order: over-broad permissions → missing rotation → drift from snapshot.
Output calibration: Assume the current permission set is too broad. Flag every binding
that could be narrowed. Surface any gap between GCP state and the recorded snapshot.
```

**`architect` calibration block:**
```
REPORTING TO: architect persona.
Priority order: structural violations → ownership gaps → missing extension points.
Output calibration: Evaluate structural fit, not just correctness. Flag single-responsibility
violations, unclear component ownership, and missing extension points.
```

**`rubber-duck` calibration block:**
```
REPORTING TO: rubber-duck persona.
Priority order: clarity → prerequisite knowledge → jargon density.
Output calibration: Translate technical findings into plain English. Assume the reader
has no prior context. Flag any part that would confuse someone new to the system.
```

### Hat Passing to Subagents

When the parent agent is wearing a hat and spawns a subagent, announce it visibly:

```
Subagent Wear Hat: operator
```

Then include the appropriate hat calibration block (below) in the subagent prompt so the
subagent applies the same domain priority. The announcement makes hat propagation observable
to the human operator.

### Per-Hat Calibration Blocks

Hat calibration blocks stack with persona calibration — persona shapes output format,
hat shapes domain priority. Paste the relevant hat block alongside the persona block when
the parent is wearing a hat.

**`operator` hat calibration block:**
```
HAT CONTEXT: operator — deploy readiness focus.
Domain priority: gates > artifacts > gitops-integrity > topology.
When citing Brain data: lead with deploy-blocking gates, then stale artifacts.
De-prioritize: least-privilege, supply-chain, everything-is-code findings.
```

**`security` hat calibration block:**
```
HAT CONTEXT: security — access control and supply chain focus.
Domain priority: least-privilege > supply-chain > defense-in-depth > gates.
When citing Brain data: lead with unreviewed bindings, then vulnerability counts.
De-prioritize: artifact freshness, gitops-integrity findings.
```

**`architect` hat calibration block:**
```
HAT CONTEXT: architect — structural health focus.
Domain priority: everything-is-code > defense-in-depth > topology > gitops-integrity.
When citing Brain data: lead with governance gaps, then coverage metrics.
De-prioritize: artifact freshness, gate urgency findings.
```

**Combined example — `incident-commander` persona + `operator` hat:**
```
REPORTING TO: incident-commander persona.
Priority order: blast radius → immediate mitigation → root cause.
Output calibration: Keep findings to ≤2 sentences per item. Mark severity:blocking for
anything requiring immediate action. Skip background explanations.

HAT CONTEXT: operator — deploy readiness focus.
Domain priority: gates > artifacts > gitops-integrity > topology.
When citing Brain data: lead with deploy-blocking gates, then stale artifacts.
```

---

**LaaS example: Debug Cloud Run — Parallel Probes Briefed to `incident-commander`**

```
Agent A prompt (Service Status — incident-commander calibrated):
"Run: gcloud run services list --project=laas-489115 --region=us-central1
 --format='table(metadata.name,status.conditions[0].status,status.latestReadyRevisionName)'
REPORTING TO: incident-commander persona.
Priority order: blast radius → immediate mitigation → root cause.
Output calibration: Keep findings to ≤2 sentences. Mark severity:blocking for services
that are not Ready. Skip background explanations.
Return: {\"passed\": bool, \"error\": null | \"<not-ready services>\",
\"services_not_ready\": [\"<name>\"], \"severity\": \"blocking|concern\",
\"persona_context\": \"incident-commander: flagged not-ready services first\"}"
```

---

## How to Pass Skill Context to Spawned Agents

**Method 1 — Reference by path with read instruction (default):**
```
"Read `.claude/skills/preflight.md` Step 1 only. ..."
```
Use when: the skill is short, the subagent should read the current version on disk, and
the parent doesn't need to filter or modify the skill content.

**Method 2 — Inline the relevant section:**
```
"Follow these exact steps: [paste the specific section text]. ..."
```
Use when: only one section of a long skill is relevant, or the parent needs the subagent
to follow a specific version of a step that may differ from what's on disk.

**Method 3 — Pass structured input:**
```
"The preflight result is: {\"passed\": false, \"failures\": [\"SA token expired\"]}.
 Read `.claude/skills/fix-apply-failure.md` and determine the fastest recovery path."
```
Use when: the subagent's task is to interpret or act on the parent's computed output,
not to re-run discovery.

---

## Envelope Protocol Integration

All subagents in this project use the LSP Brains agent protocol. See `agent-protocol.md`
for the full reference. The patterns below are orthogonal to the envelope — they describe
coordination topology, not wire format. Every subagent prompt, regardless of pattern, must
use the standard envelope.

**Step added to every pattern before spawning:**

1. Read the skill manifest for the capability → get `responsibility`, `required_hat`,
   `input_schema`, `output_schema`
2. Build the JSON request envelope (copy `required_hat` into `wear_hat`)
3. Construct the system prompt using the template from `agent-protocol.md`
4. Spawn the subagent

**Step added to every convergence check after collecting results:**

```
for each received response (raw_text, request_id, required_hat):
  1. extract delimited block:
     find "<!-- LSP-ENVELOPE:{request_id} -->" ... "<!-- /LSP-ENVELOPE:{request_id} -->"
     → not found → retry once (name the missing block), then abort
  2. parse_json(block_content) — fail → retry once, then abort
  3. check envelope fields — fail → retry once, then abort
  4. check data required fields — fail → retry once, then abort
  5. check worn_hat == required_hat — fail → retry once, then abort
  6. append outcome to .claude/brain/subagent-outcomes.jsonl
  7. accepted → proceed to convergence
```

Narrative outside the delimited block is expected and not retried.
One retry per violation type. After two failures, record SCHEMA_MISMATCH in
proposal-ledger and treat that agent as having returned `"status": "error"`.

**Envelope convergence check (replaces simple `passed` check):**
```python
results = [agent_a, agent_b, agent_c]
failures = [r for r in results if r.get("status") != "ok"]
if failures:
    error_summary = "\n".join(
        f"- [{r['capability']}] {r.get('metadata', {}).get('error', {}).get('message', 'unknown')}"
        for r in failures
    )
    raise Exception(f"Parallel checks failed:\n{error_summary}")
```

**Pattern 4 update — hat chain via envelope:**
When the parent is wearing a hat and spawns subagents, the envelope carries the hat
explicitly through `wear_hat` in the request and `worn_hat` in the response. The
calibration block in the subagent prompt is still required for domain priority shaping.
The hat chain is: manifest `required_hat` → request `wear_hat` → response `worn_hat`.
Validate `worn_hat == wear_hat` at convergence.

---

## Result Format Conventions

All subagent prompts in this project use the LSP Brains agent protocol envelope.
The parent parses JSON fields; it does not regex-parse prose.

**Minimum valid envelope:**
```json
{
  "protocol": "lsp-brains/agent/1.0",
  "responsibility": "<type>",
  "capability": "<key>",
  "schema_version": "1",
  "request_id": "<id>",
  "status": "ok | partial | error",
  "worn_hat": "<hat or null>",
  "data": { ... },
  "symbols": [],
  "metadata": { "confidence": 0.9, "sources_consulted": [], "warnings": [] }
}
```

`status: "ok"` means the concern completed and the parent can proceed.
`status: "error"` means the concern failed; `metadata.error` has details.
`symbols` is the universal observation layer — always scan it regardless of data schema.

**Legacy bare JSON** (existing patterns in this file that predate the protocol):
Patterns 1–3 examples still show legacy `{ "passed": bool, "error": null }` format for
backward reference with the LaaS domain. New capabilities use the full envelope.

**Example convergence check (new style):**
```python
results = [agent_a, agent_b, agent_c]
failures = [r for r in results if r.get("status") != "ok"]
if failures:
    error_summary = "\n".join(f"- {r['error']}" for r in failures if r.get("error"))
    raise Exception(f"Parallel checks failed:\n{error_summary}")
```

---

## Convergence Failure Handling

**Mode 1 — Subagent returns structured error (`passed: false`):**
Collect all failing agents' `error` fields. Report as a unified failure summary before
taking any further action. Do not silently skip a failed concern because others passed.

**Mode 2 — Subagent times out or returns no output:**
Treat as the most pessimistic interpretation for that concern (preflight timeout = FAIL,
not PASS). Fall back to running that concern inline sequentially. Log that the parallel
path was not used.

**Mode 3 — Subagent returns malformed output (not valid JSON, missing required fields):**
Apply the enforcement model from `agent-protocol.md`: retry once with a specific
correction, then abort. Never assume success from ambiguous output. Record
SCHEMA_MISMATCH in proposal-ledger so conformance rate is tracked over time.

These rules implement **Defense in Depth** from `archived/devops-philosophy.md` at the coordination
layer: a convergence check that gives subagents the benefit of the doubt is a single point
of failure.

---

## Hook System Boundary

Hooks are shell scripts fired by Claude Code tool events (`PreToolUse`, `PostToolUse`).
They cannot call the Agent tool. This is a hard constraint, not a convention.

The correct relationship:
```
Hook → emits observation to Claude's context
  ↓
Agent reads hook output
  ↓
Agent decides whether to spawn subagents
```

Do NOT write hooks that attempt to coordinate subagents, spawn agents, or encode agent
prompts in shell variables. Hook logic belongs in shell; coordination logic belongs in
agents reading skill context.

**Example of correct boundary:** `health-check-after-apply.sh` fires after a successful
apply and polls Cloud Run Ready status. If the agent reads this output and sees 2 of 4
services not Ready, *the agent* may decide to spawn parallel diagnosis subagents for the
failing services. The hook observed and reported; the agent decided and acted.

---

## Pattern 5 — LSP Fan-Out

N independent `Find-*Symbol.ps1` domain queries are bucketed into ≤5 subagents by speed
tier. The parent collects all results, then runs synthesizers (`Find-Brain`,
`Find-SessionContext`) inline. Orchestrators never go in a bucket.

```
  Parent
   ├── Bucket 1: Tier A tools (GateSymbol + ArtifactSymbol + TFStateSymbol)  ──┐
   ├── Bucket 2: Tier A tools (TopoSymbol + TreeSymbol + WorkflowSymbol)      ──┤
   ├── Bucket 3: Tier A + B   (SkillSymbol + ShellSymbol + TFSymbol)          ──┼── simultaneous
   ├── Bucket 4: Tier B + C   (Symbol + PySymbol)                             ──┤
   └── Bucket 5: Tier C × 2  (TSSymbol + SCASymbol)                          ──┘
         ↓ (converge: all domain results collected)
   Parent: Find-Brain.ps1 -Mode score -Plain          (inline)
   Parent: Find-SessionContext.ps1 -Action <mode> -Plain  (inline)
```

**LaaS example — Bucket 1 subagent prompt:**
```
You are an lsp-reader subagent. Read-only LSP queries only.
Do NOT edit files. Do NOT call Find-Brain or Find-SessionContext. Pass -Plain to every command.
Run:
  pwsh -NonInteractive -File scripts/dev/Find-GateSymbol.ps1 -Check -Plain
  pwsh -NonInteractive -File scripts/dev/Find-ArtifactSymbol.ps1 -Check -Plain
  pwsh -NonInteractive -File scripts/dev/Find-TFStateSymbol.ps1 -Check -Plain
Return: {"passed": bool, "error": null|"<summary>",
  "results": {"gates": {"exit_code": 0, "output": "..."},
              "artifacts": {"exit_code": 0, "output": "..."},
              "tf_state": {"exit_code": 0, "output": "..."}}}
```
(Buckets 2–5 follow the same template with their assigned tools.)

**When to use:** Delegation only pays when at least one slow tool (Tier C: PySymbol,
TSSymbol, SCASymbol) is in the batch, OR when 5+ queries are needed. All-Tier-A batches
under 5 tools are faster inline (serial ~800ms < subagent spawn ~5–15s).

**Time saving:** ~20–25s vs. ~40–50s serial for the full 11-tool health scan. Critical
path is Bucket 5 (~15–25s); parallelism caps total time at the slowest bucket.

**Companion hook:** `suggest-lsp-subagents.sh` nudges after 3 sequential direct
`Find-*Symbol.ps1` calls, pointing to `archived/lsp-subagent-queries.md`.

See `archived/lsp-subagent-queries.md` for bucketing rules, full prompt template, and convergence logic.

---

## Pattern 6 — Human-Facing Output (Communication Interface)

The human user is a consumer of the same feedback loop as other agents. The difference
is a single interpretation step: the human brain reads, decides, and responds. Communication
to humans should be designed like any other interface — ask "what are the most important
things for the consumer to know?" then deliver that with maximum information density and
minimum prose.

```
  Agent (fast cycle)                    Human (interpretation step)
  ┌─────────────────┐                  ┌─────────────────┐
  │ WORK             │                  │ READ             │
  │ execute tasks,   │                  │ links, diffs,    │
  │ run gates        │                  │ scores — ~10s    │
  │       ↓          │  rich context    │       ↓          │
  │ DISTILL          │──minimal lang──→ │ DECIDE           │
  │ what changed?    │  links first     │ approve/redirect │
  │ what matters?    │                  │ ask/trust        │
  └─────────────────┘                  └─────────────────┘
          ↑                                     │
          └──── intent, constraints, ────────────┘
                corrections
```

### The Distillation Rule

Before composing any message to the user, apply this filter:

1. **What changed?** — Links to PRs, commits, file paths (`file:line`)
2. **What matters?** — Test results, gate status, risk signals (1-2 lines)
3. **What needs a decision?** — Only if the agent is genuinely blocked
4. **Everything else** — Omit. The user can follow links for depth.

### Output Density Patterns

**Status update (after completing work):**
```
Tests: 145/145 passing
PR: owner/repo#63
Changed: correlation.ps1 (+660), modes-display.ps1 (+126), 5 more files
Risk: low — no scoring or gate interface changes
```

**Decision request (when blocked):**
```
Gateway apply will delete 2 URL map entries: /docs/*, /storybook/*
These routes serve live traffic. Proceed? [plan output: terraform/gateway/plan.txt:42]
```

**Milestone (after a phase completes):**
```
Stage 1 complete — 4 epics, 15 stories, 145 tests
Honest Scoring | Diagnostic Reasoner | Learning Brain | Hats
Next: Stage 2 (multi-project awareness) per ROADMAP.md
PR: owner/repo#63
```

### The Same Pattern at Every Layer

| Consumer | Interface | Distillation question |
|----------|-----------|----------------------|
| Agent (subagent → parent) | JSON with `passed`, `error`, task-specific fields | "What does the parent need to decide?" |
| Agent (parent → subagent) | Precise 3-5 sentence prompt | "What does this agent need to act?" |
| Human (agent → user) | Links + status + decisions needed | "What does the user need to know?" |
| Human (user → agent) | Intent, constraints, corrections | "What does the agent need to proceed?" |

Communication is an interface implementation. The format changes (JSON vs prose), the
distillation question is the same. See `VISION.md` Design Principle #8.

---

## Why This Matters

This skill implements **Fail Fast / Shift Left** from `archived/devops-philosophy.md`. Parallelism
in verification isn't about speed for its own sake — it's about closing the feedback loop
after a deploy before the next decision point arrives. A post-deploy check that serializes
4 independent probes takes 2–4 minutes; the same probes run in parallel take 30–60 seconds.
Under incident conditions, those minutes are the gap between a fast rollback decision and a
delayed one. The Platform Migration Test: on any platform, the principle "verify all
independent concerns simultaneously" survives — only the Agent tool invocation syntax changes.

---

## Troubleshooting

**Problem: Subagent returns partial output or stops mid-task**
- Cause: prompt scope was too broad; subagent hit a decision point requiring judgment
- Fix: narrow the prompt to a single bounded concern; add to the prompt: "if you encounter
  any ambiguity or need to make a judgment call, return `{\"passed\": false, \"error\":
  \"ambiguity: <description>\"}` rather than asking a question or continuing"

**Problem: Parallel results arrive in different orders**
- Cause: normal async behavior when spawning multiple agents in one message
- Fix: always key results by concern name (e.g., `{preflight: ..., plan_review: ...}`),
  never process results by list index

**Problem: Subagent reads a stale skill version from disk**
- Cause: skill was updated between when parent read it and when subagent reads it
- Fix: use Method 2 (inline the relevant section) for time-sensitive coordination where
  consistency between parent and subagent interpretation matters

---

## See Also

- `dual-review.md` — T+P review as Sequential Hand-Off (Pattern 3): Staged Agent Path section
- `post-deploy-verify.md` — Parallel Fan-Out (Pattern 1) example: Parallel Execution section
- `incident-response.md` — Staged Convergence (Pattern 2): Parallel Diagnosis Option section
- `archived/skill-chain.md` — Chains 14, 15, 16 show parallel chain notation
- `archived/hooks-reference.md` — hook system boundary documentation
- `archived/devops-philosophy.md` — Fail Fast / Shift Left and Defense in Depth principles
- `archived/lsp-subagent-queries.md` — Pattern 5 (LSP Fan-Out) full reference: bucketing rules, prompt template, convergence logic

Companion hook for Pattern 5: `suggest-lsp-subagents.sh` — fires after 3+ direct
Find-*Symbol calls in a session; nudges agent to use Pattern 5 delegation.
