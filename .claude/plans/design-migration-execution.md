# Design system migration — execution plan (NeuroGrim ← neurogrim-ide)

> **Status (2026-05-09):** plan only. Scaffolded under the v5-native + governance plan but **NOT YET EXECUTED**. The source content still lives at `D:/local-pc-operational-management/children/neurogrim-ide/design/`. This plan is the operator's runbook when ready to execute.
>
> **Hat for execution:** critic / adversary. Each step has a tripwire — verify before proceeding to the next.

## Context

The `neurogrim-design-system` skill + `design/` tree + `docs/design-loop-domain.md` methodology doc currently live at the IDE child Brain (`neurogrim-ide/`). The 2026-05-02 local-canonical pivot landed them there because the IDE was the only live consumer at the time. As of 2026-05-09 the design system needs to widen brand-wide (IDE + dashboard + CLI + future surfaces), which means moving canonical authorship into NeuroGrim itself. The IDE becomes a CONSUMER of the design system, not its home.

**Already done in the v5 + governance plan (2026-05-09):**
- ✅ Scaffolding README at `D:/Brains/NeuroGrim/design/README.md`
- ✅ Vendoring script stubs at `D:/Brains/NeuroGrim/design/scripts/pull-design-tokens.{ps1,sh}`
- ✅ This execution plan (you are reading it)
- ✅ IDE `design-loop-cmdb.json` updated with `meta.migration_status` field

## 8-step execution sequence

Each step is an atomic commit boundary in NeuroGrim's repo. Steps 2–6 also commit in the IDE repo. Run with the `plan-critic` skill on if any step has a question.

### Step 1 — Snapshot for trace (no-op, history-based)

The neurogrim-ide repo's git history already preserves the pre-migration state (commit `58260fd` and earlier). No physical archive step required.

**Tripwire:** before step 2, run `git log --oneline neurogrim-ide -- design/ | head -5` and note the most recent commit SHA. If the migration ever needs to be reverted, that SHA is the rollback target.

### Step 2 — Move `design/` tree into NeuroGrim

```powershell
# In NeuroGrim repo:
cd D:\Brains\NeuroGrim
mkdir -p design  # already exists from scaffolding step 3.2

# Copy the live tree from the IDE.
Copy-Item -Path 'D:\local-pc-operational-management\children\neurogrim-ide\design\system'    -Destination .\design\ -Recurse
Copy-Item -Path 'D:\local-pc-operational-management\children\neurogrim-ide\design\ide'       -Destination .\design\consumers\ -Recurse
Copy-Item -Path 'D:\local-pc-operational-management\children\neurogrim-ide\design\dashboard' -Destination .\design\consumers\ -Recurse
Copy-Item -Path 'D:\local-pc-operational-management\children\neurogrim-ide\design\cli'       -Destination .\design\consumers\ -Recurse
Copy-Item -Path 'D:\local-pc-operational-management\children\neurogrim-ide\design\claude-code' -Destination .\design\consumers\ -Recurse
```

**Tripwire — before commit:**
- Verify byte-identical content for `system/colors_and_type.css` (the highest-stakes file). `(Get-FileHash <ide> -Algorithm SHA256).Hash` must equal `(Get-FileHash <new-home> -Algorithm SHA256).Hash`.
- Verify the directory structure in `design/consumers/` has all four sub-trees (ide, dashboard, cli, claude-code).
- Do NOT delete the IDE-side tree yet. After step 4, the IDE will read from the new home; only then is the IDE-side `design/` redundant.

### Step 3 — Restructure to `consumers/` shape

Already done as part of step 2's copy. Verify:

```
design/
├── system/
│   ├── colors_and_type.css
│   ├── components.css
│   ├── UI Kit · Quiet Workbench.html
│   └── preview/
└── consumers/
    ├── ide/        (was design/ide/)
    ├── dashboard/  (was design/dashboard/)
    ├── cli/        (was design/cli/)
    └── claude-code/(was design/claude-code/)
```

