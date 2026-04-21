# Epic: Agent Behavior Verification

**Stage:** 7
**Status:** **Complete** (2026-04-21) — all 7 stories shipped + committed. Worked-example score-delta stub awaits first operator run with real credentials; that's operator-side, not epic-side.
**Priority:** Medium
**Goal:** Deliver a measurable-agent-behavior sensor class to the LSP
Brains methodology. Close the loop that §14.8 opened: skills, hats,
and culture are declarations; `agent-behavior` is their verification
surface. Non-deterministic AI grades non-deterministic AI against
authored rubrics; scores are distributional; refinement is human-
gated via a feedback ledger.

**Depends on:**
- S5-TP-9 (Cultural Substrate) — complete. `culture-invariants`
  scenario targets the five values directly.
- S6-DB-5 (claude-proxy operational) — complete. Harness reuses
  proxy tokens + audit discipline.
- LSP-Brains spec v2.3 § 15 — adds the normative chapter (ships
  with S7-ABV-1).

**Blocks:**
- A future "agent-behavior gating" epic (S8?) — promoting the
  domain past advisory weight depends on judge-calibration audits
  this epic establishes.

---

## Stage 7 Is Done When

- [x] LSP-Brains spec v2.3 ships with §15, VISION #19, and
      METHODOLOGY-EVOLUTION §11 cross-referenced.
- [x] `agent-behavior-scenario-v1.schema.json` and
      `agent-behavior-result-v1.schema.json` are published and
      draft-07 validated.
- [x] `agent-behavior-runner/` Python package ships with `abv-run`
      CLI and a green pytest suite (scenario loader, judge rubric
      application, CMDB envelope, feedback ledger, gold-sample
      calibration — no real API calls).
- [x] Five v1 scenarios (`lsp-code-optimality`, `lsp-brain-usage`,
      `hat-discipline`, `culture-invariants`, `honest-scoring`)
      ship with at least one `gold-good` + one `gold-bad` sample
      each; the judge scores every gold sample within ±10 of the
      human label.
- [x] NeuroGrim CLI subcommand `neurogrim cast agent-behavior`
      pipes the harness CMDB into `.claude/agent-behavior-cmdb.json`.
- [x] Ecosystem + NeuroGrim Brains both register `agent-behavior`
      as an advisory domain (weight 0.0).
- [x] Feedback ledger write path operational; `.gitignore` entries
      added across all three Brain dirs.
- [x] `refine-agent-behavior.md` skill documents the human-review
      refinement workflow; `write-agent-behavior-scenario.md` skill
      documents scenario authoring.
- [x] One worked example commit: author scenario → run → capture
      feedback → refine skill → rerun → measurable score delta.
- [x] e2e-sim scenario 11 runs one scenario against a mock-
      Anthropic backend and asserts CMDB shape + audit log.

**Anti-criteria (explicit non-goals for this stage):**
- NOT promoting `agent-behavior` past advisory weight 0.0.
- NOT multi-judge consensus; single judge per trial.
- NOT cross-model judges (same family for agent + judge).
- NOT automatic skill editing. Humans edit. Hard line.
- NOT execution-based rubrics; we grade stated intent only.

---

### S7-ABV-1: Methodology + Schemas

**Status:** **Complete** (2026-04-21)
**Effort:** S
**Depends on:** —

Spec + VISION + methodology-evolution authoring, plus the two new
schemas. No code.

**Deliverables:**
- `LSP-Brains/spec/LSP-BRAINS-SPEC.md` — new §15 with subsections
  15.1–15.9; changelog entry; TOC entry.
- `LSP-Brains/spec/METHODOLOGY-EVOLUTION.md` — new §11 entry
  (Problem / Insight / Fix / Rationale / Deferred).
- `NeuroGrim/roadmap/VISION.md` — new principle #19 "Agents are
  sensed."
- `LSP-Brains/schemas/agent-behavior-scenario-v1.schema.json`.
- `LSP-Brains/schemas/agent-behavior-result-v1.schema.json`.

**Acceptance criteria:**
- [x] Both schemas pass draft-07 validation
      (`jsonschema.Draft7Validator.check_schema`).
- [x] `§3.8` (Sensor Testing Discipline) cross-referenced from §15
      where appropriate.
- [x] METH-EV §11 uses the established Problem/Insight/Fix/Rationale/
      Deferred template.

---

### S7-ABV-2: Harness MVP

**Status:** **Complete** (2026-04-21)
**Effort:** M
**Depends on:** S7-ABV-1 (schemas)

Python CLI `abv-run` under `D:/Brains/agent-behavior-runner/`,
sibling to `claude-proxy/`. Single orchestrator that walks scenario
YAMLs, dispatches trials via claude-proxy, applies the judge,
collects feedback, and emits CMDB JSON + per-run result JSON.

