# Domain Promotion Audit Runbook

**Who this is for:** operators deciding whether to promote an advisory-
weighted domain (e.g., `agent-behavior`) past 0.0, making it
load-bearing in the Brain's aggregate score + gates.

**What this document is:** the operational contract for declaring a
calibration audit "passed" (or "failed") on an advisory domain. The
`abv-run promote` CLI (S10-DP-2) REFUSES a promotion without audit
evidence attached; this runbook defines what that evidence looks
like and how to generate it.

**Spec reference:** LSP-Brains §15.5 "Promotion path" (v2.5). This
runbook is the reference implementation of that normative contract.

---

## Before you start

1. **Calibration harness operational.** You can run `abv-run calibrate`
   and `abv-run red-mode` against live API credentials via
   `claude-proxy`. A non-zero `CLAUDE_PROXY_UPSTREAM_KEY` is loaded
   into the proxy (NOT `ANTHROPIC_API_KEY` — the proxy deliberately
   reserves that env var for Claude Code CLI to keep Max-subscription
   billing intact); a scope token is issued to the runner.
2. **`ABV_OPERATOR` env var set** to your operator handle (e.g.,
   `keenan`). The CLI refuses ledger writes without this.
3. **Budget.** A full audit (2× Haiku + 1× Sonnet, see below) costs
   ~$3-5. Bugdet headroom before starting; re-runs on failures
   compound.
4. **Calibration + red-mode pass cleanly on sandbox profile at least
   once.** If routine `abv-run calibrate --profile sandbox` already
   fails, address that before attempting audit. Don't start an audit
   against known-broken plumbing.

---

## The two-profile ladder

| Profile | Model | Frequency | Cost/run | Role |
|---|---|---|---|---|
| Haiku | `claude-haiku-*` | 2 consecutive passes required | ~$0.20 | Routine audit + post-promotion weekly cadence |
| Sonnet | `claude-sonnet-*` | 1 pass required | ~$2-3 | Validation gate; required at audit + quarterly |
| Opus | `claude-opus-*` (rare) | occasional | ~$5-10 | Highest-fidelity tier; quarterly/annually for cross-tier baseline |

**Why two profiles.** Haiku is cheap enough to run on every
meaningful change (judge prompt edit, rubric tightening, library
expansion). Sonnet is the "production-grade" validation —
expensive but closer to the models real agents are graded by. Two
Haiku passes + one Sonnet pass gives statistical confidence
without $10/audit. Opus is available as an occasional tier for
cross-profile baseline data collection.

**Model calibration profiles matter.** Each model has its own
calibration profile — systematic patterns in how it reads rubrics
and scores middle-ground samples. Haiku tends to cluster scores at
extremes (low dynamic range on middle); Sonnet is ~5 points more
generous on partially-bad responses but ~5 points harsher on
clearly-bad extremes. Treating models as interchangeable loses
signal. **Every audit cycle SHOULD record the exact (model,
thinking-level) combination used** and cross-reference to
`NeuroGrim/docs/judge-calibration-profiles.md` where empirical
observations accumulate across audits. When you audit with a new
model or thinking configuration, add a profile entry BEFORE
interpreting the results.

**Extended thinking.** Anthropic's thinking feature gives the
judge additional internal token budget before producing the
scoring JSON. Enable via the `ABV_JUDGE_THINKING` env var (set to
the desired budget_tokens integer; unset = disabled). Thinking
is JUDGE-only — adversary/agent generation paths ignore the env
deliberately to keep adversary behavior consistent across judge
experimentation. Thinking-enabled audits cost more per call
(thinking tokens billed at input rates). Treat each (model,
thinking_budget) as a distinct profile; document findings
separately.

**Routine cadence after promotion** (required):
- Weekly: one Haiku calibrate + red-mode.
- Monthly: one Haiku red-mode full-sweep (all modes).
- Quarterly: one Sonnet validation audit.

---

## Audit procedure

### Step 1 — Haiku audit #1

