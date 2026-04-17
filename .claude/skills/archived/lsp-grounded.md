# LSP-Grounded Workflow Patterns

Use this skill to understand how to work with LSP as the grounding layer — the "how do I use
all these LSP tools together?" meta-guide. Read this when starting on the LSP system, when
a workflow decision isn't obvious from a single tool's output, or when integrating LSP context
into a new skill or hook.

Role: meta
Governs: scripts/dev/Find-SessionContext.ps1, scripts/dev/Find-GateSymbol.ps1, scripts/dev/Find-SkillSymbol.ps1, scripts/dev/Find-WorkflowSymbol.ps1, scripts/dev/Find-TopoSymbol.ps1

Trigger phrases: "lsp grounded", "how do I use lsp", "lsp workflow", "grounding layer",
Domain: brain
Methodology-step: skills
"lsp as basis", "how do the lsp tools work together", "what should I run first",
"lsp decision flow", "lsp philosophy"

---

## The Grounding Loop

Every workflow follows the same LSP-grounded cycle:

```
1. Run Find-SessionContext (-Action <mode>) to establish current state
2. Read the relevant governing skill (skill-context-on-read hook surfaces its gate state)
3. Take the indicated action (run the gate, apply, commit, etc.)
4. Run Find-SessionContext again to see what changed
```

This loop prevents "works on my machine" surprises. The agent always starts from observed
state, not assumed state.

---

## Action Mode Selection

`Find-SessionContext.ps1` has five modes. Choose the one that matches your current intent:

| Situation | Mode | What it shows |
|-----------|------|--------------|
| Session start, morning, after a break | (no flag) | Full picture: all dirty/needs-run gates across all domains |
| About to `git commit` | `-Action commit` | Commit-blocking dirty gates + CHANGE IMPACT (commits since last clean) |
| About to `apply-*.ps1` or `workflow_dispatch` | `-Action deploy` | Deploy-blocking gates + topology at risk + recommended next step |
| About to open or merge a PR | `-Action review` | Merge-blocking gates + corpus health (SkillSymbol) + workflow integrity |
| Debugging a dirty gate or incident | `-Action debug` | All dirty gates + governing skills to consult + topology at risk + debug path |

**Quick rule:** Match the mode to what you're about to do. If you're about to commit, run
`-Action commit`. If something's broken, run `-Action debug`.

---

## Automatic LSP Context (What Hooks Fire)

Some LSP context surfaces automatically — you don't need to ask for it:

| Event | Hook | What fires |
|-------|------|-----------|
| Read a governing skill | `skill-context-on-read.sh` | Gate status for scripts that skill governs |
| Any gate script exits | `gate-completion.sh` | Updated gate health summary + nudge to run SessionContext |
| Any apply-*.ps1 runs | `pre-apply-lsp.sh` | Deploy readiness check from gates.json + topology |
| Edit a .ps1, .tf, .ts, .py | `lsp-on-edit.sh` | Language-specific static analysis (PSScriptAnalyzer, tsc, pyright, tf validate) |
| Edit a skill file | `assess-skill-on-edit.sh` | Quality check + T/P review nudge |

The remainder requires a manual command — primarily when you want a specific domain view or
action mode.

---

## The Governs: Circuit

The `Governs:` field in a skill's frontmatter creates a live connection between skills and
their governed scripts. The circuit works end-to-end:

```
Skill has Governs: scripts/verify/smoke-infra.ps1
  → skill-context-on-read.sh fires when agent reads the skill
  → hook matches smoke-infra.ps1 against smoke:cloud-run gate's run_command
  → hook emits: ✗ smoke:cloud-run  DIRTY
```

**For this to work, the skill needs `Governs:`.** All skills with roles `operational`,
`validation`, `diagnostic`, or `recovery` are expected to have this field. See `write-skill.md`
for the convention.

**Reverse lookup:** `Find-SkillSymbol.ps1 -Governs scripts/verify/smoke-infra.ps1` shows
which skill governs that script. Useful when starting from a file rather than a skill name.

---

## Decision Tree: From SessionContext Output to Action

