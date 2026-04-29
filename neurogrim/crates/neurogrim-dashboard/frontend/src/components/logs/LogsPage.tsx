import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  CircleAlert,
  CircleSlash,
  Filter,
  Layers,
  PauseCircle,
  XCircle,
} from "lucide-react";
import type { PublishGatesPageResponse } from "@bindings/PublishGatesPageResponse";
import type { ApprovalsPageResponse } from "@bindings/ApprovalsPageResponse";
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
import { Button } from "@/components/ui/button-ish";
import { Badge } from "@/components/ui/badge";
import { brainApi, useBrainId } from "@/lib/useBrain";

/**
 * S15-C-3: built-in Logs page.
 *
 * Filterable timeline aggregating events from multiple ledgers
 * already exposed via existing API endpoints:
 *
 * - **Publish gates** (`GET /api/brains/:id/publish-gates`) —
 *   per-gate runs from `publish-gate-ledger.jsonl`.
 * - **Approvals** (`GET /api/brains/:id/approvals`) — autonomy
 *   resolutions from `_neurogrim/approval-resolutions`.
 *
 * **v1 scope:** the S15 epic also lists invocation-ledger,
 * score-history, services.jsonl, and `_neurogrim/notifications`
 * as sources. Those land when the corresponding API endpoints
 * exist (some require new endpoints we haven't built yet —
 * `score-history` is consumed via the Overview page's
 * trajectory widget; `invocation-ledger` is exposed via Skills).
 * v1 starts with the two newest sources adopters care about
 * (publish gates + approvals); the page is structured to absorb
 * additional sources without reshuffling.
 *
 * **Filter chips** let the operator narrow by source. Toast
 * notifications for new SSE events are deferred — a follow-up
 * story.
 */