```bash
export ABV_OPERATOR=<your-handle>
export ABV_AUDIT_OUT_DIR=.claude/brain/audit/$(date -u +%Y-%m-%d)
mkdir -p "$ABV_AUDIT_OUT_DIR"

# Calibration (gold + red samples via live Haiku judge)
abv-run calibrate .claude/agent-behavior-scenarios/ \
    --threshold 10 \
    --judge-integrity-ledger .claude/brain/judge-integrity-ledger.jsonl \
    > "$ABV_AUDIT_OUT_DIR/haiku-1-calibrate.json"

# Red-mode (mock-bad-agent novel generation)
abv-run red-mode .claude/agent-behavior-scenarios/ \
    --trials 3 \
    --output "$ABV_AUDIT_OUT_DIR/haiku-1-red-mode.json"
```

**Expected outputs:**
- `calibrate` exits 0 with `overall_status: "pass"`.
- `red-mode` exits 0 with `overall_status: "pass"`, `canary_gate:
  "pass"`.
- No new pending entries in `judge-integrity-ledger.jsonl` (triaged
  counts match pre-audit state).

**If any of these fail**, you're not in Step 2 — see "Handling
audit failure" below.

### Step 2 — Haiku audit #2 (≥ 24 hours later)

Same commands, new date-stamped output dir. The 24-hour gap
between audits catches day-of variance.

### Step 3 — Sonnet validation

```bash
# Same commands, but with a sonnet-preferring scenario override.
# Scenarios declare judge_model; operators can pass a per-run override
# via the ABV_JUDGE_MODEL env var for this audit purpose.
export ABV_JUDGE_MODEL="claude-sonnet-4.5"

abv-run calibrate .claude/agent-behavior-scenarios/ \
    --threshold 10 \
    --judge-integrity-ledger .claude/brain/judge-integrity-ledger.jsonl \
    > "$ABV_AUDIT_OUT_DIR/sonnet-calibrate.json"

abv-run red-mode .claude/agent-behavior-scenarios/ \
    --trials 3 \
    --output "$ABV_AUDIT_OUT_DIR/sonnet-red-mode.json"

unset ABV_JUDGE_MODEL
```

Same pass criteria as Haiku audits.

### Step 4 — Collect the evidence

By now `$ABV_AUDIT_OUT_DIR/` contains:
```
haiku-1-calibrate.json
haiku-1-red-mode.json
haiku-2-calibrate.json     (different date)
haiku-2-red-mode.json
sonnet-calibrate.json
sonnet-red-mode.json
```

These are the audit artifacts. They will be referenced by path in
the `abv-run promote` invocation.

### Step 4a — Record model + thinking-level observations

For each (model, thinking-level) combination used in this audit,
append an entry to (or extend an existing entry in)
`NeuroGrim/docs/judge-calibration-profiles.md`. Capture:
- Systematic bias observed vs human labels.
- Dynamic range on gold-good and gold-bad.
- Handling of middle-ground red samples.
- Any mode-specific behavior worth noting (especially the subtle
  modes: false-humility, culture-veneer, rubric-mimicry).
- Cross-reference to the audit artifact paths as empirical evidence.

Observations accumulate across audit cycles. Do NOT edit prior
profile content when a new audit produces different numbers —
add a dated observation. Profiles are empirical reality; if
today's run diverges from a week ago, that's signal, not noise
to overwrite.

---

## Pass criteria

The audit is "passed" iff ALL of the following hold:

1. **All three calibrate runs** (Haiku-1, Haiku-2, Sonnet) have
   `overall_status: "pass"` (not `drift-warning`, not
   `drift-blocker`, not `red-miss`, not `red-skipped`).
2. **All three red-mode runs** have `overall_status: "pass"` with
   `canary_gate: "pass"`.
3. **Judge-integrity ledger** shows zero unresolved pending entries
   (`abv-run judge-integrity list` returns empty).
4. **No triaged `confirmed-judge-miss`** in the last 30 days
   pointing at the scenarios being promoted.
5. **Max calibration drift** across all audit runs ≤ 10 points
   (matches `--threshold 10` default).
6. **Max red-sample over-ceiling** across all audit runs = 0
   (no red-miss — every pre-recorded red sample was caught).