**"Commit blocked: pester:dev"**
1. Look at "SKILLS TO CONSULT" — it should list `test.md`
2. Read `test.md` (skill-context-on-read hook shows pester:dev status)
3. Run: `pwsh -NonInteractive -File scripts/verify/run-tests.ps1 -Target dev`
4. gate-completion hook fires → updated gate health
5. Re-run: `Find-SessionContext.ps1 -Action commit` to confirm cleared

**"Deploy blocked: smoke:cloud-run"**
1. Look at "SKILLS TO CONSULT" — it should list `smoke-infra.md`
2. Read `smoke-infra.md` (skill-context-on-read shows DIRTY)
3. Run: `pwsh -NonInteractive -File scripts/verify/smoke-infra.ps1 -Module cloud-run -ProjectID $env:LAAS_PROJECT_ID`
4. gate-completion hook fires → updated gate health
5. Re-run: `Find-SessionContext.ps1 -Action deploy` to confirm cleared

**"Corpus issue: 1 skill missing Role: tag"**
1. Run: `Find-SkillSymbol.ps1 -Check` for the full detail
2. Fix the skill (assess-skill-on-edit hook fires after your edit)
3. Re-run: `Find-SessionContext.ps1 -Action review` to confirm clean

**"Topology annotation gap"**
1. Run: `Find-TopoSymbol.ps1 -NeedsReview` for the full list
2. Update annotations in network-topology.json
3. Re-run: `Find-TopoSymbol.ps1 -Check` to confirm 0 gaps

---

## Cross-Domain Query Cheat Sheet

When SessionContext output points to a specific domain, drill deeper with the domain tool:

| SessionContext says | Drill down with |
|--------------------|----------------|
| Commit/deploy blocked by gates | `Find-GateSymbol.ps1 -Dirty` |
| "Consult smoke-infra.md" | `Read .claude/skills/smoke-infra.md` (hook fires with gate state) |
| Topology at risk | `Find-TopoSymbol.ps1 -Criticality existential` |
| Corpus health issue | `Find-SkillSymbol.ps1 -Check` then `-Name <skill>` |
| Workflow broken reference | `Find-WorkflowSymbol.ps1 -Check` |
| Symbol callers before rename | `Find-SkillSymbol.ps1 -Governs <path>` or `Grep` for PS/TF/TS |

---

## Delegation Threshold

When the grounding loop requires multiple domain queries, choose run mode by count:

| Condition | Run mode | Rationale |
|-----------|---------|-----------|
| 1–4 queries, all Tier A (<200ms each) | **Inline** | Serial < subagent spawn overhead (~5–15s) |
| Any batch with ≥1 Tier C tool | **Delegate** at 3+ queries | Tier C costs (pyright, tsc, npm audit) dominate |
| 5+ queries (any mix) | **Delegate** — ≤5 concurrent `lsp-reader` subagents | Bulk volume exceeds spawn overhead |
| `Find-Brain.ps1` | **Always inline** | Synthesizer; reads same CMDBs as subagent results |
| `Find-SessionContext.ps1` | **Always inline** | Synthesizer; run after subagent results arrive |

Read `lsp-subagent-queries.md` for the full delegation protocol: bucketing rules by
speed tier, the `lsp-reader` prompt template, and the 5-bucket worked example.

The `suggest-lsp-subagents.sh` hook nudges after 3 direct `Find-*Symbol.ps1` calls in
a session, pointing to this threshold. The nudge notes that delegation is most effective
when the batch includes at least one slow tool (PySymbol, TSSymbol, SCASymbol) or 5+ total.

---

## Why This Matters

The LSP tools gave the agent eyes. The grounding layer means those eyes are open by default
rather than waiting to be asked. Every time a skill is read, every time a gate runs, every
time an apply is about to start — the agent sees the current state automatically.

This implements **Observability Before Action** from `devops-philosophy.md` at the tooling
level: the observation doesn't require discipline or memory. It happens because the hooks
are always watching.

---

## See Also

- `lsp.md` — full command reference for all 5 Find-* tools
- `session-recap.md` — how to run a full session context at start of day
- `what-next.md` — decision flow from SessionContext output to prioritized action list
- `gate-status.md` — when a gate is DIRTY and the fix isn't obvious
- `lsp-subagent-queries.md` — delegation protocol: when/how to use LSP reader subagents
- `plan-critic.md` — the Symbol Impact Audit phase (find callers before renaming symbols)
- `write-skill.md` — Governs: required for action-role skills
