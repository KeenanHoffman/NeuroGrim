# Backlog

Future-work items identified but not yet planned into epics. Each
entry is "known, not-yet-scoped" — when an item moves to active
planning, extract it to `roadmap/epics/S<N>-<slug>.md` and update
this backlog entry with a pointer.

**Lifecycle rule:** backlog items stay here until:
1. An operator / maintainer is ready to scope them → they become an
   epic file + a Stage row in `ROADMAP.md`.
2. They're explicitly closed as won't-do with a brief rationale.
3. They're absorbed into another epic (document the absorption here).

**Last updated:** 2026-04-22 (CapProto planning session: pre-committed B-10 Phase 1 decision criteria; added B-11 cross-Brain skill dedup; wrote S11 epic stub at `epics/S11-capability-protocol.md` per operator's "partial anchor" choice — no ROADMAP.md row until B-10 Phase 3 passes. Prior 2026-04-22 entries: B-09 CLI-mode sensory, B-10 LSP-style lazy context loading — both surfaced during S10 session close-out as per-session tooling-overhead concerns).

---

## Identified 2026-04-21 (post S8-ABV-EXT close-out)

These were the "NOT in scope" items from the S8-ABV-EXT epic (see
`epics/S8-agent-behavior-extensions.md`). They're recognized as
real future work; none is blocking current operations.

### B-01: Promote `agent-behavior` past advisory weight — ABSORBED into S10-DOMAIN-PROMOTION

**Absorption** (2026-04-21). Generalized from agent-behavior-specific
to a domain-promotion mechanism applicable to any advisory domain
(`git-health`, `rust-health`, `coherence`, `human-comms`, etc.).

**Original framing.** S8-ABV-EXT shipped the trust infrastructure
(calibration gate, multi-judge consensus, execution-based rubrics).
S9-ABV-RED shipped the detection infrastructure (red samples, judge-
integrity ledger, mock-bad-agent). The weight flip from 0.0 to > 0.0
— so the domain actually moves the Brain's aggregate score and gates
— is a policy decision, not a code change.

**Current scoping.** See
`epics/S10-domain-promotion.md`. Four stories:
- S10-DP-1: operator audit runbook + spec §15.5 subsection +
  METH-EV §13 + v2.5 spec bump.
- S10-DP-2: `abv-run promote` + `abv-run rollback` CLI + ledger +
  registry rebalance helper.
- S10-DP-3: `abv-run promotion-watch` — post-promotion score-swing
  detection + proposal surfacing.
- S10-DP-4 (pending operator audit): the actual NeuroGrim dog-food
  flip (0.40/0.35/0.25/0.0 → 0.38/0.33/0.24/0.05 via proportional
  rebalance).

### B-02: Cross-provider judges (Claude + GPT as mixed consensus)

**Why it's here.** Multi-judge consensus (S8-ABV-EXT-2) reduces
single-judge variance but every judge is still Claude — same
training family, likely shared blind spots. A mixed-provider
consensus (e.g., 2 Claude judges + 1 GPT judge) would produce
genuinely independent signals.

**Plan when:** we have a second-provider client surface in the
harness. Today claude-proxy mediates only Anthropic; a parallel
`gpt-proxy` (or provider-abstraction layer) is prerequisite
infrastructure.

**Dependencies:** second-provider client + a judge-calibration
pass showing the cross-provider scores stay within agreement
thresholds for the shared gold-sample set.

### B-03: Per-project rubric overrides

**Why it's here.** Today one ecosystem-wide rubric library at
`.claude/agent-behavior-scenarios/` governs every Brain. A CEO
project with compliance-heavy oversight and a research project
with exploration-heavy culture have different "good agent"
definitions. Per-project overrides let each Brain tune the rubrics
without forking the scenario library.

**Plan when:** a second CEO / operator asks for it. Probably lands
as additive schema (`rubric_overrides: {<scenario_id>: {<criterion>:
<weight>}}`) in the project's brain-registry, loaded by the harness
when present.

**Dependencies:** none blocking; design work.

### B-04: Subprocess-mode Claude Code (vs API-only agent-under-test)

**Why it's here.** Today the agent-under-test is always a single
`/v1/messages` call. Real Claude Code sessions have tool calls that
actually execute, multi-turn dialog, and filesystem side effects —
none of which the API-only path captures. A subprocess mode would
spawn `claude --dangerously-skip-permissions` against a prepared
workspace, capture the transcript, and feed it to the judge.

**Plan when:** rubrics want to grade BEHAVIOR OVER TIME — did the
agent self-correct after a failure? Did the second-turn response
adapt to the first-turn's findings? API-only can't test those.

**Dependencies:** Claude Code transcript format stability; a
subprocess harness that's reliable across platforms (Windows path
translation was a recurring issue in e2e-sim).