export function LogsPage() {
  const brainId = useBrainId();
  const [filter, setFilter] = useState<LogSource | "all">("all");

  const { data: gates } = useQuery({
    queryKey: ["logs-publish-gates", brainId],
    queryFn: () => fetchPublishGates(brainId),
    refetchInterval: 30_000,
  });
  const { data: approvals } = useQuery({
    queryKey: ["logs-approvals", brainId],
    queryFn: () => fetchApprovals(brainId),
    refetchInterval: 30_000,
  });

  const entries = useMemo(() => {
    return aggregate(gates, approvals).filter((e) =>
      filter === "all" ? true : e.source === filter,
    );
  }, [gates, approvals, filter]);

  return (
    <div className="space-y-6 p-6" data-testid="logs-page">
      <header>
        <h1 className="text-2xl font-bold">Logs</h1>
        <p className="text-sm text-muted-foreground mt-1">
          Filterable timeline across the brain's append-only ledgers.
          Newest events first; refreshes every 30 seconds.
        </p>
      </header>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <Filter className="h-5 w-5" />
            Filter ({entries.length} {entries.length === 1 ? "event" : "events"})
          </CardTitle>
          <CardDescription>
            Click a chip to narrow the timeline.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap gap-2" data-testid="logs-filter-chips">
            <FilterChip
              label="All"
              active={filter === "all"}
              onClick={() => setFilter("all")}
              testid="filter-all"
            />
            <FilterChip
              label="Publish gates"
              active={filter === "publish-gates"}
              onClick={() => setFilter("publish-gates")}
              testid="filter-publish-gates"
            />
            <FilterChip
              label="Approvals"
              active={filter === "approvals"}
              onClick={() => setFilter("approvals")}
              testid="filter-approvals"
            />
          </div>
        </CardContent>
      </Card>

      {entries.length === 0 ? (
        <EmptyState />
      ) : (
        <Card>
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <Layers className="h-5 w-5" />
              Timeline
            </CardTitle>
          </CardHeader>
          <CardContent>
            <Table data-testid="logs-timeline">
              <TableHeader>
                <TableRow>
                  <TableHead>When</TableHead>
                  <TableHead>Source</TableHead>
                  <TableHead>Subject</TableHead>
                  <TableHead>Outcome</TableHead>
                  <TableHead>Actor</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {entries.map((e) => (
                  <LogRow key={e.id} entry={e} />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

type LogSource = "publish-gates" | "approvals";

interface LogEntry {
  id: string;
  source: LogSource;
  /** RFC3339 */
  when: string;
  subject: string;
  outcome: string;
  actor: string | null;
}

function aggregate(
  gates: PublishGatesPageResponse | undefined,
  approvals: ApprovalsPageResponse | undefined,
): LogEntry[] {
  const out: LogEntry[] = [];
  if (gates) {
    for (const e of gates.recent_ledger) {
      out.push({
        id: `gate-${e.run_id}-${e.gate_id}-${e.started_at}`,
        source: "publish-gates",
        when: e.started_at,
        subject: e.gate_id,
        outcome: e.status,
        actor: e.operator,
      });
    }
  }
  if (approvals) {
    for (const r of approvals.recent_resolutions) {
      out.push({
        id: `approval-${r.action_id}-${r.decided_at}`,
        source: "approvals",
        when: r.decided_at,
        subject: r.action_id,
        outcome: r.decision,
        actor: r.operator,
      });
    }
    for (const p of approvals.pending) {
      out.push({
        id: `approval-pending-${p.action_id}`,
        source: "approvals",
        when: p.requested_at,
        subject: p.action_id,
        outcome: "pending",
        actor: null,
      });
    }
  }
  // Newest first.
  out.sort((a, b) => (a.when < b.when ? 1 : -1));
  return out;
}

function FilterChip({
  label,
  active,
  onClick,
  testid,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  testid: string;
}) {
  return (
    <Button
      size="sm"
      variant={active ? "default" : "outline"}
      onClick={onClick}
      data-testid={testid}
    >
      {label}
    </Button>
  );
}

function EmptyState() {
  return (
    <Card data-testid="logs-empty">
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <CircleSlash className="h-5 w-5 text-muted-foreground" />
          No events yet
        </CardTitle>
        <CardDescription>
          The brain's ledgers are empty (or no events match the
          current filter).
        </CardDescription>
      </CardHeader>
      <CardContent className="text-sm text-muted-foreground">
        Once publish-gate runs land or operator approvals fire, the
        timeline will populate. v1 ships publish-gates + approvals
        sources; future stories add invocation-ledger, score-history,
        services.jsonl, and _neurogrim/notifications.
      </CardContent>
    </Card>
  );
}

function LogRow({ entry }: { entry: LogEntry }) {
  return (
    <TableRow data-testid={`log-row-${entry.id}`}>
      <TableCell className="text-xs whitespace-nowrap">
        {formatTime(entry.when)}
      </TableCell>
      <TableCell>
        <Badge variant="outline" className="capitalize text-xs">
          {entry.source.replace("-", " ")}
        </Badge>
      </TableCell>
      <TableCell className="text-xs font-medium">{entry.subject}</TableCell>
      <TableCell>
        <OutcomeBadge outcome={entry.outcome} />
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {entry.actor ?? "—"}
      </TableCell>
    </TableRow>
  );
}

function OutcomeBadge({ outcome }: { outcome: string }) {
  const { Icon, variant, label } = outcomeStyle(outcome);
  return (
    <Badge variant={variant} className="capitalize gap-1" data-testid={`outcome-${outcome}`}>
      <Icon className="h-3 w-3" />
      {label}
    </Badge>
  );
}

function outcomeStyle(outcome: string): {
  Icon: typeof CheckCircle2;
  variant: "default" | "destructive" | "outline" | "secondary";
  label: string;
} {
  switch (outcome) {
    case "passed":
    case "approve":
      return { Icon: CheckCircle2, variant: "default", label: outcome };
    case "failed":
    case "deny":
      return { Icon: XCircle, variant: "destructive", label: outcome };
    case "pending":
      return { Icon: PauseCircle, variant: "secondary", label: "pending" };
    case "deferred":
      return { Icon: CircleSlash, variant: "outline", label: "deferred" };
    case "error":
    case "timed_out":
      return { Icon: AlertTriangle, variant: "destructive", label: outcome };
    default:
      return { Icon: CircleAlert, variant: "outline", label: outcome };
  }
}

function formatTime(iso: string): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

async function fetchPublishGates(brainId: string): Promise<PublishGatesPageResponse> {
  const url = brainApi(brainId, "publish-gates");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as PublishGatesPageResponse;
}

async function fetchApprovals(brainId: string): Promise<ApprovalsPageResponse> {
  const url = brainApi(brainId, "approvals");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as ApprovalsPageResponse;
}
