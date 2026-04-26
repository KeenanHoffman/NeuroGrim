# supply-chain-review — Layer 3 Agent-Assisted Human Review

The `supply-chain-review` framework is NeuroGrim's Layer 3
implementation of LSP-Brains v2.6 §16.4 — the human-decision tier.
It puts a structured **operator triage workflow** on top of the
findings produced by Layer 1 (`supply-chain-sca`) and Layer 2
(`supply-chain-vigilance`).

Shipped 2026-04-26 as part of E-SC-6.

## Why Layer 3 exists

Layer 1 catches **known-bad with high precision**. Layer 2 catches
**deep-signal anomalies** that precede confirmed compromise. Layer
3 puts a human in the loop **when the machine answer isn't
conclusive**.

Per spec §16.4 MUST: the **human decision is the gate**, not the
agent. v1 ships the framework (hat + decision-ledger writer +
review-ticket file format + CMDB sensor + auto-create bridge from
Layer 2 + CLI). The optional **automated LLM review step**
(running an LLM-as-judge against a flagged dep's diff and
flagged-file excerpts) is a post-v1 follow-on; in v1, operators
fill `agent_findings` by hand if they choose to consult an LLM
out-of-band.

## The five components

1. **`supply-chain-auditor` hat** — declared in
   [`.claude/skills/hats/SKILL.md`](../.claude/skills/hats/SKILL.md).
   The operator persona for package-level review. Paranoid-but-
   fair mindset; remembers the human decides, the agent advises.
2. **Decision ledger** —
   `.claude/supply-chain-decision-ledger.jsonl`. Append-only JSONL
   matching the [`supply-chain-decision-ledger-v1`](../../LSP-Brains/schemas/supply-chain-decision-ledger-v1.schema.json)
   schema. Five entry kinds: `accept`, `reject`,
   `pin-to-last-good`, `review-pending`, `review-triaged`.
3. **Review tickets** — JSON files at
   `.claude/brain/supply-chain-tickets/<id>.json`. Tickets are
   open work; the durable record lives in the ledger.
4. **`supply-chain-review` CMDB sensor** —
   `neurogrim sensory supply-chain-review`. Score model v1:
   `100 - 10 × open_tickets`, capped 0.
5. **CLI**: `neurogrim sca-review create | list | resolve`.

## Trust surface

What the framework depends on:

- **No new external dependencies.** The framework is pure-Rust
  reads/writes to JSONL + JSON files. No registry calls, no LLM
  calls (in v1), no package downloads.
- Trust on the §16.7 schema (versioned + closed).
- Trust on the operator who runs the CLI (the `--operator` flag
  and `NEUROGRIM_OPERATOR` env are self-asserted; v1 has no
  cryptographic operator identity).

Documented in `audit/TOOL-TRUST-NOTES.md` (2026-04-26 E-SC-6
entry).

### Partial / corrupt input handling (2026-04-26 PRE-RELEASE C20)

Two read paths are operationally important to understand:

- **`ledger::read_all`** (when the CMDB sensor reads
  `.claude/supply-chain-decision-ledger.jsonl` for scoring):
  malformed JSONL lines are **logged at `warn` level via
  `tracing`** (with line number + truncated content) and
  **skipped**. The sensor proceeds with the remaining valid
  entries. Rationale: a single corrupt entry must not block
  the entire CMDB from emitting.
- **`ledger::append`** (when the CLI writes a new entry): the
  entry is **validated against the §16.7 schema BEFORE write**.
  Schema-invalid entries are rejected at write time, not
  silently appended. The append itself uses POSIX-atomic
  single-line append.
- **`scripts/_supply-chain-bypass-check.py`** (the
  prepublish-check.sh strict gate): malformed JSONL lines are
  **FATAL** (exit code 2). Different posture than the sensor
  read because the gate is a publish-time correctness check —
  a corrupt entry could mask a real triage decision and the
  operator would unknowingly publish on a flawed audit trail.
  See `scripts/_supply-chain-bypass-check.py` module docstring
  for the C5 + C6 rationale.

Operator action when the sensor logs a parse warning:
1. Inspect the warning's line number + content.
2. Manually edit
   `.claude/supply-chain-decision-ledger.jsonl` to repair OR
   delete the malformed line. (Editing the append-only ledger
   in-place is a deliberate exception — the alternative is the
   gate stays red forever.)
3. Re-run the sensor; verify no warnings.

Operator action when prepublish-check.sh fails with rc=2 from
the bypass-check helper: same as above, but publish is blocked
until the ledger is repaired.

## Lifecycle

### 1. Ticket creation

A ticket can be created two ways:

#### a) Auto-create from Layer 2 vigilance findings

