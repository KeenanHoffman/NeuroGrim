---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Epic: Agent Behavior Verification — Red Scenarios & Judge Integrity

**Stage:** 9
**Status:** **Complete** (2026-04-21) — all 4 stories shipped + pushed. v1 ships both architectures: pre-recorded red samples (RED-1..3, Architecture A) AND mock-bad-agent live generation (RED-4, Architecture B). 173/173 pytest green; 53 new tests added across the epic; zero regressions in prior 85.
**Priority:** Medium
**Goal:** Prove the `agent-behavior` harness has a real red path. Today
the suite can only tell us agents scored green; it cannot tell us
whether green means "agents did well" or "judges always say green."
This epic introduces **red samples** — pre-recorded bad responses
with a ceiling the judge MUST stay under — and the **judge-integrity
ledger** that records misses so humans can refine the rubric /
red-sample library over time.

**Framing:** S7-ABV made measurement possible. S8-ABV-EXT made
measurement trustworthy (calibration gate, multi-judge consensus,
execution-based evidence). S9-ABV-RED closes a still-open question:
*can the trustworthy measurement actually detect bad behavior?*
Without a control that intentionally forces red, every green score
is consistent with a judge that can only say green.

**Depends on:**
- S7-ABV (complete) — scenarios, gold samples, feedback ledger.
- S8-ABV-EXT-1 (complete) — calibration subcommand; red samples
  reuse its plumbing.
- LSP-Brains spec §15.3 (judge protocol + calibration) — red
  samples extend this chapter.

**Blocks:**
- B-01 in `roadmap/BACKLOG.md` (promote `agent-behavior` past
  advisory weight) — the promotion decision should not happen on a
  suite that can only say green.
- Future S10-ABV-MOCK (mock-bad-agent red-mode) — the live
  adversary-agent variant promoted from stretch (noted in B-06).

---

## Stage 9 Is Done When

- [ ] Scenario schema supports an optional `red_samples: []` array
      alongside the existing `gold_samples: []`. Each red sample
      has `id`, `failure_mode`, `expected_score_ceiling`,
      `response`, and optional `notes`.
- [ ] `abv-run calibrate` grades red samples in the same pass as
      gold samples; misses (score > ceiling) produce a per-sample
      `red-miss` entry in the calibration report.
- [ ] Every v1 scenario ships ≥ 2 red samples, each tagged with a
      distinct `failure_mode` drawn from the taxonomy documented
      in `docs/agent-behavior-red-taxonomy.md`.
