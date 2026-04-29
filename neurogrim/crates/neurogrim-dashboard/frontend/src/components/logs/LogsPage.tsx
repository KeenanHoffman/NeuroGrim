import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  Bell,
  CheckCircle2,
  CircleAlert,
  CircleSlash,
  Filter,
  Layers,
  PauseCircle,
  Sparkles,
  XCircle,
} from "lucide-react";
import type { PublishGatesPageResponse } from "@bindings/PublishGatesPageResponse";
import type { ApprovalsPageResponse } from "@bindings/ApprovalsPageResponse";
import type { InvocationLedgerResponse } from "@bindings/InvocationLedgerResponse";
import type { QueueReadResponse } from "@bindings/QueueReadResponse";
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
 * S15-C-3: built-in Logs page (v1: publish-gates + approvals).
 * S15-C-2 v2: extended with invocation-ledger + notifications.
 *
 * Filterable timeline aggregating events from multiple ledgers:
 *
 * - **Publish gates** — per-gate runs from `publish-gate-ledger.jsonl`
 * - **Approvals** — autonomy resolutions from `_neurogrim/approval-resolutions`
 * - **Invocations** — every `Skill` tool call recorded by the
 *   PostToolUse hook (via `record-skill-invocation.sh`)
 * - **Notifications** — adopter-facing events on
 *   `_neurogrim/notifications` (autonomy post-execution emits,
 *   future: federation discovery, etc.)
 *
 * Each source has its own filter chip; "All" defaults. Newest events
 * first; refreshes every 30 seconds via TanStack Query, plus
 * SSE-driven live invalidation when the filesystem watcher detects
 * changes to the underlying ledgers (`useDashboardEvents` hook
 * mounted in AppShell).
 *
 * **Deferred (v3 follow-ups):**
 *
 * - **score-history** — diff snapshots into per-domain "score
 *   changed by Δ" entries; needs threshold tuning to avoid noise
 *   (every snapshot would otherwise produce 17 entries).
 * - **services.jsonl** — service start/stop events; today's
 *   `ServiceRegistry` is in-memory only, so this needs a small
 *   persistence layer first.
 * - **Per-row drill-down** — click a row → see the full payload
 *   (publish-gate stdout/stderr, full notification body, etc.)
 *   in a side sheet.
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
  const { data: invocations } = useQuery({
    queryKey: ["logs-invocations", brainId],
    queryFn: () => fetchInvocationLedger(brainId),
    refetchInterval: 30_000,
  });
  const { data: notifications } = useQuery({
    queryKey: ["logs-notifications", brainId],
    queryFn: () => fetchNotifications(brainId),
    refetchInterval: 30_000,
  });

  const entries = useMemo(() => {
    return aggregate(gates, approvals, invocations, notifications).filter((e) =>
      filter === "all" ? true : e.source === filter,
    );
  }, [gates, approvals, invocations, notifications, filter]);

  return (
    <div className="space-y-6 p-6" data-testid="logs-page">
      <header>
        <h1 className="text-2xl font-bold">Logs</h1>
        <p className="text-sm text-muted-foreground mt-1">
          Filterable timeline across the brain's append-only ledgers.
          Newest events first; refreshes every 30 seconds (live updates
          arrive faster via SSE when ledger files change).
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
              count={entries.length}
              active={filter === "all"}
              onClick={() => setFilter("all")}
              testid="filter-all"
            />
            <FilterChip
              label="Publish gates"
              count={countSource(entries, "publish-gates")}
              active={filter === "publish-gates"}
              onClick={() => setFilter("publish-gates")}
              testid="filter-publish-gates"
            />
            <FilterChip
              label="Approvals"
              count={countSource(entries, "approvals")}
              active={filter === "approvals"}
              onClick={() => setFilter("approvals")}
              testid="filter-approvals"
            />
            <FilterChip
              label="Invocations"
              count={countSource(entries, "invocations")}
              active={filter === "invocations"}
              onClick={() => setFilter("invocations")}
              testid="filter-invocations"
            />
            <FilterChip
              label="Notifications"
              count={countSource(entries, "notifications")}
              active={filter === "notifications"}
              onClick={() => setFilter("notifications")}
              testid="filter-notifications"
            />
          </div>
        </CardContent>
      </Card>

      {entries.length === 0 ? (
        <EmptyState filter={filter} />
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

type LogSource =
  | "publish-gates"
  | "approvals"
  | "invocations"
  | "notifications";

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
  invocations: InvocationLedgerResponse | undefined,
  notifications: QueueReadResponse | undefined,
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
  if (invocations) {
    for (const e of invocations.entries) {
      // Composite id ensures uniqueness even if two invocations
      // landed in the same second (the ledger's `ts` is second-
      // granularity).
      const id = e.invocation_id ?? `${e.ts}-${e.name ?? "unknown"}`;
      out.push({
        id: `invocation-${id}`,
        source: "invocations",
        when: e.ts,
        subject: e.name ?? "(no name)",
        outcome: "invoked",
        actor: e.session_id ? truncate(e.session_id, 8) : null,
      });
    }
  }
  if (notifications) {
    for (const m of notifications.messages) {
      // Notification payloads are adopter-defined; pull a couple of
      // common fields for nicer rendering and fall back to "(see
      // payload)" for the subject. Future v3: clickable row → side
      // sheet with the full payload.
      const payload = (m.payload ?? {}) as Record<string, unknown>;
      const subject =
        (typeof payload.kind === "string" && payload.kind) ||
        (typeof payload.event === "string" && payload.event) ||
        (typeof payload.action_type === "string" && payload.action_type) ||
        (typeof payload.title === "string" && payload.title) ||
        "(see payload)";
      const severity =
        (typeof payload.severity === "string" && payload.severity) ||
        (typeof payload.level === "string" && payload.level) ||
        "info";
      out.push({
        id: `notification-${m.id}`,
        source: "notifications",
        when: m.produced_at,
        subject,
        outcome: severity,
        actor: null,
      });
    }
  }
  // Newest first.
  out.sort((a, b) => (a.when < b.when ? 1 : -1));
  return out;
}

function countSource(entries: LogEntry[], source: LogSource): number {
  return entries.reduce((n, e) => n + (e.source === source ? 1 : 0), 0);
}

function truncate(s: string, n: number): string {
  if (s.length <= n) return s;
  return s.slice(0, n) + "…";
}

function FilterChip({
  label,
  count,
  active,
  onClick,
  testid,
}: {
  label: string;
  count?: number;
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
      {count !== undefined && (
        <span className="ml-1.5 text-xs opacity-70">({count})</span>
      )}
    </Button>
  );
}

function EmptyState({ filter }: { filter: LogSource | "all" }) {
  const filterLabel = filter === "all" ? "across any source" : `for ${filter}`;
  return (
    <Card data-testid="logs-empty">
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <CircleSlash className="h-5 w-5 text-muted-foreground" />
          No events yet
        </CardTitle>
        <CardDescription>
          The brain's ledgers are empty {filterLabel} (or no events match
          the current filter).
        </CardDescription>
      </CardHeader>
      <CardContent className="text-sm text-muted-foreground">
        Once publish-gate runs land, operator approvals fire, the Skill
        invocation hook records calls, or notifications publish to{" "}
        <code className="text-xs">_neurogrim/notifications</code>, the
        timeline will populate.
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
        <SourceBadge source={entry.source} />
      </TableCell>
      <TableCell className="text-xs font-medium">{entry.subject}</TableCell>
      <TableCell>
        <OutcomeBadge outcome={entry.outcome} />
      </TableCell>
      <TableCell className="text-xs text-muted-foreground font-mono">
        {entry.actor ?? "—"}
      </TableCell>
    </TableRow>
  );
}

function SourceBadge({ source }: { source: LogSource }) {
  const Icon = sourceIcon(source);
  const label = source.replace("-", " ");
  return (
    <Badge variant="outline" className="capitalize text-xs gap-1">
      <Icon className="h-3 w-3" />
      {label}
    </Badge>
  );
}

function sourceIcon(source: LogSource): typeof CheckCircle2 {
  switch (source) {
    case "invocations":
      return Sparkles;
    case "notifications":
      return Bell;
    default:
      return Layers;
  }
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
    case "warn":
    case "warning":
      return { Icon: AlertTriangle, variant: "destructive", label: outcome };
    case "invoked":
      return { Icon: Sparkles, variant: "secondary", label: "invoked" };
    case "info":
      return { Icon: Bell, variant: "outline", label: "info" };
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

async function fetchInvocationLedger(
  brainId: string,
): Promise<InvocationLedgerResponse> {
  const url = `${brainApi(brainId, "logs")}/invocation-ledger?limit=50`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as InvocationLedgerResponse;
}

async function fetchNotifications(
  brainId: string,
): Promise<QueueReadResponse> {
  // The bus endpoint reads from `since` going forward; we ask for
  // a generous limit (200) so the most recent activity is in the
  // window even on chatty topics. The Logs aggregation slices to
  // newest-first naturally via the timestamp sort.
  const url = `${brainApi(brainId, "queues")}/_neurogrim/notifications?since=0&limit=200`;
  const res = await fetch(url);
  if (!res.ok) {
    // Topic file may not exist yet (no notifications published in
    // this brain). Return an empty page instead of erroring out so
    // the page still renders the other sources.
    return { topic: "_neurogrim/notifications", messages: [], next_offset: 0n };
  }
  return (await res.json()) as QueueReadResponse;
}
