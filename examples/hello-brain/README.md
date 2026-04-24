# hello-brain — minimal NeuroGrim example

A tiny project with a pre-configured Brain registry, intended for
newcomers to see the Brain produce a real score in one command.

## Run it

From inside this directory (`examples/hello-brain/`):

```bash
neurogrim score --project-root .
```

Sample output shape (your numbers will differ):

```
✦ Casting score…
NeuroGrim Score: 48/100  (confidence: 71%)
  + git-health raw:60 eff:15
  + test-health raw:40 eff:14
  + code-quality raw:45 eff:11
  - deploy-readiness raw:0 eff:0 (confidence: 12%)
Trajectory: no-data (velocity: +0.0, samples: 0)

Findings:
  ! No CI configuration found (deploy-readiness)
  ! Single test file; consider expanding coverage (test-health)
  ! No lint config detected (code-quality)
  ...
```

## What's in here

- `brain-registry.json` — 4 active domains (git-health, test-health,
  code-quality, deploy-readiness). Weights sum to 1.0.
- `src/main.py` — a deliberately trivial module so the sensors
  have something to observe.
- `tests/test_main.py` — three pytest cases so `test-health` sees
  a real test file.
- `.claude/` (empty by default; regenerated CMDBs land here).

## What's NOT in here (intentionally)

- No CI workflow → `deploy-readiness` will score low. That's the
  point: the Brain surfaces what's missing.
- No SECURITY.md / CODEOWNERS / Dependabot → if you add the
  `security-standards` domain to the registry, it'll flag these.
- No lint config → `code-quality` will note it.

Each missing thing is an entry point: pick one you care about,
add it to your real project, watch the score move.

## Next steps

- Point the Brain at YOUR project: copy `brain-registry.json`
  to your project's `.claude/` directory, trim to relevant
  domains, and run `neurogrim score --project-root .` from
  your project root.
- Read the full walkthrough at
  [`docs/getting-started.md`](../../docs/getting-started.md).
