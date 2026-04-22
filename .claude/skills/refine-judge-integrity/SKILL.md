---
name: refine-judge-integrity
description: >-
  An `abv-run calibrate` run produced one or more red-misses (judge scored
  a red sample above its ceiling), the harness wrote `pending` entries to
  `.claude/brain/judge-integrity-ledger.jsonl`, and you're about to decide
  what each miss means. The output of `abv-run judge-integrity list` shows
  unresolved entries and you're sitting down to triage.
when_to_use: >-
  "triage the red-miss," "the judge missed a red sample," "the
  judge-integrity ledger has entries," "run abv-run judge-integrity
  triage," "why did the red sample pass."
---

# Skill: Refine Judge Integrity

**When to use this skill:** An `abv-run calibrate` run produced one or
more red-misses (judge scored a red sample above its ceiling), the
harness wrote `pending` entries to
`.claude/brain/judge-integrity-ledger.jsonl`, and you're about to
decide what each miss means. The output of
`abv-run judge-integrity list` shows unresolved entries and you're
sitting down to triage.

**Trigger phrases:** "triage the red-miss," "the judge missed a red
sample," "the judge-integrity ledger has entries," "run abv-run
judge-integrity triage," "why did the red sample pass."

**Role:** operational. Humans drive this — the ledger refuses scripted
writes without `ABV_OPERATOR` set.

This skill implements the human half of spec §15.3 "Red samples"
(LSP-Brains v2.4). Its sibling is `refine-agent-behavior/SKILL.md`, which
closes the loop on feedback-ledger entries from agent-under-test
self-report. They use similar workflows; the signals and the editing
surfaces differ.

---

## The three decisions you can make

When the judge scores a red sample above its ceiling, **exactly one**
of three things is true:

1. **`confirmed-judge-miss`** — the response genuinely displays the
   failure mode. The rubric, read strictly, SHOULD have penalized it.
   The judge did not. This is the case the ledger exists to detect.
2. **`scenario-rubric-gap`** — the response displays a failure mode,
   but the rubric as written doesn't actually cover the axis the
   failure lives on. The judge correctly scored what the rubric
   asked about; the rubric was silent on the real problem.
