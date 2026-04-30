import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Activity,
  AlertTriangle,
  CircleSlash,
  FileText,
  RefreshCw,
  Server,
  X,
} from "lucide-react";
import type { ServicesListResponse } from "@bindings/ServicesListResponse";
import type { ServiceSnapshot } from "@bindings/ServiceSnapshot";
import type { PeerLogResponse } from "@bindings/PeerLogResponse";
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
 * S15-C-2: built-in Services page.
 *
 * Read-only view of every A2A peer this dashboard is currently
 * tracking (spawned via `--allow-mutations` from the Federation
 * page). Surfaces peer_name, pid, port, started_at, log_path.
 *
 * **v1:** read-only fleet view (peer_name, pid, port, uptime, log
 * path).
 *
 * **v2 (this expansion):** in-dashboard log tail viewer. Click the
 * "View log" button on any peer row to open a modal showing the
 * trailing lines of the peer's spawned-process log. The modal has
 * a manual refresh button; live SSE-streamed updates are a v3
 * follow-on. Operators no longer drop to a terminal for
 * `tail -f <peer>.log`.
 *
 * **Still deferred:** manual re-probe (federation page already
 * refetches every 30s; on-demand probe trigger is low-value); on-
 * demand sensor refresh (spawning child processes for arbitrary
 * sensors is a separate piece).
 *
 * **What this catches:** "is the peer for ecosystem-A still up
 * AND why did it crash?" without leaving the dashboard.
 */
export function ServicesPage() {
  const brainId = useBrainId();
  const { data, isLoading, error } = useQuery({
    queryKey: ["services", brainId],
    queryFn: () => fetchServices(brainId),
    refetchInterval: 5_000,
  });
  // S15-C-2 expansion: per-peer log viewer modal. Tracks the
  // currently-open peer's name; null means no modal.
  const [logPeer, setLogPeer] = useState<string | null>(null);

  if (isLoading) {
    return <PageShell>Loading services…</PageShell>;
  }
  if (error || !data) {
    return (
      <PageShell>
        <Card>
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-destructive" />
              Failed to load services
            </CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">
            {error instanceof Error ? error.message : "unknown error"}
          </CardContent>
        </Card>
      </PageShell>
    );
  }

  if (data.services.length === 0) {
    return (
      <PageShell>
        <EmptyState />
      </PageShell>
    );
  }

  return (
    <PageShell>
      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <Server className="h-5 w-5" />
            Tracked services ({data.services.length})
          </CardTitle>
          <CardDescription>
            Every A2A peer this dashboard has spawned and is currently
            tracking. Refreshes every 5 seconds.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Table data-testid="services-table">
            <TableHeader>
              <TableRow>
                <TableHead>Peer</TableHead>
                <TableHead>PID</TableHead>
                <TableHead>Port</TableHead>
                <TableHead>Uptime</TableHead>
                <TableHead>Log path</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.services.map((s) => (
                <ServiceRow
                  key={s.peer_name}
                  service={s}
                  onViewLog={() => setLogPeer(s.peer_name)}
                />
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
      {logPeer && (
        <PeerLogModal
          brainId={brainId}
          peerName={logPeer}
          onClose={() => setLogPeer(null)}
        />
      )}
    </PageShell>
  );
}

function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="space-y-6 p-6" data-testid="services-page">
      <header>
        <h1 className="text-2xl font-bold">Services</h1>
        <p className="text-sm text-muted-foreground mt-1">
          A2A peers this dashboard has spawned. Start/stop actions
          live on the Federation page; this page is read-only fleet
          telemetry.
        </p>
      </header>
      {children}
    </div>
  );
}

function EmptyState() {
  return (
    <Card data-testid="services-empty">
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <CircleSlash className="h-5 w-5 text-muted-foreground" />
          No services tracked
        </CardTitle>
        <CardDescription>
          The dashboard hasn't spawned any A2A peers in this session.
        </CardDescription>
      </CardHeader>
      <CardContent className="text-sm space-y-2">
        <p>
          To start a peer, visit the Federation page and use its{" "}
          <code className="text-xs">Start</code> button. The dashboard
          must be launched with{" "}
          <code className="text-xs">--allow-mutations</code> for those
          actions to be available.
        </p>
        <p className="text-muted-foreground">
          Adopters can also start peers manually via the CLI:{" "}
          <code className="text-xs">neurogrim a2a-serve …</code>. Those
          peers won't appear here — this page only shows
          dashboard-spawned services.
        </p>
      </CardContent>
    </Card>
  );
}

function ServiceRow({
  service,
  onViewLog,
}: {
  service: ServiceSnapshot;
  onViewLog: () => void;
}) {
  return (
    <TableRow data-testid={`service-row-${service.peer_name}`}>
      <TableCell>
        <div className="flex items-center gap-2">
          <Activity className="h-4 w-4 text-emerald-600" />
          <span className="font-medium">{service.peer_name}</span>
        </div>
      </TableCell>
      <TableCell className="font-mono text-xs">{service.pid}</TableCell>
      <TableCell>
        <Badge variant="outline">{service.port}</Badge>
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {formatUptime(service.started_at)}
      </TableCell>
      <TableCell className="text-xs text-muted-foreground font-mono truncate max-w-xs">
        {service.log_path}
      </TableCell>
      <TableCell className="text-right">
        <Button
          size="sm"
          variant="outline"
          onClick={onViewLog}
          data-testid={`service-view-log-${service.peer_name}`}
        >
          <FileText className="h-3.5 w-3.5 mr-1" />
          View log
        </Button>
      </TableCell>
    </TableRow>
  );
}