- [ ] `.claude/brain/judge-integrity-ledger.jsonl` append-only
      ledger receives one entry per red-miss (after human triage
      confirms it's a judge miss, not a mislabeled red sample).
- [ ] New skill `.claude/skills/refine-judge-integrity.md` documents
      the triage workflow: confirmed-judge-miss → rubric-phrasing
      tightening OR red-sample expansion; scenario-rubric-gap →
      rubric revision; mislabeled-red-sample → red-sample edit.
- [ ] One "canary red" sample (obviously-bad response, ceiling ≤ 5)
      is part of the library so harness failures surface fast —
      judge missing the canary is a harness problem, not an agent
      problem.
- [ ] `agent-behavior-runner/tests/` grows by ~10 tests (red-sample
      loader, red-miss classification, ledger append); 100% no-API
      coverage preserved.
- [ ] e2e-sim scenario 11 remains green (no harness regression).
- [ ] Spec §15.3 gains a short "Red samples" subsection citing this
      epic's decisions; METHODOLOGY-EVOLUTION §12 records the Why
      / Insight / Fix / Rationale.

**Anti-criteria (explicit non-goals for this stage):**
- NOT a live mock-bad-agent — we grade pre-recorded red responses.
  Mock-bad-agent mode is promoted to backlog (B-06) and tracked as
  S10-ABV-MOCK if/when scoped.
- NOT automatic judge-prompt editing. The bright line is inherited
  from S7-ABV: humans edit rubrics, red samples, and scenario
  prompts. The judge prompt itself is not a tuning surface.
- NOT a weight flip. `agent-behavior` remains advisory through this
  epic; the red-integrity signal is evidence FOR the future
  promotion decision (B-01), not the promotion itself.
- NOT per-project red-sample overrides. One ecosystem-wide library.
  Project-level overrides are captured by B-03.
- NOT retroactive application to historical trials. Red integrity
  tracks judge behavior going forward; pre-S9 scores stay as
  recorded.

---

### S9-ABV-RED-1: Schema + Harness Extension — SHIPPED

**Status:** **Complete** (2026-04-21) — shipped in LSP-Brains `236f31b` (spec §15.3 "Red samples" subsection + v2.4 changelog + METH-EV §12 + schema extensions) and ecosystem `b70da80` (harness: `RedSample` dataclass, `classify_red_miss`, `--skip-red-calibration` CLI flag, `judge-integrity:red-miss` CMDB finding, 27 new tests).
**Effort:** M
**Depends on:** —

Extend the scenario schema with `red_samples[]` and teach the
harness (`calibrator.py` + `scenarios.py`) to load, score, and
classify them. Red samples are calibration-time fixtures, not
trials — they never enter the per-run CMDB score. They only
influence whether the CMDB is written as `status: "trusted"` vs
`status: "drift-blocker"` via the existing calibration gate.

**Deliverables:**

- `LSP-Brains/schemas/agent-behavior-scenario-v1.schema.json` —
  additive `red_samples` array. Shape:
  ```json
  {
    "red_samples": [
      {
        "id": "rm-false-specifics-01",
        "failure_mode": "false-specifics",
        "expected_score_ceiling": 40,
        "response": "I will use Grep to...",
        "notes": "Cites tool names without committing..."
      }
    ]
  }
  ```
  Required: `id`, `failure_mode`, `expected_score_ceiling`,
  `response`. Optional: `notes`, `retired_in_version`.
- Schema version stays at v1 (additive; no existing field altered).
- `agent_behavior_runner/scenarios.py` — new `RedSample`
  dataclass; loader populates `scenario.red_samples`; validation
  rejects duplicate `id`s within a scenario.
- `agent_behavior_runner/calibrator.py` — extends
  `ScenarioCalibration` with `red_samples: list[RedSampleResult]`.
  A `RedSampleResult` mirrors `GoldSampleResult` but carries
  `ceiling`, `score`, `over_ceiling: bool`, `failure_mode`.
- `LSP-Brains/schemas/calibration-report-v1.schema.json` —
  additive: each scenario block gains `red_samples[]` alongside
  `gold_samples[]`.
- Calibration status aggregation: scenario drifts to
  `drift-blocker` if EITHER a gold-sample drift > threshold OR a
  red-sample over-ceiling miss is present. Documented in spec
  §15.3 subsection "Red samples."
- `--skip-red-calibration` flag (distinct from
  `--skip-calibration`) lets operators iterate on red-sample
  authoring without blocking on coverage.

**Acceptance criteria:**
- [ ] Schema validates examples with and without `red_samples`.
- [ ] Unit test: `red_samples` loader round-trip (YAML → dataclass
      → YAML).
- [ ] Unit test: red-miss classification (score > ceiling ⇒ miss;
      score == ceiling ⇒ pass; score < ceiling ⇒ pass).
- [ ] Unit test: scripted judge scoring a red sample above its
      ceiling causes `abv-run scenarios` to refuse a trustworthy
      CMDB (just like a gold-drift miss).
- [ ] Unit test: `--skip-red-calibration` produces a CMDB flagged
      `status: "red-skipped"` — explicitly lower trust than
      `trusted` but above `drift-blocker`.

---

### S9-ABV-RED-2: Red-Sample Library + Failure-Mode Taxonomy — SHIPPED

**Status:** **Complete** (2026-04-21) — shipped in NeuroGrim `b778626` (docs/agent-behavior-red-taxonomy.md — 285 lines covering six modes + canary, authoring checklist, retirement discipline) and ecosystem `a80e668` (13 red samples across 6 scenarios + 10-test library coverage suite + skill update + CEO-template README update).
**Effort:** M
**Depends on:** S9-ABV-RED-1

Author the first red-sample library. Every v1 scenario gets ≥ 2
samples targeting distinct failure modes. The taxonomy is
intentionally shallow at first (six modes) — it grows as real
misses enter the ledger.

**Initial taxonomy (documented in `docs/agent-behavior-red-taxonomy.md`):**

| Mode | What it looks like | Why it's hard to catch |
|---|---|---|
| `false-specifics` | Names tools/domains correctly without committing ("I would use Grep to...") | Surface vocabulary matches the rubric |
| `bureaucratic-polish` | Elaborate structured plan that never commits to a concrete step | Structure signals competence; judge overweights form |
| `confident-cat-grep` | Confidently proposes the wrong tool ("`cat \| grep`") | Confidence trumps correctness in naive grading |
| `rubric-mimicry` | Echoes rubric phrasing back ("I will prioritize LSP-native tools...") | Judge pattern-matches its own rubric |
| `culture-veneer` | Wraps a culture violation in polite framing ("I respectfully insist...") | Critical-but-kind is easy to fake at the surface |
| `false-humility` | Acknowledges uncertainty performatively, then acts anyway | Hedging satisfies `acknowledges_uncertainty` textually |

Plus one **canary** — obviously bad, ceiling ≤ 5 — in at least
one scenario. Judge missing the canary is a harness break, not
an agent problem.

**Deliverables:**

- `docs/agent-behavior-red-taxonomy.md` — the six-mode table
  above, with one worked example per mode drawn from the library.
  Authoring guidance: diversify modes across samples; don't
  cluster similar reds; keep samples realistic in length.
- `.claude/agent-behavior-scenarios/*.yaml` — five v1 scenarios,
  each gets ≥ 2 red samples:
  - `lsp-code-optimality` → `false-specifics`,
    `confident-cat-grep`.
  - `lsp-code-execution` → `false-specifics`,
    `rubric-mimicry` (an agent that emits `tool_use` with
    generic `pattern=".*"` — execution-rubric version of
    false-specifics).
  - `lsp-brain-usage` → `false-specifics`, `bureaucratic-polish`.
  - `hat-discipline` → `false-humility`, `bureaucratic-polish`.
  - `culture-invariants` → `culture-veneer`, `false-humility`.
  - `honest-scoring` → `confident-cat-grep` adapted to numeric
    overclaim ("the Brain scores 87, so...") + `rubric-mimicry`
    + one canary ("I'll just guess 80.").
- Samples are reviewed by a second human for label agreement
  before landing — aligns with the spec §15.3 "two humans agree
  within ±10" discipline.
- `agent-behavior-runner/tests/test_red_samples.py` — scripted
  ideal judge scores every red sample ≤ ceiling; the test fails
  if a red sample is unreachable by any reasonable judge scoring.

**Acceptance criteria:**
- [ ] Every v1 scenario carries ≥ 2 red samples; taxonomy covers
      all six modes across the library.
- [ ] Canary red sample present and referenced in
      `docs/agent-behavior-red-taxonomy.md`.
- [ ] `test_red_samples.py` green; samples proportionally scorable
      by the ideal judge.
- [ ] `write-agent-behavior-scenario.md` skill updated with the
      red-sample authoring checklist (label, choose failure mode,
      review by second human, add to test).

---

### S9-ABV-RED-3: Judge-Integrity Ledger + `refine-judge-integrity` Skill — SHIPPED

**Status:** **Complete** (2026-04-21) — shipped in LSP-Brains `cbf971f` (skill mirror + gitignore), NeuroGrim `1716a66` (skill mirror + gitignore), and ecosystem `a7c3e4c` (`judge_integrity_ledger.py` with append-only discipline + privacy allow-list + ABV_OPERATOR triage guard, `abv-run judge-integrity list|triage` CLI, calibrator auto-append wiring, `refine-judge-integrity.md` skill across all three Brain dirs, worked-example extension, 16 new tests).
**Effort:** S
**Depends on:** S9-ABV-RED-1, S9-ABV-RED-2

Wire the judge-integrity feedback loop. When calibration records a
red-miss, the harness appends a PENDING entry to
`.claude/brain/judge-integrity-ledger.jsonl`. Pending entries
require human triage before they become evidence; this keeps the
ledger honest about the difference between "judge missed a real
failure" and "red sample was mis-labeled."

**Ledger entry shape:**
```json
{
  "ts": 1776900000.0,
  "scenario_id": "lsp-code-optimality",
  "scenario_version": "1",
  "red_sample_id": "rm-false-specifics-01",
  "failure_mode": "false-specifics",
  "expected_ceiling": 40,
  "judge_score": 62,
  "judge_models": ["claude-sonnet-4.5"],
  "triage_status": "pending",
  "triage_decision": null,
  "triage_notes": null,
  "triage_by": null,
  "triage_at": null
}
```

After human triage, a new entry (not an edit — append-only) carries:
- `triage_status: "triaged"`,
- `triage_decision`: one of `confirmed-judge-miss`,
  `scenario-rubric-gap`, `mislabeled-red-sample`,
- `triage_notes`: prose,
- `triage_by`, `triage_at`: human + timestamp,
- `supersedes`: the pending entry's timestamp (for chronological
  linking).

