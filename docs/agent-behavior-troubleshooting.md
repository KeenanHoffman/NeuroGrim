# Agent-Behavior Verification — Troubleshooting Playbook

When the `agent-behavior` domain misbehaves, it's almost always one of
three canonical failure modes documented in LSP-Brains spec §15. This
document walks each one — symptoms, diagnosis, and the remediation
that actually works — so operators don't have to re-derive the
response under pressure.

**Context:** harness lives at `D:/Brains/agent-behavior-runner/`. CMDB
lands at `<brain>/.claude/agent-behavior-cmdb.json`. Ledger lives at
`<brain>/.claude/brain/agent-behavior-feedback.jsonl`. Per-run result
records land at `<brain>/.claude/brain/agent-behavior-latest.json`.

Before diving into a specific failure: **run the harness's own test
suite first.**

```bash
cd D:/Brains/agent-behavior-runner
py -3 -m pytest -q
```

If the 35 no-API tests fail, the harness itself is broken and none of
the below matters — fix that first. If they pass, the harness is
mechanically sound and the issue is in the run path (network, proxy,
judge, or scenario data).

---

## Failure mode 1: Scenario error

**Symptoms:**

- `neurogrim cast agent-behavior` exits 0 but the CMDB shows
  `score: 0` with a finding whose `name` is one of
  `scenarios_dir`, `abv_proxy_env`, `abv_run`, or `compose_file`.
- `abv-run scenarios …` prints a `ScenarioError:` or aborts with
  `error initializing client`.
- Harness reaches `sync_workspace`/judge for some scenarios but not
  others; the result record shows `errors > 0`.

### Diagnosis

`abv-run` pre-flights every requirement before firing the first API
call. Read the finding `detail` field — it names the specific miss.

| Finding `name:status` | Meaning | First move |
|---|---|---|
| `scenarios_dir:missing` | No `.claude/agent-behavior-scenarios/` under `--project-root` | Point at the right root (usually `D:/Brains`) |
| `scenarios_dir:empty` | Directory exists but has no `*.yaml` files | Add scenarios per spec §15.2 or point `--project-root` elsewhere |
| `abv_proxy_env:missing` | `ABV_PROXY_URL` or `ABV_PROXY_TOKEN` unset | Issue a scope token: `cd D:/Brains/claude-proxy && proxy-cli issue --label abv-run-operator`; export both |
| `abv_run:missing` | `abv-run` not on PATH from the `neurogrim` dispatch | `py -3 -m pip install -e D:/Brains/agent-behavior-runner`; open a new shell |
| `abv_run:failed` | Harness started but exited non-zero. `detail` carries the first ~300 chars of stderr | Read the detail. Common: `ScenarioError: schema error` (bad YAML), `ClientError: /v1/messages returned 401` (bad/revoked token) |
| `abv_run:timeout` | 10-minute outer ceiling reached | Run with fewer scenarios, smaller trial counts, or invoke `abv-run` directly (no outer ceiling) |

### Common sub-causes

**Scenario YAML fails schema validation.** The Python harness runs
draft-07 validation before any API call. Fix the YAML by re-reading
the schema:

```bash
py -3 -c "
import json, jsonschema, yaml
schema = json.load(open('D:/Brains/LSP-Brains/schemas/agent-behavior-scenario-v1.schema.json'))
doc = yaml.safe_load(open('D:/Brains/.claude/agent-behavior-scenarios/<name>.yaml'))
jsonschema.Draft7Validator(schema).validate(doc)
print('OK')
"
```

Common shape errors:
- `rubric[].weight` must be an integer, not a float. Edit to `40`, not `40.0`.
- `rubric[].description` has a minimum length (20 chars).
- `target.kind` must match the exact regex — `general`, `skill:<n>`,
  `hat:<n>`, or `culture:<invariant>`. No trailing spaces.
- `gold_samples[].human_score` must be 0–100 inclusive, integer.
- `version` is a string of digits (`"1"`), not a number.

**Claude-proxy down or unreachable.** Sanity check:

```bash
curl -sS http://127.0.0.1:4545/health
```

If that hangs or 404s, claude-proxy isn't running. Bring it back up
per `D:/Brains/claude-proxy/README.md`.

**Bind-mount visibility on Windows.** A known Docker Desktop + WSL2
quirk — documented in `D:/Brains/e2e-sim/README.md` Known limitations.
Usually surfaces as: you issued a fresh proxy token but the container
still rejects it. Workaround: restart the proxy container.

---

## Failure mode 2: Judge drift

**Symptoms:**

- Per-scenario scores drop 20+ points run-over-run, with NO edits to
  the target skill/hat/culture file.