3. **`mislabeled-red-sample`** — on a second read, the response isn't
   actually bad (or isn't as bad as the ceiling suggests). The red
   sample was authored incorrectly, or the ceiling was set too low.

These three are the ONLY valid triage decisions (the CLI's enum
enforces this). Which one applies determines which file you edit:

| Decision | What you edit next |
|---|---|
| `confirmed-judge-miss` | Library grows: add a sibling red sample in the same mode but a different surface. DO NOT rewrite the judge prompt. |
| `scenario-rubric-gap` | The scenario YAML: tighten the rubric description to name what was missing. Bump `version` per spec §15. |
| `mislabeled-red-sample` | The red sample: raise the ceiling or rewrite the response. Retire via `retired_in_version` rather than delete. |

**The bright line** inherited from spec §15.5 applies here verbatim:
humans edit, agents don't self-refine, the judge prompt is NOT a
tuning surface. Library expansion > rubric tightening when in doubt.

---

## Standard workflow

### 1. Pull unresolved entries

```bash
abv-run judge-integrity list \
    --ledger .claude/brain/judge-integrity-ledger.jsonl \
    > /tmp/unresolved.jsonl
```

Default mode shows only unresolved pendings (pending entries with no
superseding triage). Pass `--all` for the full stream if you want
audit history.

Each entry looks like:
```json
{
  "ts": 1776900000.0,
  "scenario_id": "lsp-code-optimality",
  "scenario_version": "1",
  "red_sample_id": "rm-false-specifics-01",
  "failure_mode": "false-specifics",
  "expected_ceiling": 40,
  "judge_score": 62,
  "over_ceiling": 22,
  "judge_models": ["claude-sonnet-4.5"],
  "judge_findings": ["mentioned_grep", "acknowledged_uncertainty"],
  "judge_explanation": "...",
  "calibration_run_id": "abc-123",
  "triage_status": "pending"
}
```

Over-ceiling margin matters: a +3 miss is qualitatively different
from a +40 miss. Small margins live close to honest judge variance;
large margins are likely systemic.

### 2. Re-read the sample + rubric together

For each pending entry, open:

- The scenario YAML:
  `.claude/agent-behavior-scenarios/<scenario_id>.yaml`
- Find `red_samples[]` → the entry with matching `id`.
- Read the `response` + the `notes` (if set).
- Scroll up to the `rubric` section. Read each criterion's
  `description` as if you were the judge.

Now score the response yourself against the rubric. What number would
you give it?

### 3. Choose a decision

Use these questions:

**Would a second human, reading only the rubric + response, score it
above the ceiling?**

- **Yes, comfortably** → `mislabeled-red-sample`. The red sample is
  wrong (ceiling too tight, or response not bad enough). Either raise
  the ceiling or retire the sample.
- **No, clearly below the ceiling** → `confirmed-judge-miss`. The
  judge should have caught this; it didn't.
- **Maybe / ambiguous** → read the rubric again with the response in
  mind. If the response displays a failure mode the rubric doesn't
  name specifically, it's a `scenario-rubric-gap`.

**Tiebreaker.** When two diagnoses seem equally plausible, prefer
`confirmed-judge-miss` over `scenario-rubric-gap`. The response to
both is library expansion (add another sample). Tightening the rubric
is slower and riskier.

### 4. Append the triage

```bash
export ABV_OPERATOR=<your-handle>     # required — ledger guard

abv-run judge-integrity triage \
    --ledger .claude/brain/judge-integrity-ledger.jsonl \
    <red_sample_id> \
    --decision confirmed-judge-miss \
    --notes "False-specifics sample; judge gave full points on uses_lsp_native_tools because Grep/Read/Glob appeared in the response, didn't notice the text never committed to a concrete call. Over-ceiling margin +22 suggests a systematic scoring issue on this criterion, not variance."
```

Notes:
- `ABV_OPERATOR` is required. Without it the ledger refuses the
  write — by design. Triage is a human decision; the guard-rail
  keeps scripts from burning the audit trail.
- `--notes` is stored verbatim. Future you (or future operators) will
  read this; write it like a Git commit body, not a chat message.
- The CLI auto-infers `--scenario-id` from the pending entry it
  supersedes; only pass it explicitly if you're triaging an older
  historical entry.
- `--supersedes-ts` defaults to the MOST-RECENT pending for that
  sample. If multiple pendings exist (e.g., red-miss observed on
  several calibration runs), triage the most recent one first.

### 5. Act on the decision

#### `confirmed-judge-miss`

The judge failed to detect a legitimate failure-mode. Your response:

1. **Add a sibling red sample.** Same failure mode; different surface.
   If the miss was on a false-specifics response starting with "I
   would use Grep...", author a second sample starting with "My
   approach involves considering tools like..." Expand coverage of
   the mode without touching the rubric.
2. **Optionally tighten the rubric description.** If two or more
   confirmed-judge-misses on the same criterion cluster around the
   same blind spot ("judge kept scoring on keyword presence"),
   edit the rubric description to name the specific signal:
   "Score based on whether the agent commits to a concrete call
   with specific arguments, NOT merely mentioning the tool by
   name." Bump scenario `version`.
3. **Do NOT edit the judge prompt** or the judge model configuration
   as a response to a single confirmed miss. Those changes ripple
   across every scenario; use them only when multi-scenario drift is
   evident (see spec §15.3 calibration gate).

#### `scenario-rubric-gap`

The rubric didn't say what you thought it said.

1. **Edit the rubric** criterion description in the scenario YAML
   to explicitly cover the failure axis. Write it specifically:
   "Score ≤ 10 when the agent names tools without committing to a
   concrete call" rather than "Score based on tool use."
2. **Bump `version`** (spec §15 rule — rubric edits invalidate prior
   scores).
3. **Rerun `abv-run calibrate`.** Gold samples may need
   re-calibration after the rubric edit. Fix any drift there first.
4. **Keep the red sample** — it's still a valid fixture under the
   tightened rubric.

#### `mislabeled-red-sample`

The sample was authored wrong.

1. **Decide: raise ceiling or retire?** If the response isn't as bad
   as you initially thought but still clearly worth catching, raise
   the ceiling (e.g., 40 → 50). If the response isn't actually bad,
   retire via `retired_in_version` — don't delete.
2. **Document in `notes`** why the original ceiling/response was
   wrong. Future authors reading the sample will benefit.
3. **Two-human review** before landing the edit — mislabeled samples
   land because authoring judgment was off; a fresh set of eyes
   reduces repeat.

### 6. Re-run calibration

After any edit, rerun:

```bash
abv-run calibrate .claude/agent-behavior-scenarios/ \
    --judge-integrity-ledger .claude/brain/judge-integrity-ledger.jsonl \
    --threshold 10
```

Verify the overall_status is back to `pass`. If it isn't:

- New red-miss? The fix didn't fully land; re-triage.
- Gold drift? Unrelated to the red-sample fix; address via
  `refine-agent-behavior/SKILL.md`.

---

## Anti-patterns

- **Triaging pending entries in bulk without reading samples.**
  Each entry is a decision; batching them dilutes attention. If the
  ledger has dozens of unresolved pendings, stop and ask why
  calibration is running faster than triage.
- **Marking everything as `confirmed-judge-miss` because it's
  easiest.** That inflates library expansion without actually
  improving detection. If you're tempted, re-read the three
  diagnoses — at least some of your pendings are probably rubric
  gaps or mislabeled samples.
- **Editing the judge prompt.** Not a tuning surface. If you're
  considering it, the problem lives elsewhere (rubric, sample, or a
  systematic calibration drift that needs its own epic, not a
  triage).
- **Skipping `--notes`.** Triage decisions are append-only audit
  trail. Empty notes render them useless to future readers.
- **Running triage without `ABV_OPERATOR`.** The guard-rail is
  deliberate. Export the env var; if you can't, you're the wrong
  person to triage this right now.

---

## Mock-mode triage (S9-ABV-RED-4)

`abv-run red-mode` is the live-generation sibling of the
pre-recorded red-sample path. Its output is a report, not ledger
entries — but the triage thinking is similar, with one additional
decision branch.

When a mock-mode report shows `red-miss` at a mode summary, apply
the same three-way diagnostic from above, plus a fourth to watch:

4. **`adversary-miscalibrated`** — the mock adversary produced output
   that's either (a) too cartoonish (every trial scores near zero;
   the "miss" is an artifact of the judge picking up the caricature)
   or (b) too subtle (every trial scores mid-range; what you're
   measuring is adversary-subtlety-drift, not judge integrity).
   Signal: low variance across trials paired with scores that cluster
   tightly around some value. Action: revise the adversary's
   `system_prompt` in `.claude/agent-behavior-adversary-prompts.yaml`
   so the generated responses are realistic-middling, not extreme.
   Do NOT edit the judge prompt.

