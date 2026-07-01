---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# DSP Discovery — exploring "Data Server Protocol" for agents

> **Status: discovery / pre-prototype, started 2026-04-30.**
> **Removal cost: trivial — `rm -rf experiments/dsp-discovery/`.**
> **No production code, no published crate, no federation entry.**

## What this project is

A focused inquiry into a single question:

> Can we give agents a semantic-navigation interface over data — modeled
> after LSP's *protocol shape* (not its file metaphor) — that beats raw
> SQL on a representative set of NeuroGrim's own internal questions?

The bet is that LSP-style primitives — `goto-definition`, `find-references`,
`hover`, `completion`, `diagnostics` — capture what makes LSP load-bearing
for code agents, and that those same primitives applied to **schemas + rows**
might be a real ergonomic win for agents working with structured data.

The honest worry is that agents are already very good at SQL, and any new
abstraction that doesn't measurably improve task outcomes is just one more
thing for them to learn before falling back to SQL anyway.

This project exists to **find out**.

## What this project is NOT

- **Not** a faked file-system over a database. The "everything's a file"
  metaphor was the ergonomic seed for the original idea but it's been
  rejected as the implementation shape — see `INQUIRY.md` for why.
- **Not** trying to unify SQL, document, KV, wide-column, and graph stores
  behind one protocol. The honest answer is each store family probably
  warrants its own DSP. We start with SQL because it's where NeuroGrim's
  substrate lives.
- **Not** a fully-specified protocol. We're trying to learn whether the
  shape works at all before designing it formally.
- **Not** a child Brain in the federation tree (yet). It's a research
  scaffold under NeuroGrim. If it earns graduation, it can become its own
  Brain or workspace crate.

## Why under NeuroGrim?

Two reasons:

1. **Real data, real tasks, real comparisons.** NeuroGrim's substrate after
   v4.0.0 is rich: a TSDB with 5+ instrumented series, 8+ bus topics,
   skill-invocation ledgers, score-snapshot history. Agent questions like
   "what's the trajectory of test-health over 30 days, joined against skill
   invocations during that window?" are *real* questions agents will need to
   answer to make use of the substrate. We have a built-in benchmark.

2. **Methodology fit — schema as commitment.** NeuroGrim's pre-declared tag
   dimensions on metrics, queue-config schema declarations, and CMDB shape
   contracts mean the schemas are first-class. A DSP layer needs schemas
   to navigate; we already have them.

## Layout

```
experiments/dsp-discovery/
├── README.md           ← you are here
├── INQUIRY.md          ← the questions we're trying to answer + why
├── DESIGN-SQL.md       ← proposed method shape for SQL-DSP (first target)
├── KILL-CRITERIA.md    ← concrete signals that say "shut this down"
└── tasks/              ← real NeuroGrim agent questions, in both forms
    ├── 01-domain-trajectory-diagnosis.md
    ├── 02-cache-effectiveness-analysis.md
    ├── 03-cardinality-runaway-detection.md
    └── 04-skill-decay-with-context.md
```

## Operating posture

- **Discovery first, code second.** The first artifacts are markdown — task
  specs and design proposals. No prototype until the design clears the
  paper review.
- **Compare apples-to-apples.** For every task, write both the raw-SQL
  approach and the DSP-shaped approach. The discovery is whether DSP wins
  on agent task time, accuracy, or composition with other tools.
- **Kill criteria are in `KILL-CRITERIA.md`.** Read them. If we hit any of
  them, this project ends and we delete the directory. That's a feature,
  not a failure.
- **No federation entry, no published crate.** This project is invisible
  to NeuroGrim's scoring pipeline. It will not produce a CMDB, will not
  report a score, will not affect any unified score anywhere. If it
  graduates, that's a separate explicit promotion event.

## Where to start reading

1. `INQUIRY.md` — what we're actually asking
2. `DESIGN-SQL.md` — what the protocol could look like
3. `tasks/01-domain-trajectory-diagnosis.md` — the first concrete test
4. `KILL-CRITERIA.md` — when to stop

If you (an agent or an operator) come back to this in a month and any of
the kill criteria have triggered, please honor them. Don't sunk-cost-fallacy
this into something it isn't.
