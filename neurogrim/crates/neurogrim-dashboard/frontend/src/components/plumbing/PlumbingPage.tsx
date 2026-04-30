import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Activity,
  AlertTriangle,
  Database,
  HardDrive,
  Hash,
  LineChart,
} from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { brainApi, useBrainId } from "@/lib/useBrain";

/**
 * v4.5 / B-36 iteration 1 — Plumbing page.
 *
 * Surfaces NeuroGrim's own substrate (TSDB, queue topics, eventually
 * SQLite stores + filesystem watchers) as observable + controllable
 * operator surface. The page exists because the v4.4 dogfooding
 * session moved a lot of plumbing in-tree (bus topics, SQLite stores,
 * keypath diffs) — this page makes that work visible.
 *
 * **v1 tabs:** Metrics (TSDB series listing) + Queues (bus topics).
 * **Deferred to v2 (B-36 iteration 2):** Storage (SQLite file sizes
 * + vacuum), Watchers (filesystem-watcher status), Connections (A2A
 * peer history).
 */
type Tab = "metrics" | "queues";

interface PlumbingOverview {
  metrics: {
    enabled: boolean;
    series_count: number;
    total_points: number;
    size_bytes: number;
  };
  queues: {
    size_bytes: number;
  };
}

interface SeriesInfo {
  name: string;
  point_count: number;
  cardinality: number;
  earliest_ts: string | null;
  latest_ts: string | null;
}

interface SeriesListResponse {
  series: SeriesInfo[];
}

interface QueueTopicStats {
  topic: string;
  message_count: number;
  oldest: string | null;
  newest: string | null;
  size_bytes: number;
}

interface QueueListing {
  topics: QueueTopicStats[];
}

export function PlumbingPage() {
  const brainId = useBrainId();
  const [tab, setTab] = useState<Tab>("metrics");

  return (
    <div className="space-y-6">
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">Plumbing</h1>
        <p className="text-sm text-muted-foreground">
          NeuroGrim's substrate — time-series store, bus topics, and
          (later) SQLite files + watchers. Operator-facing observability
          for the system that observes your project.
        </p>
      </header>

      <PlumbingHeader brainId={brainId} />

      <div className="border-b">
        <nav className="flex gap-1" aria-label="Plumbing tabs">
          <TabButton
            active={tab === "metrics"}
            onClick={() => setTab("metrics")}
            icon={<LineChart className="h-4 w-4" />}
            label="Metrics (TSDB)"
            testId="plumbing-tab-metrics"
          />
          <TabButton
            active={tab === "queues"}
            onClick={() => setTab("queues")}
            icon={<Activity className="h-4 w-4" />}
            label="Queues"
            testId="plumbing-tab-queues"
          />
        </nav>
      </div>

      {tab === "metrics" ? (
        <MetricsTab brainId={brainId} />
      ) : (
        <QueuesTab brainId={brainId} />
      )}
    </div>
  );
}

function TabButton({
  active,
  onClick,
  icon,
  label,
  testId,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  testId: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      data-testid={testId}
      className={
        "px-4 py-2 text-sm font-medium border-b-2 transition-colors " +
        "flex items-center gap-2 -mb-px " +
        (active
          ? "border-primary text-foreground"
          : "border-transparent text-muted-foreground hover:text-foreground")
      }
      aria-current={active ? "page" : undefined}
    >
      {icon}
      {label}
    </button>
  );
}

// ── Header overview cards ───────────────────────────────────────────────

function PlumbingHeader({ brainId }: { brainId: string }) {
  const { data, error } = useQuery({
    queryKey: ["plumbing-overview", brainId],
    queryFn: async (): Promise<PlumbingOverview> => {
      const res = await fetch(brainApi(brainId, "plumbing/overview"));
      if (!res.ok) throw new Error(`overview ${res.status}`);
      return res.json();
    },
    refetchInterval: 10_000,
  });

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center gap-2">
            <AlertTriangle className="h-4 w-4 text-destructive" />
            Plumbing overview unavailable
          </CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          {error instanceof Error ? error.message : "unknown error"}
        </CardContent>
      </Card>
    );
  }

  const m = data?.metrics;
  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
      <StatCard
        icon={<LineChart className="h-4 w-4" />}
        label="TSDB series"
        value={m ? String(m.series_count) : "…"}
        sublabel={m?.enabled ? "enabled" : "disabled"}
      />
      <StatCard
        icon={<Hash className="h-4 w-4" />}
        label="TSDB points"
        value={m ? formatNumber(m.total_points) : "…"}
        sublabel={m ? formatBytes(m.size_bytes) : ""}
      />
      <StatCard
        icon={<Database className="h-4 w-4" />}
        label="Queue topics"
        value={data ? formatBytes(data.queues.size_bytes) : "…"}
        sublabel="on disk"
      />
      <StatCard
        icon={<HardDrive className="h-4 w-4" />}
        label="Brain ID"
        value={brainId}
        sublabel="this dashboard"
      />
    </div>
  );
}

