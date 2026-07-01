---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# B-10 Phase 1 Analysis — 2026-04-22

> ## ⚠ CORRECTION BANNER (added 2026-04-22, same day) — READ FIRST
>
> **The verdict below ("proceed to Phase 2") is INVALIDATED.**
> Same-day `claude-code-guide` verification (after this analysis
> was published) confirmed that Claude Code **lazy-loads skill
> bodies on demand**, not pre-loaded at session start. Only
> names + descriptions (1,536-char budget each) are in the index.
>
> This analysis tokenized every `.md` file under `.claude/skills/`
> — it was measuring **disk cost**, not **context cost**. The
> "53,170-token worst-Brain cold-start" does not exist in the
> actual session baseline; it describes how many tokens are ON
> DISK in the skill files, not how many are INJECTED into the
> system prompt at session start.
>
> **Consequences:**
> - B-10 is PARKED. See `roadmap/BACKLOG.md` B-10 entry.
> - Phase 2 (approach selection) + Phase 3 (prototype) are
>   cancelled.
> - B-11 contracted to drift-detection-only; the "93% of the
>   overhead is duplication" finding is also a measurement of
>   disk, not context.
> - B-12 contracted to authoring-standard + `capability-hygiene`
>   domain (description quality still matters as the routing
>   contract).
> - S11 stub will not activate as a Stage.
>
> **This document is preserved for historical record.** Its
> methodology was sound; the interpretation was wrong. Do NOT
> act on the numbers in the verdict, thresholds, or "headline
> findings" sections below as if they measured per-session
> context cost. The correct framing: this is a corpus-size
> snapshot of the skill directory as it stood on 2026-04-22.

## Verdict: **proceed to Phase 2**

Both proceed-criteria fire independently — this is not a close call.

| Criterion | Threshold | Measured | Status |
|---|---|---|---|
| Worst-Brain cold-start | ≥ 20k tokens | **53,170** | ✅ FIRE |
| Four-Brain duplicated waste | ≥ 5k tokens | **49,730** | ✅ FIRE |
| Parking threshold | ≤ 8k worst-case | 53k | Not applicable |

No ambiguous-zone secondary measurement is needed.

## Headline findings

- **Worst Brain: ecosystem** at 53,170 tokens (19 skills + CLAUDE.md).
- **NeuroGrim: 50,467 tokens** — essentially tied with ecosystem
  (same ~19 skills + its own CLAUDE.md + 983 tokens of MCP
  BrainServer schemas).
- **LSP-Brains: 6,089 tokens** — small, because the spec repo only
  carries 2 skills.
- **python-starter: 860 tokens** — currently has zero skill files;
  just CLAUDE.md. (The earlier Explore agent's note of "4 skills"
  was outdated.)

## The surprise: duplication dominates the cost

**49,730 of the 53,170 worst-Brain tokens — ~93% — is cross-Brain
duplication.** The ecosystem and NeuroGrim both carry essentially the
same 15+ skill files byte-identically (`subagent-patterns.md`,
`hats.md`, `plan-critic.md`, etc.). Two skills
(`refine-judge-integrity.md`, `rubber-duck.md`) are triplicated
across ecosystem + NeuroGrim + LSP-Brains.

| Duplicated skill | Tokens each | Copies | Waste |
|---|---|---|---|
| `refine-judge-integrity.md` | 3,814 | 3 | 7,628 |
| `subagent-patterns.md` | 7,208 | 2 | 7,208 |
| `write-skill.md` | 3,459 | 2 | 3,459 |
| `plan-critic.md` | 3,209 | 2 | 3,209 |
| `hats.md` | 3,005 | 2 | 3,005 |
| `pilot-protocol.md` | 3,975 | 2 | 3,975 |
| `dual-review.md` | 2,833 | 2 | 2,833 |
| …and 10 more | | | 14,413 |

## Plan-critic implication

This reshapes the epic priorities. B-11 (cross-Brain skill dedup)
was framed in the plan as "a separate concern" out of scope for
CapProto. The data says the opposite:

**~93% of the token overhead B-10 sets out to solve is pure
duplication, not fundamental skill-catalog size.** Dedup alone —
without any lazy-loading protocol, without CapProto — would take
worst-Brain cold-start from 53k down to ~3.4k (the floor from a
single unique skill corpus + CLAUDE.md).

**Updated sequencing recommendation:**
1. **Promote B-11 out of backlog into an active mini-epic NOW.**
   It's the highest-ROI intervention and is architecturally
   independent of B-10 / S11. Expected savings: ~90% of the
   measured overhead.
2. **Re-run Phase 1 after B-11 lands.** The new worst-Brain
   cold-start will determine whether B-10 still meets its
   proceed-criteria. If B-11 cuts it below 8k, B-10 parks.
3. **Phase 2 (approach selection) starts only if post-dedup
   numbers still justify it.** The decision-criteria stand; we
   just re-measure after the cheapest win is taken.

## Distribution statistics

- Median skill: 2,172 tokens
- p90 skill: 3,975 tokens
- Max skill: 7,208 tokens (`subagent-patterns.md`)
- Unique skills across all four Brains: ~21
- Total unique-skill corpus: ~53,000 tokens (before accounting
  for overlap between `refine-judge-integrity.md` and
  `rubber-duck.md` being genuinely needed in all three)

## Secondary observation: CLAUDE.md tables are stale

Ecosystem's `CLAUDE.md` advertises 2 skills via its Skills table,
but the directory contains 19. NeuroGrim's advertises 22; the
directory contains 19 — reasonably close. LSP-Brains is accurate at
2 vs 2 (plus `refine-judge-integrity.md` unlisted).

The stale-index smell suggests: CLAUDE.md's hand-maintained skill
table drifts from the filesystem. Either the tables should be
generated, or a `skill-hygiene` domain should flag drift.

Filed as an implicit B-11 sub-concern; will surface in the B-11
write-up.

## Pre-committed Phase 3 go/no-go (only relevant if Phase 2 proceeds
post-dedup re-measurement)

From BACKLOG.md:
- Typical-session delta ≥ 5k tokens saved
- Worst-case latency ≤ 300ms per lazy-load
- No stale-cache bug in 2-week dogfood

These criteria remain unchanged. But they apply to a
post-dedup baseline, not today's.

## Next action

Update BACKLOG.md B-10 entry to record the Phase 1 verdict and
defer Phase 2 kickoff pending B-11 execution. Update B-11 to
promote it from backlog-speculative to active.

## Tokenizer caveat

`cl100k_base` is not Claude's tokenizer. Absolute numbers may be
±10-20% off. **Deltas are directionally accurate** — the
"ecosystem has ~10× more skills than LSP-Brains" signal is correct
regardless of which tokenizer is used. For the ambiguous zone
measurements (not triggered here), the re-run would use Anthropic's
token-counting API.

## Reproducibility

```bash
cd neurogrim
cargo test -p neurogrim-cli --test context_overhead -- --nocapture
```

Re-run quarterly, or when any Brain's skill count changes ±20%,
or after B-11 dedup ships.
