---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Task 04 — Skill Decay With Context

> **The premise:** *"Which skills haven't been invoked in 30 days, and
> for each, walk me through the last few times they were used to
> understand what they were doing — were they being used right?"*
>
> The reference-walking task. Foreign-key resolution would map cleanly
> if `skill_invocations` had relational links to `score_snapshots`. It
> doesn't natively, but invocations and snapshots from the same session
> share a `session_id` — DSP's `findRefs` could express that
> relationship structurally.

## What "the answer" looks like

```
hat-driven-debugging   not invoked in 47 days
  last 3 invocations:
    2026-03-14 — skill called within session abc-123
                  (also in that session: 2 score runs, brain_score 78→81)
    2026-03-09 — skill called within session def-456
                  (also in that session: 1 score run, no score change)
    2026-02-28 — ...

  inferred: the skill was used during diagnostic sessions that often
  produced score improvements — not a skill being abandoned because
  it's broken, but one that lost relevance after the issue it solved
  was fixed. Probably ok to keep dormant; mark as "rare" in registry.
```

The agent has to:

1. List all distinct skills from `_neurogrim/skill-invocations` topic
2. For each, find max `produced_at`
3. Filter to those > 30 days stale
4. For each stale skill, fetch its last N invocations
5. For each invocation, find OTHER messages in the same session
   (cross-topic JOIN: skill-invocations.session_id ↔
   score-snapshots.session_id)
6. Synthesize a narrative

## The raw-SQL path

```sql
-- Step 1: stale skills
WITH skill_last_seen AS (
  SELECT
    json_extract(payload, '$.name') AS skill_name,
    MAX(produced_at) AS last_seen
  FROM messages
  WHERE topic = '_neurogrim/skill-invocations'
  GROUP BY skill_name
)
SELECT skill_name, last_seen
FROM skill_last_seen
WHERE julianday('now') - julianday(last_seen) > 30;

-- Step 2 (per stale skill): last 3 invocations + their sessions
SELECT json_extract(payload, '$.session_id') AS session_id, produced_at
FROM messages
WHERE topic = '_neurogrim/skill-invocations'
  AND json_extract(payload, '$.name') = ?
ORDER BY produced_at DESC LIMIT 3;

-- Step 3 (per session): co-occurring messages
-- BUT: skill-invocations topic and score-snapshots topic live in
-- DIFFERENT SQLite files. Agent must `ATTACH DATABASE` or query each
-- separately and join in its head.
```

The cross-topic relationship is the friction. SQLite can do `ATTACH
DATABASE` but agents often don't reach for it. They end up writing
two queries and stitching results in the agent's reasoning.

## The DSP path

The interesting move here is whether DSP can model the cross-topic
relationship as a structural reference, even though there's no FK.

**Attempt 1 — explicit:**

```
1. db/listSchemas() → ["metrics", "skill_invocations", "score_snapshots", ...]

2. db/describe("skill_invocations", "messages")
   → reveals payload contains session_id (from sample stats)

3. db/execute("SELECT json_extract(payload,'$.name'), MAX(produced_at)
               FROM messages GROUP BY 1") in skill_invocations schema

4. (filter stale in head)

5. For each stale skill, db/execute(<last-3 query>)

6. For each (skill, session_id) tuple:
   db/execute("SELECT * FROM score_snapshots.messages
               WHERE json_extract(payload,'$.session_id') = ?")
```

That's not really better than raw SQL. The schema-listing gives the
agent the cross-topic map for free, but the actual joining is still
manual.

**Attempt 2 — speculative new method:**

```
db/findRefs(
  schema: "skill_invocations",
  table: "messages",
  match: { 'payload.session_id': 'abc-123' },
  scope: ['score_snapshots.messages', 'config_changes.messages', ...]
)
```

This says: "find messages in any of these schemas where the JSON path
`payload.session_id` matches `abc-123`."

**This is interesting.** It encodes the cross-topic correlation as a
structural query without requiring the agent to write the cross-database
SQL. The cost is adding `match` semantics to `findRefs`, which gets
fuzzy fast — what if the JSON path doesn't exist on some topics? What
about type coercion?

We don't add it to v0. But this task is the one that justifies it
being on the roadmap if discovery proceeds.

## What this task tells us

This is the **decisive test** for DSP's structural advantage.

If DSP wins big on Task 04 — and especially if it wins because of
something like the speculative `findRefs.match` — then the protocol
*is* doing something agents can't easily express in SQL alone.
Cross-store correlation has been a real friction point in NeuroGrim's
substrate (the bus topics being separate SQLite files makes natural
JOINs impossible without ATTACH DATABASE rituals).

If DSP doesn't win Task 04, the navigation thesis is in trouble. The
whole idea was that schema-aware navigation would feel different from
SQL composition. If even cross-topic reference-walking doesn't show
that difference, the protocol probably isn't pulling its weight.

This task is the one most likely to trigger kill-criterion 1 (no
measurable advantage) or to surface the core win the protocol is
chasing. Watch this benchmark closely.