**Deliverables:**

- `agent_behavior_runner/judge_integrity_ledger.py` — append-only
  writer mirroring `feedback.py`. Privacy allow-list applied (no
  raw response text in the ledger; only ids + scores + metadata).
- `abv-run judge-integrity --since-seconds N` CLI — dumps the
  ledger window as JSONL.
- `abv-run judge-integrity triage <sample_id>` CLI — appends a
  triage entry. Required args: `--decision`, `--notes`; the
  command refuses to run without `ABV_OPERATOR` env var set
  (signals that a human, not a script, is on the line).
- `.claude/skills/refine-judge-integrity.md` in all three Brain
  dirs (ecosystem + NeuroGrim + LSP-Brains) — triage workflow,
  decision rules, example walkthroughs.
- `.gitignore` entries for
  `.claude/brain/judge-integrity-ledger.jsonl` in all three
  Brain dirs (same pattern as feedback ledger).

**Triage decision rules — summarized in the skill:**
- `confirmed-judge-miss`: response genuinely displays the failure
  mode; rubric says it should score low; judge scored it high.
  Action: file a `refine-agent-behavior` session on the rubric
  phrasing, OR add additional red samples covering the same mode
  but with different surface forms.
- `scenario-rubric-gap`: the rubric, read strictly, doesn't
  actually penalize the failure mode the red sample displays.
  Action: revise the scenario rubric (bump version per spec
  §15); re-run calibration; remove or rewrite the red sample.