- One gold sample that used to score ~85 now scores ~60 (or vice
  versa).
- Harness exits non-zero with `judge-drift-blocker` somewhere in the
  stderr (when the implementation adds that signal — §15.3 specifies
  it; v1 reports drift but still writes the CMDB).

### Diagnosis

Drift has four distinguishable causes. In likelihood order:

1. **Anthropic silently rotated the model** behind your scenario's
   `judge_model` string. The model string is an alias; the underlying
   weights change. Confirm by comparing the `model_used_judge` field
   in `.claude/brain/agent-behavior-latest.json` across runs.
2. **Judge prompt was edited** and the edit changed its calibration.
   `git log -- agent-behavior-runner/agent_behavior_runner/judge.py`.
   Any change to `build_judge_prompts` affects every gold sample
   baseline.
3. **Scenario rubric was edited without a version bump**. Rubric
   changes invalidate prior scores; scenarios with changed rubrics
   should carry a new `version` string so they're not aggregated with
   old runs.
4. **Temperature / sampling variance**. 3 trials is enough to catch
   systematic drift but not enough to characterize distributional
   noise. A single outlier can swing the mean 5-10 points.

### Diagnostic procedure

```bash
# 1. Snapshot the current state.
cp D:/Brains/.claude/brain/agent-behavior-latest.json /tmp/abv-suspect.json

# 2. Run the gold-sample calibration test (no API calls — validates
#    the rubric is still proportionally scorable).
cd D:/Brains/agent-behavior-runner && py -3 -m pytest tests/test_gold_samples.py -v

# 3. Re-run the harness at 5+ trials on the drifting scenario.
#    Higher trial count averages out variance.
# (manually edit the scenario to `trials: 7` temporarily, then run)

# 4. Compare.
abv-run diff /tmp/abv-suspect.json \
    D:/Brains/.claude/brain/agent-behavior-latest.json
```

If the 7-trial run lands within 5 points of the 3-trial run:
**variance, not drift**. The 3-trial distribution is too thin for
this rubric; bump `trials` for this scenario.

If the 7-trial run still shows the drop: **real drift**. Time to
respond per §15.3.

### Response per spec §15.3

1. **Freeze the gold samples.** Do NOT edit them to accommodate the
   new judge. They are the baseline; if they change, you lose the
   ability to detect future drift.
2. **Record the drift in the proposal ledger.** A new entry with
   `category: "agent-behavior-regression"`, `severity` derived from
   the drop magnitude, `linked_scenario_id`, and a pointer to the
   feedback cluster.
3. **Promote to human attention.** Drift is not something the
   harness can auto-resolve — a human has to decide whether to pin
   the judge to a specific model version, rewrite the judge prompt,
   or accept the new calibration as the new normal.
4. **If you accept the new normal:** bump `scenario.version` and
   snapshot the gold samples at the new-calibration scores. Every
   prior run becomes incomparable — which is honest.

Until human-resolved, keep `agent-behavior`'s weight at 0.0 (advisory).

### The calibration gate (S8-ABV-EXT-1)

As of S8-ABV-EXT-1, the harness runs a live-judge calibration pass
BEFORE each trials run (`abv-run scenarios`) and refuses to emit a
trustworthy CMDB when the max per-sample drift exceeds the threshold
(default 10 points). To invoke calibration without running trials:

```bash
abv-run calibrate D:/Brains/.claude/agent-behavior-scenarios/ --threshold 10
```

Exit code: `0` on pass / drift-warning, `4` on drift-blocker. The
stdout is a JSON drift report conforming to
`calibration-report-v1.schema.json`; stderr has a human-readable
summary.

Common verdicts:

- **`drift-blocker` with `systematic_bias` ≈ 0, high `max_drift`**
  — the judge is noisy on specific samples but not uniformly
  biased. First suspect: the model rotated AND a specific
  scenario's rubric happens to sit in a region of that rotation's
  weakness. Rerun calibration 2-3 times; if drift persists only on
  one scenario, version-bump that scenario's rubric.
- **`drift-blocker` with large `systematic_bias`** (e.g., +18
  across all samples) — uniform generosity / harshness. Nearly
  always the judge prompt changed OR the model rotated to a
  different personality family. Pin the judge model explicitly
  in each scenario YAML before anything else.
- **`drift-warning`** — every sample within 2×threshold. Scenarios
  run but the CMDB carries `judge_calibration.status:
  drift-warning` so downstream consumers know trust is reduced.
  Refine rubrics OR tighten the threshold OR switch models.

