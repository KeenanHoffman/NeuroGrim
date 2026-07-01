---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Judge Calibration Profiles

**Purpose.** Empirical log of how each judge model + thinking-level
combination behaves when applied to the agent-behavior scenarios in
this ecosystem. Each model has its own **calibration profile** —
systematic patterns of how it reads rubrics, where it shows bias,
what dynamic range it produces on extreme vs middle-ground samples.
Treating the judge as a substitutable component misses real
methodology signal; treating each (model, thinking) combination as a
first-class artifact lets us accumulate knowledge as we audit more
cycles and rotate through different judge configurations.

**Scope.** This document is operator-maintained, not generated. Every
audit cycle that uses a new (model, thinking) combination should add
an entry here — or extend an existing one with new evidence. Old
evidence stays; adding new observations doesn't invalidate old ones
unless the model itself was updated (in which case we start a new
profile with the date/version marker).

**How to add a profile.**

1. Run an audit (calibrate + red-mode) with the new (model, thinking)
   combination.
2. Observe the patterns — gold-good scoring, gold-bad scoring,
   middle-ground red handling, systematic bias vs human labels.
3. Add a section below using the template at the bottom of this file.
4. Cross-reference audit artifacts (JSON reports) by path.
5. Commit.

See also:
- `NeuroGrim/docs/domain-promotion-audit.md` — operator audit runbook.
- `LSP-Brains/spec/LSP-BRAINS-SPEC.md` §15.3 — judge protocol.

---

## Profile: claude-haiku-4-5 (no thinking)

**First observed:** 2026-04-22
**Role(s) used:** primary judge (Haiku #1, Haiku #2 audits) +
secondary judge (red-mode sweeps paired with Sonnet adversary)
**Audit evidence:**
- `D:/Brains/.claude/brain/audit/2026-04-22/haiku-1/` (initial run
  pre-remediation; drift-blocker)
- `D:/Brains/.claude/brain/audit/2026-04-22/haiku-1-retry/` (post
  initial remediation; still red-miss, cross-scenario pattern)
- `D:/Brains/.claude/brain/audit/2026-04-22/haiku-1-retry2/` (post
  increments 1-5; cross-scenario still; canary prompt fix here)
- `D:/Brains/.claude/brain/audit/2026-04-22/haiku-1-retry3/` (post
  B-07 + B-08; 1 marginal miss remaining on exec rubric-mimicry)
- `D:/Brains/.claude/brain/audit/2026-04-22/haiku-1-retry4/` (final
  retry post-increment-8; calibrate clean, red-mode 3/23 variance)
- `D:/Brains/.claude/brain/audit/2026-04-22/haiku-2/` (second
  consecutive; calibrate clean, red-mode 1/23 marginal)

### Scoring profile

**Gold-good samples** (human label 85-95): Haiku tends to score at
the top of the rubric's achievable range (95-100). Gold-goods rarely
score below 85 unless the sample has a genuine gap. Consistent with
"rubric-follower with compressed dynamic range at the top."

**Gold-bad samples** (human label 5-25): Haiku scores in the
0-15 range. Our human labels at 18-22 were too generous; Haiku
(correctly, per rubric) scored them at literal zero on
substance-zero responses. Re-labeling gold-bad human scores
downward (v2-v3 remediation increments) was necessary for
calibration.

**Middle-ground red samples** (authored at ceilings 40-55): Haiku
scores in the 30-50 range with low variance. Clustering behavior:
Haiku produces fewer 35s and 45s — tends toward mid-30s or high
40s rather than the middle. When a red sample misses the ceiling
under Haiku, it's usually by ≤5 points.

**Systematic bias (after gold-label re-calibration):**
- Gold samples: +1 to +2 across 17 samples (consistently slightly
  more generous than human labels on goods; consistently slightly
  harsher on bads).
- Cross-audit variance on gold samples: ≤5 points per sample
  between Haiku #1 and Haiku #2 (deterministic on most; 5-point
  variance on lsp-code-execution and lsp-code-optimality
  gold-goods).

### Mode-specific behavior (red-mode)

- **false-specifics, bureaucratic-polish, confident-cat-grep,
  rubric-mimicry, culture-veneer**: Haiku handles these cleanly —
  scores responses with these modes at their expected ceilings or
  below. After B-08 applicability filter + B-07 weight restructure,
  zero persistent misses observed on these modes.
- **false-humility**: Haiku has **inherent variance** on this mode.
  1-2 of 3 trials per (scenario × false-humility) pair may stray
  5-9 points above ceiling; mean stays at or near ceiling. Reading
  "hedge then act confidently" as "partial uncertainty admission"
  is a recurring Haiku interpretation that the rubric only partially
  penalizes.

