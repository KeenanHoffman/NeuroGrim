---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: B-09 CLI-Mode Sensory Access (Power-User Alternative to MCP)

**Stage:** Mini-epic (not a full Stage — promoted from backlog B-09)
**Status:** **Complete (2026-04-22)** — all five DPs shipped in one
session alongside B-10 Phase 1 measurement.
**Priority:** Medium (tactical win; opt-in; default stays MCP)

---

## Goal

Ship an opt-in path where Claude Code sessions omit NeuroGrim's
MCP server from `.mcp.json` and invoke the Brain via existing CLI
subcommands. Eliminates the ~983-token tool-schema injection cost
at session start for power users.

## Origin

Promoted from `BACKLOG.md` B-09 during the 2026-04-22 CapProto
planning session (`~/.claude/plans/parallel-hugging-eich.md`).
Surfaced at S10 close-out as a per-session tooling-overhead
concern.

## Planning Decisions (captured before implementation)

- **DP-1 scope revision.** Plan-critic review during exploration
  showed the Rust-flag framing was mis-targeted — `commands::agent::run()`
  is 13 lines with zero MCP coupling, so there is no Rust-level
  flag to add. The real lever is which servers the user enables in
  `.claude/.mcp.json`. DP-1 became a documentation + config-pattern
  story rather than a Rust change.
- **Tool scope.** Only the 7 BrainServer tools
  (`get_health_score`, etc.) are in scope — they are the tools
  injected into Claude Code's context when NeuroGrim MCP is
  registered. The 11 sensory subprocesses
  (`git-health`, `code-quality`, etc.) are internal to the Brain's
  scoring pipeline, NOT registered in Claude Code's MCP config, and
  therefore NOT part of CLI-mode scope.
- **Default posture.** MCP stays the default. CLI mode is
  deliberately opt-in, documented through the `cli-mode.md` skill.

## What Shipped

| Work item | Artifact | Status |
|---|---|---|
| DP-1 | [docs/cli-mode.md](../../docs/cli-mode.md) — `.mcp.json` opt-out pattern + mode-selection rubric. | SHIPPED |
| DP-2 | [docs/cli-sensory-surface.md](../../docs/cli-sensory-surface.md) — 7-tool MCP↔CLI mapping with args, output shapes, gaps. | SHIPPED |
| DP-3 | [.claude/skills/cli-mode/SKILL.md](../../.claude/skills/cli-mode/SKILL.md) — agent-facing skill routing Bash instead of MCP tool calls. | SHIPPED |
| DP-4 | [neurogrim-cli/tests/context_overhead.rs](../../neurogrim/crates/neurogrim-cli/tests/context_overhead.rs) — `tiktoken-rs`-based benchmark; report at `roadmap/data/b09-bench-2026-04-22.json`. | SHIPPED |
| DP-5 | [CLAUDE.md](../../CLAUDE.md) "Tool Invocation Mode" section + [README.md](../../README.md) mode table. | SHIPPED |

## Benchmark Results

`roadmap/data/b09-bench-2026-04-22.json`:

| Mode | Tokens injected | Delta |
|---|---|---|
| MCP | 983 | baseline |
| CLI | 0 | **−983 (100% reduction on this axis)** |

The absolute number is modest — this axis alone does not justify
heroic measures. The value of B-09 is the escape hatch it
provides when stacked with other overhead (multiple MCP servers,
long-running conversations). Operators who hit real context
pressure now have one clean lever to pull.

## Gaps Left Open

- **`get_trajectory(domain=X)`** has no CLI equivalent — documented
  in `cli-sensory-surface.md` §2b. Workaround: parse
  `.domain_trajectories` from `neurogrim agent` output.
- **`record_subagent_outcome`** is MCP-only. Documented in
  `cli-sensory-surface.md` §7.
- Both are tracked implicitly as "if recurring friction, file a
  fast-follow" rather than committed fixes.

## Related Outputs

B-09's benchmark test shares plumbing with **B-10 Phase 1
measurement** (same test file). Phase 1 results:
`roadmap/data/b10-phase1-2026-04-22.json` +
`roadmap/data/b10-phase1-analysis.md`.

## Dependencies

None blocking. Shipped stand-alone.

## Risks (post-ship review)

1. **CLI-mode docs can drift** from CLI behavior if subcommand
   flags change. Mitigation: `cli-sensory-surface.md` references
   the exact subcommands that exist today; when any change, the
   doc's test-harness assertion in `context_overhead.rs`
   (`test_tool_count_is_current`) flags drift at the MCP side, but
   there is no current automated check for the CLI side. Filed
   implicitly: if CLI surface docs rot, fast-follow a
   `test_cli_sensory_surface_is_current` sibling test that exercises
   each documented command.
2. **Schema-validation loss** as called out in the backlog risks.
   Operators are warned in `cli-mode.md` + `cli-sensory-surface.md`.
   Not a hidden risk.
3. **Two code paths to maintain.** Mild — the "CLI path" is
   existing subcommands, no parallel implementation exists. The
   only new artifacts are documentation + one benchmark test.

## References

- Planning document: `~/.claude/plans/parallel-hugging-eich.md`
- Parent backlog item: `roadmap/BACKLOG.md` B-09 (now marked
  complete; pointer to this file)
- Visionary framing (session 2026-04-22): "third protocol vertex —
  MCP, A2A, CapProto" (CapProto captured as S11 stub pending
  B-10 Phase 3 evidence)
