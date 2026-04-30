import { useEffect, useMemo, useState } from "react";
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
  Power,
  PowerOff,
  Sparkles,
  TrendingDown,
  TrendingUp,
  Minus,
  X,
  XCircle,
} from "lucide-react";
import type { PublishGatesPageResponse } from "@bindings/PublishGatesPageResponse";
import type { ApprovalsPageResponse } from "@bindings/ApprovalsPageResponse";
import type { InvocationLedgerResponse } from "@bindings/InvocationLedgerResponse";
import type { ScoreHistoryResponse } from "@bindings/ScoreHistoryResponse";
import type { ServicesLogResponse } from "@bindings/ServicesLogResponse";
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
 * S15-C-3 expansion: score-history added as the fifth source.
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
 * - **Score history** — unified-score snapshots from
 *   `score-history.json`, each annotated with the delta against
 *   the chronologically-prior snapshot (improving / declining /
 *   stable / first). Per-domain detail lives in the Domains pages.
 * - **Services** — service-lifecycle events from `services.jsonl`
 *   (started / failed / stopped). The on-disk ledger is appended
 *   by the start / stop / readiness-watcher handlers; durable
 *   across dashboard restarts.
 *
 * Each source has its own filter chip; "All" defaults. Newest events
 * first; refreshes every 30 seconds via TanStack Query, plus
 * SSE-driven live invalidation when the filesystem watcher detects
 * changes to the underlying ledgers (`useDashboardEvents` hook
 * mounted in AppShell).
 *
 * **Per-row drill-down** — click any row to open a modal with the
 * full original record. Each source has a curated detail view
 * (publish-gate exit codes + error_detail, approvals decision
 * metadata, invocation session/invocation ids, notification full
 * payload, score-history per-snapshot detail, services lifecycle
 * reason). The raw payload also renders as pretty-printed JSON
 * below the curated fields so operators can copy any field for
 * external investigation.
 *
 * **Deferred:**
 *
 * - **Toast notifications** — surface new events while the
 *   operator is on a different page.
 */
export function LogsPage() {
  const brainId = useBrainId();
  const [filter, setFilter] = useState<LogSource | "all">("all");
  // S15-C-3 polish: per-row drill-down. Tracks the currently-open
  // detail modal entry; null means no modal.
  const [selected, setSelected] = useState<LogEntry | null>(null);

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
  const { data: scoreHistory } = useQuery({
    queryKey: ["logs-score-history", brainId],
    queryFn: () => fetchScoreHistory(brainId),
    refetchInterval: 30_000,
  });
  const { data: servicesLog } = useQuery({
    queryKey: ["logs-services", brainId],
    queryFn: () => fetchServicesLog(brainId),
    refetchInterval: 30_000,
  });

  const entries = useMemo(() => {
    return aggregate(
      gates,
      approvals,
      invocations,
      notifications,
      scoreHistory,
      servicesLog,
    ).filter((e) => (filter === "all" ? true : e.source === filter));
  }, [
    gates,
    approvals,
    invocations,
    notifications,
    scoreHistory,
    servicesLog,
    filter,
  ]);

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
            <FilterChip
              label="Score history"
              count={countSource(entries, "score-history")}
              active={filter === "score-history"}
              onClick={() => setFilter("score-history")}
              testid="filter-score-history"
            />
            <FilterChip
              label="Services"
              count={countSource(entries, "services")}
              active={filter === "services"}
              onClick={() => setFilter("services")}
              testid="filter-services"
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
                  <LogRow
                    key={e.id}
                    entry={e}
                    onSelect={() => setSelected(e)}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
      {selected && (
        <LogDetailModal entry={selected} onClose={() => setSelected(null)} />
      )}
    </div>
  );
}

type LogSource =
  | "publish-gates"
  | "approvals"
  | "invocations"
  | "notifications"
  | "score-history"
  | "services";

interface LogEntry {
  id: string;
  source: LogSource;
  /** RFC3339 */
  when: string;
  subject: string;
  outcome: string;
  actor: string | null;
  /**
   * Original record from the source. Carried so the drill-down
   * modal can render full per-source detail without a second
   * fetch. Typed loosely (`unknown`) because each source has a
   * different shape; the detail renderer narrows by `source`.
   */
  payload: unknown;
}