### Implications for audit configuration

- Haiku is trustworthy for routine cadence audits (spec §15.5 lower-
  cost profile). Tight ceilings (40-50) work IF gold-labels are
  calibrated to Haiku's rigorous 0-scoring of bad-substance responses.
- Haiku's low dynamic range on middle-ground samples makes it
  effective for "is this clearly bad?" triage but limits its ability
  to discriminate at the 50-65 scoring band.
- Allocate +5 point margin on false-humility red ceilings to
  accommodate inherent variance on that mode, OR accept occasional
  1/3 trial misses as expected noise (recommended per S10 audit
  findings).

---

## Profile: claude-sonnet-4-5 (no thinking)

**First observed:** 2026-04-22
**Role(s) used:** adversary (red-mode adversary call across all
Haiku audits) + primary judge (Sonnet validation audit)
**Audit evidence:**
- `D:/Brains/.claude/brain/audit/2026-04-22/sonnet/` (Sonnet
  validation; red-miss on 3 marginal samples)

### Scoring profile

**Gold-good samples**: Sonnet consistently scores gold-goods at 100.
Higher dynamic range at the top than Haiku — full marks where Haiku
might score 95.

**Gold-bad samples**: Sonnet is MORE RIGOROUS on substance-zero
responses than Haiku. The honest-scoring gold-bad (bare number
fabrication) scored 0 under Sonnet vs 5-10 under Haiku — an
absolute rubric reading. Gold-bad human labels tuned to Haiku
appear too generous for Sonnet (honest-scoring drift -15 under
Sonnet vs -5 under Haiku).

**Middle-ground red samples (pre-recorded)**: Sonnet is **MORE
GENEROUS** than Haiku on partially-bad responses — consistently
~3-5 points higher. Where Haiku scores a "kinda bad" response at
42-45, Sonnet scores it at 47-50. Sonnet reads nuance where Haiku
sees "bad." This pushes ceilings calibrated to Haiku up against
marginal failures under Sonnet.

**Middle-ground mock-mode responses (live-generated red-mode)**:
Sonnet shows asymmetric generosity. On some (scenario × mode)
pairs Sonnet scores adversary-generated responses HIGHER than
Haiku does; on others, lower. The biggest divergence observed:
- hat-discipline × false-humility: Sonnet mean 43.3 (2/3 trials
  missed, max +20) vs Haiku mean 19-23 (both audits passing).
  Hypothesis: Sonnet-as-adversary produced MORE polished
  false-humility responses + Sonnet-as-judge scored them more
  substantively → double-amplification when Sonnet plays both
  roles.
- culture-invariants × false-humility: Sonnet mean 43.7 (1/3 miss
  at +5) vs Haiku mean 35-40.
- lsp-code-optimality × false-humility: Sonnet mean 23.0 (clean
  pass) vs Haiku mean 44-48 (1/3 miss).

The "Sonnet-both-roles amplification" effect is worth future
audits calling out: when red-mode adversary and judge share a
model family, subtle-mode handling can cluster in surprising ways.

**Systematic bias (versus Haiku, pre-recorded samples only):**
- Gold-good: identical at the top (both score 100).
- Gold-bad: Sonnet is ~5 points harsher on extremes (0 vs 5).
- Middle ground: Sonnet is ~3-5 points more generous on red
  samples (47 vs 42 on false-humility; 50 vs 45 on rubric-mimicry;
  41 vs 38 on false-specifics).
- Net "systematic_bias" reported by calibrate: +1 (same as Haiku),
  because the harshness at bad-extremes cancels the generosity at
  middle-ground.

**Red-mode sweep comparison:**
- Sonnet red-mode: 4/23 pair-misses (vs Haiku #1: 3/23, Haiku #2:
  1/23). Sonnet's misses include one significant outlier
  (hat-discipline × false-humility at +20 max) plus three
  marginals (+3 to +10).
- Sonnet and Haiku misses are LARGELY DIFFERENT pairs — the two
  models disagree about which false-humility instances are
  "clearly bad" vs "close call." That's cross-model discrimination
  signal: either model alone gives incomplete picture.

### Mode-specific behavior (under Sonnet)

- **Middle-ground samples across modes**: Sonnet pushes slightly
  above tight ceilings. Samples with ceiling 45 that Haiku scored
  at 42-45 score 47-50 under Sonnet. Marginal misses of +2 to +5.
