---
doc-version: 1.0
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Task 02 — Cache Effectiveness Analysis

> **The premise:** *"Is the BrainContext cache actually helping us, by
> hour-of-day? Show hit rate over the last week, bucketed."*
>
> Pure aggregation task. Tests whether DSP earns its keep on
> analytical workloads, or whether SQL's expressiveness is the right
> shape and DSP gets in the way.

## What "the answer" looks like

```
hour    hits   misses   hit_rate
00:00    142     18      88.7%
01:00     97     11      89.8%
...
14:00   1840    420      81.4%   ← peak load, lower hit rate, expected
...
```

Plus a brief summary the agent composes: "Hit rate sits 85-90% off-peak,
drops to ~80% during peak hours (12-16 UTC) when concurrent load
invalidates entries faster than TTL refresh. Cache is doing useful work."

## The raw-SQL path

```sql
SELECT
  strftime('%H', ts_ms / 1000, 'unixepoch') AS hour,
  SUM(CASE WHEN json_extract(tags_json, '$.kind') = 'hit'  THEN value ELSE 0 END) AS hits,
  SUM(CASE WHEN json_extract(tags_json, '$.kind') = 'miss' THEN value ELSE 0 END) AS misses,
  ROUND(100.0 * SUM(CASE WHEN json_extract(tags_json, '$.kind') = 'hit' THEN value ELSE 0 END)
              / SUM(value), 1) AS hit_rate
FROM metric_points
WHERE metric_name = 'cache_event'
  AND ts_ms >= (strftime('%s', 'now') - 7*86400) * 1000
GROUP BY hour
ORDER BY hour;
```

Single query. Reasonably idiomatic SQL. The `cache_event` schema is
small (just `cache` + `kind` tags), so the agent doesn't need much
discovery.

## The DSP path

```
1. db/describe("metrics", "metric_points", with_stats: true)
   → sees 'cache_event' is a metric with tag-keys [cache, kind]
   → sees `kind` distribution: ~85% "hit", ~15% "miss", a few "invalidate"

2. db/execute(<the same SQL as above>)
```

Honestly: DSP adds nothing here. The query is one statement, the
schema discovery returns information the agent could have inferred
from the metric name, and there's no cross-table navigation.

## What this task tells us

This is the "control" benchmark — the case we expect SQL to win.

If DSP somehow wins this task too, the win must be coming from
something subtle (better hover output? agent confidence?). Worth
investigating.

If DSP loses this task, we've confirmed our hypothesis: **DSP isn't a
SQL replacement, it's a SQL complement.** That's a kill-criterion 4
signal — soft kill if it generalizes, but useful information.

The point of this task is **to set the floor**. If DSP loses Task 02
but wins Tasks 01, 03, 04, that's a meaningful pattern: navigation
helps, aggregation doesn't.