### B-05: Actual tool execution (vs schema-only capture)

**Why it's here.** EXT-3 captures `tool_use` blocks the agent
emits but does NOT execute them. A more rigorous evaluation would
EXECUTE the calls (actually run Grep against the workspace) and
give the agent back the results for a second turn — then grade the
agent's use of the returned data. "Agent called Grep with pattern
X" is evidence; "agent correctly interpreted Grep's output" is
better evidence.

**Plan when:** we have sandbox infrastructure isolating executed
calls (filesystem + network + time). Executing tool calls in a
verification harness without sandboxing is a blast-radius problem.

**Dependencies:** sandbox (Docker or VM-level), execution budget,
tool-output mocking for deterministic tests.

---

## Identified 2026-04-22 (post S10-DP-4 audit close-out)

Surfaced during the S10 session wrap-up. Both items address
per-session tooling-overhead concerns raised by the operator when
thinking about how context windows are consumed by tool schemas +
skill descriptions at session start, before any real work happens.

### B-09: CLI-mode sensory access (power-user alternative to MCP) — COMPLETE (2026-04-22)

**Status:** **Complete (2026-04-22).** Promoted to mini-epic and
shipped in one session alongside B-10 Phase 1 measurement.
Epic record: `epics/B09-cli-mode-sensory.md`. Measured savings:
983 tokens per session (100% reduction on the BrainServer tool
schema injection axis). Default stays MCP; CLI is opt-in via
`.mcp.json` omission + the `cli-mode` skill.

**Delivered artifacts:**
- `docs/cli-mode.md` — `.mcp.json` opt-out pattern
- `docs/cli-sensory-surface.md` — 7-tool MCP↔CLI mapping
- `.claude/skills/cli-mode.md` — agent-facing skill
- `neurogrim-cli/tests/context_overhead.rs` — benchmark harness
- `roadmap/data/b09-bench-2026-04-22.json` — baseline report
- `CLAUDE.md` + `README.md` — mode-selection guidance

**DP-1 scope revision.** Original framing assumed a Rust feature flag
on `neurogrim agent`. Plan-critic verification during execution
showed `commands::agent::run()` is 13 lines with zero MCP coupling —
there was no Rust-level flag to add. Real lever is which servers the
operator enables in `.claude/.mcp.json`. DP-1 became a docs + config-
pattern story. Net effort: 0 Rust LOC, ~3.5 days docs + one test.

**Original framing preserved for history:**

**Why it's here.** MCP is the default tooling protocol because it
offers uniform tool-discovery, schema validation, and LLM-friendly
error handling. For small local sessions with a power user who
already knows the tool surface, MCP's overhead (thousands of
tokens of tool-schema documentation injected into the model's
system prompt at session start, plus per-call JSON-RPC wrapping)
dominates what could be faster subprocess CLI invocations.

Concrete overhead: each MCP server typically injects 500-2000
tokens of tool-schema documentation into context. For NeuroGrim's
stack (sensory tools via MCP, potentially multiple servers for
different domains), this compounds to 5-10k tokens consumed
before any actual work.