- `mislabeled-red-sample`: the response isn't actually bad, or
  isn't as bad as the ceiling suggests. Action: raise the ceiling
  or retire the sample.

**Acceptance criteria:**
- [ ] Unit test: pending entry written on a scripted-judge
      red-miss.
- [ ] Unit test: triage append requires `ABV_OPERATOR`; without
      it, command errors and the ledger is not modified.
- [ ] Unit test: `judge-integrity` export round-trip (append →
      read → parse matches write).
- [ ] Skill landed + byte-identical across the three Brain dirs.
- [ ] Worked-example note in `worked-example.md` showing one
      pending → triaged sequence.

---

### S9-ABV-RED-4: Mock-Bad-Agent Red Mode — SHIPPED

**Status:** **Complete** (2026-04-21) — shipped in LSP-Brains `bd8119b` (red-mode-report-v1 schema + skill mirror), NeuroGrim `3b9822b` (taxonomy Architecture B section + skill mirror), and ecosystem `f9b10b6` (adversary prompts library with 7 entries, `mock_adversary.py` loader, `red_mode.py` orchestrator with canary gate, `abv-run red-mode` CLI subcommand, 35 new tests, worked-example extension with cost/cadence table). Promoted from BACKLOG B-06 mid-epic per operator request.
**Effort:** L
**Depends on:** S9-ABV-RED-1..3 complete.

Architecture A (pre-recorded red samples) is the deterministic
path shipped in RED-1..3: a fixed library of bad responses with
ceilings, judged once per calibration cycle. Architecture B
(this story) is the live-generation path: a second Claude call
prompted to deliberately display a specific failure mode produces
a novel response per run; the response is scored by the same
judge that grades real agents. Richer coverage (novel surfaces
per run), non-deterministic by construction, and introduces a new
trust surface — the adversary's "badness dial."

**Deliverables:**

- `.claude/agent-behavior-adversary-prompts.yaml` — library of
  system prompts keyed by failure mode. Each entry has
  `default_ceiling` + `system_prompt`. Covers the six v1 modes
  + canary.
- `agent_behavior_runner/mock_adversary.py` — prompt loader +
  adversary client (wraps existing claude-proxy client).
- `agent_behavior_runner/red_mode.py` — orchestrator. For each
  (scenario × mode × trial): generate adversary response, score
  with live judge, collect result. Produces a mock-mode report.
- `abv-run red-mode <scenario-dir>` CLI subcommand:
  `--prompts-file` (defaults to ecosystem path), `--mode` (filter
  to specific mode; default = all), `--trials N` (default 3),
  `--output` (report JSON path; stdout if omitted), `--profile`
  (inherited from existing calibrate pattern).
- **Adversary gate.** Before scaling to full mode-sweep, run the
  `canary` adversary first. Judge MUST score its output ≤ canary
  ceiling. If not, abort — either the adversary is miscalibrated
  or the judge stack is broken. Mirrors the pre-recorded canary
  discipline.
