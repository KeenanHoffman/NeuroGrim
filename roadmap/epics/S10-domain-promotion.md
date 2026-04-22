# Epic: Domain Promotion Past Advisory Weight

**Stage:** 10
**Status:** Infrastructure Complete (2026-04-21) — S10-DP-1/2/3
shipped + pushed across all three repos. 259/259 pytest green.
S10-DP-4 (the actual NeuroGrim weight flip) stays guarded-pending on
operator-led calibration audit per the runbook; epic closes fully
when operator executes the flip.
**Priority:** Medium

**Goal.** Ship the **mechanism** by which any advisory-weighted domain
can be safely promoted to a non-zero weight — then apply it to the
`agent-behavior` domain in NeuroGrim as the first dog-food case.

**Framing.** S7-ABV introduced the measurement. S8-ABV-EXT made the
measurement trustworthy. S9-ABV-RED proved the measurement can detect
failures. S10 governs the transition from "trustworthy advisory" to
"trustworthy and load-bearing." The policy work here is heavier than
the code — which is why most of this epic is doc + runbook + audit
protocol, not new algorithms.

**Absorbs:** BACKLOG B-01 (promote `agent-behavior` past advisory
weight) — originally framed as agent-behavior-specific; this epic
generalizes the mechanism so other advisory domains (`git-health`,
`rust-health`, `coherence`, `human-comms`, `secret-refs`,
`security-standards`) can follow the same path.

**Depends on:**
- S8-ABV-EXT (calibration gate, multi-judge consensus, execution-based
  rubrics) — complete.
- S9-ABV-RED (red-sample library, judge-integrity ledger, mock-mode) —
  complete.
- Operator-led calibration audit on live API credentials — **pending;
  S10-DP-4 is guarded on its successful completion.**

**Blocks:**
- Future epics that promote other advisory domains past 0.0. After
  S10-DP ships, they all route through the same runbook + CLI.

---

## Stage 10 Is Done When

- [ ] `NeuroGrim/docs/domain-promotion-audit.md` operator runbook
      ships, covering: audit protocol (commands, expected output,
      cost), pass/fail criteria, two-profile guidance (Haiku routine,
      Sonnet validation), rollback procedure, post-promotion cadence.
- [ ] LSP-Brains spec §15.5 gains "Promotion path" subsection; METH-EV
      §13 records the governance contribution; spec v2.5 (additive)
      ships.
- [ ] `abv-run promote <domain>` + `abv-run rollback <domain>` CLI
      subcommands ship with rebalance discipline (auto-proportional,
      explicit, or refuse); tests green.
- [ ] `domain-promotion-ledger-v1.schema.json` published; writer +
      reader module in the harness; append-only entries record
      from/to weights + audit evidence + operator identity.
- [ ] `abv-run promotion-watch` surfaces a proposal when
      post-promotion score swings exceed a threshold computed against
      score-history stddev; rollback remains human-initiated.
- [ ] Harness pytest suite green (173 → ~200+, preserving 100%
      no-API coverage).
- [ ] S10-DP-4 documented with exact rebalance delta (0.40→0.38,
      0.35→0.33, 0.25→0.24, 0.0→0.05) and guarded on operator audit
      — NOT executed by this epic; handoff documented.

**Anti-criteria (explicit non-goals):**
- NOT the actual NeuroGrim weight flip. That is S10-DP-4, pending
  operator audit. Infrastructure is ready; execution is operator-led.
- NOT ecosystem-Brain promotion. Every ecosystem domain stays
  advisory (0.0) in v1. A broader "ecosystem weighting philosophy"
  decision is a future epic.
- NOT automatic rollback. The system FLAGS when rollback should be
  considered (via proposal); humans execute.
- NOT multi-weight gradient promotion (e.g., 0.025 → 0.05 → 0.075
  over time). v1 is single-step promotion; further bumps are new
  audits + new promote calls.
- NOT per-branch weight overrides. One registry, one weight set.

---

### S10-DP-1: Operator Audit Runbook + Spec Subsection

