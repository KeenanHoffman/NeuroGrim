import { useQuery } from "@tanstack/react-query";
import {
  Activity,
  AlertTriangle,
  CircleSlash,
  Server,
} from "lucide-react";
import type { ServicesListResponse } from "@bindings/ServicesListResponse";
import type { ServiceSnapshot } from "@bindings/ServiceSnapshot";
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
 * S15-C-2: built-in Services page.
 *
 * Read-only view of every A2A peer this dashboard is currently
 * tracking (spawned via `--allow-mutations` from the Federation
 * page). Surfaces peer_name, pid, port, started_at, log_path.
 *
 * **v1 scope:** read-only display. Re-probe / sensor-refresh
 * actions are deferred until the relevant API endpoints land
 * (carry-over from the v3.5.1 backlog noted in the S15 epic). The
 * existing Federation page already provides start/stop buttons
 * via `PeerActions`; this page surfaces the running fleet at a
 * glance instead.
 *
 * **What this catches:** "is the peer for ecosystem-A still up?"
 * without clicking through to Federation. Useful when running
 * multiple Brains side-by-side.
 */
export function ServicesPage() {
  const brainId = useBrainId();
  const { data, isLoading, error } = useQuery({
    queryKey: ["services", brainId],
    queryFn: () => fetchServices(brainId),
    refetchInterval: 5_000,
  });

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
                <TableHead>Log</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.services.map((s) => (
                <ServiceRow key={s.peer_name} service={s} />
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
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

function ServiceRow({ service }: { service: ServiceSnapshot }) {
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
      <TableCell className="text-xs text-muted-foreground font-mono">
        {service.log_path}
      </TableCell>
    </TableRow>
  );
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