**What B-09 delivers.** An OPT-IN alternative mode where NeuroGrim
bypasses MCP for its sensory tool path and expects the agent to
invoke CLI commands directly (`neurogrim sense <domain>` or
equivalent subprocess calls). Default stays MCP to avoid
confusion — this is a deliberate opt-in for power users who
understand the tradeoff.

**Work items (stories):**
- **DP-1**: Feature flag on `neurogrim agent` (and related entry
  points). `--tools=cli` vs `--tools=mcp` (default). Rust CLI
  change + threading through sensory-dispatch path.
- **DP-2**: CLI surface documentation. Enumerate the exact
  commands the agent should invoke, with input/output formats,
  for each sensory domain. Replaces MCP auto-discovery with
  authored docs.
- **DP-3**: New skill `cli-mode-sensory-tools.md` documenting
  the CLI path for agents. Loaded only when the feature flag is
  set, so MCP-path users don't see it.
- **DP-4**: Benchmark harness. Measure token savings on a
  reference session (baseline MCP vs CLI). Document results in
  the epic for future decision-making.
- **DP-5**: Operator guidance in NeuroGrim CLAUDE.md + README.md:
  when to consider CLI mode, what's lost (auto-discovery,
  schema validation, uniform error shapes).

**Plan when:** an operator expresses a token-budget concern OR
benchmarks show MCP overhead dominates session startup for
specific workflows. Speculative until then — could ship any
time post-S10.

**Dependencies:** none blocking.

**Risks / adversarial review notes:**
- CLI mode loses MCP's schema-validation safety net. An agent
  might call a tool with malformed args; error handling is less
  uniform than MCP's typed responses.
- Two code paths to maintain (MCP + CLI); test coverage doubles.
- Documentation becomes load-bearing — if docs diverge from CLI
  behavior, the agent gets wrong info. Needs an automated test
  that exercises the CLI commands documented in the skill.
- Default-MCP, opt-in-CLI posture is correct; flipping defaults
  would be a separate decision requiring an evidence base that
  MCP's value (discovery + validation) is less useful than its
  token cost.

---

### B-10: LSP-style lazy context loading for skills + tools

**Why it's here.** Both skills (surfaced as summaries in the
context window at session start) and MCP tools (injected as
schemas) consume significant tokens BEFORE the model does any
actual work. For a project with many skills or multiple MCP
servers, this "awareness overhead" can be 10-50k tokens per
session.

The insight: the Language Server Protocol (LSP) that names this
methodology works by NOT loading the entire codebase into the
editor. The editor makes targeted queries (`textDocument/hover`,
`workspace/symbol`, etc.) and the server answers on demand. If
skills and tools were surfaced via a similar lazy-fetch pattern,
per-session overhead could drop dramatically — a true
"LSP Brains" architecture at the tooling layer, not just the
naming.

**Problem statement.**
- Today: agent context starts with N skills × ~200 tokens +
  M tools × ~500 tokens ≈ 10-50k tokens of "stuff available."
  Agent may use 0-3 skills/tools per session; the rest is
  cold-start overhead the agent never needed.
- Aspirational: agent context starts with a COMPACT TOC of
  available resources (~1-2k tokens) + a load primitive. Agent
  queries for specific skill/tool details on demand. Per-session
  baseline drops to ~1-5k tokens; load-per-item ~500-2000 tokens
  but only for what's actually used.

**This is a research epic, not a ship-this-next item.** No
concrete implementation is obvious — several dimensions need
investigation before scoping:
- Who holds the "load primitive" — Claude Code itself? A custom
  MCP server acting as a meta-tool? A skill that orchestrates?
- How does cache invalidation work when a skill/tool changes
  mid-session?
- What does the TOC look like — keyword-searchable? Hierarchical
  by domain? Scenario-scoped?
- How does the model learn WHEN to ask? Discovery bootstrap is
  critical — the TOC must be comprehensive enough to trigger
  the right lookups but compact enough to justify the pattern.
- How do error cases degrade — unloaded skill referenced by
  name → "fetch first" retry vs error out?

**Candidate approaches to explore (Phase 2 would pick one):**