Whenever `neurogrim sensory supply-chain-vigilance` runs, each
finding (other than `sensor-degradation`) is checked against open
tickets via the dedup key `(ecosystem, package_name,
finding_kind)`. New findings auto-create tickets with
`created_by: "auto"`.

Repeated scans don't multiply tickets — the dedup key is stable.
Resolved tickets matching the dedup key DO produce a fresh ticket
(operator already decided once; recurrence is a new event worth a
fresh review).

#### b) Operator-initiated

```bash
neurogrim sca-review create \
    --project-root . \
    --ecosystem PyPI \
    --package litellm \
    --version 1.82.7 \
    --signal "manual:operator-spotted-base64-spike" \
    --note "noticed unusual base64 strings in upstream diff between 1.82.6 and 1.82.7" \
    --operator alice
```

Both paths write a `review-pending` entry to the ledger AND a
ticket file under `.claude/brain/supply-chain-tickets/`.

### 2. Triage

```bash
# See all open tickets:
neurogrim sca-review list --project-root . --open-only

# Or all tickets including resolved:
neurogrim sca-review list --project-root .
```

Sample output:

```
ID                     STATUS     PACKAGE          ECO        OPENED     SIGNALS
t-2026-04-26-0001      OPEN       litellm          PyPI       2026-04-26 vigilance:typosquat-proximity
t-2026-04-26-0002      OPEN       safe-package-x   crates.io  2026-04-26 vigilance:publish-cadence-acceleration
```

Wear the `supply-chain-auditor` hat:
> Wear Hat: supply-chain-auditor — triaging Layer 3 ticket t-2026-04-26-0001.

Then for each open ticket, work through the operational checklist:

1. **Provenance verification** — does the package's declared
   provenance match the registry's records?
2. **Diff intent** — read the diff between the prior known-good
   version and the flagged version. Look for the patterns the
   ticket's `triggering_signals` flagged.
3. **Remediation path** — when a finding is suspicious, prefer
   pin-to-last-good before remove/replace.
4. **Read-only static analysis** — never `npm install` /
   `pip install` / `cargo build` the flagged package while
   reviewing. Fetch tarballs and inspect; don't let install
   hooks fire.
5. **Non-attributive language** — describe behavior, not
   maintainer intent. "This package's release pattern…" not
   "Maintainer X introduced…"

### 3. Resolution

```bash
neurogrim sca-review resolve \
    --project-root . \
    --id t-2026-04-26-0001 \
    --decision pin-to-last-good \
    --from-version 1.82.7 \
    --to-version 1.82.6 \
    --note "Pinning to 1.82.6 pending upstream fix; will re-review when 1.82.10+ ships" \
    --operator alice
```

The resolve command:

- Validates `--decision` is one of `accept | reject |
  pin-to-last-good | no-action`.
- Validates `--note` is non-empty (operator MUST document
  rationale per spec §16.4).
- Appends a `review-triaged` ledger entry that supersedes the
  prior `review-pending` (via `supersedes_ts`).
- Updates the ticket file with `resolved_at`, `resolution`,
  `resolved_by`, `resolution_notes`.
- Does NOT delete the ticket file — the durable trail is the
  ledger; the ticket file is a convenient open-work index.

### 4. Score

```bash
neurogrim sensory supply-chain-review --project-root . \
    > .claude/supply-chain-review-cmdb.json
```

Score model: `100 - 10 × open_tickets`, capped 0. Score 100 = no
pending review work. Score deteriorates as tickets pile up.

Calibration discipline: this v1 model is intentionally simple.
E-SC-8 may move to a richer model (stale-pending weighting,
no-decision-on-flagged) once the workflow is exercised in the
field.

## Decision-ledger entry kinds

Per the [`supply-chain-decision-ledger-v1`](../../LSP-Brains/schemas/supply-chain-decision-ledger-v1.schema.json)
schema:

| Entry kind | When | Required fields |
|---|---|---|
| `review-pending` | Ticket opened (auto or manual). | package, triggering_signals[], schema_version |
| `review-triaged` | Ticket resolved. Supersedes a `review-pending` via `supersedes_ts`. | package, supersedes_ts, resolution, human_operator, human_notes |
| `accept` | Operator accepts the dep without going through ticket flow (rare). | package, human_operator, human_notes |
| `reject` | Operator rejects the dep without going through ticket flow. | package, human_operator, human_notes |
| `pin-to-last-good` | Operator pins to a known-good version without going through ticket flow. | package, from_version, to_version, human_operator, human_notes |

Note: most operator workflows go through `review-pending` →
`review-triaged`. The flat `accept` / `reject` / `pin-to-last-
good` kinds exist for ad-hoc use cases (e.g., manual import of
prior decisions from another system) and are validated equally.

