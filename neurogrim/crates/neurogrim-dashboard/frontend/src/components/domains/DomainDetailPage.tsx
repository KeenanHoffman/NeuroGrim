import { useQuery } from "@tanstack/react-query";
import {
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { ArrowLeft } from "lucide-react";
import type { DomainDetailResponse } from "@bindings/DomainDetailResponse";
import type { FindingDto } from "@bindings/FindingDto";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
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
import { useNavigate } from "@tanstack/react-router";
import { hatToQuery, useHat } from "@/lib/useHat";

interface DomainDetailPageProps {
  /** Domain name (kebab-case) parsed from the URL `/domains/<name>`. */
  name: string;
}

async function fetchDomainDetail(
  name: string,
  hat: string | null
): Promise<DomainDetailResponse> {
  const url = hat
    ? `/api/domains/${encodeURIComponent(name)}?hat=${encodeURIComponent(hat)}`
    : `/api/domains/${encodeURIComponent(name)}`;
  const res = await fetch(url);
  if (res.status === 404) {
    throw new Error(`Domain '${name}' not found in the registry.`);
  }
  if (!res.ok) {
    throw new Error(`${url} returned ${res.status}`);
  }
  return (await res.json()) as DomainDetailResponse;
}

export function DomainDetailPage({ name }: DomainDetailPageProps) {
  const navigate = useNavigate();
  const { hat } = useHat();
  const queryHat = hatToQuery(hat);
  const { data, isLoading, error } = useQuery({
    queryKey: ["domain-detail", name, queryHat],
    queryFn: () => fetchDomainDetail(name, queryHat),
  });

  if (isLoading) return <DetailSkeleton />;
  if (error || !data) {
    return (
      <div className="space-y-4">
        <BackButton onClick={() => navigate({ to: "/domains" })} />
        <Card className="border-destructive">
          <CardHeader>
            <CardTitle className="text-destructive">Failed to load domain</CardTitle>
          </CardHeader>
          <CardContent>
            <pre className="text-xs">{(error as Error)?.message ?? "Unknown error"}</pre>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <BackButton onClick={() => navigate({ to: "/domains" })} />

      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-4">
            <div>
              <CardTitle className="text-2xl">{data.display_name}</CardTitle>
              <CardDescription className="font-mono text-xs">
                {data.name}
              </CardDescription>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {data.weight === 0 ? (
                <Badge variant="outline">advisory</Badge>
              ) : (
                <Badge variant="secondary">weight {data.weight.toFixed(2)}</Badge>
              )}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            <Stat label="Effective" value={data.effective_score} colored />
            <Stat label="Raw" value={data.raw_score} />
            <Stat label="Confidence" value={`${data.confidence}%`} />
            <Stat label="Trajectory" value={data.trajectory_class} />
          </div>
          {data.sensor_intent && (
            <div className="mt-6 rounded border border-border bg-muted/30 p-4 text-sm">
              <div className="mb-1 text-xs uppercase tracking-wider text-muted-foreground">
                Sensor authoring intent
              </div>
              <div className="text-foreground/90">{data.sensor_intent}</div>
            </div>
          )}
        </CardContent>
      </Card>

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Score history</CardTitle>
            <CardDescription>
              {data.history.length === 0
                ? "No history yet — the Brain hasn't recorded any score samples for this domain."
                : `Last ${data.history.length} sample${data.history.length === 1 ? "" : "s"}`}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {data.history.length > 0 ? (
              <HistorySparkline history={data.history} />
            ) : (
              <div className="flex h-48 items-center justify-center text-sm text-muted-foreground">
                Run <code className="mx-1 rounded bg-muted px-1.5 py-0.5">neurogrim score</code>{" "}
                to record a sample.
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-lg">CMDB metadata</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <div>
              <div className="text-xs uppercase tracking-wider text-muted-foreground">
                Path
              </div>
              <div className="font-mono text-xs break-all">{data.cmdb_path}</div>
            </div>
            <div>
              <div className="text-xs uppercase tracking-wider text-muted-foreground">
                Last updated
              </div>
              <div>{data.last_updated ?? "—"}</div>
            </div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg">Findings ({data.findings.length})</CardTitle>
          <CardDescription>
            Per-finding observations from the most recent sensor run.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {data.findings.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              No findings — either the sensor produced none or no CMDB exists yet.
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead className="w-24">Status</TableHead>
                  <TableHead className="w-20 text-right">Points</TableHead>
                  <TableHead>Detail</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.findings.map((f, i) => (
                  <FindingRow key={`${f.name}-${i}`} finding={f} />
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function FindingRow({ finding }: { finding: FindingDto }) {
  const variant = (() => {
    switch (finding.status.toLowerCase()) {
      case "pass":
      case "ok":
      case "success":
        return "success" as const;
      case "warn":
      case "warning":
        return "warning" as const;
      case "error":
      case "fail":
      case "critical":
        return "danger" as const;
      case "info":
      default:
        return "outline" as const;
    }
  })();
  return (
    <TableRow>
      <TableCell className="font-mono text-xs">{finding.name}</TableCell>
      <TableCell>
        <Badge variant={variant} className="text-xs">
          {finding.status}
        </Badge>
      </TableCell>
      <TableCell
        className={`text-right font-mono text-xs ${
          finding.points < 0
            ? "text-red-400"
            : finding.points > 0
              ? "text-emerald-400"
              : "text-muted-foreground"
        }`}
      >
        {finding.points > 0 ? "+" : ""}
        {finding.points}
      </TableCell>
      <TableCell className="text-sm text-muted-foreground">
        {finding.detail ?? "—"}
      </TableCell>
    </TableRow>
  );
}

interface StatProps {
  label: string;
  value: string | number;
  colored?: boolean;
}

function Stat({ label, value, colored }: StatProps) {
  const className = colored && typeof value === "number" ? scoreColor(value) : "";
  return (
    <div>
      <div className="text-xs uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <div className={`text-2xl font-semibold ${className}`}>{value}</div>
    </div>
  );
}

function scoreColor(score: number): string {
  if (score >= 75) return "text-emerald-400";
  if (score >= 50) return "text-amber-400";
  return "text-red-400";
}

interface HistorySparklineProps {
  history: { scored_at: string; score: number; confidence: number }[];
}

function HistorySparkline({ history }: HistorySparklineProps) {
  const data = history.map((h) => ({
    ts: h.scored_at.slice(0, 10),
    score: h.score,
    confidence: h.confidence,
  }));
  return (
    <div className="h-48 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 8, right: 16, bottom: 8, left: 0 }}>
          <XAxis
            dataKey="ts"
            stroke="hsl(var(--muted-foreground))"
            fontSize={10}
            tickLine={false}
            axisLine={false}
          />
          <YAxis
            domain={[0, 100]}
            stroke="hsl(var(--muted-foreground))"
            fontSize={10}
            tickLine={false}
            axisLine={false}
            width={28}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: "hsl(var(--card))",
              border: "1px solid hsl(var(--border))",
              borderRadius: "0.5rem",
              fontSize: "0.75rem",
            }}
          />
          <Line
            type="monotone"
            dataKey="score"
            stroke="#10b981"
            strokeWidth={2}
            dot={{ r: 2 }}
            activeDot={{ r: 4 }}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

function BackButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors"
    >
      <ArrowLeft className="h-4 w-4" />
      All domains
    </button>
  );
}

function DetailSkeleton() {
  return (
    <div className="animate-pulse space-y-6">
      <div className="h-6 w-32 rounded bg-muted/50" />
      <div className="h-32 rounded-lg bg-muted/50" />
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <div className="h-64 rounded-lg bg-muted/50" />
        <div className="h-64 rounded-lg bg-muted/50" />
      </div>
      <div className="h-96 rounded-lg bg-muted/50" />
    </div>
  );
}