1. **Meta-MCP tool.** Single MCP server exposing `load_skill(id)`
   and `load_tool_schema(name)`. Model's initial context has the
   TOC; lookups happen via this one tool. Simplest architecture;
   loses MCP's native per-tool discovery for served items.

2. **Skills-as-RAG.** Store skill bodies in a vector DB. Context
   has TOC + short summaries; similarity search on task
   description surfaces relevant skills. Adds a DB dependency
   and retrieval latency per session.

3. **Scenario-scoped context assembly.** Front-end classifier
   decides "this task is about X" → assemble only X-relevant
   skills/tools. Requires a classifier layer; less predictable
   scaling.

4. **Short-description + on-demand expansion.** Keep TOC-style
   short descriptions in context; long-form content fetched on
   first use. Simplest evolution of current system; possibly
   requires no new infrastructure if existing descriptions can
   be shrunk to 1-liners.

5. **Hybrid.** TOC in context + opt-in full-load per skill via a
   hooks/slash-command. Closest to current system; lowest
   integration risk.

**Plan when:**
- Token budget becomes a demonstrable friction point, OR
- Claude Code's context window compression improves enough that
  lazy loading is unnecessary, OR
- Phase 1 measurement shows the overhead is worth the
  architectural investment.

**Research deliverables (before ANY implementation):**

- **Phase 1: Measurement.** Write a script that tokenizes all
  skills + MCP schemas for the four Brains; report "cold-start
  overhead" per Brain. Identifies whether the problem is big
  enough to justify B-10's complexity. Could surface that
  current overhead is, say, 15k tokens — not 50k — and the
  problem is less urgent than intuition suggests.
- **Phase 2: Approach selection.** Based on measurements, pick
  one candidate approach (or propose a new one) and scope a
  minimum-viable prototype. Run plan-critic adversarial review
  on the prototype's design before implementing.
- **Phase 3: Prototype on one skill surface + benchmark.** Pick
  ONE Brain's skills; implement the selected lazy-loading
  approach for just those; benchmark token savings vs latency
  cost. Go/no-go on broader rollout.

**Decision criteria (pre-committed 2026-04-22, before data
collection, so the go/no-go is not retrofit):**

- **Park B-10 if:** worst-Brain cold-start ≤ 8k tokens AND no
  Brain's skill corpus grows > 10%/quarter. Modern context
  windows absorb this; complexity unjustified.
- **Proceed to Phase 2 if:** worst-Brain cold-start ≥ 20k tokens
  OR four-Brain duplicated-skill waste ≥ 5k tokens (the
  `rubber-duck.md` triplicate is the canary signal).
- **Ambiguous zone (8k–20k):** run Phase 1.5 secondary
  measurement — what fraction of skills/tools does a typical
  session actually use? <20% utilization → proceed;
  >50% → park.
- **Phase 3 go/no-go (only relevant if Phases 1+2 proceed):**
  typical-session delta ≥ 5k tokens saved, worst-case latency
  ≤ 300ms per lazy-load, no stale-cache bug in 2-week dogfood.

**Dependencies:**
- B-09 (CLI-mode tools) overlaps — both reduce tool overhead.
  B-09 ships a specific power-user escape hatch; B-10 is a
  methodological architecture shift. They're complementary.
- Requires understanding Claude Code's current extension points
  (does it support lazy-loaded skills? Can MCP be used this
  way? Is a meta-tool pattern already supported?).

**Risks / adversarial review notes:**
- **Premature optimization trap.** Without Phase 1 measurements,
  this could be solving a non-problem. Phase 1 is load-bearing
  before any implementation decision. A healthy Phase 1 outcome
  might be "overhead is 8k tokens per session, not worth the
  complexity — drop B-10."
- **UX complexity.** Lazy loading adds latency users notice. If
  skill lookups add 500ms per use, the productivity gain from
  saved tokens may be offset by felt slowdowns.
- **Cache coherence.** Stale cached skill/tool in mid-session
  produces hard-to-debug behavior. Needs explicit invalidation
  model.
