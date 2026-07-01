---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Task 03 — Cardinality Runaway Detection

> **The premise:** *"Are any TSDB series approaching cardinality cliff?
> List series with > 50 distinct tag combinations and show me which tag
> values are blowing up."*
>
> Tests `db/hover` and `db/describe` with stats. This is where DSP
> *should* win — the question is fundamentally "tell me about the
> shape of this thing", which is exactly what `hover` is for.

## What "the answer" looks like

```
request_duration_ms    cardinality 13   tags: [path × 9, status × 4]
                       most common: path="/api/brains/:id/overview" (821 pts)
                       cardinality watch: OK (well below 50)

cache_event            cardinality  3   tags: [cache × 1, kind × 3]
                       most common: kind="hit" (4128 pts)
                       cardinality watch: OK

domain_score           cardinality  9   tags: [domain × 9]
                       cardinality watch: OK

(no series above 50 — all healthy)
```

If any series WERE above 50, the agent would drill in and report which
tag values are over-represented (e.g. "the `path` tag has 73 distinct
values because every individual brain_id is leaking through; expected
the path normalizer to collapse them to `:id`").

## The raw-SQL path

```sql
SELECT
  metric_name,
  COUNT(DISTINCT tags_json) AS cardinality,
  COUNT(*) AS total_points
FROM metric_points
GROUP BY metric_name
HAVING cardinality > 50
ORDER BY cardinality DESC;

-- Then for each over-cardinality series, the agent has to write:
SELECT
  json_extract(tags_json, '$.path') AS path,  -- or whatever tag key
  COUNT(*) AS pt_count
FROM metric_points
WHERE metric_name = '<offender>'
GROUP BY path
ORDER BY pt_count DESC
LIMIT 10;
```

The agent has to *know* which tag keys to extract, which means either
guessing or pre-querying with `json_each(tags_json)` to discover them.
This is the interesting friction.

## The DSP path

```
1. db/describe("metrics", "metric_points", with_stats: true)
   → returns ObjectShape with stats for each metric_name AND a
     breakdown of tag-key cardinality per metric:

     metric_points stats: {
       per_metric: [
         { name: "request_duration_ms",
           point_count: 1247,
           cardinality: 13,
           tags: { path: 9, status: 4 } },
         { name: "cache_event",
           point_count: 4128,
           cardinality: 3,
           tags: { cache: 1, kind: 3 } },
         ...
       ]
     }

2. (filter to cardinality > 50 in the agent's head — small N)

3. For each offender:
   db/hover("metrics", "metric_points", "tags_json",
            scope: { metric_name: "<offender>" })
   → returns ColumnHover with top-values per tag key
```

**This is where DSP plausibly shines.** The `describe` call returns
pre-computed cardinality breakdown. The agent doesn't have to know
SQL to ask "what's the shape of this data?" — that's the protocol's
job. The result is the answer, mostly.

The hover-with-scope pattern (filtering `tags_json` to a specific
`metric_name`) is a small extension to the v0 design that this task
surfaces — worth noting in DESIGN-SQL.md if we proceed.

## What this task tells us

If DSP wins Task 03 by a meaningful margin, the protocol's value is in
**operational shape questions** — the questions agents ask when they're
investigating their own substrate, not when they're querying business
data.

That's a real and defensible niche. NeuroGrim's substrate is full of
"is this thing healthy?" questions. So is any agent's runtime
observability surface. A protocol that makes those easy might be
worth shipping even if it doesn't generalize to all data access.

If DSP doesn't win Task 03 — if the agent's `db/describe` call returns
a stats blob that's hard to parse, or if the agent ends up writing the
GROUP BY anyway because it didn't trust the protocol's stats — then
the methodology is suspect. Stats-on-demand has to actually save work.