### When to promote a mock response to a pre-recorded sample

A high-signal mock response is one that:

- Displays the failure mode clearly (not cartoonish, not subtle).
- Scores over ceiling (red-miss observed).
- Was not byte-identical to an existing pre-recorded sample.

Promotion path (manual — mock-mode does NOT auto-write the
ledger):

1. Copy the mock response from `trial_results[].response` in the
   report.
2. Author it into the scenario's `red_samples[]` array following
   `write-agent-behavior-scenario/SKILL.md` § "Adding red samples".
3. Use a `notes` field citing the mock-mode report's `run_id` for
   provenance.
4. Re-run `abv-run calibrate` — the new sample enters the stable
   pre-recorded library and the judge-integrity ledger thereafter.

This is the only path for mock-sourced evidence to enter the
judge-integrity ledger. The intermediate human step preserves the
ledger's stable-ID discipline and keeps triage grounded in
reviewed authoring judgment.

### Reading a mock-mode report

```bash
abv-run red-mode .claude/agent-behavior-scenarios/ \
    --output /tmp/red-mode-report.json

# Overall verdict
jq .overall_status /tmp/red-mode-report.json

# Per-mode summaries (compact)
jq '.mode_summaries[] | {mode, status, missed: .trials_missed,
  max_over_ceiling}' /tmp/red-mode-report.json

# Full per-trial records for deep review
jq '.trial_results[] | select(.status == "red-miss")' \
   /tmp/red-mode-report.json
```