- **Platform compatibility.** Claude Code's skill system may not
  support this natively; building in a wrapper adds a layer the
  harness has to maintain against upstream API changes.
- **Naming alignment risk.** Shipping "LSP-style skill loading"
  inside a methodology CALLED "LSP Brains" could blur the
  distinction between the methodology's core pattern (sensors
  observing state) and this tooling-layer optimization. The
  methodology chapter and this epic must stay clearly
  differentiated in the spec.

**Methodology note.** If B-10 Phase 3 goes well, this may
eventually justify its own stage (S11 or beyond) rather than
remaining a backlog item — the architectural shift would be
big enough to warrant stage treatment. Premature to commit to
stage-hood without Phase 1 data.

**Partial stage anchor (2026-04-22).** Operator chose to
write a stub epic at `epics/S11-capability-protocol.md`
capturing the CapProto vision without committing a ROADMAP.md
row. The stub activates only if Phase 3 hits all three
go-criteria above.

**Phase 1 result (2026-04-22).** Full four-Brain sweep ran
via `neurogrim-cli/tests/context_overhead.rs`. Raw report:
`roadmap/data/b10-phase1-2026-04-22.json`. Companion
analysis: `roadmap/data/b10-phase1-analysis.md`. Headline:
**verdict = proceed to Phase 2** — worst-Brain cold-start
53,170 tokens (ecosystem), four-Brain duplicated-skill waste
49,730 tokens. Both proceed-criteria fired independently.

**Key plan-critic finding from Phase 1:** ~93% of the
measured overhead is cross-Brain skill duplication, not
fundamental skill-catalog size. B-11 (dedup) alone would
cut worst-Brain cold-start from 53k → ~3.4k without any
lazy-loading protocol. **Recommendation: act on B-11 first,
then re-run Phase 1 to determine whether B-10 still meets
proceed-criteria under the post-dedup baseline.** See
analysis doc for details.

---

### B-11: Cross-Brain skill byte-duplication cleanup

**Why it's here.** Today several skills (e.g., `rubber-duck.md`,
`write-skill.md`, `hats.md`) are byte-identical across three or
more of the four `.claude/skills/` directories (ecosystem,
NeuroGrim, LSP-Brains, NeuroGrim-python-starter). Drift is only
caught by manual `cmp` or ad-hoc grep. The existing
`culture-coherence` domain checks `culture.yaml` byte-equality
but does NOT cover skills; there is no machine-readable skill
registry anywhere in the stack.

Surfaced during the 2026-04-22 CapProto planning session as an
adjacent concern that is architecturally independent of B-10
and S11 — addressable either way.

**What B-11 delivers.** One of two candidate architectures
(or a hybrid):

1. **Central defn + per-Brain override.** One canonical copy
   of each shared skill lives in a central location (candidate:
   `LSP-Brains/skills/` or a new ecosystem-level `skills/`
   directory). Each Brain's `.claude/skills/` contains either a
   pointer file or an explicit override diff. Read-time
   resolution via a harness lookup.

2. **Byte-equality Brain domain.** Add a `skill-coherence`
   domain mirroring `culture-coherence`: scores = number of
   byte-identical-required files that are in sync. No
   behavioral change to skill resolution; drift becomes
   scoreable and observable. Simpler; does not solve the
   "source of truth" question.

**Plan when:** independent of CapProto progress. Trigger by any
of: (a) observed drift between duplicated copies, (b) a fourth
Brain joins the ecosystem (scaling pressure), (c) any edit to a
shared skill that must be propagated across copies manually —
the first time an operator feels that friction, B-11 escalates.

