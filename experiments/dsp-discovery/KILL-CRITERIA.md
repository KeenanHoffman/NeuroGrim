---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# KILL-CRITERIA.md

> *Explicit signals that mean "shut this down". When an experiment
> declares its failure modes upfront, sunk-cost fallacy doesn't get a
> seat at the table.*

## Hard kills (any one of these → delete the directory)

### 1. No measurable advantage on tasks after 4 weeks of evaluation

If we run all four `tasks/*.md` benchmarks against both raw-SQL and DSP
paths, capture turn counts + accuracy + wall-clock, and the DSP doesn't
win on at least 2 of the 4 by a meaningful margin (≥ 25% reduction in
turns OR ≥ 25% improvement in first-try accuracy), the protocol shape
isn't pulling its weight.

**Walk-away action:** delete `experiments/dsp-discovery/`, write a
backlog entry titled "B-NN: DSP-shaped data access — explored, no
measurable benefit on internal tasks, see archive note."

### 2. We can't write the protocol prototype without compromising the design

If, three weeks in, we find ourselves adding methods we explicitly excluded
in `DESIGN-SQL.md` (a fake `listRows`, a "well, just for this case let's
add a query DSL"), or smuggling SQL strings through method parameters, the
abstraction isn't clean. A clean abstraction is the load-bearing claim;
without it, this is just a wrapper.

**Walk-away action:** as above.

### 3. The proposed methods are universally satisfied by `db/execute` plus a
schema cache

If the agent's path through every benchmark task collapses to "call
`db/describe` once, then call `db/execute` for everything else", the DSP
isn't navigation — it's documentation. We don't need a protocol for that;
we need a CLI tool that prints the schema and a slightly nicer SQL prompt.

**Walk-away action:** capture the negative result. Possibly note that
"first-class schema discovery" is a separate, smaller idea worth pursuing
on its own — but NOT as a protocol.

## Soft kills (combination signals → reconsider scope)

### 4. The benefit is real but only on toy queries

If DSP wins on the simple lookup tasks but agents revert to SQL the moment
they need anything analytical (group-by, window functions, recursive CTEs),
we haven't built a protocol — we've built a fancy autocomplete.

**Walk-away action:** consider scoping DSP to *just* navigation methods
(`resolveRef`, `findRefs`, `hover`, `describe`) and explicitly NOT trying to
serve analytical workloads. That's a smaller, more honest project. If even
that smaller version doesn't justify the spec work, hard-kill.

### 5. The protocol works for SQL but doesn't generalize

If we get something working for SQL but every attempt to apply the same
shape to NeuroGrim's bus-topic JSON payloads or the CMDB JSON files
requires fundamentally different methods, the "DSP family" framing is
wrong. Each store family probably warrants its own protocol with no
meaningful shared core.

**Walk-away action:** narrow scope to `neurogrim-sql-dsp` only, drop the
generality framing. That's still a useful artifact — just smaller than
"a new protocol family."

### 6. Modern agents become so good at SQL between now and evaluation that the gap
closes

If during the evaluation window models improve enough that raw-SQL
performance ties or beats DSP on tasks where DSP previously won, the
problem is dissolving on its own. We don't need to ship a solution to a
problem the platform is solving for us.

**Walk-away action:** capture as a "the wave passed us" note and move on.

## Preservation criteria (NOT signals to kill)

These are things that look like failure but are NOT:

- **A draft of `tasks/*.md` that turns out to be unrealistic.** Replace
  the task, don't kill the project. The benchmarks need to evolve as we
  learn what's representative.
- **An agent picking SQL over DSP on a single task.** We need pattern,
  not anecdote. Look at totals.
- **A method in `DESIGN-SQL.md` that no one calls.** That's discovery —
  drop the method, keep the project. The minimum-set question is one of
  the inquiry's stated goals.
- **A v0 prototype that's slow.** Performance is solvable; design isn't.
  Slowness disqualifies a production crate, not a discovery prototype.

## Process

If any hard-kill criterion triggers:

1. Open a 1-page post-mortem at `experiments/dsp-discovery/POST-MORTEM.md`
   describing what we learned, what didn't work, what we'd try
   differently if we revisited.
2. Open a backlog entry in `roadmap/BACKLOG.md` (e.g. B-37 or next free)
   capturing the negative result so future-us doesn't re-explore from
   scratch.
3. Delete the `experiments/dsp-discovery/` directory.
4. Commit with message: "experiments: retire dsp-discovery — see B-NN
   for negative result".

Negative results are productive when they're explicit. This list is the
"explicit" part.