## File layout

```
<project_root>/
├── .claude/
│   ├── supply-chain-decision-ledger.jsonl          (append-only durable record)
│   ├── supply-chain-review-cmdb.json               (per-scan output)
│   ├── brain/supply-chain-tickets/
│   │   ├── t-2026-04-26-0001.json
│   │   ├── t-2026-04-26-0002.json
│   │   └── ...
│   └── skills/hats/SKILL.md                        (supply-chain-auditor hat declared)
└── docs/supply-chain-review.md                     (this guide)
```

The ledger + tickets directory may be gitignored or committed
depending on operator preference. Committing makes review
decisions auditable across the team; gitignoring keeps decisions
private to each operator's machine. NeuroGrim itself commits
both for dogfooding visibility.

## CMDB output shape

| Field | Meaning |
|---|---|
| `score` | 0–100 per rubric (100 - 10 × open_tickets) |
| `findings[]` | One per open ticket (operator-readable) |
| `tickets_open` | Open ticket count |
| `tickets_resolved_total` | Resolved ticket count |
| `tickets_total` | All tickets |
| `ledger_entries_total` | Total ledger entries |
| `decision_kinds_seen` | Per-kind counter from the ledger |
| `latest_decision_per_package` | Folded latest-state per `(eco, name)` |
| `score_model` | `"open-ticket-count-v1"` |

## Graceful degradation

- **No tickets directory** → score 100 (clean state). First-run.
- **No ledger file** → empty ledger; no folded state. First-run.
- **Malformed ticket JSON** → logged + skipped; sensor continues.
- **Malformed ledger line** → logged + skipped; sensor continues.
- **CLI fails to write ticket** → ledger entry NOT appended (tx-
  consistent at the framework level: ledger is appended FIRST in
  the CLI flow; if ticket write fails, you have an orphan
  pending entry that subsequent `list` command does NOT show
  but the CMDB counts. v1 tradeoff; v2 candidate: write ticket
  first, then ledger).

## What v1 does NOT do (deferred)

- **Automated LLM-as-judge** — out-of-band in v1. Operator can
  invoke any LLM (Claude Code session, ChatGPT, etc.), then fill
  `agent_findings` field manually before resolve. E-SC-6b candidate.
- **Cross-Brain finding aggregation via A2A `supply-chain-signal`
  message type** — speced in §16.6 (E-SC-7); wire-up is E-SC-10.
- **Ticket signing/integrity** — JSON files are operator-curated;
  v2 candidate if real forgery scenarios surface.
- **Auto-trigger from Layer 1 SCA findings** — only auto-trigger
  from Layer 2 in v1. Layer 1 advisories already have the
  `.claude/supply-chain-accepted-advisories.toml` triage path.
- **Stale-ticket auto-close** — open tickets stay open until
  resolved. v2 candidate: warn / auto-close after configured
  timeout.

## Cross-references

**Spec (LSP-Brains v2.6, 2026-04-25):**

- [§16.4 Layer 3 — Agent-assisted Human Review](../../LSP-Brains/spec/LSP-BRAINS-SPEC.md)
- [§16.5 The supply-chain-auditor Hat](../../LSP-Brains/spec/LSP-BRAINS-SPEC.md)
- [§16.7 Schemas](../../LSP-Brains/spec/LSP-BRAINS-SPEC.md)
- [METHODOLOGY-EVOLUTION §15](../../LSP-Brains/spec/METHODOLOGY-EVOLUTION.md)

**Schemas:**

- [`supply-chain-decision-ledger-v1.schema.json`](../../LSP-Brains/schemas/supply-chain-decision-ledger-v1.schema.json)
- [`a2a-supply-chain-signal-v1.schema.json`](../../LSP-Brains/schemas/a2a-supply-chain-signal-v1.schema.json) (cross-Brain signal; wire-up E-SC-10)

**Companion guides:**

- [`docs/supply-chain-sca.md`](supply-chain-sca.md) — Layer 1
- [`docs/supply-chain-vigilance.md`](supply-chain-vigilance.md) — Layer 2

**Plans + scaffolding:**

- Ecosystem plan: `~/.claude/plans/parallel-hugging-eich.md` (E-SC-6)
- Per-epic plan: `~/.claude/plans/parallel-hugging-eich-e-sc-6.md`
- Rollback procedures: `audit/ROLLBACK-PLAYBOOK.md § E-SC-6`
- Trust-chain notes: `audit/TOOL-TRUST-NOTES.md` 2026-04-26 entry
- BEFORE-PUBLIC-RELEASE.md gate 11