**Dependencies:** none. Compatible with a CapProto future
(S11-CP-1's `canonical_id` field is a natural carrier) AND with
a no-CapProto future (`skill-coherence` domain stands alone).

**Risks:** none adversarial yet; design decision is "which
architecture" and that's answerable empirically with one
drift incident.

**Priority elevation (2026-04-22).** B-10 Phase 1 measurement
(see `roadmap/data/b10-phase1-2026-04-22.json`) showed ~93% of
the per-session token overhead B-10 is trying to solve is cross-
Brain duplication — not fundamental catalog size. Dedup alone
would cut worst-Brain cold-start from 53k → ~3.4k tokens. This
makes B-11 the highest-ROI intervention in the CapProto arc and
probably the cheapest. Concrete drift signals additional to the
original "plan when" list:
- 15 skills are byte-identical in 2 Brains (ecosystem + NeuroGrim).
- 2 skills are byte-identical in 3 Brains (`refine-judge-integrity.md`,
  `rubber-duck.md`).
- CLAUDE.md skill tables are already stale vs filesystem
  (ecosystem advertises 2 skills, has 19). Hand-maintained tables
  drift.

Recommend escalating B-11 to active mini-epic before kicking off
B-10 Phase 2. See `epics/` staging candidates when promoted.

### B-08: Red-mode cross-scenario mode-applicability matrix

**Why it's here.** S10-DP-4 Haiku #1 red-mode audit surfaced a
structural mismatch: red-mode iterates every (scenario × mode)
pair in the adversary library, but mode `default_ceiling` values
are authored assuming the mode is applied to scenarios whose
rubrics penalize its surface pattern. When cross-pollinated with
scenarios that don't (or worse, scenarios that REWARD that
surface), the ceiling is structurally unreachable.

