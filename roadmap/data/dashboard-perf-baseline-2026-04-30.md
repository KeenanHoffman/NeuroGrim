# Dashboard performance baseline — 2026-04-30

**Captured immediately after the v4.4 dogfooding session** (items A/B/C/D/E
shipped: bus-topic SQLite migrations + handler wrapper + helpers). This is
the **post-refactor** baseline. There is no pre-refactor baseline because
performance regression detection didn't exist as a discipline before this
work — that gap is a primary motivator for the TSDB epic (B-29).

Future runs of this harness should land alongside this file as
`dashboard-perf-baseline-<date>.md` so we can detect regressions even
before TSDB-driven self-instrumentation is wired up.

## Setup

- Dashboard binary: `target/debug/neurogrim` (debug build — release would be
  faster; this captures a worst-case picture for development workflows)
- Brain registry: `D:/Brains/NeuroGrim/.claude/brain-registry.json`
- Loopback only: `127.0.0.1:8430`
- Browser: Playwright via `mcp__Claude_Preview` toolset
- Methodology: navigate to a route, wait for query settlement, collect
  `performance.getEntriesByType('resource')` filtered to `/api/`

## Harness

The harness installs a global `__perfMark()` / `__perfReport()` pair via
`preview_eval`. `__perfMark` clears resource timings and records the start
time; `__perfReport` returns:

```json
{
  "wallClock_ms": <max(start + duration) across calls>,
  "totalCpu_ms": <sum of durations>,
  "callCount": <number of /api/ requests>,
  "location": <current pathname>,
  "calls": [{ "url": "...", "duration": ms, "bytes": N, "startMs": offset }]
}
```

Source (paste into `preview_eval`):

```js
window.__perfMark = () => {
  window.__perfStart = performance.now();
  performance.clearResourceTimings();
};
window.__perfReport = () => {
  const elapsed = Math.round(performance.now() - (window.__perfStart || 0));
  const calls = performance.getEntriesByType('resource')
    .filter(e => e.name.includes('/api/'))
    .map(e => ({
      url: e.name.split('/api/')[1].split('?')[0],
      duration: Math.round(e.duration),
      bytes: e.transferSize,
      startMs: Math.round(e.startTime - (window.__perfStart || 0)),
    }))
    .sort((a, b) => a.startMs - b.startMs);
  const totalCpu = calls.reduce((s, c) => s + c.duration, 0);
  const wall = calls.length
    ? Math.max(...calls.map(c => c.startMs + c.duration)) : 0;
  return { wallClock_ms: wall, totalCpu_ms: totalCpu,
           callCount: calls.length, location: location.pathname, calls };
};
```

## Per-page summary

| Page                 | Wall-clock | Calls | Total CPU | Notes                                     |
|----------------------|-----------:|------:|----------:|-------------------------------------------|
| Overview (cold)      |    ~110 ms |    13 |    396 ms | Fan-out per-domain + cache miss           |
| Overview (warm)      |    ~ 70 ms |    11 |    202 ms | BrainContext cached but CMDBs re-read     |
| Domains list         |    ~110 ms |     1 |     92 ms | Single endpoint, expensive on server      |
| Logs                 |    ~ 60 ms |     6 |    267 ms | All 6 readers ~42-47 ms each              |
| Settings (Culture)   |    ~ 10 ms |     1 |      5 ms | Just culture.yaml                         |
| Services             |    ~ 13 ms |     1 |      4 ms | New SQLite topic — fastest                |
| **Federation**       | **2200 ms**|     1 |  2192 ms  | **Sequential peer probes — known issue**  |

## Detail: Overview cold-load (13 calls)

```
url                                                bytes   ms   start
brains                                                635   8       0
brains/neurogrim/hats                                 680  45      82
brains/neurogrim/overview                            1623  44      83
brains/neurogrim/dashboard-layout                    3071  42      86
brains/neurogrim/domains/test-health                 1456  19     196
brains/neurogrim/domains/code-quality                1450  20     196
brains/neurogrim/domains/deploy-readiness            1477  20     197
brains/neurogrim/domains/rust-health                 1650  28     197
brains/neurogrim/domains/security-standards          2783  37     198
brains/neurogrim/domains/supply-chain-vigilance      5797  38     199
brains/neurogrim/domains/agent-behavior              1559  31     199
brains/neurogrim/domains/capability-hygiene          2803  32     200
brains/neurogrim/domains/skill-coherence             1541  32     200
```

## Detail: Overview warm-cache (11 calls; BrainContext hot)

```
url                                                bytes   ms   start
brains/neurogrim/domains/test-health                 1456   6      29
brains/neurogrim/domains/code-quality                1450  21      30
brains/neurogrim/domains/deploy-readiness            1477   8      30
brains/neurogrim/domains/rust-health                 1650  14      31
brains/neurogrim/domains/security-standards          2783  13      32
brains/neurogrim/domains/supply-chain-vigilance      5797  17      34
brains/neurogrim/domains/agent-behavior              1559  27      40
brains/neurogrim/domains/capability-hygiene          2803  27      41
brains/neurogrim/domains/skill-coherence             1541  18      43
brains/neurogrim/overview                            1623  26      44
brains/neurogrim/dashboard-layout                    3071  25      45
```

Per-domain calls dropped from ~30 ms to ~15-25 ms each — server cache hit
saves the BrainContext rebuild cost but each `domain_detail` still re-reads
the per-domain CMDB JSON file from disk on every call.

## Identified bottlenecks (root-caused)

### B-33: Federation peer probes are sequential

[`routes.rs:849`](../../crates/neurogrim-dashboard/src/routes.rs) loops
peers with `await` per probe. Each `probe_peer` has a 1500 ms timeout. With
N peers and any unreachable, wall-clock = sum-of-timeouts.

**Fix:** `tokio::spawn` + `futures::future::join_all`. Wall-clock collapses
to max-of-timeouts.

### B-34: Per-domain CMDB re-reads on every request

Every `/domains/{name}` and `/domains` call hits
[`read_json_value(&cmdb_full)`](../../crates/neurogrim-dashboard/src/routes.rs)
on every request. With 9 domains × ~20 ms each = ~180 ms of CPU on a single
Overview load that's largely repeated work — CMDBs only change when a
sensor runs.

**Fix:** Stat-validated cache. Hold parsed `serde_json::Value` per
`PathBuf`; on read, `fs::metadata().modified()`; if mtime unchanged,
return cached clone. Filesystem watcher events evict eagerly.

### B-35: `ScoreChanged` clears the entire cache

[`cache.rs:96`](../../crates/neurogrim-dashboard/src/cache.rs) wipes the
whole BrainContext cache on any `ScoreChanged` event, even though the event
carries `domain: Option<String>`. After a single domain's CMDB write, every
page navigation triggers full re-builds for every domain.

**Fix:** Granular invalidation — when `ScoreChanged { domain: Some(d) }`,
evict only entries that depend on that domain.

## Why this baseline matters

Before the v4.4 dogfooding session, several of these endpoints were
reading entire JSON arrays / JSONL files on every request. The
`score-history.json` reader in particular was doing read-modify-write of
the whole array on every score run. The numbers above are **after** that
work; pre-refactor numbers are unrecoverable but were materially worse.

The TSDB epic (B-36) will close this observability gap permanently:
`request_duration{path, status}` self-instrumentation lands in iteration 2,
turning future regressions from invisible drift into visible time-series.
