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

**Last updated:** 2026-04-23 (**Brain-vs-control experiment — Phase 2
SHIPPED, L2 arm scoped**. The first rigorous delta-vs-no-Brain
measurement landed at commit `fcf7d60`: static Brain-context injection
(L1) helps repo-aware tasks by +9 pts (CI barely crosses 0), hurts
anti-Brain trivial tasks by −10 pts (statistically significant, CI
entirely below 0), drops groundedness by ~7 pts on trivial tasks,
and costs 3.07× L0 equal-weighted. Two of four pre-registered
falsification criteria triggered (anti-Brain drag exceeds repo-aware
gain; groundedness regression on anti-Brain class). Mixed verdict is
the pre-registration-anticipated outcome: *"the simplest claim — add
the Brain, get better agent behavior — is not supported on aggregate;
selective access when the task benefits IS supported."* Findings
absorbed into `NeuroGrim/CLAUDE.md` as operator guidance (route
Brain-equipped sessions to repo-aware work; prefer Sonnet+ for
Brain-augmented sessions; favor on-demand over eager injection).
Phase 1 pilot (Haiku, N=2, 48 trials, $0.47) + Phase 2 full (Sonnet,
N=10, 240 trials, $4.79) ran with blind judge (pinned 2026-04-22),
judge model + SHA-pinned arm prompts per ledger entry,
bootstrap-CI (10k resamples) per delta. Experiment reproducer:
`py -3 .claude/experiments/brain-vs-control/analyze.py --phase 2
--falsification`.

Follow-on in progress: **L2 arm (live `brain_query` tool access)** —
tests whether agents SELF-ROUTE to the Brain on repo-aware tasks and
SKIP it on trivial ones. Same 12 tasks, same rubric, same blind
judge. Hypothesis: L2 beats L1 on repo-aware (always-on context is
wasted by L1 when unused) AND matches L0 on anti-Brain (agent skips
the tool). New pre-registered criterion: tool-refusal rate > 50% on
repo-aware = L2 declared unhelpful. Plan in
`C:/Users/koff0/.claude/plans/parallel-hugging-eich.md`.

Prior entry preserved: **Tier A migration — COMPLETE**.
All 22 live Brain skills migrated from legacy
`.claude/skills/<name>.md` to plugin
`.claude/skills/<name>/SKILL.md` with YAML frontmatter. Every
skill is now genuinely `Skill`-tool invocable by Claude Code and
observable by the Axis 4 invocation ledger. 41 SKILL.md files
total across 4 Brains (ecosystem 19, NeuroGrim 20, LSP-Brains 2,
python-starter 0; byte-identical across duplicates so
skill-coherence stays 100). All frontmatter validates as YAML
(folded block scalars `>-` used where body text contained
colons); all `description + when_to_use` combined ≤ 1,536 chars
per Claude Code's skill-index budget. Migration automated via
`scripts/migrate-skill-to-plugin.py` + a `patch-missing-when-to-
use.py` follow-up for skills whose legacy bodies used
`**Trigger phrases:**` (bold markdown) or `**When to read
this:**` variants. Cross-references swept across 10 live skills
+ 4 CLAUDE.md files + 5 `docs/*-guide.md` files; `archived/`
refs preserved intact. `capability_breakdown.skills`:
`format:legacy` 20 → 0, `format:plugin` 0 → 20 (NeuroGrim).
`context_overhead.rs` benchmark tests updated to scan both
formats. Earlier same-day: Axis 4 v1 empirical-self-
observability shipped (PostToolUse hook, `capability-hygiene`
ledger reader, dead-skill classifier with 30-day grace / 90-day
default window / 365-day for `usage-rarity: rare`; advisory
findings only). Tier 2 generalized `capability-hygiene` to 6
capability types. Tier 1 (B-13 full rollout + Axis 2
registration). B-11/B-12 shipped; B-10 parked; S11 closed; B-09
shipped. Hook contract validated against Claude Code 2.1.111
(matcher `"Skill"` fires; stdin via plain `cat` on Git Bash);
`CLAUDE_PROJECT_DIR` guardrail added (concern C1 from 2026-04-22
deep audit). All 4 Brains at 100/100 on capability-hygiene +
skill-coherence. 292 workspace tests green.).

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

### B-10: LSP-style lazy context loading — PARKED 2026-04-22 (measurement invalidated)

**Status: PARKED 2026-04-22.** Post-ship plan-critic verification
(`claude-code-guide` subagent, pointing to official Claude Code
docs) confirmed that **skill bodies are lazy-loaded on demand,
not pre-loaded at session start**. Only names + descriptions
(1,536-char budget per skill) are in the index; actual baseline
context cost for 41 skills across four Brains is ~500 tokens,
not 104k. Phase 1's sweep (`context_overhead.rs`) tokenized
every `.md` file on disk — it was measuring **disk cost**, not
**context cost**. The "53k cold-start overhead" never existed in
the actual session baseline.

**Consequences of the correction:**
- The Phase 1 "proceed to Phase 2" verdict is **invalidated**.
- Phase 2 (approach selection) + Phase 3 (prototype) are
  **cancelled**.
- The pattern Phase 1.5 identified — "description + outline
  captures routing signal" — is valid but describes what
  **Claude Code already implements natively**. Confirming a
  pattern is not the same as delivering it.
- Combined "B-11 + B-12 → 97-99% reduction" claim collapses to
  phantom. Real per-session savings post this correction:
  only B-09's 983 tokens (MCP tool-schema injection is still
  pre-loaded — that measurement was correct).

**What's preserved elsewhere:**
- B-09 remains COMPLETE; its savings are real.
- B-11 contracts to drift-detection only (see below).
- B-12 contracts to authoring-standard + hygiene domain (see
  below).
- S11 stub closes out; see `epics/S11-capability-protocol.md`.

**Do not act on numbers in:**
- `roadmap/data/b10-phase1-2026-04-22.json` (disk-cost
  measurement; useful as a corpus-size snapshot only).
