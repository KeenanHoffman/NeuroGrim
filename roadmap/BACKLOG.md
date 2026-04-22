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

**Last updated:** 2026-04-21 (B-06 absorbed back into S9-ABV-RED-4 per operator request; v1 ships both Architectures A and B).

---

## Identified 2026-04-21 (post S8-ABV-EXT close-out)

These were the "NOT in scope" items from the S8-ABV-EXT epic (see
`epics/S8-agent-behavior-extensions.md`). They're recognized as
real future work; none is blocking current operations.

### B-01: Promote `agent-behavior` past advisory weight

**Why it's here.** S8-ABV-EXT shipped the trust infrastructure
(calibration gate, multi-judge consensus, execution-based rubrics).
The weight flip from 0.0 to > 0.0 — so the domain actually moves
the Brain's aggregate score and gates — is a **policy decision, not
a code change**. An epic scopes (a) what threshold triggers
automatic promotion, (b) which Brains enable first, (c) the
rollback trigger when scores swing post-flip, and (d) the operator
runbook for doing a calibration audit and declaring it passed.

**Plan when:** a real project needs agent-behavior to affect
deploy-readiness. Early-adopter candidate: the NeuroGrim Brain
dog-fooding against its own skills.

**Dependencies:** S8-ABV-EXT complete (done); operator-led
calibration audit passed on live credentials.

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