- Mock-mode report schema (new
  `red-mode-report-v1.schema.json`): per-(scenario, mode, trial)
  records carrying adversary response, judge score,
  `over_ceiling`, status (pass | red-miss | error). NOT the
  judge-integrity ledger — mock runs are interpretive, not
  triage-enforcing.
- `refine-judge-integrity.md` skill update: new subsection on
  mock-mode triage. Fourth implicit decision branch:
  "adversary-miscalibrated" — when mock responses cluster above
  ceiling OR well below zero, the adversary prompt needs
  tightening, not the judge.
- `worked-example.md` update: one new section demonstrating a
  full mock-mode sweep end-to-end.
- `docs/agent-behavior-red-taxonomy.md` update: note that
  adversary prompts are the live-generation sibling to
  pre-recorded samples; same six modes, same ceiling philosophy.

**Design choices:**

- **Mock misses do NOT auto-append to judge-integrity ledger**
  in v1. Mock runs produce interpretive reports; if a specific
  mock response is worth preserving as evidence, operators
  author it as a pre-recorded red sample (manual promotion
  preserves the ledger's stable-ID discipline).
- **Same judge configuration as the scenario declares.** Mock
  responses flow through exactly the same scoring path as real
  agent-under-test responses. No separate judge path.
- **Per-mode ceiling defaults** colocated with the adversary
  prompts. Hardcoded per authoring intent; overridable per run
  via a future flag if needed.
- **No cross-provider adversary in v1.** Same Claude family for
  adversary and judge — mirrors the RED-1..3 constraint. Cross-
  provider is a stretch (BACKLOG B-02 direction).

**Acceptance criteria:**
- [ ] `abv-run red-mode --help` covers the subcommand.
- [ ] Unit test: adversary-prompt loader round-trip + every v1
      mode covered.
- [ ] Unit test: canary gate fires when scripted judge
      over-scores the canary adversary's output.
- [ ] Unit test: scripted adversary + scripted judge, per-mode
      filter honored; one mode runs when `--mode X` passed.
- [ ] Unit test: report schema validates; per-trial records
      carry mode, ceiling, judge_score, over_ceiling, status.
- [ ] Full pytest suite green (no regression in prior 138 tests).
- [ ] Skill + worked-example + taxonomy doc updated.

---

## Adversarial review (plan-critic hat, full power)

### 🔴 Blocking

*None identified at this time.* The epic is additive; it does not
change existing calibration semantics, scoring, or the feedback-
ledger bright line. The one weight-bearing discipline — "no judge-
prompt editing" — is inherited from S7-ABV and remains load-
bearing here.

### 🟡 Concerns

1. **Rubric gaming in reverse.** We're optimizing the judge against
   a FIXED red-sample library. Over time, judges (or humans
   rephrasing rubrics to catch known reds) could pattern-match
   surface features of the library rather than the failure modes
   themselves. A subtle real-world failure that doesn't look like
   any library sample would still pass through.
   - **Mitigation:** red samples GROW (new samples land as new
     failure modes emerge in production feedback); the existing
     library never shrinks silently (retired samples carry
     `retired_in_version` metadata). Gold samples stay frozen
     (regression protection); red samples expand (coverage).
     Coverage metric: `(# failure modes × # scenarios using them)
     / total` — report trend, not absolute number.

2. **Judge adaptation to fixed red set.** Related but distinct —
   Claude doesn't learn between calls at inference time, but a
   static set of 10–15 red samples only tests the slice of
   failure space they cover. If real-world failures cluster
   elsewhere, our red coverage reports a healthy signal while
   missing the actual drift.
   - **Mitigation:** diversify AT AUTHORING time (six-mode taxonomy
     + one canary); commit to expand the library when real misses
     surface in feedback / judge-integrity triage; flag "red
     coverage staleness" when no new red samples have been added
     for N months.

3. **Doubled calibration cost.** Every scenario now has gold
   samples + red samples in one calibration pass. Five scenarios
   × (2–3 gold + 2–3 red) × N judges ≈ 25–90 judge calls per
   calibration run. Still cheap (~$0.05–$0.15 sandbox, ~$1 full
   profile) but up from ~$0.02 pre-S9. Not a blocker; note it in
   `agent-behavior-troubleshooting.md` and the runner README.
   - **Mitigation:** `--skip-red-calibration` for rapid iteration;
     default cadence stays weekly, not per-PR; multi-judge
     consensus remains opt-in per scenario.

4. **Judge-failure vs scenario-bug vs red-sample mislabeling is
   genuinely hard.** When the judge scores a red sample above its
   ceiling, three candidates compete: the judge missed, the rubric
   doesn't actually cover the failure mode, or the sample itself
   was mis-labeled. Without human triage this collapses into "the
   judge is broken" — which is exactly the wrong signal for the
   scenario-rubric-gap case.
   - **Mitigation:** ledger entries start as `pending`; only
     triaged entries count as evidence. The skill documents the
     three decision branches. Two-human review of new red samples
     at authoring time reduces mislabeling upfront.

5. **Self-referential feedback.** We use judges to grade agents; we
   use humans to refine judges' ability to grade agents; humans
   decide what "bad agent response" means. If humans' notion of
   bad drifts, the system encodes that drift. This is a floor
   property of any LSP Brain feedback loop — same concern as
   S7-ABV's refinement loop — but compounded because now we're
   refining the refiner.
   - **Mitigation:** same as §15 — two-human label agreement is
     ground truth; spec §15.3 + red-taxonomy doc documents the
     shared meaning of each failure mode; ledger append-only
     preserves audit trail; `METHODOLOGY-EVOLUTION §12` records
     this epic's intent so future readers can audit the
     methodology itself.

6. **Canary false-negative risk.** The canary sample is designed
   to be trivially-bad. But "trivially bad" is a surface property
   — a canary that's only bad because of a specific phrase can
   be gamed the same way as any other sample. If the canary
   passes but a subtle red fails, operators might declare the
   harness "working" and move on.
   - **Mitigation:** canary is a NECESSARY-not-sufficient check;
     document clearly that canary passing only proves the harness
     isn't fully broken, NOT that the judge is sound. The main
     evidence remains the red-sample library + judge-integrity
     ledger.

7. **Red samples may become training data.** If the library is
   ever published or scraped, future Claude training corpora
   could include our labeled reds. Future models might avoid
   those surface features — which either improves agent behavior
   (good) or just shifts failure modes to novel surfaces (bad).
   Not an immediate concern (the library lives in private Brain
   dirs), but document the policy.
   - **Mitigation:** `agent-behavior-red-taxonomy.md` names the
     concern explicitly; scenarios + red samples stay in `.claude/`
     under ecosystem control; no CI export of the library.

8. **Failure-mode taxonomy drift.** Six modes is minimal. Over
   time real misses surface new modes ("what do we call THIS?").
   Free-text `failure_mode` is flexible but risks drift — two
   operators tagging the same failure differently.
   - **Mitigation:** `failure_mode` is free-text v1 BUT the
     taxonomy doc enumerates known values and encourages reuse;
     quarterly review pulls new tags from the ledger and either
     absorbs them into the taxonomy or formalizes them as new
     modes. Deferred governance, not ungoverned.

9. **Overfitting judges to red samples == underfitting to novel
   failures.** The more aggressively we tighten rubric phrasing
   to catch every library red, the narrower the slice of failure
   space we cover well. A judge that scores all 15 library reds
   correctly may still miss a 16th novel failure mode.
   - **Mitigation:** prefer library EXPANSION over rubric
     tightening. When the ledger shows a real-world miss, the
     first response is "add a red sample covering this," not
     "rewrite the rubric." Spec §15 guidance.

10. **Interaction with multi-judge consensus.** With `judge_models:
    [A, B, C]` a red-miss could be one judge disagreeing with the
    other two. Is that a red-miss for the consensus, or for the
    individual outlier? Current plan: grade the CONSENSUS score,
    not the individual judges. But the ledger entry records the
    per-judge outlier data so operators can see whether
    disagreement clustered.
    - **Mitigation:** documented explicitly; `judge_integrity`
      entries carry `judge_models` + `per_judge_scores` so
      post-hoc analysis is possible.

### 🔵 Suggestions

- **"Calibrate only reds" quick command.** `abv-run calibrate
  --reds-only` lets operators iterate on new red samples without
  paying for gold-sample calls. Small win but high-cadence.
- **Taxonomy-coverage badge.** Harness emits a line on calibrate:
  "Red coverage: 6/6 modes, 11/5 samples across 5 scenarios."
  Cheap visibility for the trend.
- **Human-spot-check on triage.** Every N-th triage decision gets
  flagged for a second human's review. Same pattern as the
  judge-drift spot-check suggested in S7-ABV's adversarial
  review; kept cheap by randomization.
- **Red-mode in worked example.** Extend
  `worked-example.md` with a red-miss scenario: author a red
  sample, run calibrate, find it misses, triage, expand the
  library, re-run. Demonstrates the full loop.
- **Scenario-level opt-out.** Let specific scenarios declare
  `red_calibration: skip` if they're genuinely unsuited to red
  samples (stretch; revisit if any scenario actually needs it).

### 🟢 Strengths

- **Architecturally cheap.** Architecture A reuses
  `calibrator.py`, feedback-ledger patterns, skill structure.
  ~300 LOC Python + 6 YAML edits + 2 docs. No new processes.
- **Honest about what it proves.** Red samples only prove the
  judge detects the failure modes the library covers. The epic
  names this limit, documents the expansion protocol, and
  offers the coverage metric to surface staleness.
- **Preserves the bright line.** Humans still edit; agents still
  don't self-refine; judge prompt remains off-limits. The
  refinement loop extends from "rubric tightening" to "library
  expansion" — but the boundary of what agents can modify is
  unchanged.
- **Unblocks B-01 (promotion past advisory).** A judge
  demonstrably capable of detecting red samples is a necessary
  precondition to a weight-flip decision. This epic supplies
  evidence the promotion epic will need.
- **Methodology contribution.** Red scenarios + judge-integrity
  ledger formalize a pattern that's implicit in classical test
  engineering (mutation testing / fault injection) but absent
  from the LLM-as-judge literature we've seen. METH-EV §12 ships
  the contribution.

---

## North Star Check

- **Does this make the pattern more general?** Yes. Red samples
  + judge-integrity ledger is a transferable pattern for any
  LLM-as-judge system — not specific to agent behavior.
- **Does this make the ecosystem Brain easier?** Indirectly. By
  proving the `agent-behavior` signal is sound in both
  directions (green means good AND red means bad), the ecosystem
  Brain can eventually surface it alongside first-class
  deterministic sensors without caveat.
- **Is it the simplest thing that could work?** Architecture A is
  intentionally minimal. The deferred stretch (B-06 mock-bad-
  agent) is where sophistication lives if v1 isn't enough.

---

## Data Architecture Notes

- `agent-behavior-scenario-v1.schema.json` gains `red_samples[]`
  (additive; no version bump).
- `calibration-report-v1.schema.json` gains per-scenario
  `red_samples[]` block (additive).
- `.claude/brain/judge-integrity-ledger.jsonl` is a new ledger —
  append-only, one JSON object per line, privacy allow-list
  applied (ids + scores + metadata, no raw response text).
- `docs/agent-behavior-red-taxonomy.md` is a new spec-adjacent
  doc (lives in NeuroGrim `docs/` for operator reference;
  spec §15.3 cross-references it).

---

## Files to Modify

Cross-repo. Highlights:

**LSP-Brains:**
- `schemas/agent-behavior-scenario-v1.schema.json` — additive
  `red_samples[]`.
- `schemas/calibration-report-v1.schema.json` — additive
  red-sample block per scenario.
- `spec/LSP-BRAINS-SPEC.md` §15.3 — new "Red samples" subsection.
- `spec/METHODOLOGY-EVOLUTION.md` — new §12 (Why/Insight/Fix/
  Rationale).

**ecosystem (`D:/Brains/`):**
- `agent-behavior-runner/agent_behavior_runner/calibrator.py` —
  red-sample calibration path.
- `agent-behavior-runner/agent_behavior_runner/scenarios.py` —
  `RedSample` dataclass + loader.
- `agent-behavior-runner/agent_behavior_runner/judge_integrity_ledger.py`
  — new.
- `agent-behavior-runner/agent_behavior_runner/cli.py` —
  `judge-integrity` + `judge-integrity triage` subcommands.
- `agent-behavior-runner/tests/test_red_samples.py` — new.
- `agent-behavior-runner/tests/test_judge_integrity_ledger.py` —
  new.
- `.claude/agent-behavior-scenarios/*.yaml` — red samples added
  to each of the 5 v1 scenarios + `lsp-code-execution.yaml`.
- `.claude/skills/refine-judge-integrity.md` — new.
- `.claude/skills/write-agent-behavior-scenario.md` — update with
  red-sample checklist.
- `ceo-project-template/.claude/agent-behavior-scenarios/` — same
  red-sample additions, mirrored for CEO adopters.

**NeuroGrim:**
- `docs/agent-behavior-red-taxonomy.md` — new.
- `docs/agent-behavior-troubleshooting.md` — red-calibration
  failure section.
- `agent-behavior-runner/worked-example.md` (lives in NeuroGrim
  per S7-ABV-6) — red-miss worked example.
- `.claude/skills/refine-judge-integrity.md` — mirror of ecosystem
  copy.
- `roadmap/ROADMAP.md` — Stage 9 row + BACKLOG pointer + "Last
  updated" bump.
- `roadmap/BACKLOG.md` — add B-06 (mock-bad-agent mode).
- `.claude/brain/` — `.gitignore` entry for
  `judge-integrity-ledger.jsonl`.

**Commit plan:** Four logical commits, one per story (1 = schema
+ harness; 2 = library + taxonomy doc; 3 = ledger + skill), plus
one spec commit in LSP-Brains for §15.3 subsection + METH-EV §12.

---

## Reused Patterns

- **Calibration subcommand** (S8-ABV-EXT-1) — red samples ride
  the same `abv-run calibrate` surface.
- **Ledger pattern** (feedback / incident / proposal / score-
  history) — append-only JSONL, privacy allow-list, time-windowed
  export.
- **Two-human label agreement** (spec §15.3 gold-sample rule) —
  same discipline for red-sample authoring.
- **Hat discipline** (plan-critic during planning; rubber-duck
  during taxonomy debate; adversary during triage of suspected
  judge misses).
- **Skill-anatomy** (trigger phrases / role / workflow /
  cross-refs) — new `refine-judge-integrity.md` follows the
  established shape.
- **Schema-first for additive changes** (LSP-Brains convention) —
  `red_samples` lands in the schema BEFORE harness consumes it.

---

## Verification Plan

1. **Schema tests green** — `red_samples` examples validate; old
   scenarios without `red_samples` still validate.
2. **Unit tests green** — red-sample loader round-trip; red-miss
   classification; harness refusal on red-miss; triage CLI
   requires operator env; append-only ledger shape.
3. **Library-level test** — scripted ideal judge scores every
   landed red sample ≤ ceiling (proves samples are reachable).
4. **Live smoke** — one `abv-run calibrate` run against the real
   API via claude-proxy (budget ≤ $0.20); asserts calibration
   report carries red-sample block.
5. **e2e-sim scenario 11 green** — no harness regression.
6. **Worked example** — one red-miss walked through end-to-end:
   pending ledger entry → human triage → taxonomy doc update.
7. **Two-human red-sample review** — documented sign-off on the
   initial library (label agreement within ±10 on every sample).

---

## Scope Limits (v1)

- **NO** mock-bad-agent mode. Architecture A only. B-06 tracks
  the stretch.
- **NO** automatic rubric tightening based on ledger data.
  Humans read, decide, edit.
- **NO** automatic red-sample generation from feedback. Humans
  author samples after triage.
- **NO** per-project red-sample overrides. Ecosystem-wide library.
  B-03 covers the per-project concern separately.
- **NO** cross-model red-sample comparison. One judge family per
  calibration run; multi-judge consensus grades the consensus,
  not cross-family drift.
- **NO** continuous / per-PR red calibration. Weekly on-demand,
  plus pre-promotion gate when B-01 advances.
- **NO** red-miss as a gating signal in v1. Red misses are
  evidence for the promotion decision (B-01), not a weight flip
  themselves.

---

## Verdict

**PROCEED** as a Stage 9 epic. The control this introduces —
"prove the test has a real red" — is the kind of rigor the
`agent-behavior` domain needs before any promotion decision.
Architecture A is intentionally minimal; we ship the pattern, see
how it ages, then decide on B-06 mock-bad-agent mode with real
operational data rather than speculation.

The adversarial review's blocking column is empty for a reason:
this epic is additive, preserves every bright line from S7/S8,
and moves the trust needle forward by creating evidence rather
than asking us to trust harder. The concerns listed are all
mitigatable with discipline (library expansion, taxonomy
governance, two-human review); none require a redesign.

Expected total scope: ~300 LOC Python + ~100 LOC schema + ~400
LOC tests + ~800 words spec subsection + ~500 words METH-EV §12
+ 6 YAML edits (red samples × scenarios) + 2 new docs + 2 skill
edits. Roughly 2–3 focused sessions to ship v1.
