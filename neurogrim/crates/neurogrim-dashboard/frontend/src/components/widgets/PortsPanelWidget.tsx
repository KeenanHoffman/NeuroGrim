import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, AlertCircle, FileQuestion } from "lucide-react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { brainApi, useBrainId } from "@/lib/useBrain";

/**
 * v3.5.0 — ports-panel widget. Surfaces the project's persisted
 * port allocation (`<root>/.claude/brain/ports.json`) plus a live
 * "is this port currently bound?" probe.
 *
 * Data shape from `/api/brains/:id/ports` is loose-typed — the
 * server emits a small JSON object directly rather than a
 * dedicated DTO. The two shapes:
 *
 * - `{ missing: true, ports_file }` when no ports.json exists
 * - `{ missing: false, schema_version, dashboard_port, a2a_port,
 *      created_at, generated_by, dashboard_port_bound,
 *      a2a_port_bound, ports_file }`
 */
type PortsResponse =
  | {
      missing: true;
      ports_file: string;
    }
  | {
      missing: false;
      schema_version: string;
      dashboard_port: number;
      a2a_port: number;
      created_at: string;
      generated_by: string;
      dashboard_port_bound: boolean;
      a2a_port_bound: boolean;
      ports_file: string;
    };

async function fetchPorts(brainId: string): Promise<PortsResponse> {
  const url = brainApi(brainId, "ports");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as PortsResponse;
}

export function PortsPanelWidget({ title }: { title?: string | null }) {
  const brainId = useBrainId();
  const { data, isLoading, error } = useQuery({
    queryKey: ["ports", brainId],
    queryFn: () => fetchPorts(brainId),
    refetchInterval: 30_000,
  });

  return (
    <Card className="h-full" data-testid="ports-panel-widget">
      <CardHeader>
        <CardTitle className="text-lg">{title ?? "Ports"}</CardTitle>
      </CardHeader>
      <CardContent className="text-sm">
        {isLoading && (
          <div className="text-xs text-muted-foreground">Loading…</div>
        )}
        {error && (
          <div className="text-xs text-destructive">
            Failed to load ports: {(error as Error).message}
          </div>
        )}
        {data && data.missing && (
          <div className="flex items-start gap-2 text-xs text-muted-foreground">
            <FileQuestion className="h-4 w-4 shrink-0 translate-y-0.5" />
            <div>
              <div>No <code className="font-mono">ports.json</code> on disk yet.</div>
              <div className="mt-1 break-all">
                Expected: <code className="font-mono">{data.ports_file}</code>
              </div>
              <div className="mt-1">
                Run <code className="font-mono">neurogrim ui</code> or
                <code className="font-mono">neurogrim a2a-serve</code> to allocate.
              </div>
            </div>
          </div>
        )}
        {data && !data.missing && (
          <div className="space-y-3">
            <PortRow
              label="Dashboard"
              port={data.dashboard_port}
              bound={data.dashboard_port_bound}
            />
            <PortRow
              label="A2A peer"
              port={data.a2a_port}
              bound={data.a2a_port_bound}
            />
            <div className="border-t border-border pt-3 text-xs text-muted-foreground">
              <div className="break-all">
                <span className="uppercase tracking-wider mr-2">File</span>
                <code className="font-mono">{data.ports_file}</code>
              </div>
              <div className="mt-1">
                <span className="uppercase tracking-wider mr-2">Created</span>
                <code className="font-mono">{data.created_at}</code>
              </div>
              <div className="mt-1">
                <span className="uppercase tracking-wider mr-2">By</span>
                <code className="font-mono">{data.generated_by}</code>
              </div>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function PortRow({
  label,
  port,
  bound,
}: {
  label: string;
  port: number;
  bound: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-2">
      <div>
        <div className="text-xs uppercase tracking-wider text-muted-foreground">
          {label}
        </div>
        <div className="font-mono text-base">{port}</div>
      </div>
      {bound ? (
        <span
          className="flex items-center gap-1 rounded bg-emerald-500/10 px-2 py-1 text-xs text-emerald-600 dark:text-emerald-400"
          title="Port is currently bound — likely by this project's service"
        >
          <CheckCircle2 className="h-3 w-3" />
          bound
        </span>
      ) : (
        <span
          className="flex items-center gap-1 rounded bg-amber-500/10 px-2 py-1 text-xs text-amber-600 dark:text-amber-400"
          title="Port is currently free — service is not running"
        >
          <AlertCircle className="h-3 w-3" />
          free
        </span>
      )}
    </div>
  );
}
