import { useQuery } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  CircleAlert,
  CircleSlash,
  Clock,
  PauseCircle,
  XCircle,
} from "lucide-react";
import type { PublishGatesPageResponse } from "@bindings/PublishGatesPageResponse";
import type { PublishGateView } from "@bindings/PublishGateView";
import type { PublishGateLedgerView } from "@bindings/PublishGateLedgerView";
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
 * S12-G-6: read-only publish-gates page.
 *
 * Renders the brain's `publish-gates.yaml` manifest joined with its
 * `publish-gate-ledger.jsonl` so operators can see "what's pending
 * acknowledgement, what passed last run, what failed" at a glance.
 *
 * Three render branches:
 * - **No manifest** — empty-state banner pointing at `neurogrim
 *   explain publish-gates`.
 * - **Malformed manifest** — error banner; suggests running
 *   `neurogrim doctor` (which validates the manifest against
 *   `publish-gates-v1.schema.json`).
 * - **Valid manifest** — gate table + recent ledger timeline.
 *
 * Read-only by design. Operators ack manual gates via the CLI
 * (`neurogrim publish-gate ack --gate <id>`). A future story can
 * add ack buttons here once the audit-trail discipline for
 * dashboard-side mutations is settled.
 */
export function PublishGatesPage() {
  const brainId = useBrainId();
  const { data, isLoading, error } = useQuery({
    queryKey: ["publish-gates", brainId],
    queryFn: () => fetchPublishGates(brainId),
    refetchInterval: 30_000,
  });

  if (isLoading) {
    return <PageShell>Loading publish-gates manifest…</PageShell>;
  }
  if (error || !data) {
    return (
      <PageShell>
        <Card>
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <CircleAlert className="h-5 w-5 text-destructive" />
              Failed to load publish-gates
            </CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">
            {error instanceof Error ? error.message : "unknown error"}
          </CardContent>
        </Card>
      </PageShell>
    );
  }

  return (
    <PageShell>
      {/* Empty state — no manifest authored yet. */}
      {!data.manifest_present && data.recent_ledger.length === 0 && (
        <EmptyState />
      )}

      {/* Schema-corrupt state — manifest exists but won't parse. */}
      {data.manifest_present && data.manifest_error && (
        <Card>
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-destructive" />
              publish-gates.yaml is malformed
            </CardTitle>
            <CardDescription>
              The manifest is present but failed schema validation. Run{" "}
              <code className="text-xs">neurogrim doctor</code> for a
              detailed diagnosis.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <pre className="text-xs whitespace-pre-wrap bg-muted/30 p-3 rounded">
              {data.manifest_error}
            </pre>
          </CardContent>
        </Card>
      )}

      {/* Gate table — current state per gate from the manifest. */}
      {data.gates.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">
              Gates ({data.gates.length})
            </CardTitle>
            <CardDescription>
              Current state per gate. Status reflects the most recent
              ledger entry; <code className="text-xs">no_runs</code> means
              this gate has never been executed.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table data-testid="publish-gates-table">
              <TableHeader>
                <TableRow>
                  <TableHead>Gate</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Last run</TableHead>
                  <TableHead>Operator</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.gates.map((g) => (
                  <GateRow key={g.id} gate={g} />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      {/* Recent ledger — last N runs across all gates, newest first. */}
      {data.recent_ledger.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">
              Recent activity ({data.recent_ledger.length})
            </CardTitle>
            <CardDescription>
              Last {data.recent_ledger.length} ledger entries, newest first.
              Includes runs of gates no longer in the manifest.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table data-testid="publish-gates-ledger">
              <TableHeader>
                <TableRow>
                  <TableHead>When</TableHead>
                  <TableHead>Gate</TableHead>
                  <TableHead>Mode</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Operator</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.recent_ledger.map((e) => (
                  <LedgerRow
                    key={`${e.run_id}-${e.gate_id}-${e.started_at}-${e.status}`}
                    entry={e}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </PageShell>
  );
}

function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="space-y-6 p-6" data-testid="publish-gates-page">
      <header>
        <h1 className="text-2xl font-bold">Publish gates</h1>
        <p className="text-sm text-muted-foreground mt-1">
          The structured pre-publish pipeline. Edit{" "}
          <code className="text-xs">.claude/brain/publish-gates.yaml</code>{" "}
          to declare gates; run <code className="text-xs">neurogrim
          publish-gate run</code> to execute them.
        </p>
      </header>
      {children}
    </div>
  );
}

function EmptyState() {
  return (
    <Card data-testid="publish-gates-empty">
      <CardHeader>
        <CardTitle className="text-lg">No publish gates declared</CardTitle>
        <CardDescription>
          This brain doesn't have a publish-gates manifest yet.
        </CardDescription>
      </CardHeader>
      <CardContent className="text-sm space-y-2">
        <p>To get started:</p>
        <ol className="list-decimal list-inside space-y-1 ml-2">
          <li>
            Author{" "}
            <code className="text-xs">
              .claude/brain/publish-gates.yaml
            </code>{" "}
            with one or more gates.
          </li>
          <li>
            Run <code className="text-xs">neurogrim doctor</code> to
            validate the manifest.
          </li>
          <li>
            Run <code className="text-xs">neurogrim publish-gate run</code>{" "}
            to execute the gates.
          </li>
        </ol>
        <p className="mt-3 text-muted-foreground">
          See <code className="text-xs">neurogrim explain publish-gates</code>{" "}
          for the full guide.
        </p>
      </CardContent>
    </Card>
  );
}

function GateRow({ gate }: { gate: PublishGateView }) {
  return (
    <TableRow data-testid={`gate-row-${gate.id}`}>
      <TableCell>
        <div className="font-medium">{gate.id}</div>
        <div className="text-xs text-muted-foreground">{gate.description}</div>
      </TableCell>
      <TableCell>
        <Badge variant="outline" className="capitalize">
          {gate.gate_type}
        </Badge>
        {!gate.blocking && (
          <span className="ml-2 text-xs text-muted-foreground">advisory</span>
        )}
      </TableCell>
      <TableCell>
        <StatusBadge status={gate.current_status} />
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {gate.last_run_at ? formatTime(gate.last_run_at) : "—"}
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {gate.operator ?? "—"}
      </TableCell>
    </TableRow>
  );
}

function LedgerRow({ entry }: { entry: PublishGateLedgerView }) {
  return (
    <TableRow data-testid={`ledger-row-${entry.run_id}-${entry.gate_id}`}>
      <TableCell className="text-xs whitespace-nowrap">
        {formatTime(entry.started_at)}
      </TableCell>
      <TableCell>
        <div className="font-medium text-sm">{entry.gate_id}</div>
        <div className="text-xs text-muted-foreground capitalize">
          {entry.gate_type}
        </div>
      </TableCell>
      <TableCell className="text-xs text-muted-foreground capitalize">
        {entry.mode}
      </TableCell>
      <TableCell>
        <StatusBadge status={entry.status} />
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {entry.operator ?? "—"}
      </TableCell>
    </TableRow>
  );
}

function StatusBadge({ status }: { status: string }) {
  const { Icon, variant, label } = statusStyle(status);
  return (
    <Badge
      variant={variant}
      className="capitalize gap-1"
      data-testid={`status-badge-${status}`}
    >
      <Icon className="h-3 w-3" />
      {label}
    </Badge>
  );
}

function statusStyle(status: string): {
  Icon: typeof CheckCircle2;
  variant: "default" | "destructive" | "outline" | "secondary";
  label: string;
} {
  switch (status) {
    case "passed":
      return { Icon: CheckCircle2, variant: "default", label: "passed" };
    case "failed":
      return { Icon: XCircle, variant: "destructive", label: "failed" };
    case "pending":
      return { Icon: PauseCircle, variant: "secondary", label: "pending" };
    case "timed_out":
      return { Icon: Clock, variant: "destructive", label: "timed out" };
    case "deferred":
      return { Icon: CircleSlash, variant: "outline", label: "deferred" };
    case "error":
      return { Icon: AlertTriangle, variant: "destructive", label: "error" };
    case "no_runs":
      return { Icon: CircleSlash, variant: "outline", label: "no runs" };
    default:
      return { Icon: CircleAlert, variant: "outline", label: status };
  }
}

function formatTime(iso: string): string {
  // Display as `YYYY-MM-DD HH:MM` UTC for consistency with other
  // pages. The dashboard never localizes timestamps because the
  // ledger is operator-shared and timezone-agnostic.
  if (!iso) return "—";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

async function fetchPublishGates(
  brainId: string,
): Promise<PublishGatesPageResponse> {
  const url = brainApi(brainId, "publish-gates");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as PublishGatesPageResponse;
}