**Tripwire:** the `system/` README and skill references (e.g., links from skill §1 to `colors_and_type.css`) MUST resolve. Since the SKILL.md is moving in step 6, this step's verification is provisional — full resolution lives at end of step 6.

### Step 4 — IDE vendoring path (replace canonical with vendored copy)

The IDE's `src/styles/tokens.css` and `src/styles/base.css` need to become vendored copies (or imports) sourced from `D:/Brains/NeuroGrim/design/system/`, not the IDE-side `design/system/`.

Three approaches; pick one:

**(a) Direct vendoring via script** (v1 — use this initially):
1. Author the actual logic in `D:/Brains/NeuroGrim/design/scripts/pull-design-tokens.{ps1,sh}` (currently stubs). Body: `Copy-Item D:/Brains/NeuroGrim/design/system/colors_and_type.css → <consumer-root>/src/styles/tokens.css`.
2. Run the script with `--consumer ide --consumer-root D:/local-pc-operational-management/children/neurogrim-ide`.
3. Add a pre-build hook (`package.json` `predev` / `prebuild`) so the IDE re-pulls on every dev / build invocation.

**(b) Symlink** (cleaner but Windows-fragile):
- `New-Item -ItemType SymbolicLink -Path '<ide>/src/styles/tokens.css' -Value 'D:/Brains/NeuroGrim/design/system/colors_and_type.css'`
- Cons: Windows symlinks need Developer Mode or admin; git treats them as inert text files unless `core.symlinks=true`.

**(c) `neurogrim-design-tokens` crate / npm package** (long-term):
- Out of scope for v1; cite as future follow-up in `docs/design-loop.md`.

**Tripwire:** after running the script, the IDE's tokens.css must byte-identical to `system/colors_and_type.css`. `npm run dev` must still launch and render correctly.

### Step 5 — Move methodology doc

```powershell
git mv D:\local-pc-operational-management\children\neurogrim-ide\docs\design-loop-domain.md `
       D:\Brains\NeuroGrim\docs\design-loop.md
```

(Two-repo move — actually a copy + delete + commits in both.)

Update the `_purpose` field of the IDE's `design-loop` domain entry in `.claude/brain-registry.json` to point at the new path. Update the IDE's `design-loop-cmdb.json#meta.methodology_doc_at` if such a field exists.

**Tripwire:** grep for cross-references to `docs/design-loop-domain.md` across both repos:

```powershell
Select-String -Path 'D:\Brains\**\*.md','D:\local-pc-operational-management\children\neurogrim-ide\**\*.md' -Pattern 'design-loop-domain.md'
```

Each match must be updated to `D:/Brains/NeuroGrim/docs/design-loop.md` or to a relative path that resolves correctly from its file.

### Step 6 — Move `neurogrim-design-system` skill

```powershell
Copy-Item -Recurse -Path 'D:\local-pc-operational-management\children\neurogrim-ide\.claude\skills\neurogrim-design-system' `
                   -Destination 'D:\Brains\NeuroGrim\.claude\skills\'