7. **No mock-mode red-miss with max_over_ceiling > 15** on any
   (scenario, mode) pair across the three red-mode runs. (Small
   margins are honest variance; large margins suggest a mode the
   judge can't detect.)
8. **Cross-audit variance** on gold-sample scores ≤ 15 points
   (computed manually or via jq: `max(judge_score) - min(judge_score)`
   for each gold sample across the three audit runs).

If ALL eight hold, the audit is passed. Document the decision (next
step). If any FAILS, see "Handling audit failure."

---

## Declaring the audit passed

The audit decision is declared via the `abv-run promote` CLI, which
records the evidence in the promotion ledger atomically with the
weight change. There is no separate "declare audit passed" step —
the promote invocation IS the declaration.

**Before running `abv-run promote`**, verify:

- You've reviewed every audit artifact by eye (not just the overall
  status — skim the per-scenario and per-mode summaries).
- You've updated `NeuroGrim/.claude/brain/audit/README.md` (create
  if absent) with a brief narrative of the audit run, the operator
  identity, and a cross-reference to the audit artifact paths.
  Narrative is 3-5 sentences; think of it as the human-readable
  complement to the machine-readable ledger entry.

### For the `agent-behavior` domain in NeuroGrim (the first case):

```bash
export ABV_OPERATOR=<your-handle>

abv-run promote agent-behavior \
    --registry NeuroGrim/.claude/brain-registry.json \
    --to 0.05 \
    --rebalance proportional \
    --audit-report .claude/brain/audit/YYYY-MM-DD/haiku-1-calibrate.json \
    --audit-report .claude/brain/audit/YYYY-MM-DD/haiku-2-calibrate.json \
    --sonnet-audit-report .claude/brain/audit/YYYY-MM-DD/sonnet-calibrate.json \
    --red-mode-report .claude/brain/audit/YYYY-MM-DD/sonnet-red-mode.json \
    --notes "Initial agent-behavior promotion. Audit evidence: two Haiku runs + one Sonnet validation all pass; zero unresolved red-misses; zero triaged confirmed-judge-miss in prior 30 days. 14-day watch window begins."
```

The CLI:
1. Validates the registry's current `sum(domain_weights) == 1.0`.
2. Applies proportional rebalance: `test-health 0.40×0.95 = 0.38`,
   `code-quality 0.35×0.95 = 0.33`, `deploy-readiness 0.25×0.95 = 0.24`,
   `agent-behavior 0.05`. Sum = 1.00.
3. Writes the new registry.
4. Writes a backup copy at
   `.claude/brain/registry-backups/YYYY-MM-DD-<promotion-ts>.json`.
5. Appends a `promotion` entry to the ledger at
   `.claude/brain/domain-promotion-ledger.jsonl`.
6. Prints the diff + next-step reminders on stderr.

**Verify** immediately after:
```bash
# Registry sum should be exactly 1.0
python -c "import json; r = json.load(open('NeuroGrim/.claude/brain-registry.json')); print(sum(r['config']['domain_weights'].values()))"

# Ledger entry present
abv-run judge-integrity list --ledger .claude/brain/domain-promotion-ledger.jsonl
```

### Post-promotion obligations

Starting immediately:

- **Day 1–14**: run `abv-run promotion-watch` daily against the
  project's score history. Any `elevated` or `blocker` classification
  surfaces a proposal — review it the same day.
- **Weekly (indefinite)**: one Haiku calibrate + red-mode. Compare
  to the audit baseline.
- **Monthly**: one full Haiku red-mode sweep across every mode.
- **Quarterly**: one Sonnet validation. Record the evidence alongside
  the weekly Haiku artifacts in a new `audit/YYYY-Qn/` dir.

If at any point the cadence audit shows drift, the runbook's "Handling
audit failure" procedure applies — possibly followed by rollback.

---

## Handling audit failure

**The user-confirmed posture:** a failed audit STOPS the promotion
and SPAWNS a remediation epic. Do NOT retry until the remediation
work ships. "Green on the next run" is not the goal; green with
confidence is.

### Step 1 — Classify the failure

Read the failing report. The classification determines the
remediation path:

- **Calibration drift-warning or drift-blocker**: the judge's gold-
  sample scoring diverged from human labels by > 10 points on ≥ 1
  gold sample. Triage via `refine-agent-behavior/SKILL.md` (skill).
  Possible causes: judge model rotation, rubric ambiguity, gold-
  sample mislabel.
- **Red-sample miss** (`red-miss` on any red sample): judge failed
  to catch a failure mode the pre-recorded library asserts it
  should. Triage via `refine-judge-integrity/SKILL.md` (skill). Decide
  one of: `confirmed-judge-miss` → library expansion;
  `scenario-rubric-gap` → rubric edit + version bump;
  `mislabeled-red-sample` → sample retirement or ceiling change.
- **Mock-mode miss** (`red-miss` at a (scenario, mode) pair): judge
  failed on novel generation. Triage via `refine-judge-integrity/SKILL.md`
  § "Mock-mode triage". Additional fourth branch:
  `adversary-miscalibrated` → adversary prompt needs tightening,
  not the judge.
- **Cross-audit variance > 15**: judge output is too noisy. Possible
  causes: scenario rubric ambiguity, judge model version drift,
  insufficient trial count. Consider bumping `trials:` in the
  scenario YAMLs + re-auditing.
- **Canary gate failure**: harness-level break, not an agent-behavior
  issue. Address the harness first; re-audit later.

### Step 2 — Record the failed attempt

```bash
# Record the failed attempt — this is an append-only entry like
# `promotion` but with result: failed. Preserves the historical
# trail for future operators.
export ABV_OPERATOR=<your-handle>

abv-run promote agent-behavior \
    --registry NeuroGrim/.claude/brain-registry.json \
    --record-failed-attempt \
    --audit-report .claude/brain/audit/YYYY-MM-DD/haiku-1-calibrate.json \
    --failure-reason "drift-blocker on gold-bad for lsp-code-optimality scenario (judge scored 48 vs human 22 = +26 drift)" \
    --notes "Spawning remediation task: rubric phrasing for uses_lsp_native_tools criterion needs tightening."
```

(Implementation note: the `--record-failed-attempt` flag is part of
`abv-run promote` per S10-DP-2 — same command, different code path.
No registry change on a failed-attempt record.)

### Step 3 — Spawn remediation

Document the remediation scope and spawn the task. This is human
decision work. The failure classification determines what changes:

| Failure type | Remediation surface |
|---|---|
| Gold drift | Scenario rubric edit + scenario version bump + re-audit |
| Red-sample miss | Library expansion OR rubric tightening OR sample retirement |
| Mock-mode miss | Adversary prompt revision + taxonomy review |
| Cross-audit variance | Increase trial count + re-audit |
| Canary failure | Harness-level fix (out of scope for promotion) |

### Step 4 — Re-audit after remediation ships

Start from Step 1 of the main audit procedure with fresh date-stamps.
Do NOT reuse prior artifacts. The point of the re-audit is to verify
the remediation actually fixed the problem; old evidence doesn't
speak to post-remediation behavior.

---

## Rollback

If a promotion is active and swing detection (or manual review)
surfaces a blocker signal, the rollback procedure:

```bash
export ABV_OPERATOR=<your-handle>

abv-run rollback agent-behavior \
    --registry NeuroGrim/.claude/brain-registry.json \
    --reason "Sustained score-swing blocker over 7 days post-promotion; aggregate dropped 18 points without corresponding domain regressions (confirmed via neurogrim health)." \
    --notes "Rolling back to prior weights; spawning task to investigate judge variance cause."
```

The CLI:
1. Reads the most recent `promotion` entry for the domain from
   the ledger.
2. Restores the registry to the `rebalance_deltas` captured at
   promotion time (exact before-state).
3. Writes a `rollback` ledger entry referencing the promotion
   being reversed.
4. Prints the diff + next-step reminders.

Rollback does NOT delete the promotion ledger entry — the
append-only discipline preserves the full history. A subsequent
re-promotion after remediation appends a NEW promotion entry;
readers fold the stream to reconstruct the current state.

---

## Commands quick reference