**Status:** **Complete** (2026-04-21) — shipped in NeuroGrim `483e80a` (445-line runbook at `docs/domain-promotion-audit.md` covering audit protocol, two-profile Haiku/Sonnet ladder, eight pass criteria, five-branch failure classification, rollback procedure, post-promotion cadence, tabletop decision flowchart, and governance note on generalization to other advisory domains) and LSP-Brains `56bf97c` (spec §15.5 "Promotion path" subsection with SHALL-level requirements + METH-EV §13 governance-via-evidence entry + v2.5 additive changelog bump).
**Effort:** S
**Depends on:** —

Normative + operational doc package. Answers "what makes a calibration
audit 'pass' and who decides?" so the subsequent stories have a
grounded definition.

**Deliverables:**

- `NeuroGrim/docs/domain-promotion-audit.md` (new) — the operator
  runbook. Covers:
  - **Audit protocol**: commands, flags, expected duration, cost per
    profile. Two-profile ladder: Haiku for routine audits (~$0.20
    sandbox), Sonnet as an occasional validation gate (~$2 full,
    quarterly or before major flips).
  - **Pass criteria**: 2 consecutive Haiku audits + 1 Sonnet audit
    with `overall_status: "pass"` on both calibration AND red-mode,
    zero unresolved pending entries in the judge-integrity ledger,
    no triaged `confirmed-judge-miss` in the last 30 days, at least
    one red-mode sweep showing `overall_status: "pass"` at the
    scenario level.
  - **Fail criteria**: any `drift-blocker`, any unresolved red-miss,
    cross-audit variance above threshold. **A failed audit STOPS
    the promotion and SPAWNS a remediation epic** — does not loop
    on "re-try until green." (This is the user-confirmed posture:
    audit failure is itself a useful signal, not a thing to paper
    over.)
  - **Operator declaration**: how to record "I ran the audit, here's
    the evidence, I declare it passed" — operator writes to the
    promotion ledger via the S10-DP-2 CLI.
  - **Rollback procedure**: when to roll back, what evidence to
    record, the CLI invocation, the proposal that surfaces.
  - **Post-promotion cadence**: required calibration runs weekly
    (Haiku), red-mode sweeps monthly, full Sonnet validation
    quarterly.

- `LSP-Brains/spec/LSP-BRAINS-SPEC.md` §15.5 — new "Promotion path"
  subsection formalizing the audit → promote → watch → rollback
  cycle. Normative RFC-2119 language: implementations that promote
  `agent-behavior` past advisory weight SHALL record the audit
  evidence; MAY follow the cadence recommendations in the reference
  implementation's runbook.

- `LSP-Brains/spec/METHODOLOGY-EVOLUTION.md` §13 — new entry
  (Problem / Insight / Fix / Rationale / Deferred) documenting the
  promotion-governance methodology contribution.

- `LSP-Brains/spec/LSP-BRAINS-SPEC.md` header — bump v2.4 → v2.5 with
  additive changelog entry.

**Acceptance criteria:**
- [ ] Runbook validates against a tabletop exercise: given a sample
      calibration report + red-mode report, an operator can follow
      the runbook to a clear pass/fail decision.
- [ ] Spec subsection reviewed for RFC-2119 discipline.
- [ ] METH-EV §13 cites the runbook + names the generalization
      (applies to any advisory domain, not just agent-behavior).

---

### S10-DP-2: Promotion + Rollback CLI + Ledger

**Status:** **Complete** (2026-04-21) — shipped in LSP-Brains `8d4ceff` (domain-promotion-ledger-v1 schema with three entry types: promotion / rollback / failed-attempt) and ecosystem `4c8195d` (`promotion_ledger.py` with ABV_OPERATOR guard + three-way folding; `registry.py` with three rebalance strategies preserving sum=1.0; `abv-run promote` + `abv-run rollback` CLI with audit-evidence enforcement, registry backup, --dry-run preview, --record-failed-attempt path; 59 new tests across promotion_ledger / registry / promote_cli).
**Effort:** M
**Depends on:** S10-DP-1 (the runbook defines what the CLI enforces)

The executable half: a CLI that mutates a registry's weights in a
governed way + an append-only ledger that records the decision.

**Deliverables:**