If drift-blocker is recurring and you need to iterate on scenarios
while it's unblocked, the `--skip-calibration` flag on `abv-run
scenarios` is the iteration-mode escape hatch. The CMDB emitted
under `--skip-calibration` is flagged `judge_calibration.status:
skipped` so no consumer accidentally trusts it.

**Never promote `agent-behavior` past advisory weight while
calibration is blocking.** That's the main thing this gate is for.

---

## Failure mode 3: Calibration failure

**Symptoms:**

- `tests/test_gold_samples.py::test_ideal_judge_recovers_human_scores_within_10_points` fails.
- A gold-good sample scores ≤ 50 or a gold-bad sample scores ≥ 70
  under an ideal-judge fixture.

### Diagnosis

This is the "the scenario itself is broken" signal — distinct from
judge drift because it fires against a deterministic ideal judge, not
a live LLM. When this test fails, the rubric cannot reach the human-
intended score with ANY allocation of rubric points. Either:

1. The rubric weights don't sum to enough to cover the human_score
   (a `gold-good` at 90 against a rubric whose max is 70).
2. The human_score was assigned in a way no rubric-follower would
   reproduce (the human judged the response on axes the rubric
   doesn't name).
3. A recent rubric edit reduced the max-achievable score below a
   human_score that was calibrated against the prior rubric.

### Response

1. **Bump `scenario.version`.** Rubric changes + human-score changes
   both invalidate prior aggregate data; versioning is how you signal
   that. Old runs are not comparable to new runs.
2. **Either raise the rubric max OR lower the human_score.**
   - If the rubric max was dropped below reasonable human scores, the
     rubric change is the bug. Re-add weight to the omitted criteria.
   - If the human assigned a score the rubric doesn't justify, the
     human was wrong. Re-score the sample against the rubric literally
     and use that number.
3. **Re-run the calibration test locally** before pushing:
   ```bash
   cd D:/Brains/agent-behavior-runner
   py -3 -m pytest tests/test_gold_samples.py -v
   ```
4. **Document the delta** in the scenario's inline comment — why the
   human_score changed, whether the rubric changed, which run broke
   it.

---

## Not-quite-failure: uncertain reads

Some observations look like failures but are expected non-determinism:

| Observation | Why it's fine |
|---|---|
| Same scenario scores 68, 74, 62 across three back-to-back runs | Single-trial variance. Look at `mean_score` + `score_stddev`; if stddev < 10 and mean is stable across trial batches, the scenario is behaving. |
| A scenario passes on Monday, fails on Tuesday, passes on Wednesday | Same as above. The pass/fail threshold is a single cut through a distribution; low-signal scenarios will straddle it. Solution: more trials OR widen the pass threshold, never both. |
| Feedback ledger has entries for scenarios that "passed" | Feedback is elicited on every trial regardless of pass/fail. The agent may have passed on the rubric while still having useful signal to share. |
| CMDB `score` differs slightly from the per-scenario means' arithmetic average | By design — the CMDB rolls up per-scenario `mean_score` values with equal weighting, and the per-scenario mean excludes error trials. Small drift vs a naïve average is expected. |
| `score_stddev > 15` for a single scenario | Probably under-specified rubric. Either the criteria descriptions are too vague, or the prompt admits too wide a range of valid responses. Refine via `refine-agent-behavior.md`. |

---

## When to escalate to a human

Past the self-serve paths above, escalate to a human (via
`.claude/brain/proposal-ledger.json`) when:

- **Drift persists after model-id pinning.** The judge's bias has
  shifted underneath us in a way pinning didn't fix.
- **Calibration failures recur across multiple scenario rewrites.**
  The rubric format itself may be the problem, not any individual
  rubric.
- **Gold samples disagree with each other.** If two gold-good samples
  rated by different humans land 20 points apart, the scenario's
  prose isn't specific enough for consistent grading — that's a
  methodology issue worth pausing to address.
- **`agent-behavior` score contradicts the aggregated Brain score
  persistently.** The Brain says project health is 75 and agent-
  behavior says the agents working on the project are at 30. Dig in.

The escalation entry should carry: scenario id, observation window,
drift magnitude, the three diagnostic data points above (snapshot,
7-trial rerun, gold-sample test result), and your current hypothesis.

---

## Spec cross-references

- §15.3 — judge protocol + calibration
- §15.4 — distributional interpretation
- §15.5 — feedback loop + refinement safety rail
- §15.7 — privacy + cost discipline
- §14.8 — culture drift sensor (generalized into §15)
- VISION principle #18 — "sensors need sensors"
- VISION principle #19 — "agents are sensed"
- METHODOLOGY-EVOLUTION §11 — rationale
