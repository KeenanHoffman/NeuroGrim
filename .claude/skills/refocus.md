# Refocus

Regain clarity during long or drifting agent sessions. Shows a compact brain health pulse,
surfaces the highest-priority blockers, and recommends the single next action to take.

Role: operational · diagnostic
Governs: Find-SessionContext.ps1
Trigger phrases: "refocus", "what should I do next", "I'm lost", "session drift",
Domain: deploy
Methodology-step: skills
"get back on track", "where was I", "what's the priority"

---

## Quick Reference

```powershell
# Compact refocus view
pwsh -File scripts/dev/Find-SessionContext.ps1 -Action refocus -Plain

# Refocus with brain score header
pwsh -File scripts/dev/Find-SessionContext.ps1 -Action refocus -Brain
```

## When to Refocus

Use refocus when any of these conditions are true:

- **Session age >2 hours** — Context decay makes priorities fuzzy
- **Context was compacted** — After a conversation summary, re-anchor on current state
- **Multiple tangents** — You started task A, got pulled into B, now unsure what's next
- **Returning from break** — Quickly re-orient without a full session-recap
- **After branch switch** — Gate state may have changed; refocus shows what matters now

## What Refocus Shows

1. **Brain health score** — Single-line unified score (if `-Brain` flag is set)
2. **Gate status counts** — How many gates are dirty, stale, needs-run, clean
3. **Top 3 priority gates** — Highest-priority non-clean gates with their clear commands,
   sorted by tier: immediate > before-merge > pre-deploy > advisory
4. **Next action** — One-line actionable recommendation:
   - Dirty gates exist: "Next: clear [gate] → [command]"
   - Only stale gates: "Next: refresh [gate] (stale Nh)"
   - All clean: "All gates clean — ready to commit/deploy"

## Refocus vs Other Session Modes

| Mode | When | Depth |
|------|------|-------|
| `refocus` | Quick re-orient, "what's next?" | Light — top 3 gates + recommendation |
| `commit` | About to commit | Medium — commit-blocking gates + change impact |
| `deploy` | About to deploy | Medium — deploy-blocking gates + topology |
| `debug` | Something is broken | Deep — dirty gates + governing skills + debug path |
| (empty) | Session start | Full — all non-clean gates across all domains |

## Manual Refocus Checklist

When the tool output isn't sufficient:

1. Run `gate-advisor.ps1 -Action commit` — are any gates blocking?
2. Check `git status` — any uncommitted work?
3. Check `git log --oneline -5` — what was the last thing done?
4. Run `Find-Brain.ps1 -Mode health -Plain` — full brain health
5. Re-read the current plan file if one exists

## See Also

- `session-recap.md` — Full session startup synthesis (heavier than refocus)
- `what-next.md` — Prioritized action list based on current state
- `gate-status.md` — Deep gate inspection and clearing procedures
- `brain.md` — Unified health score and cross-domain correlation
