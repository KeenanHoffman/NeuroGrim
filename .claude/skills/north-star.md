# North Star

Role: meta · guiding
Governs: .claude/roadmap/VISION.md, .claude/roadmap/ROADMAP.md

Trigger phrases: "north star", "vision check", "does this advance", "are we on track",
Methodology-step: skills
"roadmap", "backlog", "which stage", "what stage are we in"

---

## What It Is

A lightweight alignment check. Before committing to a plan or after completing significant
work, this skill asks: **does this advance us toward Stage 5?**

Stage 5 is the north star: LSP Brains becomes a transferable specification — a methodology
any project can adopt. Moth(er):Br+AI+n is the product that proves it. See `.claude/roadmap/VISION.md` for the full vision.

---

## Quick Check (3 questions)

When evaluating any proposed work against the north star:

1. **Does this make the pattern more general or more specific?**
   General is good — it transfers. Specific is fine for now but shouldn't lock us in.

2. **Does this make the ecosystem Brain easier or harder to build later?**
   If a change only works for one project's domain names, hardcoded paths, or specific
   CMDB schemas, it's a warning sign.

3. **Which stage and backlog item does this advance?**
   If the answer is "none," ask whether the work should exist. Not everything needs to
   advance the roadmap — maintenance and bug fixes are valid. But new features should
   have a home on the roadmap.

4. **Does this respect truth separation?**
   Source truth in git, runtime truth in external systems, derived truth compiled on demand.
   If a compiled index is being committed, or external state is being wedged into a source
   file, it's a warning sign.

---

## Roadmap Reference

```
Stage 1: Honest Single Brain                      [COMPLETE]
Stage 2: Interface Contract + Framework Extraction [PLANNED]
Stage 3: Prescriptive Autonomy                     [PLANNED]
Stage 4: Fractal Composition                       [PLANNED]
Stage 5: Transferable Practice                     [NORTH STAR]
```

Full roadmap: `.claude/roadmap/ROADMAP.md`
Dependencies: `.claude/roadmap/DEPENDENCIES.md`
Data architecture: `.claude/roadmap/DATA-ARCHITECTURE.md`
Epic files: `.claude/roadmap/epics/`
Vision document: `.claude/roadmap/VISION.md`

---

## When to Invoke

- After writing a plan (before plan-critic)
- When choosing between two approaches and unsure which to pick
- At session start if working on Brain/LSP/EaC features
- When scope creep feels likely

---

## Design Principles (from VISION.md)

1. Declarations over dashboards
2. Scoring must be honest
3. Observation is as valuable as action
4. The Brain should learn from its own recommendations
5. Hats are how agents think
6. Fractal by design
7. The pattern is the product (thesis statement of LSP Brains)
8. Absorption over invention
9. Communication is an interface, not a side effect
10. Every file is interpretable
11. Separate source truth from runtime truth from derived truth
12. Trajectories reveal more than snapshots

When in doubt, choose the option that advances these.

---

## See Also

- `imagination-mode.md` — pre-plan exploration (visionary hat)
- `plan-critic.md` — adversarial review before implementation
- `brain.md` — current Brain reference
- `devops-philosophy.md` — the 8 DevOps principles underlying domain weights
- `.claude/roadmap/DEPENDENCIES.md` — critical path and parallelization