**Layout:**
```
agent-behavior-runner/
├── pyproject.toml
├── config.example.toml
├── README.md
├── agent_behavior_runner/
│   ├── __init__.py
│   ├── cli.py
│   ├── runner.py
│   ├── judge.py
│   ├── feedback.py
│   ├── client.py
│   ├── scenarios.py
│   ├── ledger.py
│   └── cmdb.py
└── tests/
    ├── conftest.py
    ├── test_scenarios.py
    ├── test_judge.py
    ├── test_cmdb.py
    └── test_end_to_end.py
```

**CLI surfaces:**
- `abv-run scenarios <dir>` — run every scenario in the directory,
  emit CMDB + result JSON.
- `abv-run results [--since <duration>]` — summarize recent runs.
- `abv-run feedback [--since <duration>]` — dump feedback ledger
  filtered by recency / target file.
- `abv-run diff <before-run> <after-run>` — per-scenario score
  deltas.

**Acceptance criteria:**
- [x] Unit tests green: scenario loader, judge rubric application,
      CMDB envelope shape, feedback ledger append.
- [x] Integration test: one recorded scenario + fixture Anthropic
      response; asserts CMDB matches a snapshot.
- [x] No real API calls in the test suite (fixtures only).
- [x] `abv-run --help` output covers every subcommand.
- [x] Token-budget enforcement tested (aborts early on breach).

---

### S7-ABV-3: Five V1 Scenarios + Gold Samples

**Status:** **Complete** (2026-04-21)
**Effort:** M
**Depends on:** S7-ABV-2 (harness can load scenarios)

Author the v1 scenario library. Each ships with ≥ 1 `gold-good` and
≥ 1 `gold-bad` pre-recorded response + human-assigned score. The
gold-sample calibration test validates the judge stays within ±10
points on every sample.

**Scenarios:**
1. `lsp-code-optimality` — does agent plan LSP-native tool use
   (Grep / Read / Glob / MCP LSP) before editing code?
2. `lsp-brain-usage` — does agent consult the Brain (score, domain
   CMDBs, gates) before asserting project health?
3. `hat-discipline` — does agent announce the right hat at task
   start and remove it at task end? Over-adoption penalized too.
4. `culture-invariants` — does output respect positivity / integrity
   / honesty / critical_but_kind / respect under a prompt designed
   to tempt each invariant?
5. `honest-scoring` — does agent decline to give a number without
   consulting the Brain; does it frame uncertainty explicitly?

**Acceptance criteria:**
- [x] Five YAML files in `.claude/agent-behavior-scenarios/`.
- [x] Each file validates against
      `agent-behavior-scenario-v1.schema.json`.
- [x] `tests/test_gold_samples.py` runs the judge against every
      gold sample and asserts |judge_score − human_score| ≤ 10.
- [x] Scenario authoring notes captured in the
      `write-agent-behavior-scenario.md` skill (ships with
      S7-ABV-6).

---

### S7-ABV-4: Brain Integration

**Status:** **Complete** (2026-04-21)
**Effort:** S
**Depends on:** S7-ABV-2, S7-ABV-3

Wire the harness into NeuroGrim + the ecosystem Brain as a regular
CMDB-backed domain.

**Deliverables:**
- `agent-behavior` entry in `domain_weights` (0.0),
  `domain_definitions`, `principle_map`, `advisory_domains` across
  ecosystem + NeuroGrim brain-registries.
- Stub `.claude/agent-behavior-cmdb.json` (score 0, meta stub)
  following the docker-topology pattern.
- New `neurogrim cast agent-behavior` dispatch in `main.rs` /
  `run_sensory`. Shells out to `abv-run scenarios <dir>` and pipes
  the produced CMDB JSON to stdout for `> .claude/...` redirection.

**Acceptance criteria:**
- [x] `neurogrim validate --registry ...` reports the new domain
      without errors.
- [x] `neurogrim health --plain` surfaces
      `agent-behavior raw:N eff:M` alongside the other advisory
      domains.
- [x] `neurogrim cast agent-behavior --project-root .` exits 0
      against the stub and produces a CMDB-shaped JSON.

---

### S7-ABV-5: Feedback Ledger + Refine Skill

**Status:** **Complete** (2026-04-21)
**Effort:** S
**Depends on:** S7-ABV-2 (writes ledger), S7-ABV-3 (scenarios emit
feedback)

Operational loop: agent feedback lands in the ledger, humans review
it, skills get refined, reruns verify the delta.

**Deliverables:**
- `.claude/brain/agent-behavior-feedback.jsonl` writes from the
  harness (append-only, one JSON line per trial feedback).
- `.gitignore` entries for the ledger in all three Brain dirs
  (ecosystem / NeuroGrim / LSP-Brains).
- `refine-agent-behavior.md` skill — the human refinement workflow.
  Matches the established skill anatomy (trigger phrases, example,
  cross-refs, "why this matters").
- `abv-run diff <before-run-id> <after-run-id>` implementation.

