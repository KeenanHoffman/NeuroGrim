<!-- topic: hat — bundled in neurogrim-cli v3.5 -->
# Hats — declared decision-making lenses

A **hat** is a declared lens an agent or operator wears to filter
attention and bias decision-making. Hats are formal contracts:
each declares a domain emphasis, a communication style, and a set
of typical recommendations. The same Brain state, viewed through
two hats, produces two coherent-but-different priority orderings.

To narrate this Brain's state through a hat:

```bash
neurogrim narrate --hat <hat-name>
```

To see this Brain's declared hats:

```bash
neurogrim agent --prose
```

## The 8 declared hats

NeuroGrim ships 8 hat profiles, defined in
`.claude/skills/hats/SKILL.md` and the per-hat files alongside it:

| Hat                       | Bias                                        |
|---------------------------|---------------------------------------------|
| `adversary`               | Find what could break; hostile review       |
| `architect`               | Long-term shape, dependencies, layering     |
| `incident-commander`      | Blast radius, time-to-mitigate              |
| `rubber-duck`             | Socratic listener; surfaces operator's own thinking |
| `security-auditor`        | SOC2 / ISO27001 / NIST CSF posture          |
| `supply-chain-auditor`    | SCA + vigilance + Layer 3 review            |
| `visionary`               | Where this should go; opportunity over risk |
| `source-reader`           | What the code actually says, not what we think it does |

Each hat has a hat-narration template that produces 3–5 lines of
prose summarizing the Brain's state through that hat's bias. Run
`neurogrim narrate --hat adversary` and contrast with
`neurogrim narrate --hat visionary` on the same Brain — same data,
different priority surface.

## When to wear a hat

- **Before reviewing a plan**: `--hat adversary` (find weaknesses)
  paired with `--hat visionary` (find missed opportunity)
- **During incident response**: `--hat incident-commander`
- **For architectural decisions**: `--hat architect`
- **When stuck**: `--hat rubber-duck` paired with the
  `rubber-duck` skill (Socratic conversation, not narration)
- **Pre-release**: `--hat security-auditor` and
  `--hat supply-chain-auditor` in sequence
- **For unfamiliar code**: `--hat source-reader` (read the
  artifact, suspend assumption)

## Hat-mediated decision-making

Hats are not just visualization — they are decision-making
contracts. When operating under a declared hat, an agent should:

1. **Announce the hat** at the start of the response:
   `Wear Hat: <hat-name>`
2. **Filter through the hat's bias** — what does this hat care
   about that another wouldn't?
3. **Use the hat's communication style** — adversary is blunt;
   visionary is expansive; incident-commander is terse and
   action-first; rubber-duck asks more than it tells.
4. **Stay coherent** — don't switch hats mid-response unless the
   situation explicitly calls for the switch (e.g., "Wear Hat:
   adversary first; then Wear Hat: incident-commander to plan
   mitigation").

## Adding a new hat

Hats live in `registry.config.hats` and as
`.claude/skills/hats/<name>.md` files. Adding a new hat is a
two-step process: declare it in the registry (so `narrate` can
find it) and author its skill file (so agents know how to wear
it). Templates ship in
`neurogrim-cli/data/narration-templates/<hat>.toml`.

Most projects don't need new hats — the 8 declared cover the
common decision-making postures. Author one only when:
- A project has a recurring decision-making mode the existing 8
  don't cover (e.g., `compliance-officer` for a regulated
  industry)
- The hat's bias is materially different from existing hats
- You can produce a 3–5 line narration template that makes the
  bias concrete

## Cultural substrate

Hats can only narrow attention; they cannot loosen the cultural
invariants. An adversary hat is allowed to be blunt — but not
disrespectful. A visionary hat is allowed to be expansive — but
not dishonest. Floor only, never ceiling. See
`neurogrim explain culture`.

## Cross-references

- `neurogrim explain methodology` — how hats fit in the overlay
- `neurogrim narrate --hat <name>` — see this Brain narrated through a hat
- `.claude/skills/hats/SKILL.md` — full hat-system documentation
- Spec §5.4.1 — formal hat contract
