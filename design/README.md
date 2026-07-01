---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# NeuroGrim Design System

> **Methodology entry point.** Brand-wide design system shipping with NeuroGrim — the canonical source for visual language across the IDE, dashboard, CLI, and future surfaces.
>
> **Status (2026-05-09):** This README is a **scaffolding stub**. The actual design system content (`design/system/`, `design/consumers/{ide,dashboard,cli}/`, methodology doc, `neurogrim-design-system` skill) currently lives at `D:/local-pc-operational-management/children/neurogrim-ide/design/` and will move here per the 8-step plan at `.claude/plans/design-migration-execution.md`. **Files have not been moved yet.** Until the migration executes, `neurogrim-ide/design/` is the live canonical source.

## What this directory will contain (post-migration)

```
neurogrim/design/                              ← canonical home (this dir)
├── system/                                    ← brand-wide tokens, components, voice rules
│   ├── colors_and_type.css                    ← every color + type value (276° aubergine ramp)
│   ├── components.css                         ← `.ng-btn`, `.ng-card`, `.mission-strip`, etc.
│   ├── UI Kit · Quiet Workbench.html          ← canonical assembled surface
│   ├── preview/<name>.html                    ← per-pattern preview cards
│   └── README.md                              ← design system handbook (operator-facing)
├── consumers/                                 ← per-surface bindings (read-only consumers)
│   ├── ide/                                   ← Tauri + SolidJS + CodeMirror binding
│   ├── dashboard/                             ← React + Tailwind + shadcn/ui bridge
│   ├── cli/                                   ← Rust ANSI 24-bit truecolor binding
│   └── …                                      ← future: web docs, marketing, ecosystem-dashboard
├── scripts/                                   ← vendoring tools (pull tokens to consumers)
│   ├── pull-design-tokens.ps1                 ← PowerShell port
│   └── pull-design-tokens.sh                  ← bash port
└── README.md                                  ← this file
```

The methodology doc currently at `neurogrim-ide/docs/design-loop-domain.md` will move to `D:/Brains/NeuroGrim/docs/design-loop.md`. The `neurogrim-design-system` skill at `neurogrim-ide/.claude/skills/neurogrim-design-system/SKILL.md` will move to `D:/Brains/NeuroGrim/.claude/skills/neurogrim-design-system/SKILL.md`.

## The non-negotiables (carry-forward, codified in the skill §2)

Five identity statements that travel with the migration. Don't drift; don't relax.

1. **Aubergine pigment.** 276° HSL ramp for backgrounds — deep aubergine ink → dusty rose. NEVER navy (222°). NEVER light mode. The 5-stop background tier sits at saturation ≤22% and reads as patina, not "colored UI."
2. **Color = signal.** Blue = primary action; Purple = agent; Green = pass; Amber = warn; Red = fail; Teal = primary CTA in chrome. Decorative color that doesn't carry signal is forbidden.
3. **Type ramp + motif utilities.** `--display-1` through `--label`. Component primitives: `.ng-btn[--primary|--ghost]`, `.ng-card`, `.ng-kbd`. Status motifs: `.ng-uppercase-label`, `.ng-pill`, `.ng-num`. Every consumer ships these in their native shape (CSS classes for web, ANSI helpers for CLI, struct constants for embedded Rust contexts).
4. **Pretext for editor text (IDE-specific binding).** The IDE uses `@chenglou/pretext` for CodeMirror text rendering. This stays in `consumers/ide/`, NEVER in `system/` — it's an IDE binding detail, not brand-wide.
5. **Send / Submit / Looks-good / Needs-work clicks remain prohibited** in any Claude Design surface even if loosened elsewhere. Account-lifecycle clicks (Reload, Edit, Draw, Present, Share, Profile, "Use this system", "New design") are also off-limits for agent-driven automation.

## Per-consumer binding contract

Every UI surface NeuroGrim ships pulls tokens from `system/` and emits them in the shape that surface expects. Three patterns ship today; future surfaces follow one of them.

| Consumer | Binding pattern | Source-of-truth path |
|---|---|---|
| **IDE** (Tauri + SolidJS + CodeMirror) | CSS variables in `src/styles/tokens.css` + `base.css`; CodeMirror theme in `src/lib/codemirror.ts`. Vendored from `system/colors_and_type.css` via `scripts/pull-design-tokens.{ps1,sh}`. | `consumers/ide/` |
| **Dashboard** (React + Tailwind + shadcn/ui) | `tokens-bridge.css` remaps shadcn vars onto NeuroGrim tokens. `additions.css` ships dashboard-specific primitives (`.ng-table`, `.ng-filter-pill`, `.ng-chart-legend`). Tailwind extension at `tailwind-extension.md` adds `ng-bg-*` / `ng-text-*` / `ng-accent-*` utilities. | `consumers/dashboard/` |
| **CLI** (Rust ANSI 24-bit truecolor) | `colors.toml` vendored at compile time + `glyphs.md` (●/◐/·/○/◎/✓/⊘/✗) + `voice-style-guide.md` (hat-locked narration). | `consumers/cli/` |
| **Future surfaces** (web docs, marketing site, ecosystem-dashboard, Slack-bot output, etc.) | Adapt one of the three above. Document in a new `consumers/<surface>/README.md`. | TBD per surface |

The same primitives ship to every consumer in the format that consumer expects: CSS variables for web/IDE, tokens-bridge for shadcn-based stacks, TOML + glyph tables for ANSI surfaces, plain `&[(name, r, g, b)]` constants for embedded Rust contexts.

## Direction of flow

**Consumers READ tokens. Consumers DO NOT write back.** Bidirectional sync is out of scope. If the operator wants to tweak a brand-wide token, they tweak it in `system/` directly; consumers re-pull on next vendor invocation.

If a consumer needs a surface-specific token (IDE layout geometry like `--mission-strip-h`, LiquidCanvas physics like `--mat-*`), it goes in `consumers/<surface>/`, NEVER in `system/`.

## Migration tracking

Active execution plan: [`D:/Brains/NeuroGrim/.claude/plans/design-migration-execution.md`](../.claude/plans/design-migration-execution.md). Pre-migration trace: this README's `_status` line + the IDE's `design-loop-cmdb.json#meta.migration_status` field.
