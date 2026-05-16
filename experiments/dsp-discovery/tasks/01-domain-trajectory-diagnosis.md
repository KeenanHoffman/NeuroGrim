# Task 01 — Domain Trajectory Diagnosis

> **The premise:** an operator (or higher-level agent) asks a NeuroGrim
> agent: *"Which domains have been declining over the last 30 days, and
> for each, what's the most recent activity that touched them?"*
>
> This is a real question NeuroGrim's substrate is supposed to support
> — `domain_score` series + `_neurogrim/skill-invocations` topic both
> sit in the same SQLite cluster. We compare how an agent answers it
> raw-SQL vs. DSP.

## What "the answer" looks like

A list, ordered by severity of decline:

```
test-health           was 78 → now 56 (Δ -22)   last skill: "neurogrim-onboarding" 14 days ago
supply-chain-vigilance was 90 → now 73 (Δ -17)  last skill: "dependency-discipline" 9 days ago
deploy-readiness      was 82 → now 71 (Δ -11)   last skill: (none in window)
...
```

The agent has to:

1. Identify all distinct `domain` tag values in `domain_score`
2. For each, find the score 30 days ago (closest sample) and most recent
3. Compute the delta, filter to declining ones
4. Cross-reference each declining domain to the `skill-invocations` bus
   topic for relevant recent activity (where "relevant" is fuzzy —
   probably skills mentioning the domain name? skills run during the
   decline window? both?)
5. Order by severity of decline

## The raw-SQL path

What an agent today probably writes:

```sql
WITH score_endpoints AS (
  SELECT
    json_extract(tags_json, '$.domain') AS domain,
    FIRST_VALUE(value) OVER (
      PARTITION BY json_extract(tags_json, '$.domain')
      ORDER BY ts_ms ASC
      RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
    ) AS earliest_value,
    LAST_VALUE(value) OVER (
      PARTITION BY json_extract(tags_json, '$.domain')
      ORDER BY ts_ms ASC
      RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
    ) AS latest_value
  FROM metric_points
  WHERE metric_name = 'domain_score'
    AND ts_ms >= (strftime('%s', 'now') - 30*86400) * 1000
),
deltas AS (
  SELECT DISTINCT domain, earliest_value, latest_value,
         (latest_value - earliest_value) AS delta
  FROM score_endpoints
),
declining AS (
  SELECT * FROM deltas WHERE delta < 0 ORDER BY delta ASC
)
SELECT * FROM declining;
-- ...then a SECOND query against the skill-invocations topic for each domain
```

Actual agent friction observed (anticipated, to verify during evaluation):

- Has to know `metric_points` schema with `tags_json` extraction syntax
- Has to know SQLite-specific `strftime` epoch math
- Has to know about window functions to get earliest/latest per group
- Then has to write a *second* query (or a complex JOIN) to get the
  skill-invocations cross-reference, because the topic data lives in a
  different SQLite file under `.claude/brain/queues/`
- Has to manually open both database files (or do `ATTACH DATABASE`)
- Result correlation done in the agent's head, not in SQL

## The DSP path

```
1. db/listObjects(schema: "metrics")
   → sees `metric_points`, `acks`, `schema_version`

2. db/describe("metrics", "metric_points", with_stats: true)
   → sees columns, sees `metric_name` distinct values include
     "domain_score", sees `tags_json` is a JSON column with sampled
     keys including "domain"

3. db/execute(
     "SELECT json_extract(tags_json, '$.domain') AS domain,
             value, ts_ms
      FROM metric_points
      WHERE metric_name = 'domain_score'
        AND ts_ms >= ?",
     [thirty_days_ago_ms]
   )
   → raw points by domain

4. (agent computes per-domain earliest/latest in its head — small N,
    one row per (domain, day) at most for a 30-day window)

5. For each declining domain:
     db/listObjects(schema: "skill_invocations")
     db/describe("skill_invocations", "messages")
     db/execute(
       "SELECT payload, produced_at FROM messages
        WHERE produced_at >= ?
          AND json_extract(payload, '$.name') LIKE ?
        ORDER BY produced_at DESC LIMIT 1",
       [decline_start, "%" || domain || "%"]
     )
```

So... is this actually better?

## The honest comparison

Looking at the two paths side by side, here's what I notice:

**The DSP version is NOT obviously shorter.** Both end up calling
`db/execute` with SQL anyway. The DSP wins on `describe` (the agent
discovers schema via a structured response instead of writing
`PRAGMA table_info`), but loses some of that win to the back-and-forth
of method calls.

**The DSP version IS more incremental.** Each step is independently
verifiable. The agent can sanity-check `domain_score` is the right
metric_name before continuing. With raw SQL, the agent commits to a
single 25-line query; if any line is wrong, the whole thing fails
opaquely.

**The DSP version surfaces cross-database structure better.** The
agent learns there's a `metrics` schema and a `skill_invocations`
schema (one per topic). With raw SQLite, it has to know to `ATTACH
DATABASE` and remember which file holds which tables — that's
operational tax.

**Neither version naturally expresses "find skills mentioning the
domain name."** The fuzzy correlation happens in the SQL string in
both cases. DSP doesn't have a structural advantage here unless we
add some kind of `db/searchPayloads` method — which would smell of
scope creep.

## What this task tells us

If we run this benchmark with both paths and capture:

1. **Number of tool calls** — DSP probably loses (more chatty)
2. **Number of agent-side errors / retries** — DSP probably wins (each
   step independently verifies)
3. **First-try success rate** — likely a wash (most of the complexity
   is in the SQL, not the navigation)
4. **Composition with follow-ups** — e.g., "now drill into test-health
   specifically." DSP might win here because the schema shape is
   already cached on the agent.

**Hypothesis:** DSP's win on Task 01 will be modest. The SQL is complex
enough that the JOIN-and-window-function lift dominates the total work,
and DSP doesn't help with that.

If DSP's win is much bigger than expected on this task, we've underestimated
the protocol. If it's the same or smaller, that's information too — Task 01
is not where DSP shines, and we should look harder at tasks 03 and 04
which lean on navigation.

## Evaluation rubric

For a representative Sonnet-class agent, run both paths cold (no schema
prefetched). Capture:

| Metric | Raw-SQL | DSP | Winner |
|--------|---------|-----|--------|
| Tool calls to first answer | | | |
| Retries / corrections needed | | | |
| Wall-clock (rough) | | | |
| Correctness of first answer | | | |
| Reusable schema knowledge after task | | | |

Run **at least 5 trials per path** to smooth over single-run variance.
Document each trial's transcript verbatim — agent reasoning is the
interesting part.

## Out of scope for this task

- **Performance.** Both paths hit the same SQLite. DSP adds RPC overhead
  on top, but we're not optimizing for hot paths in v0.
- **Concurrency.** Single-agent, single-DB, read-only.
- **Caching.** No cache between trials. Each trial starts fresh.
- **Federation.** We're querying NeuroGrim's local substrate. Cross-Brain
  data access is a separate question.
