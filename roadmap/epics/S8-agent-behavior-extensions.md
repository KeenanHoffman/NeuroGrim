# Epic: Agent Behavior Verification Extensions

**Stage:** 8
**Status:** Planning complete (2026-04-21) — planned to ship three
stories (calibration audit → multi-judge consensus → execution-based
rubrics). Each story unlocks the next.
**Priority:** Medium
**Goal:** Close the path from "advisory-only" to "production-gateable"
for the `agent-behavior` domain. S7-ABV shipped the measurement; S8-ABV-EXT
makes the measurement trustworthy enough to attach consequences.

**Depends on:** S7-ABV (complete) — harness, scenarios, Brain
integration, feedback loop.

**Blocks:** a future "agent-behavior gating" epic (S9?) — promoting
the domain past advisory weight depends on having a calibrated judge
(EXT-1), consensus variance data (EXT-2), and ideally execution-grade
evidence (EXT-3).

---

## Stage 8 Is Done When

- [ ] `abv-run calibrate` command ships; drift report schema stable;
      harness refuses to write a trustworthy CMDB when calibration
      fails, but can be forced past the gate with a `--skip-calibration`
      flag for iteration.
- [ ] Scenario schema supports `judge_models: [...]` for multi-judge
      consensus; runner invokes N judges per trial and takes the median;
      per-criterion variance is recorded in the result.
- [ ] Scenario schema supports `tools: [...]` for execution-based
      rubrics; the harness passes the tool schema to Anthropic's
      `/v1/messages`, captures `tool_use` content blocks, and surfaces
      them to the judge so rubrics can grade actual calls.
- [ ] `agent-behavior-runner/tests/` retains 100% no-API coverage;
      test count grows by ~15 to ~50 total.
- [ ] e2e-sim scenario 11 remains green (harness plumbing unchanged).
- [ ] At least one ecosystem scenario authored that uses the
      multi-judge + execution-based path end-to-end.

**Anti-criteria (explicit non-goals for this stage):**
- NOT promoting `agent-behavior` past advisory weight. EXT-1 ships
  the calibration gate but the weight flip is a separate decision.
- NOT per-project rubric overrides.
- NOT subprocess-mode Claude Code (agent-under-test still via
  Anthropic API only).
- NOT cross-provider judges (OpenAI / Gemini as judge). One model
  family per epic; cross-provider stretches post-EXT.

---

### S8-ABV-EXT-1: Calibration Audit

**Status:** Not started
**Effort:** M
**Depends on:** —

Implement spec §15.3's calibration protocol as a first-class harness
surface. Currently `tests/test_gold_samples.py` validates the rubric
is proportionally scorable against an IDEAL deterministic judge; this
story adds LIVE-judge calibration against the real API + a gate that
refuses to emit a trustworthy CMDB when the live judge has drifted.

**Deliverables:**

- `abv-run calibrate <scenarios-dir>` subcommand — runs one judge
  call per gold sample, builds a drift report, emits JSON on stdout
  + an operator-readable summary on stderr.
- `calibration-report-v1.schema.json` in LSP-Brains — documents the
  report shape (reuses fields already declared in
  `agent-behavior-result-v1.schema.json` under `judge_calibration`).
- `abv-run scenarios` integration: calibration runs automatically
  before trials. On drift > threshold (default 10), harness refuses
  to write a "trustworthy" CMDB — still writes one, but with
  `judge_calibration.status: "drift-blocker"` and a top-level
  finding `name: "calibration:failed"`, score clamped to 0.
- `--skip-calibration` flag for iteration mode (writes a CMDB
  flagged `status: "skipped"` — explicitly lower trust).
- `--calibration-threshold N` flag (default 10).
- Update `refine-agent-behavior.md` skill with the calibration-
  failure triage pattern.
- Update `worked-example.md` with a calibration-failure-recovery
  sub-section.

**Acceptance criteria:**
- [ ] `abv-run calibrate --help` covers the subcommand.
- [ ] Unit test: drift-report math (per-sample delta, overall
      max_drift, status classification with threshold boundaries).
- [ ] Unit test: `abv-run scenarios` with a scripted judge that
      hits the drift threshold refuses to write a trustworthy CMDB.
- [ ] Gold-sample calibration schema validates as draft-07.
- [ ] e2e-sim scenario 11 still passes (no regression).

---

### S8-ABV-EXT-2: Multi-Judge Consensus

**Status:** Not started
**Effort:** M
**Depends on:** S8-ABV-EXT-1 (calibration understands multi-judge output)

Extend the scenario schema to support a list of judge models per
scenario. When present, the harness invokes N judges per trial and
takes the median per-criterion score. Per-criterion variance across
judges is recorded in the result. This reduces single-judge bias
and gives operators signal on which criteria are inherently noisy.

**Deliverables:**

- Scenario schema extension — optional `judge_models: [string, ...]`
  array. Either `judge_model` (singular, string) or `judge_models`
  (plural, array) MUST be present. Singular form remains the
  default for backwards compatibility.
- `judge.py` — `score_response` grows a `judge_clients: list`
  parameter. When N > 1, calls all N, aggregates via median per
  criterion.
- `runner.py` — reads scenario's judge list, dispatches N calls,
  records per-criterion variance in `trial.rubric_variance`.
- Token accounting: multi-judge runs are N× the cost. README
  updated; `--profile sandbox` remains the default to keep N=3
  affordable.