- `LSP-Brains/schemas/domain-promotion-ledger-v1.schema.json` (new) —
  schema for promotion ledger entries. Two entry types:
  - `promotion`: records an advisory → weighted transition (or a
    weight change within weighted). Fields: `ts`, `domain`,
    `registry_path`, `from_weight`, `to_weight`, `rebalance_deltas`
    (per-domain before/after), `audit_evidence` (pointer to
    calibration + red-mode run_ids + sonnet_run_id + operator
    declaration), `operator`, `notes`.
  - `rollback`: reverses a prior promotion. Fields: `ts`, `domain`,
    `registry_path`, `supersedes_ts` (the promotion entry's ts),
    `restored_weights` (full before-state), `reason`, `operator`,
    `notes`.

- `agent_behavior_runner/promotion_ledger.py` (new) — writer +
  reader, mirroring `judge_integrity_ledger.py`. ABV_OPERATOR env
  guard on both writes; decision-enum validation; append-only
  discipline.

- `agent_behavior_runner/registry.py` (new) — registry
  read/mutate/write helper. Reads the JSON, validates the
  `domain_weights` structure, applies a rebalance strategy:
  - `proportional`: auto-shrink every existing non-advisory domain
    proportionally so `sum == 1.0` post-change.
  - `explicit`: caller supplies `--domain=weight` pairs; validates
    sum.
  - `none`: refuses the change if `sum != 1.0`.
  Preserves the original registry on disk (backup file stamped
  with the promotion's run_id).

- `agent_behavior_runner/cli.py` additions:
  - `abv-run promote <domain> --registry <path> --to <weight>` with
    flags: `--rebalance <strategy>`, `--explicit <domain=weight>`
    (repeatable), `--audit-report <path>` (evidence),
    `--sonnet-audit-report <path>` (evidence for the full-profile
    gate), `--notes <text>`, `--dry-run`.
    ABV_OPERATOR env required.
  - `abv-run rollback <domain> --registry <path> --reason <text>
    [--to-promotion-ts <ts>]` — reverts to the state before the
    named promotion (default: most recent promotion for that
    domain). ABV_OPERATOR required.

- Tests in `tests/test_promotion_ledger.py` +
  `tests/test_registry.py` + `tests/test_promote_cli.py`.

**Acceptance criteria:**
- [ ] Schema validates both entry types with draft-07.
- [ ] Unit test: proportional rebalance preserves `sum == 1.0`
      within ±0.001 for floating-point tolerance.
- [ ] Unit test: explicit-mode rejects deltas that don't sum to 1.0.
- [ ] Unit test: rollback restores the exact before-state recorded
      in the promotion entry's `rebalance_deltas`.
- [ ] Unit test: ABV_OPERATOR guard fires on promote + rollback.
- [ ] Unit test: `--dry-run` writes nothing and exits 0 with a
      preview-diff on stdout.
- [ ] Unit test: attempting to promote without `--audit-report`
      refuses (the CLI MUST verify audit evidence is attached).
- [ ] Integration test: against a fixture registry, promote from
      advisory, verify ledger + registry + backup; rollback, verify
      restoration.

---

### S10-DP-3: Score-Swing Detection + Proposal Surfacing

**Status:** **Complete** (2026-04-21) — shipped in ecosystem `049a381` (`swing_detector.py` with `classify_swing` + zero-stddev fallback, ISO 8601 timestamp parser tolerating Z/offset/nanosecond variants, `detect_post_promotion_swing` with baseline/recent windowing; `abv-run promotion-watch` CLI reading score-history + promotion-ledger; 27 new tests). Deliberately does NOT auto-write to NeuroGrim's Rust-managed proposal-ledger (external writes would race the scoring pipeline); operators review the report + decide. Exit 4 on blocker for CI visibility.
**Effort:** S
**Depends on:** S10-DP-2 (ledger provides the "promotion happened at
ts X" signal that the detector keys off of)

Post-promotion safety net. Reads the project's `score-history.json`
and flags score swings that exceed stddev-based thresholds. Surfaces
a proposal ledger entry so the existing NeuroGrim Rust machinery
shows the signal in `neurogrim agent` output. Rollback remains
human-initiated; the detector just says "consider rolling back."

**Deliverables:**

- `agent_behavior_runner/swing_detector.py` (new) — reads
  `score-history.json` (per-project path), consults the promotion
  ledger for the domain's most recent promotion timestamp, and
  computes:
  - Last N runs' aggregate scores (N configurable, default 20).
  - Stddev and mean.
  - Delta from the pre-promotion baseline (windowed mean).
  - Swing-severity classification: `normal` | `elevated` |
    `blocker`.
  - When classification is `elevated` or `blocker`, emit a proposal
    with `category: "domain-promotion-swing"`, `severity: high`
    (for blocker) or `medium` (elevated), `linked_promotion_ts`,
    and a recommended action (watch / consider-rollback).

- `agent_behavior_runner/cli.py` — new subcommand
  `abv-run promotion-watch <score-history.json>` with flags:
  `--promotion-ledger <path>`, `--domain <name>`, `--window N`,
  `--threshold-stddev 2.0`, `--proposal-ledger <path>` (writes
  there when threshold exceeded).

- Tests in `tests/test_swing_detector.py` + CLI smoke test.

**Acceptance criteria:**
- [ ] Unit test: score history with a single large post-promotion
      dip triggers `blocker` classification.
- [ ] Unit test: score history that's stable post-promotion returns
      `normal` with no proposal written.
- [ ] Unit test: score history with no matching promotion in the
      ledger is a no-op (exits 0, no writes).
- [ ] Unit test: the proposal entry matches `proposal-ledger.json`
      schema (reuses existing structure; S10 only adds a category
      tag).
- [ ] Integration test: end-to-end with fixture score history +
      fixture promotion ledger → proposal ledger receives the
      expected entry.

---

### S10-DP-4 (pending-operator): Dog-food flip in NeuroGrim

**Status:** **Pending B-08 remediation** (2026-04-22) — audit #1
was attempted against live Haiku 4.5 + Sonnet 4.5 via
claude-proxy. Calibrate passed cleanly (6/6 scenarios, max_drift
= 10, zero within-scenario red-misses) across 7 shipped
remediation increments (judge JSON hardening + false-specifics &
false-humility library expansion + rubric tightening + gold-bad
re-labeling + ceiling adjustment + canary prompt tightening +
model-id typo fix). Red-mode surfaced a cross-scenario
mode-applicability structural issue documented in BACKLOG B-08:
6/36 (scenario × mode) pairs red-missed due to modes authored
for specific scenarios producing inevitable ceiling violations
when cross-pollinated with scenarios whose rubrics don't penalize
that surface. Failed-attempt ledger entry recorded at
`NeuroGrim/.claude/brain/domain-promotion-ledger.jsonl`
(classification: `mock-mode-miss`, ts 1776834120.37,
operator=claude-code-session). Do not retry audit against
current config; post-B-08 remediation will restore a meaningful
red-mode pass criterion.

Total live-audit cost through audit #1: $0.9322 (Sonnet
adversary + Haiku judge) across 309 API calls. Infrastructure
fully proven end-to-end; promotion path is operator-ready once
B-08 resolves the red-mode cross-scenario gating semantics.
**Effort:** S (once unblocked)
**Depends on:** S10-DP-1..3 complete; operator-led calibration audit
passed per the runbook in S10-DP-1.

The actual rebalance in NeuroGrim:
- Before: `test-health=0.40, code-quality=0.35, deploy-readiness=0.25,
  agent-behavior=0.0` (sum = 1.00).
- After:  `test-health=0.38, code-quality=0.33, deploy-readiness=0.24,
  agent-behavior=0.05` (sum = 1.00).
- Rebalance strategy: proportional on the three weighted domains
  (0.40×0.95, 0.35×0.95, 0.25×0.95 ≈ 0.38, 0.33, 0.24; slight
  rounding to land exactly on sum=1.00).

**Operator handoff:**

When operator is ready:

```bash
# Record audit evidence (paths to calibrate + red-mode reports)
export ABV_OPERATOR=keenan
abv-run promote agent-behavior \
    --registry NeuroGrim/.claude/brain-registry.json \
    --to 0.05 \
    --rebalance proportional \
    --audit-report audit/2026-04-XX-haiku-calibrate.json \
    --sonnet-audit-report audit/2026-04-XX-sonnet-calibrate.json \
    --notes "Initial dog-food promotion; 14-day watch window begins."
```

- Promotion ledger records the event.
- Registry file updates; backup written to `.claude/brain/
  registry-backups/YYYY-MM-DD-<promotion-ts>.json`.
- 14-day watch window begins; `abv-run promotion-watch` runs daily
  (operator cadence).
- If swing detected → proposal surfaces → operator decides
  rollback or iterate.

**Acceptance criteria (for when this eventually runs):**
- [ ] Registry change applied; sum = 1.00 preserved.
- [ ] Promotion ledger entry written with full audit evidence.
- [ ] Backup of original registry stored.
- [ ] Neurogrim score pipeline continues to produce valid scores
      post-flip (no crash, no NaN).
- [ ] Watch window runs for 14 days with no blocker-severity swing.

---

### S10-DP-5: Spec update — Promotion Path normatively documented

**Absorbed into S10-DP-1** above. See S10-DP-1's LSP-Brains spec
§15.5 and METH-EV §13 deliverables.

---

## Adversarial review (plan-critic hat)

### 🔴 Blocking

*None.* The mechanism ships standalone; the flip is governed by
operator evidence and reversible via a single CLI call. No changes
compel promotion action.

### 🟡 Concerns

1. **Rebalance is lossy for projects on narrow score margins.** A
   project currently passing a gate at score 71 with threshold 70
   could tip below after proportional rebalance even if no underlying
   domain scores worse — the weights shifted. The promotion ledger
   records the exact deltas so post-hoc analysis is possible, but
   operators should expect some projects to show "score moved" for
   non-substantive reasons on the flip day. Mitigation: document this
   in the runbook as expected behavior for 14-day watch window;
   distinguish "rebalance drift" from "real regression" in the swing
   detector's classification.

2. **The audit protocol can't fully eliminate judge noise.** Even
   with 2 Haiku + 1 Sonnet consecutive passes, a bad cycle later is
   possible. Mitigation: post-promotion, require weekly Haiku +
   quarterly Sonnet on a cadence; document this as an obligation,
   not a suggestion. S10-DP-3's swing detector is the short-term
   safety net between cadence checks.

3. **Operator identity in the ledger is self-asserted.** ABV_OPERATOR
   env var isn't authentication. If the ledger's integrity matters,
   git commit signing + commit-gated promotion would be stronger.
   Scoped out for v1 (matches the existing judge-integrity-ledger
   posture) but worth flagging for a future hardening epic.

4. **Cost escalates across the audit ladder.** Sandbox Haiku is
   ~$0.10-0.30 per audit; Sonnet validation is ~$2-3. Quarterly
   Sonnet validation across all domains eventually promoted
   compounds. Mitigation: runbook makes the cost explicit; operator
   chooses the cadence; the two-profile ladder keeps routine audits
   cheap.

5. **"Audit failure → spawn remediation epic" needs a clean
   handoff.** When audit fails, we don't re-try. Instead the
   runbook prescribes: document the failure in the promotion ledger
   as an unsuccessful-attempt entry (new shape?), spawn a task for
   the remediation work, re-run only after remediation ships.
   Mitigation: add an `attempt` entry kind to the ledger schema —
   same shape as promotion but with `result: failed` and notes —
   so the historical trail preserves failed attempts.

6. **Ecosystem Brain still at zero weights.** The epic explicitly
   scopes ecosystem out, but operators running `neurogrim score`
   against the ecosystem after NeuroGrim's promotion might expect
   symmetric behavior. Runbook documents this asymmetry; a future
   epic addresses ecosystem weighting.

7. **Score-swing detection is a lagging indicator.** By the time
   the detector fires, the bad scores already propagated into the
   score-history. That's unavoidable for swing detection — it's
   inherently historical. Mitigation: pair with the existing
   calibration cadence so prevention happens before swings are
   observed.

### 🔵 Suggestions

- **Backups** stored under `.claude/brain/registry-backups/`
  timestamped by promotion_ts. Makes "what did the registry look
  like before this flip?" a stat operation.
- **Dry-run by default** for promote/rollback commands. Force
  operators to pass `--apply` to actually mutate. Reduces fat-
  finger risk on a consequential command.
- **`abv-run promotion-status <registry>`** — read-only query: "are
  any domains currently promoted? when? audit evidence links?" —
  useful for ops review without grepping the ledger.
- **Watch-window automation via scheduled agent trigger** (future,
  not v1). A cron agent runs `abv-run promotion-watch` daily for
  14 days post-flip; surfaces swings automatically.

### 🟢 Strengths

- **Generalizes.** Any advisory domain can promote via this path.
- **Evidence-first.** Audit evidence is required; lack of evidence
  blocks the CLI.
- **Reversible.** Rollback is a single command; audit trail
  preserved through both directions.
- **Separation of infrastructure from action.** This epic ships the
  mechanism; the actual NeuroGrim flip waits on operator evidence.
  The epic can close "successful infrastructure delivery" without
  requiring a policy commitment.

---

## Files to Modify

**LSP-Brains:**
- `schemas/domain-promotion-ledger-v1.schema.json` (new)
- `spec/LSP-BRAINS-SPEC.md` §15.5 subsection + v2.5 changelog bump
- `spec/METHODOLOGY-EVOLUTION.md` §13 entry

**ecosystem (`D:/Brains/`):**
- `agent-behavior-runner/agent_behavior_runner/promotion_ledger.py` (new)
- `agent-behavior-runner/agent_behavior_runner/registry.py` (new)
- `agent-behavior-runner/agent_behavior_runner/swing_detector.py` (new)
- `agent-behavior-runner/agent_behavior_runner/cli.py` — promote,
  rollback, promotion-watch subcommands
- `agent-behavior-runner/tests/test_promotion_ledger.py` (new)
- `agent-behavior-runner/tests/test_registry.py` (new)
- `agent-behavior-runner/tests/test_promote_cli.py` (new)
- `agent-behavior-runner/tests/test_swing_detector.py` (new)
- `.gitignore` — `.claude/brain/domain-promotion-ledger.jsonl` +
  `.claude/brain/registry-backups/`

**NeuroGrim:**
- `docs/domain-promotion-audit.md` (new) — operator runbook
- `.gitignore` — promotion ledger + registry backups

**NOT modified in this epic (deliberate):**
- `NeuroGrim/.claude/brain-registry.json` — registry stays
  unchanged; S10-DP-4 changes it once operator audit passes.
- Ecosystem + LSP-Brains registries.

---

## Verification Plan

1. **Schema validation** — `domain-promotion-ledger-v1` validates
   example promotion + rollback + attempt entries.
2. **Unit tests green** — every helper + CLI command tested with
   fixture registries, no real API calls.
3. **Runbook tabletop** — given a fixture calibration report +
   red-mode report, runbook produces an unambiguous pass/fail
   decision in under 5 minutes.
4. **Integration test** — against a fixture registry, promote
   (proportional + explicit), rollback, attempt-failure; verify
   ledger entries + registry state at each step.
5. **CLI help strings** — every new subcommand documents its flags.
6. **No regression** in existing 173 pytest suite.

---

## Scope Limits (v1)

- NO actual NeuroGrim weight flip (S10-DP-4 is operator-led).
- NO ecosystem Brain promotion.
- NO automatic rollback.
- NO gradient promotion across multiple weight steps.
- NO per-branch registry overrides.
- NO commit-signed operator authentication (ABV_OPERATOR env only).
- NO scheduled cron trigger for promotion-watch (operator-manual).

---

## Commit Strategy

Three logical commits, mirroring stage discipline from S9-ABV-RED:

1. **S10-DP-1** — LSP-Brains (spec §15.5 + METH-EV §13 + v2.5
   changelog) + NeuroGrim (runbook doc). Docs-only commits across
   both repos.
2. **S10-DP-2** — LSP-Brains (schema) + ecosystem (promotion
   ledger + registry helper + CLI + tests) + gitignore updates.
3. **S10-DP-3** — ecosystem (swing detector + CLI + tests).

S10-DP-4 is not committed by this epic; its commit happens when the
operator runs `abv-run promote` and commits the registry change +
auto-generated ledger entry.

---

## Verdict

**PROCEED** with S10-DP-1..3 as a governance + infrastructure
epic. The promotion mechanism is load-bearing for `agent-behavior`
AND for any future domain that wants to move past advisory. The
separation of "ship the mechanism" from "execute the flip" keeps
the epic honest — we don't over-claim a policy decision we haven't
actually made. The operator-handoff for S10-DP-4 is explicit; when
ready, one CLI call executes the flip and records the evidence.

Expected scope: ~600 LOC Python + ~350 LOC tests + ~500 words of
spec + ~500 words of METH-EV + ~800 lines of runbook. Roughly 2
focused sessions to ship S10-DP-1..3.