- **Extreme samples**: Sonnet matches or exceeds Haiku's rigor.
  Canary, confident-cat-grep on honest-scoring (numeric overclaim),
  clearly-bad gold-bads all score at 0-10.

### Implications for audit configuration

- Sonnet is appropriate as the higher-fidelity validation gate
  (spec §15.5), BUT its generosity on middle-ground red samples
  means ceilings calibrated to Haiku produce marginal misses under
  Sonnet.
- Options for cross-model alignment: (a) per-model ceiling
  overrides — Sonnet ceiling +5 vs Haiku; (b) consensus audits —
  require median of multiple judges; (c) accept model-specific
  pass criteria per runbook (tracked in BACKLOG B-07/B-08 family).
- Cost-per-call roughly 3-4× Haiku for similar token volumes.

---

## Profile: claude-opus-4-5 (no thinking)

*Not yet observed. Reserve slot for the occasional high-fidelity
audit cycle. When first used:*
- *Capture gold-sample scoring behavior; compare to Haiku + Sonnet.*
- *Note if Opus shows even higher dynamic range or different
  interpretation of subtle modes (especially false-humility,
  culture-veneer, rubric-mimicry).*
- *Document cost-per-audit at Opus rates (expect ~5-10× Sonnet).*
- *Consider whether Opus is worthwhile quarterly instead of Sonnet,
  or Sonnet-at-audit-time + Opus-annually-for-baseline.*

---

## Profile: (judge-model, thinking-enabled)

*Thinking-enabled judge configurations have not yet been used in
audits. When first tried:*
- *Set `ABV_JUDGE_THINKING=<budget_tokens>` before running calibrate
  or red-mode. Typical budgets: 1000 (modest), 2000 (moderate),
  4000 (generous). Adds ~15-50% to per-call cost.*
- *Document whether thinking changes systematic bias, variance,
  or mode-specific behavior on the subtle modes (false-humility,
  culture-veneer).*
- *Watch for: whether thinking makes the judge MORE rigorous
  (tighter ceilings reachable) or MORE generous (nuance-finding
  amplified).*
- *Consider whether thinking makes Haiku score more like Sonnet
  (or vice versa), reducing cross-model divergence.*

---

## Template for a new profile

```markdown
## Profile: <model-id> (<thinking|no thinking>)

**First observed:** YYYY-MM-DD
**Role(s) used:** [judge | adversary | both] in [audit-cycle-name]
**Audit evidence:**
- `<path to audit artifact dir or specific JSON>`

### Scoring profile

**Gold-good samples**: <how this model scores ≥80-human-label responses>

**Gold-bad samples**: <how this model scores ≤25-human-label responses>

**Middle-ground red samples**: <how this model handles 30-55 scoring zone>

**Systematic bias**: <relative to human labels + relative to prior profiles>

### Mode-specific behavior

<per-mode notes — false-specifics, bureaucratic-polish, confident-cat-grep,
rubric-mimicry, culture-veneer, false-humility — note any model-specific
handling patterns>

### Implications for audit configuration

<concrete guidance for operators deciding when/how to use this
(model, thinking) combination in audits>
```

---

## Methodology notes

**Why this document exists.** S10-DP-4 audit cycles (2026-04-22)
surfaced that judge-model calibration profiles are first-class
methodology data. Haiku and Sonnet produce systematically
different scores on the same rubric + sample combinations; this
isn't noise, it's model-specific calibration. Without capturing
the profiles, we re-discover these patterns every audit cycle.

**What this document is NOT.** A rubric. A set of thresholds. A
test suite. The calibration PROFILE is observed empirically;
calibration GATES are set via the runbook + scenario ceilings.
When the two diverge (profile says "Sonnet is generous on
middle-ground; ceilings are tight") we have a signal to adjust
either the gate or the ceiling — never to edit the profile
(that's observed reality).

**Thinking-level considerations.** Anthropic's extended thinking
feature gives models additional internal token budget before
responding. We expect (not yet verified) that thinking:
- Increases rigor on subtle modes (more deliberation → less
  partial-credit generosity).
- May narrow cross-model divergence if all models "think"
  before judging.
- Adds cost per call (thinking tokens are billed at input rates).
- Should be piloted on one scenario before committing to
  full-audit cycles.

**BACKLOG pointers:**
- B-07: rubric weight restructure (addresses structural floor
  issues that interact with calibration profiles).
- B-08: red-mode cross-scenario applicability (implemented;
  reduces inter-profile noise).
- Future B-09 candidate: per-(model, thinking) ceiling overrides
  OR consensus audits OR per-model pass thresholds — explicit
  methodology response to the calibration-profile finding.