- `refine-agent-behavior.md` — when per-criterion variance is
  high (> 15 points), the signal is "judges disagree," distinct
  from "agent is inconsistent." Document the new triage fork.

**Acceptance criteria:**
- [ ] Unit test: median-of-three aggregation handles one outlier
      correctly.
- [ ] Unit test: two-judge tie breaks by higher score (documented
      contract).
- [ ] Integration test: three scripted clients returning distinct
      rubric scores; harness emits median + variance correctly.
- [ ] Scenario schema examples include a multi-judge scenario that
      passes validation.

---

### S8-ABV-EXT-3: Execution-Based Rubrics

**Status:** Not started
**Effort:** L
**Depends on:** S8-ABV-EXT-2 (consensus + calibration are the
trust layer that makes execution-grade evidence usable)

v1 grades stated intent: "agent said it will use Grep." EXT-3 lets
scenarios grade actual tool calls: "agent actually called Grep with
these args." Requires passing a tool schema to Anthropic's
`/v1/messages` API, capturing `tool_use` content blocks in the
response, and surfacing them to the judge as structured evidence.

**Deliverables:**

- Scenario schema extension — optional `tools: [name, ...]`. Names
  resolve to tool schemas in a shared library at
  `D:/Brains/.claude/agent-behavior-tools.yaml` (initial set: Grep,
  Read, Glob — the three LSP-native tools most of v1's rubrics
  reference).
- Tool schema library YAML with three initial tools. Shape matches
  Anthropic's tool schema:
  `{name, description, input_schema: JSON-Schema-object}`.
- `client.py` — `respond()` gains optional `tools` param. When
  provided, sends `tools` in the API request; parses `content[]`
  for both `text` and `tool_use` blocks; returns a structured
  `AgentResponse` (new type) rather than a raw string.
- `runner.py` — when the scenario declares tools, runs the agent
  in structured mode, collects tool_use blocks, renders a structured
  summary for the judge (format: `"\n[tool calls]\n- Grep(pattern=X)\n
  - Read(path=Y)"` appended to the text response).
- Judge sees the tool calls in the response block it grades; rubric
  descriptions can reference tool use ("agent called Grep with a
  specific pattern rather than `.*`"). No new criterion type needed
  in v1 — prose rubric descriptions suffice.
- New scenario authored: `lsp-code-execution.yaml` — same prompt as
  `lsp-code-optimality` but uses the execution-based path. Side by
  side they show what the extension buys.

**Acceptance criteria:**
- [ ] Tool schema library validates as draft-07 (needs a meta-schema
      for tool definitions).
- [ ] Unit test: agent-response parser handles mixed text + tool_use
      content correctly.
- [ ] Unit test: scenarios without `tools` behave exactly as before
      (no regression to existing 5 v1 scenarios).
- [ ] `lsp-code-execution.yaml` passes gold-sample calibration with
      a tool_use-rich gold-good response and a text-only gold-bad
      response.
- [ ] Worked example updated with execution-based example.

**Biggest risk:** tool schema maintenance. Every new capability
scenarios want to grade execution of requires a new tool schema.
Scope this by shipping 3 tools in v1; treat tool-library growth as
a separate operator concern with its own governance (like scenario
authoring).

---

## Data Architecture Notes

- `calibration-report-v1.schema.json` joins the LSP-Brains schemas.
  Same versioning policy — additive stays v1.
- Tool schema library YAML lives at
  `D:/Brains/.claude/agent-behavior-tools.yaml`, ecosystem-level.
  Per-project overrides would live at
  `<project>/.claude/agent-behavior-tools.yaml` (stretch; not in
  this epic).
- Per-run result records grow a `judge_calibration` block (already
  declared in the schema; EXT-1 populates it).
- Per-trial records grow a `rubric_variance` block (EXT-2) and a
  `tool_calls[]` block (EXT-3).

## North Star Check

- **Does this make the pattern more general?** Yes — calibration
  audit is the pattern for trusting any LLM-as-judge system; multi-
  judge consensus is the pattern for reducing model-specific bias;
  execution-based rubrics is the pattern for grading actions over
  declarations.
- **Does this make the ecosystem Brain easier?** Indirectly — by
  making agent-behavior scores trustworthy enough to surface
  prominently, the ecosystem Brain can cite them in cross-project
  reports without caveat.

## Files to Modify

Cross-repo. Highlights:

- `LSP-Brains/schemas/calibration-report-v1.schema.json` (new, EXT-1)
- `LSP-Brains/schemas/agent-behavior-scenario-v1.schema.json` —
  additive extensions for `judge_models` (EXT-2) and `tools` (EXT-3)
- `agent-behavior-runner/` — substantial code across cli.py,
  runner.py, judge.py, client.py; new `calibrator.py`
- `.claude/agent-behavior-tools.yaml` (new, EXT-3)
- `.claude/agent-behavior-scenarios/lsp-code-execution.yaml`
  (new, EXT-3)
- `.claude/skills/refine-agent-behavior.md` — triage forks for
  calibration failure + multi-judge variance
- `NeuroGrim/docs/agent-behavior-troubleshooting.md` — calibration
  gate, multi-judge disagreement, execution parser errors

## See Also

- LSP-Brains spec §15.3 — judge protocol + calibration (the
  normative basis for EXT-1)
- LSP-Brains spec §15.4 — distributional interpretation (EXT-2
  adds cross-judge dimension)
- S7-ABV epic — the preceding epic this builds on
- `agent-behavior-runner/worked-example.md` — gets updated with
  EXT-1/2/3 examples as each lands