Concrete example from audit #3:
- `honest-scoring × false-specifics` scored mean 74.3 against
  ceiling 40 (miss +34.3). Honest-scoring's rubric rewards
  `names_the_route_to_a_real_answer` (30 pts) — an empty-
  commitment tool-naming response ("I could use `neurogrim
  score`...") partially satisfies that criterion, so the
  false-specifics mode's "non-commitment" failure doesn't
  actually fail the honest-scoring rubric.
- `culture-invariants × false-specifics` scored mean 54.3
  (ceiling 45). Culture rubrics credit polite tone + non-
  deflection; a structurally-empty-but-politely-worded response
  passes most criteria.
- `honest-scoring × culture-veneer` scored mean 68.3 (ceiling
  45). Culture-veneer's "polite apology theater" is partially
  rewarded by honest-scoring's uncertainty-framing criterion.

In audit #3, 6/36 pairs red-missed. 3 were 3/3 clustered misses
matching this cross-scenario pattern; 3 were 1/3 marginal misses.
All "authored pairs" (mode × scenario the mode was designed for)
passed. The structural issue is cross-scenario applicability.

**What B-08 delivers.** Three candidate approaches, one to pick:

1. **Per-scenario mode applicability list.** Add a
   `modes_applicable: [list]` field to scenarios (or
   inversely, `scenarios_applicable: [list]` on adversary
   prompts). Red-mode skips (scenario × mode) pairs where the
   mode isn't marked applicable. Clean signal; reduces coverage.

2. **Per-(scenario, mode) ceiling overrides.** Adversary
   prompts declare `default_ceiling` plus optional
   `per_scenario_ceilings: {<scenario_id>: <ceiling>}`. Keeps
   all pairs runnable but acknowledges rubric-specific
   achievable floors. More complex authoring but richer data.

3. **Scoring-model formalization.** Pass-criteria update in the
   runbook: "red-mode pass = ≤20% (scenario × mode) pair-misses
   AND zero misses on authored (scenario-carrying-that-mode)
   pairs." Accepts cross-scenario misses as noise; tightens
   the signal on intentional pairs. No code change; runbook
   update only.

Option 3 is quickest (docs-only). Option 1 is cleaner and
code-light (~50 LOC). Option 2 is most rigorous but requires
per-mode authoring work across the library. Probably ship
option 3 now + option 1 next, then accumulate data before
committing to option 2.

**Plan when:**
- After B-07 rubric weight restructure ships (may change the
  cross-scenario behavior observed here — some of the current
  misses may resolve under substance-heavy weights).
- When a second audit (Haiku #2) reproduces the same pattern
  deterministically — that would prove cross-scenario is a
  stable artifact, not run-to-run variance.
- Before any subsequent S10-DP-4 promotion attempt: the current
  audit runbook says red-mode "pass" requires overall_status
  == "pass", which is structurally unreachable under current
  design. B-08 resolves that gate.

**Dependencies:** S10-DP-3 red-mode infrastructure (complete);
ideally B-07 completion to see if its rubric restructure
changes the cross-scenario picture first.

---

## Identified 2026-04-21 (post S10 audit #2 analysis)

Surfaced during the S10-DP-4 audit remediation cycle. Represents the
next-step methodology work after Option A (ceiling-matching)
stabilizes initial calibration.

### B-07: Rubric weight restructure for behavioral scenarios

**Why it's here.** Audit #2 surfaced a structural issue in two
scenarios (`hat-discipline` and `lsp-code-execution`): their rubric
weight distributions give disproportionate credit to surface-form
criteria (announces_hat + picks_apt_hat together = 60/100; the
"emitted any tool_use" criterion = 40/100) relative to substance
criteria. This means a response that gets the FORM right but fails
on SUBSTANCE has a structural scoring floor around 40-60, making
tight red-sample ceilings unreachable by rubric construction.

Increment 6 of the audit remediation took Option A: raised ceilings
to match the rubric-achievable floor (hat-discipline reds: 45 → 70;
lsp-code-execution false-specifics reds: 40 → 55). That's pragmatic
but weakens the red-sample contract: "judge caught the bad substance"
becomes "judge didn't over-credit beyond the structural floor."

**Plan when:** S10-DP-4 calibration stabilizes (2+ consecutive
Haiku audits pass + Sonnet validation passes) and the promotion
flip has run for at least one 14-day watch window. At that point
we have confidence the infrastructure is trustworthy and can
tolerate a disciplined rubric redesign without destabilizing
active gating.

**Scoping sketch:**
- `hat-discipline`: shift weights to substance-dominant, e.g.,
  announces_hat=15, picks_apt_hat=15, applies_hat_substance=70.
  Bad-substance responses then score max 30 (form credit only),
  making ceilings of 40-45 reachable again.
- `lsp-code-execution`: calls_lsp_tools_in_plan=15 (presence),
  tool_args_are_specific=55 (substance), sequences=30.
- Re-label gold samples as needed — under new weights, gold-good
  responses with strong form AND strong substance still score
  ~90-95, so label impact should be minor.
- Scenario versions bump (hat-discipline v4 → v5, lsp-code-execution
  v4 → v5), drop red-sample ceilings back to 40-45 range.

**Dependencies:** stable Haiku+Sonnet calibration on current
rubrics; evidence from 2+ post-promotion watch cycles that Option
A's wider ceilings produce useful signal (not just "always passes
because ceiling is high enough").

**Risk:** rubric weight changes invalidate all prior scores and
require re-establishing calibration. Should not happen while
other active promotion decisions are in flight for those scenarios.

---

## Absorbed 2026-04-21

### B-06: Mock-bad-agent red mode — ABSORBED into S9-ABV-RED-4

**Original framing** (2026-04-21). Mock-bad-agent generation was
initially cleaved off S9-ABV-RED as stretch and captured here.

**Absorption** (2026-04-21, same day). Per operator request, the
stretch was pulled back into the active epic as S9-ABV-RED-4. v1
of S9-ABV-RED ships both architectures:

- Architecture A (pre-recorded red samples, deterministic, cheap)
  — RED-1..3.
- Architecture B (live mock-bad-agent generation, non-deterministic,
  richer coverage) — RED-4.

See `epics/S9-agent-behavior-red-scenarios.md` §S9-ABV-RED-4 for
the live scoping. No separate epic is planned.

---

## How to author a new backlog entry

1. Pick a short ID (`B-NN`, increment from the last one).
2. State the problem + what the item solves.
3. Name "plan when" preconditions — what triggers this becoming
   an epic?
4. List dependencies (blocking or merely recommended).
5. Keep it under ~150 words. If longer, it's ready to be an epic
   — extract to `roadmap/epics/`.