function StatCard({
  icon,
  label,
  value,
  sublabel,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  sublabel?: string;
}) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardDescription className="flex items-center gap-1.5 text-xs">
          {icon}
          {label}
        </CardDescription>
      </CardHeader>
      <CardContent className="pt-0">
        <div className="text-2xl font-semibold tabular-nums">{value}</div>
        {sublabel ? (
          <div className="text-xs text-muted-foreground mt-0.5">{sublabel}</div>
        ) : null}
      </CardContent>
    </Card>
  );
}

// ── Metrics tab ─────────────────────────────────────────────────────────

function MetricsTab({ brainId }: { brainId: string }) {
  const [selected, setSelected] = useState<string | null>(null);
  const { data, isLoading, error } = useQuery({
    queryKey: ["plumbing-series", brainId],
    queryFn: async (): Promise<SeriesListResponse> => {
      const res = await fetch(brainApi(brainId, "plumbing/metrics/series"));
      if (!res.ok) throw new Error(`series ${res.status}`);
      return res.json();
    },
    refetchInterval: 10_000,
  });

  if (isLoading) {
    return (
      <Card>
        <CardContent className="py-6 text-sm text-muted-foreground">
          Loading metric series…
        </CardContent>
      </Card>
    );
  }
  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center gap-2">
            <AlertTriangle className="h-4 w-4 text-destructive" />
            Failed to load series
          </CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          {error instanceof Error ? error.message : "unknown"}
        </CardContent>
      </Card>
    );
  }
  if (!data || data.series.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base">No series recorded yet</CardTitle>
          <CardDescription>
            Metrics start populating after the first dashboard request,
            score run, or peer probe. Stay on this page for ~30 seconds
            and series should appear (auto-refreshes every 10s).
          </CardDescription>
        </CardHeader>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">
            Registered series ({data.series.length})
          </CardTitle>
          <CardDescription>
            Click a series to query recent points. Cardinality = distinct
            tag combinations recorded for that series; high cardinality
            (&gt;~50) is a typical TSDB footgun worth investigating.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Series</TableHead>
                <TableHead className="text-right">Points</TableHead>
                <TableHead className="text-right">Cardinality</TableHead>
                <TableHead>Earliest</TableHead>
                <TableHead>Latest</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.series.map((s) => (
                <TableRow
                  key={s.name}
                  onClick={() => setSelected(s.name)}
                  className={
                    "cursor-pointer hover:bg-accent/50 " +
                    (selected === s.name ? "bg-accent/40" : "")
                  }
                  data-testid={`series-row-${s.name}`}
                >
                  <TableCell className="font-mono text-sm">{s.name}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatNumber(s.point_count)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    <Badge
                      variant={s.cardinality > 50 ? "destructive" : "secondary"}
                    >
                      {s.cardinality}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {formatTs(s.earliest_ts)}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {formatTs(s.latest_ts)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {selected ? <SeriesDetailCard brainId={brainId} name={selected} /> : null}
    </div>
  );
}

interface SeriesDetailResponse {
  name: string;
  since: string;
  point_count: number;
  points: Array<{
    ts: string;
    value: number;
    tags: Record<string, string>;
  }>;
}

function SeriesDetailCard({
  brainId,
  name,
}: {
  brainId: string;
  name: string;
}) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["plumbing-series-detail", brainId, name],
    queryFn: async (): Promise<SeriesDetailResponse> => {
      const url = brainApi(
        brainId,
        `plumbing/metrics/${encodeURIComponent(name)}?window_s=86400&limit=200`,
      );
      const res = await fetch(url);
      if (!res.ok) throw new Error(`series detail ${res.status}`);
      return res.json();
    },
    refetchInterval: 10_000,
  });

  return (
    <Card data-testid={`series-detail-${name}`}>
      <CardHeader>
        <CardTitle className="text-base font-mono">{name}</CardTitle>
        <CardDescription>
          {data
            ? `${data.point_count} points in the last 24h (capped at 200)`
            : "loading…"}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="text-sm text-muted-foreground">Loading…</div>
        ) : error ? (
          <div className="text-sm text-destructive">
            {error instanceof Error ? error.message : "unknown"}
          </div>
        ) : !data || data.points.length === 0 ? (
          <div className="text-sm text-muted-foreground">
            No points in the last 24h.
          </div>
        ) : (
          <div className="max-h-72 overflow-auto rounded border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-44">Timestamp</TableHead>
                  <TableHead className="text-right w-24">Value</TableHead>
                  <TableHead>Tags</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.points.slice(-50).reverse().map((p, i) => (
                  <TableRow key={`${p.ts}-${i}`}>
                    <TableCell className="font-mono text-xs">
                      {formatTs(p.ts)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums text-xs">
                      {formatValue(p.value)}
                    </TableCell>
                    <TableCell className="text-xs">
                      {Object.entries(p.tags).length === 0 ? (
                        <span className="text-muted-foreground">—</span>
                      ) : (
                        Object.entries(p.tags).map(([k, v]) => (
                          <span
                            key={k}
                            className="inline-block mr-2 font-mono text-muted-foreground"
                          >
                            {k}={v}
                          </span>
                        ))
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ── Queues tab ──────────────────────────────────────────────────────────

function QueuesTab({ brainId }: { brainId: string }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["plumbing-queues", brainId],
    queryFn: async (): Promise<QueueListing> => {
      const res = await fetch(brainApi(brainId, "queues"));
      if (!res.ok) throw new Error(`queues ${res.status}`);
      return res.json();
    },
    refetchInterval: 10_000,
  });

  if (isLoading) {
    return (
      <Card>
        <CardContent className="py-6 text-sm text-muted-foreground">
          Loading queue topics…
        </CardContent>
      </Card>
    );
  }
  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center gap-2">
            <AlertTriangle className="h-4 w-4 text-destructive" />
            Failed to load queue topics
          </CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          {error instanceof Error ? error.message : "unknown"}
        </CardContent>
      </Card>
    );
  }
  if (!data || data.topics.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base">No queue topics yet</CardTitle>
          <CardDescription>
            Topics appear after the first publish — config edits, score
            runs, service starts, etc. all create topics on demand.
          </CardDescription>
        </CardHeader>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">
          Bus topics ({data.topics.length})
        </CardTitle>
        <CardDescription>
          Persistent storage for the message bus. <code>_neurogrim/*</code>{" "}
          are system topics (score-snapshots, services, skill-invocations,
          approvals, config-changes, etc.). Adopter-defined topics live
          alongside.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Topic</TableHead>
              <TableHead className="text-right">Messages</TableHead>
              <TableHead className="text-right">Size</TableHead>
              <TableHead>Oldest</TableHead>
              <TableHead>Newest</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.topics.map((t) => (
              <TableRow
                key={t.topic}
                data-testid={`queue-row-${t.topic.replace(/\//g, "-")}`}
              >
                <TableCell className="font-mono text-sm">{t.topic}</TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatNumber(t.message_count)}
                </TableCell>
                <TableCell className="text-right tabular-nums text-xs text-muted-foreground">
                  {formatBytes(t.size_bytes)}
                </TableCell>
                <TableCell className="text-xs text-muted-foreground">
                  {formatTs(t.oldest)}
                </TableCell>
                <TableCell className="text-xs text-muted-foreground">
                  {formatTs(t.newest)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

// ── Formatting helpers ──────────────────────────────────────────────────

function formatNumber(n: number | bigint): string {
  return Number(n).toLocaleString();
}

function formatBytes(n: number | bigint): string {
  const num = Number(n);
  if (num < 1024) return `${num} B`;
  if (num < 1024 * 1024) return `${(num / 1024).toFixed(1)} KB`;
  if (num < 1024 * 1024 * 1024) return `${(num / 1024 / 1024).toFixed(1)} MB`;
  return `${(num / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatTs(ts: string | null | undefined): string {
  if (!ts) return "—";
  try {
    const d = new Date(ts);
    return d.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return ts;
  }
}

function formatValue(v: number): string {
  if (Number.isInteger(v)) return v.toLocaleString();
  if (Math.abs(v) >= 1000) return v.toFixed(0);
  if (Math.abs(v) >= 1) return v.toFixed(2);
  return v.toFixed(4);
}
