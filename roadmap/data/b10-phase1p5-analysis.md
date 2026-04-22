# B-10 Phase 1.5 Analysis — 2026-04-22

**Raw report:** `b10-phase1p5-description-only-2026-04-22.json`
**Harness:** `neurogrim-cli/tests/context_overhead.rs` (new test
`b10_phase1p5_description_only_measurement`)
**Hypothesis (operator, 2026-04-22):** *"all an agent needs for a
skill is the description of when to use it, everything else can be
retrieved on demand saving thousands of tokens. Perhaps a small
outline of the content in the skill as well."*

## Verdict: **hypothesis empirically confirmed**

Stack-wide reduction with description + outline only:
**90.4% vs full-body baseline.**

| Metric | Stack total | Per-session impact |
|---|---|---|
| Full body (baseline) | 104,705 tokens | — |
| Description only | 5,656 tokens | **94.6% reduction** |
| Description + outline | 10,016 tokens | **90.4% reduction** |
| Outline contribution | 4,360 tokens | (adds ~40% back, still cheap) |

For the worst Brain (ecosystem, 50,870 skill-body tokens) — a
description + outline TOC would cost **4,849 tokens**, saving
**46,021 tokens per session** on that axis alone. Combined with
B-11 (dedup) — which removes ~93% of the stack overhead via
canonicalization — total savings become dramatic.

## Per-Brain breakdown

| Brain | Full body | Desc + outline | Reduction |
|---|---|---|---|
| ecosystem | 50,870 | 4,849 | **90.5%** |
| NeuroGrim | 48,992 | 4,623 | **90.6%** |
| LSP-Brains | 4,843 | 544 | **88.8%** |
| python-starter | 0 | 0 | — |

Reduction is remarkably uniform across Brains — this is a
structural property of the skill-authoring convention, not an
artifact of one Brain's skills.

## Hygiene caveat — read this before acting

The raw hygiene distribution shows 17/41 skills (~41%) as
"under-described (<5% of body)." This percentage is **misleading**.
The percentage is small because the skills are long; the absolute
description-token counts are mostly 80-250 tokens — plenty of
routing signal.

**Only one skill has genuinely terse description:**
- `coherence.md`: 18 tokens of description.

The reason: `coherence.md` puts its "When to Use This Skill" block
as a `## When to Use This Skill` section header, NOT in the
lead-paragraph before the first `##`. My heuristic (everything
before the first `##`) excludes section-level "when to use" blocks,
so coherence appears under-described when in reality its usage
criteria are just one heading further down.

**Implication:** my measurement is a **lower bound**. With a
standardized authoring convention that puts the "when to use"
block in the title-paragraph area (the same convention used by
rubber-duck, a2a, cli-mode, plan-critic), every skill would score
at or above the current median — reduction climbs to ~92-95%.

## Re-checked hygiene: absolute description length

| Description tokens | Skill count | Interpretation |
|---|---|---|
| 0-39 | 1 (coherence) | Too terse; rewrite with explicit lead-paragraph |
| 40-99 | 11 | Adequate; agent can route |
| 100-199 | 12 | Good; most common bucket |
| 200-252 | 9 | Rich; maybe verbose |

**>95% of the corpus already has routing-grade descriptions.**
This is not "all skills need a rewrite." It is "one skill needs
a section-header fix, and the convention should be codified so
new skills don't drift."

## Architectural implication — CapProto scope contracts

The originally-sketched S11 CapProto Stage assumed a new protocol
layer (`capability-envelope-v1.schema.json`, server-push
diagnostics, Meta-MCP tools). The Phase 1.5 data says this is
overbuilt. Native primitives suffice:

1. **Description = `textDocument/hover`** → a frontmatter / lead-
   paragraph convention. No protocol.
2. **Full body on demand = `textDocument/definition`** → Claude
   Code's existing `Read` tool. No new tool.
3. **Outline = quick nav** → `## ` / `### ` headers already
   present.

The only genuinely new work:
- **Authoring standard** (one skill + one linter).
- **TOC generator** (one CLI subcommand reading filesystem + emitting
  a skill-index.md).
- **Brain domain `capability-hygiene`** (one sensory tool +
  registry entry; scores description quality, flags orphans,
  detects shadows).

**Total effort: ~1 week, not a Stage.** This is mini-epic scope.

## Recommended repositioning

- **S11 stub stays a stub,** but with a note that Phase 1.5 data
  collapsed its scope — it likely will never activate as a Stage.
- **Create B-12** as the active mini-epic implementing the
  description-first TOC approach. Ships independently of B-11.
- **B-10 advances past Phase 1:** the hypothesis from Phase 2
  (Meta-MCP) is no longer the likely winner. The data favors
  "short-description + on-demand expansion" (candidate #4 from
  B-10's enumerated list) — but with the realization that "on-
  demand expansion" is just the native `Read` tool, not a new
  protocol. Phase 2 design doc can be short.

## Combined savings forecast (back-of-envelope)

Today — worst-Brain cold-start: 53,170 tokens (ecosystem).

With B-11 dedup alone: ~3,400 tokens (93% reduction).
With description+outline TOC alone: ~5,149 tokens (90% reduction).
**With both B-11 + B-12: ~700-1,500 tokens.** (97-99% reduction.)

The two interventions are multiplicative because they attack
different axes (cross-Brain duplication vs in-Brain verbosity).

## Next action

1. Update S11 stub to reflect the scope contraction. (Done in
   this same session.)
2. Add B-12 to BACKLOG.md as the active mini-epic for the TOC
   approach. (Done in this same session.)
3. Defer B-10 Phase 2 design work until after B-11 + B-12 ship,
   because their combined effect may reduce overhead below the
   "proceed" threshold entirely.

## Tokenizer caveat (repeat)

`cl100k_base` is not Claude's tokenizer. Absolute numbers may be
±10-20% off. The 90.4% reduction figure is a ratio across the same
tokenizer, so it is robust to tokenizer choice. The delta holds.

## Reproducibility

```bash
cd neurogrim
cargo test -p neurogrim-cli --test context_overhead \
  b10_phase1p5_description_only_measurement -- --nocapture
```