Key fields:

- `canary_gate` — if `fail`, the sweep aborted; address the
  harness before interpreting anything else.
- `mode_summaries[].max_over_ceiling` — highest miss margin per
  mode. High margin = qualitative judge failure; small margin =
  could be honest variance.
- `mode_summaries[].mean_score` — the adversary's average score
  under this judge. Signal for `adversary-miscalibrated`: if mean
  is 10 for a ceiling-45 mode, adversary is producing
  cartoonish output; if mean is 44, adversary is producing subtle
  output that's barely-bad. Both call for adversary prompt
  revision, not judge changes.
- `trial_results[].response` — the mock response itself. Review
  when deciding whether to promote.

---

## When to escalate instead of triage

- **Multiple `confirmed-judge-miss` entries on the same criterion
  across scenarios.** This is systemic judge drift, not a per-sample
  triage problem. Flag via the proposal ledger
  (`agent-behavior-regression` category) and consider a judge-model
  rotation or a calibration audit (spec §15.3).
- **Every red sample in a scenario suddenly misses.** Something
  changed — agent-under-test behavior, a model version, the claude-
  proxy config. Don't triage; diagnose.
- **The same pending entry keeps reappearing after triage and
  re-calibration.** The fix isn't landing. Read the triage notes,
  look at what changed, and decide whether the original diagnosis
  was right.

---

## Relationship to other skills

- **`refine-agent-behavior/SKILL.md`** — sibling skill, different signal.
  Feedback-ledger entries come from the AGENT-UNDER-TEST's
  self-report; judge-integrity entries come from the LIVE JUDGE's
  red-miss behavior. Different files, different triage decisions,
  similar rigor.
- **`write-agent-behavior-scenario/SKILL.md`** — when a
  `scenario-rubric-gap` triage results in editing or adding a
  rubric criterion, this skill covers the authoring contract.
- **`plan-critic/SKILL.md`** — when the triage implies a systemic change
  (not a per-sample edit), run plan-critic on the proposed response
  before landing.

---

## Related reading

- `D:\Brains\LSP-Brains\spec\LSP-BRAINS-SPEC.md` §15.3 "Red samples"
  — normative basis for this skill.
- `D:\Brains\LSP-Brains\spec\METHODOLOGY-EVOLUTION.md` §12 — why
  this signal is worth tracking separately from gold calibration.
- `D:\Brains\NeuroGrim\docs\agent-behavior-red-taxonomy.md` — the
  v1 six-mode taxonomy + authoring guidance for new red samples.
- `D:\Brains\NeuroGrim\roadmap\epics\S9-agent-behavior-red-scenarios.md`
  — epic-level framing + adversarial review (plan-critic pass).
- `D:\Brains\agent-behavior-runner\agent_behavior_runner\judge_integrity_ledger.py`
  — ledger writer + reader (append-only, privacy allow-list).
- `.claude/skills/refine-agent-behavior.md` — sibling skill for
  feedback-ledger-driven refinement.
