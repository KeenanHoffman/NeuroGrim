# Skill Gap Tracker

Living record of known gaps in the skill library: tasks that come up frequently but have
no skill, skills that are stubs needing expansion, and cross-reference errors.

Role: meta

Trigger phrases: "what skills are missing", "skill gap", "undocumented task", "no skill for",
Methodology-step: skills
"skill needs updating", "add to gap tracker", "skill coverage"

---

## How to Use This File

- **Before writing a new skill** — check here to see if the gap is already tracked
- **When you discover a missing skill** — add it to the appropriate section below
- **During skill assessment** — review this list to find the next most valuable skill to write
- **When closing a gap** — move the item to the Closed section with the date and skill filename

Process for closing a gap:
1. Write the skill (see `write-skill.md`)
2. Move the gap entry from **Open** to **Closed**
3. Update `CLAUDE.md` skills index and `skill-index.md`

---

## Open Gaps

### Missing Skills (no skill file exists)

| Task / Situation | Priority | Notes |
|-----------------|---------|-------|
| Cost estimation from `terraform plan` output | Medium | How to estimate GCP billing before applying; Cloud Run per-request + min-instance cost model |
| ~~Branching strategy / feature branch lifecycle~~ | ~~High~~ | **Closed 2026-04-07** → `start-feature.md` + `check-new-branch-base.sh` + `suggest-pr-on-push.sh` |
| Monitoring & alerts setup | Medium | Cloud Monitoring dashboards, uptime checks, log-based alerts, wiring to gate system |
| Load testing the API | Low | k6 or locust against `/generate` and `/generate/from-prompt`; baseline metrics |
| Architecture decisions log | Low | ADR-style doc: why IAP only on API, why sslip.io, why Nextra, why pnpm workspace |
| ~~LSP integration for agents~~ | ~~High~~ | **Closed 2026-04-07** → `lsp.md` — covers PSScriptAnalyzer symbol search, terraform-ls module inspection, and tsc type-aware navigation; `lsp-on-edit.sh` hook automates at edit time |
| ~~Troubleshooting sections remaining~~ | ~~Medium~~ | **Closed 2026-04-07** — 53/59 skills now have `## Troubleshooting`; 6 exempt (demo, devops-philosophy, skill-chain, skill-index, skill-hook-pairs, verify deprecated stub) |
| ~~Why This Matters sections remaining~~ | ~~Medium~~ | **Closed 2026-04-07** — 53/59 skills now have `## Why This Matters`; 6 exempt (demo, devops-philosophy, skill-chain, skill-deprecation, skill-index, verify deprecated stub) |

### Proposed Skill+Hook Pairs

Pairs where a skill describes automatable or enforceable behavior but no companion hook
exists yet. When a pair is implemented, move it to `skill-hook-pairs.md` implemented table
and remove it from here.

*(No proposed pairs at this time — all implemented pairs are cataloged in `skill-hook-pairs.md`.
When check-10 in `assess-skill-on-edit.sh` fires for a skill, add its proposed pair here.)*

| Skill | Proposed hook | Type | Priority |
|-------|--------------|------|---------|
| Bash hook regression coverage | `pester:hooks` gate or shellcheck CI step | Enforcement | Medium |

---

### Stubs Needing Expansion

| Skill file | What's missing |
|-----------|----------------|
| `verify.md` | Deprecated stub — intentionally redirects to `post-deploy-verify.md`. No action needed. |
| `test.md` | Short — could include more detail on the spy pattern and how to write new Pester tests |
| `retire.md` | Covers teardown sequence but missing: what to clean up in GCS state, how to handle shared singletons |
| `setup.md` | Missing `$env:LAAS_IAP_SUPPORT_EMAIL` setup instructions (referenced in many scripts) |

### Suspected Inaccuracies

| Skill file | Suspected issue | Last verified |
|-----------|----------------|---------------|
| `diagnose-iap.md` | IAP brand/client steps may be stale since fix-iap-state.ps1 was added | Never formally verified |
| `local-proxy.md` | References topology file format that may have changed | 2026-04-01 |
| `deployment-flow.md` | DAG diagram may not reflect new detect-changes + update-services jobs | 2026-04-05 |

### Known Cross-Reference Gaps

| Skill file | References | Status |
|-----------|-----------|--------|
| `playwright-e2e.md` | `add-new-app.md` | ✓ now exists |
| `write-smoke-tests.md` | topology annotation format | Verify format still matches |

---

## Closed Gaps

| Task | Closed | Skill file |
|------|--------|-----------|
| Playwright e2e smoke tests | 2026-04-05 | `playwright-e2e.md` |
| Add a new sub-app end-to-end | 2026-04-06 | `add-new-app.md` |
| Debug a failing Cloud Run service | 2026-04-06 | `debug-cloud-run.md` |
| Roll back a bad deploy | 2026-04-06 | `rollback-deployment.md` |
| Incident response / 3am playbook | 2026-04-06 | `incident-response.md` |
| Post-deploy verification checklist | 2026-04-06 | `post-deploy-verify.md` |
| Secrets / Secret Manager workflow | 2026-04-06 | `secrets-management.md` |
| How to write a skill | 2026-04-06 | `write-skill.md` |
| Session handoff / clean session end | 2026-04-06 | `session-handoff.md` |
| Multi-skill sequence navigation | 2026-04-06 | `skill-chain.md` |
| Full skill discovery / index | 2026-04-06 | `skill-index.md` |
| LSP / symbol search for agents | 2026-04-07 | `lsp.md` |
| Troubleshooting sections across all skills | 2026-04-07 | 53/59 skills (6 meta/deprecated exempt) |
| Why This Matters sections across all skills | 2026-04-07 | 53/59 skills (6 meta/deprecated exempt) |
| Stub expansion: test.md (spy pattern + how-to-write) | 2026-04-07 | `test.md` |
| Stub expansion: retire.md (GCS cleanup + shared singletons) | 2026-04-07 | `retire.md` |

---

## How to Add a Gap

When you notice a missing skill during work, add a row to the appropriate Open table:

```markdown
| Brief task description | High/Medium/Low | Any relevant context, e.g. "comes up every deploy", "referenced in X skill" |
```

Assign priority based on:
- **High** — comes up in every session or blocks common workflows
- **Medium** — comes up occasionally; agent has to improvise without it
- **Low** — niche; agent can figure it out from first principles

---

## Troubleshooting

**Problem: Not sure whether to add a gap as "Missing Skill" or "Suspected Inaccuracy"**
- Missing Skill: no skill file exists at all for the task
- Suspected Inaccuracy: a skill file exists but specific content may be wrong or stale
- Stub Needing Expansion: a skill file exists but is too short to be useful (<300 chars)

**Problem: A gap was added but never gets prioritized**
- Gaps stay open forever if no one reviews this file. Run the Gap Assessment (below) at the
  start of each major work block to pick the highest-priority open gap and write it.
- If a gap has been open for multiple sessions, elevate its priority or note why it's blocked.

---

## Gap Assessment (run periodically)

To identify new gaps systematically, ask:

1. What tasks came up in the last 5 sessions that required reading more than 2 skills to answer?
2. What questions did the user ask that had no clean skill to point to?
3. What steps in any `skill-chain.md` sequence have no backing skill?
4. Are all `CLAUDE.md` skills index entries still accurate?
5. Are all cross-references in existing skills still valid?