/**
 * S15-C-2 expansion: in-dashboard log tail viewer. Reads the most
 * recent ~256 KB of `<peer_brain>/.claude/brain/logs/<peer>.log`
 * via the per-peer log endpoint and renders it in a centered
 * modal.
 *
 * Manual refresh button reloads the tail; live streaming via SSE
 * is a v3 follow-on (current refresh-on-click is enough for
 * "what just happened" debugging without engineering the
 * file-watcher complexity yet).
 */
function PeerLogModal({
  brainId,
  peerName,
  onClose,
}: {
  brainId: string;
  peerName: string;
  onClose: () => void;
}) {
  const { data, isLoading, error, refetch, isFetching } = useQuery({
    queryKey: ["peer-log", brainId, peerName],
    queryFn: () => fetchPeerLog(brainId, peerName),
    // No refetchInterval — operator clicks Refresh when they want
    // a new tail. Auto-polling is v3 polish.
  });

  // ESC closes the modal. Keep wiring minimal — backdrop click +
  // explicit close button cover the rest.
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
      data-testid="peer-log-backdrop"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="bg-background border rounded-lg shadow-lg max-w-4xl w-full m-4 flex flex-col overflow-hidden max-h-[85vh]"
        onClick={(e) => e.stopPropagation()}
        data-testid={`peer-log-modal-${peerName}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="peer-log-title"
      >
        <header className="flex items-center justify-between p-4 border-b gap-3">
          <div className="min-w-0 flex-1">
            <h2
              id="peer-log-title"
              className="text-lg font-bold flex items-center gap-2"
            >
              <FileText className="h-5 w-5" />
              <span className="truncate">{peerName}</span>
              <span className="text-xs text-muted-foreground font-normal">
                log tail
              </span>
            </h2>
            {data && (
              <p className="text-xs text-muted-foreground mt-1 font-mono truncate">
                {data.log_path}
                {data.truncated && (
                  <span className="ml-2 text-amber-600">
                    (showing last ~256 KB of{" "}
                    {formatBytes(Number(data.total_size_bytes ?? 0))})
                  </span>
                )}
              </p>
            )}
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <Button
              size="sm"
              variant="outline"
              onClick={() => refetch()}
              disabled={isFetching}
              data-testid="peer-log-refresh"
            >
              <RefreshCw
                className={`h-3.5 w-3.5 mr-1 ${isFetching ? "animate-spin" : ""}`}
              />
              Refresh
            </Button>
            <button
              type="button"
              onClick={onClose}
              className="p-1 hover:bg-muted rounded"
              aria-label="Close"
              data-testid="peer-log-close"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </header>
        <div className="flex-1 overflow-hidden flex flex-col">
          {isLoading && (
            <div className="p-4 text-sm text-muted-foreground">
              Loading log…
            </div>
          )}
          {error && (
            <div
              className="p-4 text-sm text-destructive flex items-center gap-2"
              data-testid="peer-log-error"
            >
              <AlertTriangle className="h-4 w-4" />
              Failed to load log:{" "}
              {error instanceof Error ? error.message : "unknown error"}
            </div>
          )}
          {data && !data.present && (
            <div
              className="p-4 text-sm text-muted-foreground flex items-center gap-2"
              data-testid="peer-log-absent"
            >
              <CircleSlash className="h-4 w-4" />
              No log file yet for{" "}
              <code className="text-xs">{peerName}</code> — the peer
              hasn't been started in this dashboard's lifetime, or
              the file was rotated/deleted out of band.
            </div>
          )}
          {data && data.present && data.lines.length === 0 && (
            <div
              className="p-4 text-sm text-muted-foreground"
              data-testid="peer-log-empty"
            >
              Log file exists but is empty (0 bytes).
            </div>
          )}
          {data && data.present && data.lines.length > 0 && (
            <pre
              className="text-xs font-mono bg-muted/30 px-4 py-3 overflow-auto whitespace-pre-wrap break-all flex-1"
              data-testid="peer-log-content"
            >
              {data.lines.join("\n")}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Pretty-prints byte counts (1.2 MB, 256 KB, etc.). Used in the
 * modal header to show how big the full log is when only a tail
 * is rendered.
 */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  const gb = mb / 1024;
  return `${gb.toFixed(2)} GB`;
}

function formatUptime(startedAt: string): string {
  if (!startedAt) return "—";
  const started = new Date(startedAt);
  if (isNaN(started.getTime())) return startedAt;
  const elapsedMs = Date.now() - started.getTime();
  if (elapsedMs < 0) return "just now";
  const seconds = Math.floor(elapsedMs / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

async function fetchServices(brainId: string): Promise<ServicesListResponse> {
  const url = brainApi(brainId, "services");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as ServicesListResponse;
}

async function fetchPeerLog(
  brainId: string,
  peerName: string,
): Promise<PeerLogResponse> {
  const url = `${brainApi(brainId, "peers")}/${encodeURIComponent(peerName)}/log?lines=200`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as PeerLogResponse;
}