function aggregate(
  gates: PublishGatesPageResponse | undefined,
  approvals: ApprovalsPageResponse | undefined,
  invocations: InvocationLedgerResponse | undefined,
  notifications: QueueReadResponse | undefined,
  scoreHistory: ScoreHistoryResponse | undefined,
  servicesLog: ServicesLogResponse | undefined,
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
        payload: e,
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
        payload: r,
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
        payload: p,
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
        payload: e,
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
        payload: m,
      });
    }
  }
  if (servicesLog) {
    for (const e of servicesLog.entries) {
      // Subject names the peer; outcome encodes the lifecycle
      // direction (started / failed / stopped) so the badge styling
      // matches the operator's mental model.
      const portSuffix =
        e.kind === "started" && e.port !== null && e.port !== undefined
          ? ` (port ${e.port})`
          : "";
      const subject = `${e.peer_name}${portSuffix}`;
      out.push({
        id: `service-${e.ts}-${e.peer_name}-${e.kind}`,
        source: "services",
        when: e.ts,
        subject,
        outcome: e.kind, // "started" | "failed" | "stopped"
        actor: e.pid !== null && e.pid !== undefined ? `pid ${e.pid}` : null,
        payload: e,
      });
    }
  }
  if (scoreHistory) {
    for (const e of scoreHistory.entries) {
      // Subject shows the unified score; outcome encodes the
      // direction so the row's badge mirrors what the operator sees
      // on the trajectory badge elsewhere.
      const scoreLabel = e.score === null ? "N/A" : `score ${e.score}`;
      const deltaLabel =
        e.delta === null
          ? null
          : e.delta > 0
          ? `+${e.delta}`
          : e.delta < 0
          ? `${e.delta}`
          : "±0";
      const subject = deltaLabel
        ? `${scoreLabel} (${deltaLabel})`
        : scoreLabel;
      const outcome =
        e.delta === null
          ? "first"
          : e.delta > 0
          ? "improved"
          : e.delta < 0
          ? "declined"
          : "stable";
      out.push({
        id: `score-${e.scored_at}`,
        source: "score-history",
        when: e.scored_at,
        subject,
        outcome,
        actor: null,
        payload: e,
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

function LogRow({
  entry,
  onSelect,
}: {
  entry: LogEntry;
  onSelect: () => void;
}) {
  return (
    <TableRow
      data-testid={`log-row-${entry.id}`}
      onClick={onSelect}
      onKeyDown={(e) => {
        // Keyboard activation parity for screen readers + power users.
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
      role="button"
      tabIndex={0}
      className="cursor-pointer hover:bg-muted/40 focus-visible:bg-muted/40 focus-visible:outline-none"
    >
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
    case "score-history":
      return TrendingUp;
    case "services":
      return Power;
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
    // S15-C-3 expansion: score-history outcomes.
    case "improved":
      return { Icon: TrendingUp, variant: "default", label: "improved" };
    case "declined":
      return { Icon: TrendingDown, variant: "destructive", label: "declined" };
    case "stable":
      return { Icon: Minus, variant: "secondary", label: "stable" };
    case "first":
      return { Icon: Sparkles, variant: "outline", label: "first" };
    // S15-C-3 expansion follow-on: services lifecycle outcomes.
    case "started":
      return { Icon: Power, variant: "default", label: "started" };
    case "stopped":
      return { Icon: PowerOff, variant: "secondary", label: "stopped" };
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

async function fetchScoreHistory(
  brainId: string,
): Promise<ScoreHistoryResponse> {
  const url = `${brainApi(brainId, "logs")}/score-history?limit=50`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as ScoreHistoryResponse;
}

async function fetchServicesLog(
  brainId: string,
): Promise<ServicesLogResponse> {
  const url = `${brainApi(brainId, "logs")}/services?limit=50`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as ServicesLogResponse;
}

/**
 * Per-row drill-down modal. Renders source-specific curated detail
 * fields above a pretty-printed JSON dump of the original payload.
 *
 * Closes via:
 * - ESC key
 * - Backdrop click
 * - Close (×) button
 *
 * Centered modal pattern (matching SecretsPage's SetSecretModal)
 * rather than a slide-in side sheet — operators view one entry at a
 * time; the modal blocks the timeline so it's clear the focus has
 * shifted. Side-sheet drift can be a v3 enhancement if comparison
 * across entries becomes a workflow.
 */
function LogDetailModal({
  entry,
  onClose,
}: {
  entry: LogEntry;
  onClose: () => void;
}) {
  // ESC closes. Pre-focusing the close button isn't strictly needed
  // (the backdrop captures focus via React's portal-less render).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      data-testid="log-detail-backdrop"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="bg-background border rounded-lg shadow-lg max-w-2xl w-full m-4 flex flex-col overflow-hidden max-h-[85vh]"
        onClick={(e) => e.stopPropagation()}
        data-testid={`log-detail-${entry.id}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="log-detail-title"
      >
        <header className="flex items-center justify-between p-4 border-b">
          <div className="min-w-0">
            <h2
              id="log-detail-title"
              className="text-lg font-bold flex items-center gap-2"
            >
              <SourceBadge source={entry.source} />
              <span className="truncate">{entry.subject}</span>
            </h2>
            <p className="text-xs text-muted-foreground mt-1 font-mono">
              {entry.when}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1 hover:bg-muted rounded shrink-0 ml-2"
            aria-label="Close"
            data-testid="log-detail-close"
          >
            <X className="h-4 w-4" />
          </button>
        </header>
        <div className="p-4 space-y-4 overflow-y-auto flex-1">
          <LogDetailBody entry={entry} />
          <PayloadJson payload={entry.payload} />
        </div>
      </div>
    </div>
  );
}

/** Source-specific curated detail block. Falls back to a minimal
 *  outcome-only block for unknown shapes. */
function LogDetailBody({ entry }: { entry: LogEntry }) {
  switch (entry.source) {
    case "publish-gates":
      return <PublishGatesDetail payload={entry.payload} />;
    case "approvals":
      return <ApprovalsDetail payload={entry.payload} outcome={entry.outcome} />;
    case "invocations":
      return <InvocationsDetail payload={entry.payload} />;
    case "notifications":
      return <NotificationsDetail payload={entry.payload} />;
    case "score-history":
      return <ScoreHistoryDetail payload={entry.payload} />;
    case "services":
      return <ServicesDetail payload={entry.payload} />;
    default:
      return null;
  }
}

function DetailField({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <div
      className="grid grid-cols-[8rem_1fr] gap-2 text-sm items-baseline"
      data-testid={`detail-field-${label.toLowerCase().replace(/\s+/g, "-")}`}
    >
      <div className="text-xs text-muted-foreground uppercase tracking-wider">
        {label}
      </div>
      <div className={mono ? "font-mono break-all" : "break-words"}>
        {value ?? <span className="text-muted-foreground">—</span>}
      </div>
    </div>
  );
}

function PublishGatesDetail({ payload }: { payload: unknown }) {
  const p = (payload ?? {}) as Record<string, unknown>;
  return (
    <div className="space-y-1">
      <DetailField label="Gate" value={String(p.gate_id ?? "—")} mono />
      <DetailField label="Run id" value={String(p.run_id ?? "—")} mono />
      <DetailField label="Type" value={String(p.gate_type ?? "—")} />
      <DetailField label="Mode" value={String(p.mode ?? "—")} />
      <DetailField
        label="Started"
        value={String(p.started_at ?? "—")}
        mono
      />
      <DetailField
        label="Completed"
        value={p.completed_at ? String(p.completed_at) : "still running"}
        mono
      />
      <DetailField label="Status" value={<OutcomeBadge outcome={String(p.status ?? "—")} />} />
      <DetailField
        label="Blocking"
        value={p.blocking ? "yes" : "no"}
      />
      <DetailField
        label="Exit code"
        value={p.exit_code === null || p.exit_code === undefined ? "—" : String(p.exit_code)}
        mono
      />
      <DetailField label="Operator" value={p.operator ? String(p.operator) : null} />
      {typeof p.error_detail === "string" && p.error_detail.length > 0 && (
        <div className="pt-2">
          <div className="text-xs text-muted-foreground uppercase tracking-wider mb-1">
            Error detail
          </div>
          <pre className="text-xs bg-muted/50 p-2 rounded whitespace-pre-wrap break-words">
            {p.error_detail}
          </pre>
        </div>
      )}
    </div>
  );
}

function ApprovalsDetail({
  payload,
  outcome,
}: {
  payload: unknown;
  outcome: string;
}) {
  const p = (payload ?? {}) as Record<string, unknown>;
  return (
    <div className="space-y-1">
      <DetailField
        label="Action id"
        value={String(p.action_id ?? "—")}
        mono
      />
      <DetailField label="Decision" value={<OutcomeBadge outcome={outcome} />} />
      {Boolean(p.tool) && (
        <DetailField label="Tool" value={String(p.tool)} mono />
      )}
      {Boolean(p.action_type) && (
        <DetailField label="Action type" value={String(p.action_type)} />
      )}
      {Boolean(p.requested_at) && (
        <DetailField label="Requested" value={String(p.requested_at)} mono />
      )}
      {Boolean(p.decided_at) && (
        <DetailField label="Decided" value={String(p.decided_at)} mono />
      )}
      <DetailField label="Operator" value={p.operator ? String(p.operator) : null} />
    </div>
  );
}

function InvocationsDetail({ payload }: { payload: unknown }) {
  const p = (payload ?? {}) as Record<string, unknown>;
  return (
    <div className="space-y-1">
      <DetailField
        label="Skill"
        value={p.name ? String(p.name) : "(no name)"}
        mono
      />
      <DetailField label="Type" value={String(p.entry_type ?? "skill")} />
      <DetailField
        label="Timestamp"
        value={String(p.ts ?? "—")}
        mono
      />
      <DetailField
        label="Session id"
        value={p.session_id ? String(p.session_id) : null}
        mono
      />
      <DetailField
        label="Invocation id"
        value={p.invocation_id ? String(p.invocation_id) : null}
        mono
      />
    </div>
  );
}

function NotificationsDetail({ payload }: { payload: unknown }) {
  const p = (payload ?? {}) as Record<string, unknown>;
  const msgPayload = (p.payload ?? {}) as Record<string, unknown>;
  return (
    <div className="space-y-1">
      <DetailField label="Message id" value={String(p.id ?? "—")} mono />
      <DetailField
        label="Topic"
        value={String(p.topic ?? "_neurogrim/notifications")}
        mono
      />
      <DetailField label="Priority" value={String(p.priority ?? "normal")} />
      <DetailField
        label="Produced at"
        value={String(p.produced_at ?? "—")}
        mono
      />
      {typeof msgPayload.kind === "string" && (
        <DetailField label="Kind" value={msgPayload.kind} />
      )}
      {typeof msgPayload.severity === "string" && (
        <DetailField
          label="Severity"
          value={<OutcomeBadge outcome={msgPayload.severity} />}
        />
      )}
    </div>
  );
}

function ScoreHistoryDetail({ payload }: { payload: unknown }) {
  const p = (payload ?? {}) as Record<string, unknown>;
  const scoreLabel =
    p.score === null || p.score === undefined
      ? "N/A (advisory-only)"
      : String(p.score);
  let deltaLabel: React.ReactNode = "first snapshot";
  if (p.delta !== null && p.delta !== undefined) {
    const n = Number(p.delta);
    if (n > 0) deltaLabel = `+${n}`;
    else if (n < 0) deltaLabel = String(n);
    else deltaLabel = "±0";
  }
  return (
    <div className="space-y-1">
      <DetailField label="Scored at" value={String(p.scored_at ?? "—")} mono />
      <DetailField label="Unified score" value={scoreLabel} mono />
      <DetailField label="Delta" value={deltaLabel} mono />
      <p className="text-xs text-muted-foreground pt-2">
        Per-domain scores for this snapshot live on the Domains page;
        the Logs reader projects only the unified score to keep the
        timeline terse.
      </p>
    </div>
  );
}

function ServicesDetail({ payload }: { payload: unknown }) {
  const p = (payload ?? {}) as Record<string, unknown>;
  return (
    <div className="space-y-1">
      <DetailField label="Peer" value={String(p.peer_name ?? "—")} mono />
      <DetailField
        label="Outcome"
        value={<OutcomeBadge outcome={String(p.kind ?? "—")} />}
      />
      <DetailField label="Timestamp" value={String(p.ts ?? "—")} mono />
      <DetailField
        label="PID"
        value={
          p.pid === null || p.pid === undefined ? null : String(p.pid)
        }
        mono
      />
      <DetailField
        label="Port"
        value={
          p.port === null || p.port === undefined ? null : String(p.port)
        }
        mono
      />
      {typeof p.reason === "string" && p.reason.length > 0 && (
        <div className="pt-2">
          <div className="text-xs text-muted-foreground uppercase tracking-wider mb-1">
            Reason
          </div>
          <pre className="text-xs bg-muted/50 p-2 rounded whitespace-pre-wrap break-words">
            {p.reason}
          </pre>
        </div>
      )}
    </div>
  );
}

/** Pretty-prints the original payload as JSON below the curated
 *  detail block. Lets operators copy any field for external
 *  investigation, and surfaces fields the curated view doesn't
 *  highlight (especially for adopter-defined notification payloads). */
function PayloadJson({ payload }: { payload: unknown }) {
  const text = useMemo(() => {
    try {
      return JSON.stringify(payload, null, 2);
    } catch {
      return String(payload);
    }
  }, [payload]);
  return (
    <details className="border rounded" data-testid="log-detail-raw-payload">
      <summary className="cursor-pointer px-3 py-2 text-xs font-medium text-muted-foreground hover:bg-muted/40 select-none">
        Raw payload
      </summary>
      <pre className="text-xs bg-muted/30 p-3 overflow-x-auto whitespace-pre-wrap break-words max-h-[40vh] overflow-y-auto">
        {text}
      </pre>
    </details>
  );
}
