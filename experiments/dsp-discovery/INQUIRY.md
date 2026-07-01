---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# The inquiry

> *What we're actually trying to learn, and why.*

## The seed observation

LSP works for code agents. Not because it's a file abstraction (it isn't,
really — it's request-response over JSON-RPC keyed by URIs and ranges),
but because it gives agents primitives that match how they *already think*
about code: navigate to definition, find references, what is this thing,
what's wrong with this code, what could go here.

Database access today, by contrast, is mostly: agent composes a SQL string,
agent gets back a result blob. There's no equivalent semantic layer. Every
question goes through the same fat pipe of "write SQL, parse result."

The seed question: **does that fat-pipe model leave ergonomic value on the
floor?** And specifically — would the LSP-shaped primitives apply
meaningfully to data?

## The four sub-questions we want answers to

### 1. Does semantic navigation over data measurably help?

Concretely: pick a set of NeuroGrim agent tasks (`tasks/*.md`). For each,
compare:

- **Raw-SQL path:** the agent writes SQL against the SQLite directly
- **DSP path:** the agent uses the proposed DSP methods

Measure:

- Time-to-correct-answer (turns of tool use, wall clock)
- Accuracy (does the agent get the right result first try?)
- Composition (can the result feed into a follow-up question naturally?)
- Failure modes (what does the agent do when stuck?)

If raw SQL wins on every task, the DSP idea is a solution looking for a
problem. If DSP wins on some tasks and not others, the boundary tells us
what the protocol's actual value is.

### 2. What's the *minimum* DSP method set?

LSP has dozens of methods. Most are unused most of the time. For data,
what's the smallest set that captures the bulk of the value?

Hypothesis (to test, not assume):

- `db/describe` — schema-level discoverability
- `db/hover` — column-level inline metadata
- `db/resolve-ref` — foreign-key navigation
- `db/find-refs` — reverse foreign-key navigation
- `db/diagnose-query` — pre-execution validation
- `db/execute` — escape hatch (the agent can always drop to SQL)

If even half of these never get called by agents, they don't belong in
the protocol. If agents constantly need something we didn't list,
that's a discovery.

### 3. Does the abstraction hold up under schema variation?

NeuroGrim's substrate isn't homogeneous. The TSDB metrics table has a
fixed schema. The bus topic SQLite tables share a schema family but
each topic stores arbitrary JSON payloads. The CMDBs are JSON, not
SQL at all. Does a DSP for SQL stretch to cover these gracefully, or
does each shape want its own protocol?

This is the spec-design question with the highest "I-don't-know"
quotient.

### 4. Is the agent benefit purely about SQL skill replacement, or is there a deeper structural win?

If DSP just lets agents avoid writing SQL by hand, the benefit is small
(and shrinks as models get better at SQL). The *interesting* question is:
does DSP enable agents to *compose* data operations in ways raw SQL
doesn't?

Example: an agent walking a foreign-key chain across three tables to
answer "where did this score come from?" can express that as:

```
1. resolve-ref(domain_score row 8723) → invocation row
2. resolve-ref(invocation row) → session row
3. find-refs(session row) → all skill calls in that session
```

vs. composing one large SQL JOIN. The DSP version is more
*incremental* — each step is independently verifiable, and the agent
can branch off at any point. Whether that's actually useful is an
empirical question.

## Why we're skeptical of ourselves

Three honest worries we should keep front of mind:

1. **Modern agents are extremely good at SQL.** This space might be a solved
   problem we're trying to re-solve.
2. **The abstraction layer adds a learning surface.** Agents have to learn
   when to use DSP vs. when to drop to SQL. That meta-decision can
   exceed the protocol's intrinsic value.
3. **"It feels nicer" is not enough.** If DSP doesn't measurably win on
   the comparison tasks, "it feels more LSP-like" is not a sufficient
   justification to ship it.

If we read these worries six months from now and feel they were
prescient — kill the project. Don't sunk-cost.

## Why we're cautiously optimistic anyway

Three honest reasons to take it seriously:

1. **Foreign-key navigation as `goto-definition` is a real cognitive match.**
   Agents understand reference-following naturally. SQL's JOIN syntax
   forces them to encode something they think of as navigation as
   set-relations. That impedance mismatch is real.
2. **Hover-over-column for typed metadata is useful in a way agents
   currently route around.** Right now they query `information_schema`
   or read DDL or guess. A protocol primitive for "what is this column?"
   could be a clean win.
3. **Schema as commitment fits NeuroGrim's methodology.** The "everything
   inspectable as files" plus "schema as producer commitment" principles
   already require us to make schemas explicit. Building a DSP that
   *uses* those schemas natively continues the dogfood.

## What success looks like

Three months from now, one of:

- **(A) Clear win on at least 2 of 4 benchmark tasks.** The DSP shape demonstrably
  helps agents navigate NeuroGrim's substrate. We graduate the project
  to a real crate (`neurogrim-dsp`) and start designing the formal protocol.
- **(B) Mixed results, clear boundary.** DSP wins on some kinds of tasks
  (e.g. reference-walking) and loses on others (e.g. aggregations).
  We document the boundary and ship a *narrow* DSP that addresses only
  what it's good at, leaving the rest to SQL.
- **(C) No measurable benefit.** Raw SQL is fine, the abstraction adds
  cognitive load without removing burden. We delete this directory and
  capture the negative result in a backlog entry so future-us doesn't
  re-explore the same dead end.

(C) is a perfectly good outcome. Negative results are productive when
they're explicit.