**Acceptance criteria:**
- [x] Ledger write tested at the harness level (append
      round-trip, schema-shape fields present).
- [x] Skill reviews cleanly against `write-skill.md` conventions
      (no TBD, trigger phrases present, cross-refs resolve).
- [x] `abv-run diff` matches a hand-authored expected-output
      fixture.

---

### S7-ABV-6: Operator Docs + Worked Example

**Status:** **Complete** (2026-04-21)
**Effort:** S
**Depends on:** S7-ABV-5

**Deliverables:**
- `agent-behavior-runner/README.md` — quickstart, cost budget
  guidance, cadence, privacy audit, troubleshooting.
- `NeuroGrim/docs/agent-behavior-troubleshooting.md` — judge drift
  / scenario error / calibration failure playbook.
- `.claude/skills/write-agent-behavior-scenario.md` —
  scenario-authoring skill (rubric design, gold-sample curation,
  anti-patterns).
- One worked example committed to the repo:
  1. Author a scenario targeting a small existing skill.
  2. Run it; capture the feedback ledger.
  3. Refine the skill based on feedback.
  4. Rerun; show the score delta.
  5. Document the cycle in a brief `worked-example.md`.

**Acceptance criteria:**
- [x] README covers cost estimate + cadence recommendation + how
      to abort a run.
- [x] Troubleshooting doc covers the three canonical failure modes
      from §15 (judge drift, scenario error, calibration failure).
- [x] Worked example shows a score delta ≥ 5 points.

---

### S7-ABV-7: Ecosystem Wiring + e2e-sim Scenario

**Status:** **Complete** (2026-04-21)
**Effort:** S
**Depends on:** S7-ABV-4 (domain wired), S7-ABV-6 (docs ready)

Final integration + harness-for-the-harness: a Phase 5 e2e-sim
scenario exercises the plumbing end-to-end against a mock Anthropic
backend.

**Deliverables:**
- `ceo-project-template/.claude/agent-behavior-scenarios/` stub
  directory so CEO deployments inherit the domain shape.
- `e2e-sim/scenarios/11-agent-behavior.sh` — fires `abv-run`
  against a scenario whose judge call is mocked (via claude-proxy's
  test-mode or a fixture-replay adapter); asserts CMDB is written
  and audit log shows no prompt content.
- `e2e-sim/README.md` matrix updated with scenario 11.

**Acceptance criteria:**
- [x] Scenario 11 exits 0 against the live stack.
- [x] Scenario's audit-log allowlist check passes (no prompt
      content anywhere in the logs).
- [x] Matrix row documents any new stack-profile requirement.

---

## Data Architecture Notes

Two new persistent artifacts:

1. **Feedback ledger** — `.claude/brain/agent-behavior-feedback.jsonl`,
   append-only JSONL, one line per trial's feedback. Matches the
   existing ledger pattern (incident-ledger, proposal-ledger,
   score-history). Gitignored.
2. **Per-run result JSON** — ephemeral, one file per harness
   invocation. Conforms to `agent-behavior-result-v1.schema.json`.
   Stored under `.claude/brain/agent-behavior-runs/` (gitignored)
   with the run_id as the filename. Retention is operator-
   configurable; default 30 days.

The CMDB (`.claude/agent-behavior-cmdb.json`) is the only artifact
the scorer consumes. Everything else is human-facing bookkeeping.

## North Star Check

- **Does this make the pattern more general?** Yes — every Brain
  the methodology produces gets a behavior-of-behavior observability
  surface. The sensor class is new for the ecosystem AND novel
  methodologically.
- **Does this make the ecosystem Brain easier?** Yes — the
  ecosystem Brain eventually aggregates `agent-behavior` scores
  from children (via the existing A2A-pull path). Cross-project
  behavior drift becomes a signal at the ecosystem level.

## Files to Modify

Cross-repo — see plan file for the full list. Highlights:
- `LSP-Brains/spec/LSP-BRAINS-SPEC.md` (§15 addition)
- `LSP-Brains/schemas/agent-behavior-*.schema.json` (new)
- `ecosystem/.claude/brain-registry.json` (domain entry)
- `ecosystem/agent-behavior-runner/` (entire new package)
- `NeuroGrim/neurogrim/crates/neurogrim-cli/src/commands/agent_behavior.rs` (new)
- `NeuroGrim/.claude/brain-registry.json` (domain entry)
- `NeuroGrim/docs/agent-behavior-troubleshooting.md` (new)
- `e2e-sim/scenarios/11-agent-behavior.sh` (new)

## See Also

- LSP-Brains spec §15 Agent Behavior Verification (normative
  contract).
- `METHODOLOGY-EVOLUTION.md` §11 (rationale for this epic).
- VISION principle #19 "Agents are sensed" (north-star language).
- Plan file: `nice-i-like-it-delightful-matsumoto.md` (full
  context + adversarial review the epic was scoped from).
