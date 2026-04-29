import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowUpDown, ChevronDown, ChevronUp } from "lucide-react";
import type { DomainListItemDto } from "@bindings/DomainListItemDto";
import type { DomainsListResponse } from "@bindings/DomainsListResponse";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
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

type SortKey =
  | "name"
  | "weight"
  | "raw_score"
  | "effective_score"
  | "confidence"
  | "trajectory_class"
  | "last_updated";
type SortDir = "asc" | "desc";

async function fetchDomains(hat: string | null): Promise<DomainsListResponse> {
  const url = hat
    ? `/api/domains?hat=${encodeURIComponent(hat)}`
    : "/api/domains";
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as DomainsListResponse;
}

export function DomainsPage() {
  const navigate = useNavigate();
  const { hat } = useHat();
  const queryHat = hatToQuery(hat);
  const { data, isLoading, error } = useQuery({
    queryKey: ["domains", queryHat],
    queryFn: () => fetchDomains(queryHat),
  });

  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");

  const sorted = useMemo(() => {
    if (!data) return [];
    const rows = [...data.domains];
    rows.sort((a, b) => compareRows(a, b, sortKey));
    return sortDir === "asc" ? rows : rows.reverse();
  }, [data, sortKey, sortDir]);

  if (isLoading) return <DomainsSkeleton />;
  if (error || !data) {
    return (
      <Card className="border-destructive">
        <CardHeader>
          <CardTitle className="text-destructive">
            Failed to load domains
          </CardTitle>
        </CardHeader>
        <CardContent>
          <pre className="text-xs">{(error as Error)?.message ?? "Unknown error"}</pre>
        </CardContent>
      </Card>
    );
  }

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir(sortDir === "asc" ? "desc" : "asc");
    } else {
      setSortKey(key);
      setSortDir("asc");
    }
  };

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-2xl">Domains</CardTitle>
          <p className="text-sm text-muted-foreground">
            {data.domains.length} declared domain{data.domains.length === 1 ? "" : "s"}
            . Click a row to drill into findings + history.
          </p>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <SortHeader
                  current={sortKey}
                  dir={sortDir}
                  field="name"
                  onClick={handleSort}
                >
                  Domain
                </SortHeader>
                <SortHeader
                  current={sortKey}
                  dir={sortDir}
                  field="weight"
                  onClick={handleSort}
                  className="w-20"
                >
                  Weight
                </SortHeader>
                <SortHeader
                  current={sortKey}
                  dir={sortDir}
                  field="effective_score"
                  onClick={handleSort}
                  className="w-24 text-right"
                >
                  Effective
                </SortHeader>
                <SortHeader
                  current={sortKey}
                  dir={sortDir}
                  field="raw_score"
                  onClick={handleSort}
                  className="w-20 text-right"
                >
                  Raw
                </SortHeader>
                <SortHeader
                  current={sortKey}
                  dir={sortDir}
                  field="confidence"
                  onClick={handleSort}
                  className="w-24 text-right"
                >
                  Confidence
                </SortHeader>
                <SortHeader
                  current={sortKey}
                  dir={sortDir}
                  field="trajectory_class"
                  onClick={handleSort}
                  className="w-32"
                >
                  Trajectory
                </SortHeader>
                <SortHeader
                  current={sortKey}
                  dir={sortDir}
                  field="last_updated"
                  onClick={handleSort}
                  className="w-44"
                >
                  Last updated
                </SortHeader>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sorted.map((row) => (
                <TableRow
                  key={row.name}
                  className="cursor-pointer"
                  onClick={() =>
                    navigate({
                      to: "/domains/$name",
                      params: { name: row.name },
                    })
                  }
                  data-testid={`row-${row.name}`}
                >
                  <TableCell className="font-medium">{row.display_name}</TableCell>
                  <TableCell>
                    {row.weight === 0 ? (
                      <Badge variant="outline">advisory</Badge>
                    ) : (
                      <span className="font-mono text-xs">{row.weight.toFixed(2)}</span>
                    )}
                  </TableCell>
                  <TableCell className={`text-right font-mono ${scoreColor(row.effective_score)}`}>
                    {row.effective_score}
                  </TableCell>
                  <TableCell className="text-right font-mono text-muted-foreground">
                    {row.raw_score}
                  </TableCell>
                  <TableCell className="text-right font-mono text-muted-foreground">
                    {row.confidence}%
                  </TableCell>
                  <TableCell>
                    <TrajectoryDot trajectoryClass={row.trajectory_class} />
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {formatRelative(row.last_updated)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}

function compareRows(
  a: DomainListItemDto,
  b: DomainListItemDto,
  key: SortKey
): number {
  switch (key) {
    case "name":
      return a.display_name.localeCompare(b.display_name);
    case "weight":
      return a.weight - b.weight;
    case "raw_score":
      return a.raw_score - b.raw_score;
    case "effective_score":
      return a.effective_score - b.effective_score;
    case "confidence":
      return a.confidence - b.confidence;
    case "trajectory_class":
      return a.trajectory_class.localeCompare(b.trajectory_class);
    case "last_updated":
      return (a.last_updated ?? "").localeCompare(b.last_updated ?? "");
  }
}

interface SortHeaderProps {
  field: SortKey;
  current: SortKey;
  dir: SortDir;
  onClick: (k: SortKey) => void;
  className?: string;
  children: React.ReactNode;
}

function SortHeader({ field, current, dir, onClick, className, children }: SortHeaderProps) {
  const Icon =
    current === field ? (dir === "asc" ? ChevronUp : ChevronDown) : ArrowUpDown;
  return (
    <TableHead
      className={`cursor-pointer select-none hover:text-foreground ${className ?? ""}`}
      onClick={() => onClick(field)}
    >
      <span className="inline-flex items-center gap-1">
        {children}
        <Icon className="h-3 w-3 opacity-60" />
      </span>
    </TableHead>
  );
}

function TrajectoryDot({ trajectoryClass }: { trajectoryClass: string }) {
  const { variant, label } = (() => {
    switch (trajectoryClass) {
      case "improving":
        return { variant: "success" as const, label: "improving" };
      case "degrading":
        return { variant: "danger" as const, label: "degrading" };
      case "volatile":
        return { variant: "warning" as const, label: "volatile" };
      case "stable":
        return { variant: "secondary" as const, label: "stable" };
      default:
        return { variant: "outline" as const, label: "no data" };
    }
  })();
  return (
    <Badge variant={variant} className="text-xs">
      {label}
    </Badge>
  );
}

function scoreColor(score: number): string {
  if (score >= 75) return "text-emerald-400";
  if (score >= 50) return "text-amber-400";
  return "text-red-400";
}

function formatRelative(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const diffMs = Date.now() - d.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
  if (diffDays === 0) {
    const hours = Math.floor(diffMs / (1000 * 60 * 60));
    if (hours === 0) return "just now";
    return `${hours}h ago`;
  }
  if (diffDays === 1) return "1 day ago";
  if (diffDays < 7) return `${diffDays} days ago`;
  return d.toISOString().slice(0, 10);
}

function DomainsSkeleton() {
  return (
    <div className="animate-pulse space-y-4">
      <div className="h-12 w-48 rounded bg-muted/50" />
      <div className="h-96 rounded-lg bg-muted/50" />
    </div>
  );
}