```

**Adapt the skill's §0 paths.** The current SKILL.md hard-codes `design/system/` relative to `D:\local-pc-operational-management\children\neurogrim-ide\`. After the move, paths should reference `D:\Brains\NeuroGrim\design\system\` (the new canonical home) and the IDE consumer binding stays at `consumers/ide/`.

**Decision point:** should the skill remain in BOTH locations (NeuroGrim canonical + IDE local copy) or only at NeuroGrim? Per Phase 1 critic-lens findings: only at NeuroGrim. Adopters that want it `cp -r` from there. Remove the IDE-side copy; the IDE's Brain inherits the parent NeuroGrim Brain's skills via Claude Code's plugin discovery.

**Tripwire:** the skill's §1 file references (`colors_and_type.css`, `components.css`, `UI Kit · Quiet Workbench.html`) must resolve from `D:/Brains/NeuroGrim/design/system/` post-move.

### Step 7 — Activate dashboard + CLI bindings (operator-driven)

Currently `consumers/dashboard/` and `consumers/cli/` are "primitives-staged" per the IDE's design-loop CMDB — bridge artifacts authored but not applied to the live consumer codebases.

**Operator decision points:**
- **Dashboard** (`D:/Brains/NeuroGrim/neurogrim/crates/neurogrim-dashboard/frontend/`): copy `consumers/dashboard/tokens-bridge.css` into the dashboard's `index.css` `:root` block; replace shadcn navy palette. Apply Tailwind extension. This is a separate-repo edit — pause here unless the operator explicitly approves.
- **CLI** (`D:/Brains/NeuroGrim/neurogrim/crates/neurogrim-cli/`): vendor `consumers/cli/colors.toml` into a `crate::ui` module. Wire ANSI helpers around it. Same separate-repo posture.

**Tripwire:** activation lands in dashboard / CLI commits with `meta.consumers.<id>.binding_status_history` updated in the IDE-side CMDB (or the new ecosystem-level CMDB if the loop's authoritative state moves out of the IDE Brain).

### Step 8 — Document the binding contract for future surfaces

Add a "Binding Contract for New Consumers" section to `D:/Brains/NeuroGrim/docs/design-loop.md` (the moved methodology doc) detailing:

- Per-consumer dir structure (`consumers/<surface>/README.md`, bridge file, additions file, preview)
- Direction-of-flow rule (read-only consumers, never write back to system/)
- Vendoring path (script invocation, frequency, when to re-pull)
- Wrapper-viewer route convention (`localhost:1420/?showcase=design/<surface>` for the IDE's preview surface)
- Drift-detection workflow (Workflow F adapted to the new layout)

**Tripwire:** the new section must include at least three worked examples — IDE, dashboard, CLI — so a future surface author has a template instead of a guess.

## Post-execution housekeeping

After all 8 steps land:

- Delete `D:/local-pc-operational-management/children/neurogrim-ide/design/` (the source-of-truth has moved; IDE-side dir is now redundant)
- Update IDE CLAUDE.md to point at NeuroGrim as the design-system home
- Update IDE design-loop-cmdb.json `meta.migration_status` from `scaffolded_2026_05_09: true` to `executed_<date>: true`
- Run `/validate-ide-behaviors` from the IDE — should still pass at the new step count

## Risk register

| Risk | Likelihood | Mitigation |
|---|---|---|
| `tweaks_applied` ledger in IDE design-loop-cmdb is lost during the move | Low | Append-only; never delete entries. Step 5 only copies the methodology doc; CMDB stays at the IDE side as historical record. |
| Dashboard / CLI binding activation breaks live UI | Medium | Step 7 is opt-in operator-driven. Defer if Live UI smoke-test fails. |
| The IDE consumer's `pull-design-tokens.ps1` invocation hard-codes a stale source path | Medium | Script accepts `--consumer-root` arg explicitly. Bake source path into the script body, not into consumer call sites. |
| Cross-references to `docs/design-loop-domain.md` go unfixed | Medium | Step 5's tripwire greps both repos; failure to resolve any match blocks step 6. |
| Skill is removed from IDE before NeuroGrim copy is verified working | Low | Copy first, verify resolution from new home, THEN delete IDE-side copy. Don't `git mv`. |
| Direction-of-flow rule violated by a future agent (write back to system/ from consumers/) | Medium | README at NeuroGrim's `design/` explicitly documents the rule. Skill §6 reiterates it. Reviewer hat catches it on PRs. |

## What this plan does NOT do

- Doesn't promote any IDE advisory domain to weighted (separate concern)
- Doesn't fold the IDE source into NeuroGrim's mono-crate (per IDE ONBOARDING-FOR-NEUROGRIM.md "explicitly NOT in scope" — IDE stays a separate workspace)
- Doesn't replace the local-canonical loop with a Claude-Design-driven loop (the 2026-05-02 pivot stands)
- Doesn't bidirectionally sync (consumers READ; the loop is one-way)