- `roadmap/data/b10-phase1p5-description-only-2026-04-22.json`
  (ratio calculations assume the full body was ever injected
  into context — it wasn't).
Both analysis docs have correction banners at the top.

**Original framing preserved for historical record below:**

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

**Phase 1.5 result (2026-04-22).** Operator intuition test:
"agents likely only need the description + outline of a
skill to route; full body loads on demand." New test
`b10_phase1p5_description_only_measurement` in the same
harness. Raw report: `roadmap/data/b10-phase1p5-description-
only-2026-04-22.json`. Analysis: `roadmap/data/
b10-phase1p5-analysis.md`. Headline: **hypothesis confirmed
— 90.4% stack reduction** with description + outline TOC
vs full-body baseline. >95% of skills already have routing-
grade descriptions; only 1 skill (`coherence.md`) needs an
authoring fix (put the "when to use" block in the lead
paragraph, not under `## When to Use`).

**Biggest architectural implication of Phase 1.5.** The
originally-sketched S11 Stage scope (new protocol, envelope
schema, diagnostics channel, Meta-MCP tool) is overbuilt.
Native Claude Code primitives suffice: description-only
TOC is `textDocument/hover`-like; `Read` tool is
`textDocument/definition`-like. The concrete work contracts
to a mini-epic: authoring standard + TOC generator +
`capability-hygiene` Brain domain. Filed as **B-12 below.**
S11 stub updated; unlikely to ever activate as a Stage.

**Combined savings forecast (B-11 + B-12):** worst-Brain
cold-start 53k → ~700-1.5k tokens (97-99% reduction). The
interventions are multiplicative; they attack different
axes (cross-Brain duplication vs in-Brain verbosity). B-10
Phase 2 design work is **deferred** — after B-11 and B-12
ship, re-measure; the overhead may fall below the "proceed"
threshold entirely, parking the original B-10 Phase 2/3
arc.

---

### B-11: Cross-Brain skill byte-duplication cleanup — SHIPPED 2026-04-22 (drift-detection domain)

**Status: SHIPPED 2026-04-22.** Approach 2 (byte-equality Brain
domain) landed as the new `skill-coherence` sensory tool.

**Delivered:**
- `neurogrim-sensory/src/skill_coherence.rs` — auto-discovers
  sibling + child Brains via `.claude/skills/` probing, compares
  byte-identical duplicates, reports drifts. 6 unit tests.
- `neurogrim-cli/src/main.rs` — `neurogrim sensory skill-coherence`
  CLI dispatch.
- `.claude/brain-registry.json` — domain registered at weight
  0.0 (advisory per spec principle #2) with exported variables
  for correlation rules.
- `.claude/skill-coherence-cmdb.json` — initial CMDB: score 70,
  3 drifts detected (`coherence.md`, `rubber-duck.md`,
  `write-skill.md` — all from same-session authoring edits that
  haven't propagated to sibling Brains yet).

**Scoring formula:** 100 baseline, -10 points per drifted
basename; floor at 0. Each "drift" is one basename whose bytes
differ across the Brains that carry it — 3 basenames divergent
across 2+ Brains = -30 = score 70.

**Original contracted-scope framing preserved for historical
record below:**

**Contracted scope 2026-04-22.** The loading-model correction
(see B-10 PARK banner above) removed the token-savings motivation
that elevated B-11 earlier in the same day. B-11 stays in the
backlog as a **governance concern only**: duplicated skills can
still drift out of sync, and drift degrades quality even when it
doesn't cost tokens.

**Architecture choice locked in:** Approach 2 (byte-equality
Brain domain mirroring `culture-coherence`). Approach 1 (central
defn + per-Brain override) is **rejected** — it required harness
changes justified only by phantom token savings. Approach 2 is a
pure detector: no behavioral change to skill resolution, no new
storage format, no drift in Claude Code's native skill discovery.
Probably a single sensory tool + one registry entry + a
`skill-coherence` domain at weight 0.0 (advisory) in v1.

**Priority elevation (pre-correction, 2026-04-22) is rescinded.**
B-11 no longer captures "93% of the overhead." It captures a real
governance concern, not a token win. Effort: 1-2 days when
planned into an active cycle.

**Original framing preserved for historical record below:**

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

---

### B-12: Skill description authoring standard + capability-hygiene Brain domain — SHIPPED 2026-04-22

**Status: SHIPPED 2026-04-22.** Both contracted deliverables
landed.

**Delivered (Part A — authoring standard):**
- `.claude/skills/write-skill.md` — new section "The Lead
  Paragraph — Routing-Critical" codifies the 1,536-char
  description-block contract, the required "when to use"
  lead-paragraph signal, size targets (40-200 tokens ideal,
  <40 under-described, >300 over-described), and the anti-
  pattern that caused `coherence.md`'s original miss (using
  `## When to Use` as a section header instead of the lead).
- `.claude/skills/coherence.md` — fixed: lead paragraph now
  carries a full "**When to use this skill:**" block before
  the first `##` header. Canonical example of the pattern.

**Delivered (Part B — capability-hygiene Brain domain):**
- `neurogrim-sensory/src/capability_hygiene.rs` — scores each
  skill's lead-paragraph description against the authoring
  standard. Checks: under-description (< 40 approx tokens),
  missing when-to-use signal, index-budget overflow (> 1,536
  chars). 6 unit tests.
- `neurogrim-cli/src/main.rs` — `neurogrim sensory
  capability-hygiene` CLI dispatch.
- `.claude/brain-registry.json` — domain registered at weight
  0.0 with exported variables.
- `.claude/capability-hygiene-cmdb.json` — initial CMDB: score
  78, 11/20 skills compliant, 9 flagged as missing when-to-use
  signal (description length adequate but no canonical phrase).

**Scoring formula:** each skill earns 0-10 points:
- 10: compliant (≥ 40 approx tokens + when-to-use signal, ≤
  1,536 chars).
- 7: over-budget (description exceeds the 1,536-char index
  truncation threshold).
- 5: missing when-to-use signal (length OK, but no canonical
  phrase).
- 0: under-described (< 40 approx tokens — the `coherence.md`
  anti-pattern).
Final = `round(100 * earned / possible)`, clamped 0-100.

**Remaining operator work (not committed here):** the 9
`missing-when-to-use` skills that the first scoring run flagged
(dual-review, human-comms, imagination-mode, north-star,
pilot-protocol, secret-refs, security-standards, skill-
deprecation, subagent-patterns) could be brought to compliant
with lead-paragraph rewrites. The `capability-hygiene` CMDB
will tick up toward 100 as authors adopt the convention.

**Body-size optimization (B-13 pilot + full rollout shipped
2026-04-22).** See B-13 below — `subagent-patterns.md` piloted
the pattern (7,208 → ~1,433 tokens, ~80% reduction), and the
remaining 4 fat skills completed the rollout the same day:
`write-skill.md`, `pilot-protocol.md`, `plan-critic.md`, and
`write-agent-behavior-scenario.md` (the last with a moderate
split per plan-critic flag — procedure-dense skill stays largely
inline; red-sample authoring + scoping + related reading moved
to `D:/Brains/docs/write-agent-behavior-scenario-guide.md`).
All 5 fat skills now have companion guides.

**Original contracted-scope framing preserved for historical
record below:**

**Contracted scope 2026-04-22.** The loading-model correction
(see B-10 PARK banner above) invalidated B-12's original
token-savings motivation. Phase 1.5's "90% reduction from
description+outline TOC" was phantom — Claude Code already
implements description-first lazy loading natively. The pattern
is right; the savings calculation assumed a baseline that
doesn't exist.

**What B-12 still delivers (two parts, both real):**

1. **Authoring standard for skill descriptions.** The
   1,536-character description IS the routing contract in Claude
   Code's native skill index. Description quality directly
   determines whether the correct skill gets invoked. Standard
   codifies:
   - Required lead-paragraph "When to use this skill:" block.
   - Minimum description length (~40 tokens) to carry routing
     signal; maximum ~200 tokens to respect the 1,536-char
     index budget.
   - Discouraged patterns (descriptions that read as "what the
     skill is" rather than "when to reach for it").
   - Revise `write-skill.md` + one-time pass over existing
     skills to bring under-described ones (e.g., `coherence.md`
     which puts its "When to Use This Skill" in a `## ` section
     instead of the lead paragraph) into compliance.

2. **`capability-hygiene` Brain domain.** Advisory (weight 0.0
   in v1 per spec principle #2) domain that scores:
   - Presence + length of description field per skill
   - Orphan detection (skill file present but not in any index)
   - Shadow detection (two skills with overlapping trigger
     phrases or descriptions)
   - Deprecation markers honored
   Emits a CMDB envelope per `cmdb-envelope-v1.schema.json`.
   Integrates with the S10 domain-promotion pipeline so a
   quality floor can be argued and advanced over time.

**What's removed from B-12 scope post-correction:**
- ~~TOC generator~~ — Claude Code has the skill index natively;
  generating a parallel `.claude/SKILLS-INDEX.md` is redundant.
  A future CLAUDE.md-table auto-maintenance tool may be worth
  ~1 day of work if the tables drift badly, but it's not B-12.
- ~~Combined 97-99% reduction forecast~~ — phantom.
- ~~Meta-MCP tool / new protocol~~ — Claude Code's Skill tool
  already IS the `textDocument/definition` mechanism.

**Optional follow-on: body-size optimization.** Operator
insight (2026-04-22, post-loading-model correction): once a
skill is invoked via the Skill tool, its body stays in context
for the rest of the session. Fat skill bodies compound cost
across subsequent turns. Named candidates for compression
(push depth to `docs/`, keep skill body terse with pointers):
- `subagent-patterns.md` (7,208 tokens)
- `write-skill.md` (3,459)
- `pilot-protocol.md` (3,975)
- `plan-critic.md` (3,209)
- `write-agent-behavior-scenario.md` (3,954)

This is **not** committed B-12 scope. Revisit when any of these
is refactored for other reasons, or when a skill-invocation-
heavy session hits real context pressure. If promoted, it
becomes its own work item (candidate ID: B-13).

**Effort estimate (contracted):** 3-5 days when planned into an
active cycle. Split:
- 1-2 days: authoring standard doc + `write-skill.md` revision
  + one-time compliance pass over the current 19-skill corpus.
- 2-3 days: `capability-hygiene` sensory tool + registry entry
  + basic tests.

**Plan when:** opportunistically. No blocker; no deadline. A
good candidate for a "catch-up hygiene" slot between larger
stages.

**Original framing preserved for historical record below:**

**Why it's here.** Phase 1.5 measurement (2026-04-22) confirmed
the operator's intuition: a description + section-outline extract
captures **90.4% of the skill routing signal at ~10% of the
token cost** (`roadmap/data/b10-phase1p5-description-only-2026-04-
22.json`). This means the architecture work originally scoped
under S11 CapProto (new protocol, envelope schema, Meta-MCP tool,
diagnostics channel) is overbuilt. The concrete per-session win is
achievable with three small pieces of work — each of which fits
inside a week and none of which needs a new protocol.

**What B-12 delivers.** A description-first pattern for skills,
with tooling + governance to keep authors honest:

1. **Authoring standard.** Revise `write-skill.md` to require:
   - Lead-paragraph "When to use this skill:" block before any
     `## ` section header.
   - Description block ≤ ~200 tokens (soft cap; linter warning).
   - Consistent frontmatter / field convention so the TOC
     generator can parse uniformly.
2. **TOC generator.** A new `neurogrim` CLI subcommand (or a
   small Rust binary) that reads `.claude/skills/*.md`, extracts
   each file's description + `##` outline, and emits a
   `.claude/SKILLS-INDEX.md` (or whatever path the convention
   picks). Claude Code + CLAUDE.md references the generated
   index, not the individual files.
3. **`capability-hygiene` Brain domain.** Advisory (weight 0.0
   in v1 per spec principle #2) domain that scores:
   - Presence + length of description field per skill
   - Outline presence + depth
   - Shadow detection (two skills with overlapping trigger phrases)
   - Orphan detection (skill file present but not referenced by
     any generated index)
   - Deprecation markers honored
   Emits a CMDB envelope per `cmdb-envelope-v1.schema.json`.

**Non-goals (preserve from S11 contraction):**
- NOT a new protocol. No envelope schema. No new MCP server.
- NOT a replacement for `Read` tool. Agents load skill bodies
  on demand via existing primitives.
- NOT ecosystem-wide; each Brain maintains its own skills (this
  is B-11's concern).
- NOT a semantic/embedding search. Description quality, not
  vector similarity, is the routing contract.

**Combined with B-11:** B-11 (dedup) + B-12 (TOC) are
multiplicative. Expected worst-Brain cold-start reduction after
both: **53k → ~700-1,500 tokens** (97-99% reduction).

**Plan when:** immediately after B-11 ships. B-12 is tractable in
~1 week of focused work; no external dependencies.

**Dependencies:**
- None blocking.
- Compatible with a no-S11 future (native primitives only).
- Compatible with a future-S11 revival (B-12's authoring standard
  becomes CP-3; `capability-hygiene` domain becomes CP-6).

**Risks / adversarial review notes:**
- **Convention drift.** If the authoring standard lands but the
  linter or TOC generator doesn't enforce it, skills will drift
  back toward prose bodies with weak leads. Mitigation: the
  `capability-hygiene` Brain domain scores this; hygiene below
  threshold surfaces as a recommendation.
- **Outline volatility.** If skills frequently refactor their
  `##` heading structure, the generated TOC churns. Mitigation:
  the TOC is regenerated on demand; churn is benign if committed
  alongside the skill edit.
- **Description under-specificity.** 18-token descriptions (like
  current `coherence.md` when measured naively) would route
  poorly. Mitigation: linter enforces minimum length (~40 tokens
  as a floor); Phase 1.5 shows >95% of existing skills already
  clear this bar.
- **Naming-firewall discipline.** Same as S11: "LSP Brains" (the
  methodology) ≠ "LSP-inspired capability indexing" (the tooling
  optimization). Any prose introducing B-12 in spec must open
  with the two-sentence differentiator.

**Reference implementation location:** probably lives in
`neurogrim-cli` as a new subcommand (`neurogrim skills index` or
similar), with the hygiene domain as a new sensory tool in
`neurogrim-sensory`. Exact names TBD.

---

### B-13: Skill body-size compression (push depth to docs/) — PILOT SHIPPED 2026-04-22

**Status: PILOT SHIPPED 2026-04-22.** The operator-surfaced
insight (2026-04-22, post-loading-model correction) that
once a skill is invoked, its body stays in context for the
rest of the session — so **fat skill bodies compound cost
across subsequent turns** — validated as a real
per-invocation saving axis.

**Pilot target:** `subagent-patterns.md` (7,208 tokens originally).

**Approach:** split skill body into two files:
- `.claude/skills/subagent-patterns.md` — the **decision surface**:
  lead paragraph, decision table (spawn vs inline), 6-pattern
  summary with one-line each + link, envelope-protocol summary,
  top-3 troubleshooting, "Why This Matters". Size: ~1,433 tokens
  (~80% reduction from original).
- `docs/subagent-patterns-guide.md` — the **deep reference**:
  every pattern's full walk-through, worked LaaS examples,
  all hat-calibration blocks (output-format + domain-priority),
  envelope integration details, convergence failure handling,
  hook-system boundary. Size: ~7,400 tokens.

**Per-invocation impact:** when an agent invokes the skill, only
the skill body loads into context (~1,433 tokens, down from
7,208). If the agent needs deep detail (e.g., the incident-commander
calibration block), they `Read` the guide on demand — a separate
~7,400 token load, but only if actually needed. Typical
invocation — which reaches for the decision table + pattern
summary without needing every calibration block — saves ~5,775
tokens per invocation per session.

**Remaining candidates — all shipped 2026-04-22:**
- ✅ `write-skill.md` (~3,459 → ~1,500 tokens) — extracted
  template, role taxonomy, companion-hook rubric, wiring steps,
  style conventions, Why-This-Matters details to
  `docs/write-skill-guide.md`.
- ✅ `pilot-protocol.md` (~3,975 → ~1,400 tokens) — extracted
  per-responsibility-type `data` schemas, full subagent system
  prompt template, capability discovery, Interface Contract
  YAML example, hat chain traceability, integration points to
  `docs/pilot-protocol-guide.md`.
- ✅ `plan-critic.md` (~3,209 → ~1,700 tokens) — extracted
  Step 0 full calibration Qs, Step 2b Symbol Impact Audit
  tables, Step 2a Scaled Review variant, tone rules, worked
  example to `docs/plan-critic-guide.md`.
- ✅ `write-agent-behavior-scenario.md` (~3,954 → ~2,800 tokens;
  moderate split per plan-critic flag — procedure-dense skill)
  — extracted red-sample authoring (S9-ABV-RED), common failure
  modes, scoping rubric, "when NOT to write", related-reading
  bibliography to `D:/Brains/docs/write-agent-behavior-scenario-
  guide.md`.

**Plan when:** COMPLETE. Pattern fully rolled out across all 5
originally-flagged fat skills.

**Dependencies:** none. The pattern is validated; adoption is
pure authoring work.

**Risks:**
- **Agents may not Read the guide when they need depth.**
  Mitigation: the skill carries explicit pointers to the right
  guide section for each pattern; the table's "Full walk-through"
  column links directly.
- **Drift between skill and guide.** If a pattern's semantics
  change, both files need updating. Mitigation: `capability-
  hygiene` domain does not (yet) check cross-file consistency,
  but a future extension could; meanwhile, the convention is
  "update the skill first, then mirror to the guide".

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

### B-14: Task-class dispatch as first-class Brain capability — CANDIDATE

**Why it's here.** The 2026-04-22/23 brain-vs-control experiment
(Phase 1-3, shipped at `0db4a41`; full reports under
`.claude/experiments/brain-vs-control/reports/`) measured three
Brain access patterns across three task classes and found no single
pattern dominates: L0 (no Brain) wins trivial tasks, L1 (static
context) wins repo-aware, L2 (live tool access) ties L0 on trivial
but lags L1 on repo-aware. Visionary-hat synthesis (2026-04-23)
argued the methodology's unit of analysis should shift from "does
the session have the Brain?" to "which access pattern fits this
task?" — and that **task-class dispatch is the real first-class
capability** the methodology hasn't named yet.

Documented in spec `METHODOLOGY-EVOLUTION.md` §14 as observational.

**Plan when** Tier 2a oracle-ceiling analysis on the existing
ledger shows ≥5 pt headroom over the best single arm equal-weighted
AND Tier 2b realistic-dispatcher experiment validates that the
ceiling is achievable by a real agent. Details in
`C:/Users/koff0/.claude/plans/parallel-hugging-eich.md`.

**Dependencies:** L2 harness complete (`0db4a41`); `analyze.py`
oracle extension (Tier 2a of the methodology-reframe plan).

**Adversarial note.** One experiment with 12 tasks is not enough
evidence to promote dispatch to a first-class capability. This
entry is scoped to a CANDIDATE state precisely to force an evidence
gate before scope expansion; spec/VISION changes are explicitly NOT
in scope at this status.

**Decision 2026-04-23 (Tier 2b outcome — realistic-dispatcher
experiment DECLINED).** Tier 2a oracle-ceiling came in at +8.78 pts
(5-10 bucket per the pre-registered decision rule). Tier 2b-rule
(descriptive analysis at
`.claude/experiments/brain-vs-control/reports/dispatcher-rule-analysis.md`)
surfaced a one-sentence operator rule that captures ~89% of the
oracle ceiling:

> *Inject Brain context (L1) only when the expected correct answer
> requires referencing project-specific definitions or state data
> that the model cannot reasonably generate from the prompt alone.
> Otherwise, use the plain-assistant baseline (L0). Injecting
> context on tasks without that requirement risks over-assertion,
> context regurgitation, or groundedness regressions.*

Building a code-level LLM-classifier dispatcher (Tier 2b-arch) would
at best approximate this rule and would cost API credits to re-derive
what the rule already captures. **Tier 2b-arch declined;** shippable
output is the operator-guidance update instead.

**New gate for Tier 3 (spec changes):** rule back-tested on N≥20
held-out tasks with ≥75% direct agreement OR ≥85% within-5-pt
agreement with oracle winner. Until then the rule stays operator
guidance, not normative spec.

**Update 2026-04-23 (held-out contradiction).** The `reframe/
factual-augmentation` branch executed the held-out back-test (22
tasks, 440 trials, Sonnet). Result: direct agreement **40.9%**,
within-5-pts **68.2%**. Both kill-criterion thresholds tripped
(< 50% direct, < 70% within-5-pts). Pre-registered K1 fired;
branch ABANDONED (not merged) at commit `reframe/factual-
augmentation@4856aa8`.

The rule did not generalize. L1 won 18 of 22 held-out tasks —
broader than the "factual-augmentation service" framing predicted.
B-14 stays CANDIDATE but with stronger skepticism: any future
attempt to elevate a dispatch rule to first-class status needs a
task set substantially more diverse than the original 12-task
benchmark (candidate: N≥50 stratified across shapes with
rubric-sensitivity controls).

Full post-mortem:
[`reports/reframe-post-mortem.md`](../.claude/experiments/brain-vs-control/reports/reframe-post-mortem.md).
Pre-registration discipline saved ~20 hrs engineering + ~$50-100
API that would have gone into prototype + validation before the
gate fired.

**Scope note 2026-04-23.** Single-turn experimental evidence
cannot settle this question. Dispatch decisions are per-response
concerns; the Brain's primary value hypothesis is longitudinal
(consistent project awareness across sessions). B-14 remains
CANDIDATE with the explicit framing that even a validated
dispatch rule would address a narrow single-turn concern — not
the longitudinal value that makes the Brain worth shipping at
all. See ROADMAP.md "Evidence + Hypothesis Posture" for the
broader framing.

---

### B-15: Capability-audit-driven review process — CANDIDATE

**Why it's here.** The 2026-04-23 Brain capability audit
(`docs/brain-capability-audit-2026-04-23.md`) classified 13 of ~16
Brain capabilities as "plausibly valuable but untested" against the
432-row brain-vs-control ledger. Most of the methodology's machinery
(individual sensors, correlations, hats, culture, trajectory, gated
governance, A2A, MCP, skills, invocation ledger, capability-hygiene
domain) has never been the *independent variable* in a controlled
experiment. This is an evidence gap, not a value judgment — but it
needs a structured review cadence rather than ad-hoc worry.

**Plan when** operator requests a review cycle OR a specific
capability comes up for retirement / promotion consideration. At
that point, this candidate becomes an epic scoping the review
process: (a) which capability to prioritize, (b) what targeted
experiment would fill its evidence gap, (c) what outcome
criteria would move it from "untested" to "supported" or
"contradicted."

**Dependencies:** B-14 dispatch rule validated or contradicted on
held-out tasks (per the `reframe/factual-augmentation` branch).
Branch outcome informs which capabilities still need independent
measurement vs. which are absorbed into the reframe.

**Adversarial note.** The audit is *not* a retire-list. Absence of
evidence ≠ evidence of absence. This candidate's job is to close
evidence gaps, not prune capabilities. Any retirement decision
stays its own case-by-case review with its own adversarial gate.

---

### B-16: Content-freshness as a load-bearing Brain invariant — CANDIDATE

**Why it's here.** The `reframe/factual-augmentation` branch held-out
run surfaced a real failure mode: L1 context injection on
factual-lookup tasks **crashed worse than L0** on 3 of 6 such tasks
(vision-principle, culture-values, spec-section — L0 winning by 2.6,
19.5, and 29.1 pts respectively). The plausible mechanism: L1's
injected content was **stale or incomplete** for those specific
topics, and the agent confidently over-asserted wrong specifics where
L0's "I don't know" scored higher on groundedness.

If true, this elevates **content freshness** from an implicit
property of the Brain's state to a **first-class invariant** the
methodology should surface — each Brain-sourced fact carries a
freshness timestamp + a staleness policy; agents see freshness on
query and can decline stale content.

**Plan when** an operator hits the freshness failure mode in a real
workflow OR when a content-freshness experiment is scoped (one
candidate design: run the same factual-lookup tasks against stale
vs fresh L1 context, measure the gap).

**Dependencies:** none. Could ship as a CMDB-envelope metadata
extension without touching scoring logic.

**Adversarial note.** The held-out data is suggestive but doesn't
prove the freshness-is-the-cause hypothesis. Could also be that the
judge rewards "I don't know" over "here's wrong-but-specific" on
these tasks regardless of content. Before scoping an epic, a targeted
experiment should isolate freshness.

---

### B-17: L2 synthesis-under-multi-turn improvement — CANDIDATE

**Why it's here.** Phase 3 L2 repo-aware = 61.12 vs L1 = 73.88
(−12.75 pts, CI does not cross 0). Agent self-routing to the brain_
query tool was perfect; the gap was in SYNTHESIS — Sonnet applies
tool-result JSON less effectively than the same content pre-loaded
as system-prompt context. The post-mortem characterized this as a
**prompt-engineering frontier, not an architectural one**.

CANDIDATE improvements to try:
- Revised tool-result framing (e.g., "Brain data, authoritative for
  project state but apply judgment" preamble on results)
- Reduced accumulated context across turns (drop earlier tool_results
  from the conversation once their content has been incorporated)
- Explicit "synthesize from tool results" instruction in system prompt
- Cached-digest pattern (agent gets a compressed pre-digest in system
  prompt + tool for drill-down — hybrid L1/L2)

**Plan when** L2 becomes a serious deployment candidate (currently
deprioritized behind the longitudinal-value focus — see
ROADMAP.md Evidence + Hypothesis Posture).

**Dependencies:** L2 harness complete (shipped at 0db4a41).

**Adversarial note.** The L2 repo-aware gap may not be worth closing
if the Brain is primarily longitudinal. Single-turn synthesis
quality is a narrow concern; an operator running Brain-augmented
sessions for days at a time probably doesn't notice one-shot
+/-12 pt gaps.

---

### B-18: Measurement-instrument sensitivity characterization — CANDIDATE

**Why it's here.** The brain-vs-control experiments rest on a rubric
(correctness 40 / groundedness 25 / efficiency 20 / actionability 15)
and a judge (Sonnet blind to arm). The rubric's groundedness
weighting rewards epistemic humility — L0's "I don't know" answers
score well because they're humble. This interacts strongly with task
shape. The rubric is a **hidden variable** we haven't characterized.

Before any future experimental claim is elevated toward spec, the
instrument's sensitivity needs characterization: run the same tasks
under (a) the current rubric, (b) a correctness-heavy rubric that
penalizes humility, (c) a specificity-heavy rubric. If conclusions
are rubric-stable, trust them. If they flip, the single-turn
apparatus is too fragile for load-bearing claims.

**Plan when** a future single-turn experiment is scoped AND the
operator commits to making its findings load-bearing (e.g., by
feeding them into spec).

**Dependencies:** existing 432-row ledger; `comparison.py`
infrastructure.

**Adversarial note.** This entry could also be interpreted as
"make the experiments more rigorous." That's fine, but it shouldn't
become a prerequisite that blocks the longitudinal-value work. If
single-turn experiments are secondary instruments (per ROADMAP
posture), B-18 can stay CANDIDATE indefinitely — only activate when
someone wants to make a strong single-turn claim.

---

### B-19: Longitudinal-value artifacts as primary evidence — CANDIDATE

**Why it's here.** The Brain's primary value hypothesis is
longitudinal (see ROADMAP Evidence + Hypothesis Posture). Controlled
longitudinal experiments are impractical — we can't run a project
twice, once with a Brain and once without. So the natural evidence
base isn't controlled comparison but **artifacts the Brain produces
over time**: invocation-ledger patterns, proposal-ledger decisions
applied or ignored, promotion-ledger decision history, capability-
hygiene drift-detection hits over months, skill-coherence drift over
multi-Brain topologies, culture-substrate invariant holds.

This entry proposes a study of **what "Brain is working" looks like
in artifact patterns** — characterizing, across a 6-12 month
operator-observable window, which artifacts correlate with felt
value and which don't.

**Plan when** the invocation ledger, proposal ledger, and promotion
ledger have 2+ months of real operator data (currently just
weeks). Running too early produces noise.

**Dependencies:** B-01 adjacent (agent-behavior promotion flip
produces the first promotion-ledger entry with real operator
evidence); invocation-ledger has been accumulating since Axis 4 v1
shipped.

**Adversarial note.** This entry is particularly vulnerable to
selection bias — operators who run the Brain for months are already
the ones who believe in it. Falsification would require either
artifact patterns that clearly don't show value (hard to define
cleanly) or comparison against an operator who ran WITHOUT a Brain
over the same period (not currently measurable).

---

### B-20: PyPI publish of `lsp-brains` SDK — CANDIDATE (no current plan)

**Status update 2026-04-24:** Re-framed from "deferred pending
incident review" to **"no current plan to publish."** The
operator decision following the 2026-04-23 PyPI supply-chain
incident + Layer-1 supply-chain design work was that the
ecosystem's canonical SDK is Rust (`neurogrim-core` +
`neurogrim-sensory`), not Python. The Python SDK remains in-repo
as dogfood / internal-example / adopter-convenience, installable
from source only. B-20 is no longer a "resume when conditions
clear" item; it's a dormant roadmap entry that reactivates only
on substantive new inputs (see "Reactivation triggers" below).

**Why it's here.** v3.0-rc.1 originally planned to publish the
`lsp-brains` Python SDK to PyPI alongside the Rust crates. The
2026-04-23 incident (second-order scanner-chain compromise:
trojanized security-scanner binary → exfiltrated CI credentials →
trojanized package releases) surfaced a class of attack we would
not confidently defend against on a PyPI artifact today. Combined
with the Rust-first adoption posture (spec + reference impl +
native SCA all in Rust), we chose to not ship Python at all for
this release track.

**What's unchanged.** Package name `lsp-brains` remains reserved
(we do not recommend squatting, but also will not surrender the
name); `pyproject.toml` metadata lives in the repo; source install
works today (`pip install -e NeuroGrim/sdk-python/`).
`NeuroGrim-python-starter/README.md`, [`docs/sdk.md`](../docs/sdk.md),
and the v3.0-rc.1 release notes all document the source-install
path as the supported adoption path for Python-needing adopters.

**Reactivation triggers** (any ONE of):

1. Concrete user demand that cannot be served by the Rust SDK
   (e.g., an adopter organization where Python is a hard
   operational constraint AND `pip install -e` from source is
   insufficient for their deployment environment).
2. PyPI's trusted-publishing / attestation / SBOM story matures
   to a point where our integrity posture on a PyPI artifact
   would match or exceed the current Rust-only posture.
3. Our own native-Python SCA coverage (E-SC-3) reaches Layer 2+3
   parity with Layer 1 AND has demonstrated calibration against
   fresh real-world incidents.
4. A directional change in the ecosystem's canonical-SDK decision
   (non-incremental; would require operator-led re-planning).

**Dependencies.** Fully independent of the eleven SCA epics. None
of E-SC-0 through E-SC-10 require B-20 activation; B-20 remains
dormant regardless of how those progress.

**Adversarial note.** The risk of holding B-20 dormant is minor —
adopters who need Python can install from source today; the Rust
SDK is the canonical path. The risk of re-opening B-20 without
clear triggers is real: PyPI package names are irrevocable, publish
is one-way, and the "just ship it" energy is exactly what the
2026-04-23 incident class exploits. Dormancy is a feature, not a
stall.

---

### B-21: Native Rust license + ban-list sensor — CANDIDATE (deferred from E-SC-2)

**Why it's here.** E-SC-2's original scaffolding scope included a
`cargo-deny`-style license + ban-list checker as part of the
supply-chain SCA pipeline. Phase-1 research (2026-04-24) surfaced
that embedding `cargo-deny` as a library adds ~20+ transitive deps
and targets library use only partially (~28% docs.rs coverage).
That conflicts with the supply-chain-sca trust-surface posture
(small, pinned, auditable). License compliance is also a distinct
concern from supply-chain *attack surface* — separating concerns
kept E-SC-2 tight. Re-filed here as a separate, smaller epic.

**What it would add (sketched):**

- A new advisory-weight sensor (`license-compliance` or similar)
  that reads project `Cargo.toml` files for declared licenses,
  walks `Cargo.lock` for transitive deps, and asserts every
  observed license against an operator-curated allow-list.
- A complementary `dep-ban-list` sensor that asserts no entry in
  `Cargo.lock` matches an operator-curated ban list (by name +
  version-range).
- **Native parsing only** — no `cargo-deny` library embed. Use
  `spdx` crate for license-string normalization (small, focused,
  RustLang-team-adjacent). Hand-roll the allow-list/ban-list
  matching logic; ~100-200 LOC.

**Plan when:**
1. There is concrete operator demand for license-compliance
   gating (rare for libraries, more common for products with
   distribution constraints).
2. AND the supply-chain Layer 1+2+3 (E-SC-2 through E-SC-6) is
   complete + dogfood-stable. License compliance shouldn't take
   resources away from finishing the immune-system core.

**Dependencies.** None blocking. Cleanly slots in alongside
E-SC-3 / E-SC-4 / E-SC-5 if/when activated.

**Adversarial note.** This entry is intentionally narrower than
"port cargo-deny." NeuroGrim's posture is: every shipped sensor
should justify its trust-surface expansion. A 100-LOC native
license-allow-list checker is a smaller commitment than embedding
a 20+ transitive-dep library, even if the library would do more.
If operator demand turns out to need the more, the right move is
to revisit the embed-vs-rewrite decision then with fresh
information, not to over-build now.

---

### B-22: Python lockfile formats — poetry.lock + Pipfile.lock — CANDIDATE (deferred from E-SC-3)

**Why it's here.** E-SC-3's locked decisions (2026-04-24) scoped
Python lockfile coverage to `uv.lock` + `requirements*.txt` only,
deferring `poetry.lock` and `Pipfile.lock` to a follow-on. The
trade-off: NeuroGrim's own ecosystem doesn't use Poetry or pipenv,
so the dogfood signal is zero; adding two more parsers without
real test cases is speculative effort. Phase 1 research also
flagged that there's no good Rust crate for poetry.lock — a
hand-rolled parser would be ~200-300 LOC, which compounds the
speculative-effort concern.

**What it would add:**
- `lockfile/poetry.rs` — hand-rolled TOML parser for poetry.lock's
  shape (`[[package]]` arrays similar to uv.lock but with
  Poetry-specific source fields: `source.type = "git"`,
  `source.url`, `source.reference`, etc.).
- `lockfile/pipenv.rs` — JSON parser (Pipfile.lock is JSON, not
  TOML) — `serde_json` already in workspace deps.
- Variants `PoetryLock` + `PipenvLock` added to `DetectedLockfile`.
- ~5 unit tests per parser.

**Plan when:**
1. AND: an adopter explicitly asks for poetry/pipenv coverage
   (signals real demand).
2. AND: at least one of the two has a stable representative in
   the adopter's project (gives us a real fixture).
3. NOT BEFORE: E-SC-5/6/7/8 are done. Layer 2 vigilance and
   Layer 3 agent review are higher-leverage than format coverage.

**Dependencies.** None blocking. Slots cleanly into the
lockfile-dispatch infrastructure shipped in E-SC-3.

**Adversarial note.** Poetry's market share has plateaued/declined
since uv launched (2024-2025). By the time this entry activates,
the better path may be "just use uv, generate uv.lock from your
poetry pyproject" — Astral's tooling can ingest poetry-style
deps. We may end up never needing this entry.

---

### B-23: Brains-2.0 v2 enhancements — CANDIDATE (B2-deferred)

**Why it's here.** The Brains-2.0 scaffolding
(`audit/BRAINS-2-0-CHARTER.md`) shipped v1 primitives with natural
v2 enhancements deliberately deferred to keep v1 scoped + the
trust-budget posture conservative:

- **E-B2-3 v2 — runtime hat-contract enforcement.** v1 is static
  (file-audit) only. v2 adds runtime checks via session-trace
  analysis (capability-hygiene observes hat-tagged invocations vs
  declared anti-capabilities).
- **E-B2-4 v2 — trust-budget hard gates.** v1 is soft (advisory)
  findings. v2 promotes select budget violations to hard gates IF
  calibration data justifies.
- **E-B2-7 v3 — cross-Brain reputation decay.** Federated patterns
  v1 has bidirectional opt-in but no reputation. v2/v3 adds
  reputation decay on flood-prone peers + signal weighting by peer
  reliability.
- **E-B2-6 v2 — real-time disposition calibration.** v1 captures
  disposition via explicit operator action. v2 infers disposition
  from session traces (operator immediately invoked another skill,
  edited a file, etc.).

**Plan when:**
1. v1 of each primitive has shipped + dogfood-stable on the
   four-Brain ecosystem.
2. Calibration data shows the v1 advisory-only posture is producing
   actionable signal that operators currently dismiss for lack of
   teeth.
3. NOT BEFORE: E-B2-8 spec promotion to v3.0.

**Dependencies.** Brains-2.0 campaign complete (E-B2-0..E-B2-8).

**Adversarial note.** v2 enhancements are deliberately deferred,
not forgotten. The supply-chain campaign's R-1 (false-positive
fatigue) applies double here: hard gates without calibration data
causes operator burnout. v2 must wait for v1 to demonstrate signal
quality.

---

### B-24: Adjacent observability sensors — CANDIDATE (B2-adjacent)

**Why it's here.** Two sensor ideas surfaced during Brains-2.0
visionary discussion (2026-04-26) adjacent to the seven-direction
scope but outside the nine epics:

- **Negative-space / expected-absence sensor.** Reports what should
  be in the codebase that isn't. "If you have X, you should have Y;
  you have X without Y → flag." Example: project has `cargo audit`
  triggered but no `pip-audit` for a Python-mixed project.
- **Spec-prose ↔ test-fixture traceability sensor.** Each normative
  MUST in spec prose should have a corresponding test fixture in
  `conformance/`. Sensor checks: every spec MUST has at least one
  fixture; every fixture references a spec section.

**Plan when:**
1. AND: a concrete operator demand surfaces ("I want to know what
   I'm missing").
2. AND: Brains-2.0 is dogfood-stable (no new sensors before v3.0
   publishes).

**Dependencies.** None blocking; would build on capability-hygiene
+ spec-impl-alignment patterns.

**Adversarial note.** Negative-space is an unbounded class — the
variants of "what should be there" can grow without limit. v1 would
need to scope tightly: e.g., "every Python project with a
Dockerfile should also have a pyproject.toml with declared deps."
Specific, measurable, advisory.

---

### B-25: Brain identity primitives — CANDIDATE (B2-deferred)

**Why it's here.** Brains-2.0 visionary discussion surfaced two
adjacent ideas the plan-critic pass deliberately tabled as
premature:

- **Cryptographic Brain naming.** Brains today are identified by
  their `.claude/` directory + registry. Two Brains claiming the
  same identity have no way to disambiguate. Wait for genuine
  multi-org topology before paying the complexity cost.
- **Multi-operator governance.** Currently a small set of operators
  per Brain. As scale grows: how do we handle multi-operator
  Brains? Multi-org Brains? Cultural-substrate values apply to
  outputs but who's accountable when an operator violates them?

**Plan when:**
1. AND: Brains start being deployed in genuinely multi-org
   topologies (not the current four-Brain ecosystem owned by one
   operator).
2. AND: A real identity-collision incident has occurred OR a
   governance dispute has surfaced.

**Dependencies.** Brains-2.0 v3.0 publish (gives the four-Brain
ecosystem to a wider audience first).

**Adversarial note.** "Solving a problem we don't have yet" is the
failure mode. The four-Brain ecosystem is owned by one operator
with cultural-substrate + air-gapped-by-default doing real work.
Identity primitives without an actual incident is over-engineering.

---

### B-26: Active blocking / auto-rollback policies — CANDIDATE (B2-deferred)

**Why it's here.** Brains-2.0 visionary discussion surfaced
active-blocking policies as a possible future direction. The locked
decision: **advisory-only is doing real work; preserve.**

What it would add:
- A new `blocking-policies.toml` per Brain declaring which signal
  severities + confidence levels auto-block (refuse to proceed) vs
  auto-rollback (revert to previous good state).
- New CMDB extras field for "blocked" vs "rolled-back" states.
- Spec section for normative blocking semantics.

**Plan when:**
1. AND: Multiple operators have independently proposed
   blocking-policy work.
2. AND: At least one published incident showed advisory-only was
   insufficient (operator dismissed signal, attack succeeded).

**Dependencies.** Brains-2.0 v3.0 publish + operator-calibration
ledger demonstrates operator dismissal patterns.

**Adversarial note.** The supply-chain campaign's R-1
(false-positive fatigue) applies. Active blocking that fires on a
false positive is more disruptive than advisory + dismissed. The
"advisory by default" stance is a feature, not an oversight.
Re-evaluate only with concrete incident evidence.

---

### B-27: ML-based sensor models — CANDIDATE (B2-deferred)

**Why it's here.** Brains-2.0 visionary discussion considered
embedding ML models in sensors (vs the current heuristic +
LLM-as-judge approach). The locked decision: **heuristic +
LLM-as-judge is sufficient; opacity violates "sensors need sensors"
(principle #18).**

What it would add:
- ML models inside specific sensors (e.g., a typosquat detector
  trained on registry data + adversarial patterns).
- Model versioning + reproducibility infrastructure.
- Training-data audit trails.

**Plan when:**
1. AND: A specific sensor has demonstrably hit the ceiling of
   heuristic + LLM-as-judge approaches (e.g., typosquat detection
   where Levenshtein-≤1 misses confusable Unicode that a model
   could catch).
2. AND: The model can be made auditable + reproducible (open
   weights or open training set).
3. NOT BEFORE: Brains-2.0 is dogfood-stable.

**Dependencies.** Brains-2.0 v3.0 publish + at least one sensor
with measurable ceiling-hit + clear model story (not opaque
proprietary).

**Adversarial note.** Principle #18 ("sensors need sensors") implies
sensors must themselves be observable. A black-box ML model in a
sensor breaks that — the model is making decisions whose rationale
can't be inspected. v1 of any sensor should remain heuristic +
LLM-as-judge until explicitly justified otherwise.

---

## How to author a new backlog entry

1. Pick a short ID (`B-NN`, increment from the last one).
2. State the problem + what the item solves.
3. Name "plan when" preconditions — what triggers this becoming
   an epic?
4. List dependencies (blocking or merely recommended).
5. Keep it under ~150 words. If longer, it's ready to be an epic
   — extract to `roadmap/epics/`.
