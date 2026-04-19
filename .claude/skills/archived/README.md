# Archived Skills — Starter-Kit Era Reference

> **Archived 2026-04-17** — all files in this directory are preserved as
> **read-only historical reference** from the archived PowerShell starter-kit
> (moved to `D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\` on 2026-04-17).

## Why these are archived

These skills were written for an earlier project context — a LaaS platform
built on GCP, Terraform, Cloud Run, Pester tests, and PowerShell tooling
(`Find-*.ps1`, `scripts/utility/*.ps1`, etc.). NeuroGrim is a Rust
reimplementation and does not share that tooling; following these skills
would route operators to commands and tools that no longer exist.

The methodology patterns in some of these files remain interesting as
historical context, but none describe the Rust Brain's actual workflow.
**Do not follow the commands inside. Read for reference only.**

## If you're looking for the Rust-Brain live inventory

The active skill set is in the parent directory (`.claude/skills/`). See
`CLAUDE.md` at the repo root for the Skills Index.

## Closest live equivalents (where one exists)

| Archived skill | Closest live skill / command |
|---|---|
| `gate-status.md` | `neurogrim health` |
| `what-next.md` | `neurogrim score` (correlation engine recommendations) |
| `refocus.md` | `neurogrim health --plain` |
| `operational-memory.md` | `neurogrim trend` |
| `brain.md` | The binary at `neurogrim/target/release/neurogrim`; see `CLAUDE.md` |
| `lsp.md` and LSP-family | No direct equivalent; the Rust Brain has no LSP layer |
| `hooks-reference.md` | No direct equivalent; the Rust Brain uses different automation |
| `skill-index.md` | Use the CLAUDE.md Skills Index |
| `hats.md` | Hat concept lives in spec §6 / §8; no rewritten skill yet |

## Recovering an archived skill

If one of these turns out to still be useful, recovery is one command:

```bash
git mv archived/<name>.md <name>.md
# Update the body for the Rust Brain context
# Re-add to the Skills Index in CLAUDE.md
```

## Provenance

All files here were originally at `.claude/skills/<name>.md` and were moved
wholesale via `git mv`. Git history is preserved. The triage decision record
lives in the session plan at `C:\Users\koff0\.claude\plans\` on the host
that performed the archive.