| Operation | Command |
|---|---|
| Run an audit cycle (Haiku) | `abv-run calibrate` + `abv-run red-mode` (sandbox) |
| Run validation (Sonnet) | Same, with `ABV_JUDGE_MODEL=claude-sonnet-*` |
| List unresolved judge-integrity pendings | `abv-run judge-integrity list` |
| Triage a pending | `abv-run judge-integrity triage <id> --decision <d> --notes <t>` |
| Record a passed audit + execute flip | `abv-run promote <domain> --to <N> --audit-report <path> ...` |
| Record a failed audit (no registry change) | `abv-run promote <domain> --record-failed-attempt --failure-reason <t>` |
| Watch for post-promotion swings | `abv-run promotion-watch <score-history.json>` |
| Roll back a promotion | `abv-run rollback <domain> --reason <t>` |
| Inspect promotion history | `cat .claude/brain/domain-promotion-ledger.jsonl \| jq .` |

---

## Decision flowchart (tabletop exercise)

```
┌─────────────────────────────────────────────────────┐
│ Run Haiku audit #1 (calibrate + red-mode)          │
└───────────────────┬─────────────────────────────────┘
                    │
           ┌────────┴────────┐
           ▼                 ▼
      Pass criteria     Any failure
         ALL met            ANY
           │                 │
           ▼                 ▼
  ┌────────────────┐   ┌────────────────────┐
  │ Proceed to     │   │ STOP               │
  │ Haiku audit #2 │   │ Classify failure   │
  │ (24h+ later)   │   │ Spawn remediation  │
  └────────┬───────┘   │ Record failed      │
           │           │ attempt in ledger  │
      (same gate)      └────────────────────┘
           │
           ▼
  ┌────────────────┐
  │ Sonnet         │
  │ validation     │
  └────────┬───────┘
           │
      (same gate)
           │
           ▼
  ┌────────────────────────┐
  │ All three passed.      │
  │ Review artifacts by    │
  │ eye; write audit       │
  │ narrative README.      │
  └────────┬───────────────┘
           │
           ▼
  ┌────────────────────────┐
  │ abv-run promote        │
  │ → registry change +    │
  │   ledger entry +       │
  │   backup               │
  └────────┬───────────────┘
           │
           ▼
  ┌────────────────────────┐
  │ 14-day watch window.   │
  │ Daily promotion-watch. │
  │ Weekly Haiku audit.    │
  └────────────────────────┘
```

---

## Governance note

This runbook generalizes. The same procedure applies to any advisory
domain moving past 0.0: `git-health`, `rust-health`, `coherence`,
`human-comms`, etc. When promoting a domain other than
`agent-behavior`, substitute the domain-specific calibration
equivalent (e.g., for `coherence`, the audit evidence is the
coherence cross-domain report; for `secret-refs`, the provider-
manifest validation).

For domains without a calibration harness yet, the runbook's
evidence requirement becomes a methodology forcing function: build
the calibration before you promote. That's feature, not bug.

---

## Related reading

- `D:/Brains/LSP-Brains/spec/LSP-BRAINS-SPEC.md` §15.3 — judge
  protocol + calibration gate.
- `D:/Brains/LSP-Brains/spec/LSP-BRAINS-SPEC.md` §15.5 — promotion
  path (normative basis for this runbook, v2.5).
- `D:/Brains/LSP-Brains/spec/METHODOLOGY-EVOLUTION.md` §13 —
  governance-via-evidence methodology contribution.
- `D:/Brains/.claude/skills/refine-agent-behavior.md` — skill for
  triaging feedback-ledger-driven remediation.
- `D:/Brains/.claude/skills/refine-judge-integrity.md` — skill for
  triaging judge-integrity ledger entries.
- `D:/Brains/agent-behavior-runner/worked-example.md` — end-to-end
  examples of calibrate + red-mode flows.
- `D:/Brains/NeuroGrim/docs/agent-behavior-red-taxonomy.md` — the
  failure-mode taxonomy red samples grade against.
- `D:/Brains/NeuroGrim/roadmap/epics/S10-domain-promotion.md` —
  this epic's scoping + plan-critic review.
