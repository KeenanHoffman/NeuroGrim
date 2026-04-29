import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  CheckCircle2,
  HelpCircle,
  ShieldAlert,
  XCircle,
} from "lucide-react";
import type { ApprovalsPageResponse } from "@bindings/ApprovalsPageResponse";
import type { ApprovalRequestView } from "@bindings/ApprovalRequestView";
import type { ApprovalResolutionView } from "@bindings/ApprovalResolutionView";
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
 * S13-B-6: autonomy approvals page.
 *
 * Joins `_neurogrim/approvals` (mutation requests pending operator
 * decision) with `_neurogrim/approval-resolutions` (operator
 * decisions) so operators see "what's waiting" + "what just happened".
 *
 * Approve / Deny buttons emit on `_neurogrim/approval-resolutions`
 * via the dashboard server's POST endpoint, which stamps the
 * resolution with the server's `$NEUROGRIM_OPERATOR` env handle.
 *
 * Buttons gated by `--allow-mutations`; without it, the buttons
 * still render but hitting them returns 403 (matches the convention
 * for service start/stop, layout edits, etc.).
 */
export function ApprovalsPage() {
  const brainId = useBrainId();
  const qc = useQueryClient();
  const { data, isLoading, error } = useQuery({
    queryKey: ["approvals", brainId],
    queryFn: () => fetchApprovals(brainId),
    refetchInterval: 10_000,
  });

  const resolve = useMutation({
    mutationFn: async (params: { actionId: string; decision: "approve" | "deny" }) => {
      const url = `${brainApi(brainId, "approvals")}/${encodeURIComponent(
        params.actionId,
      )}/resolve`;
      const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ decision: params.decision }),
      });
      if (!res.ok) {
        const body = await res.text();
        throw new Error(`${url} returned ${res.status}: ${body}`);
      }
      return res.json();
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["approvals", brainId] }),
  });

  if (isLoading) {
    return <PageShell>Loading approvals…</PageShell>;
  }
  if (error || !data) {
    return (
      <PageShell>
        <Card>
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-destructive" />
              Failed to load approvals
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
      {data.pending.length === 0 && data.recent_resolutions.length === 0 && (
        <EmptyState />
      )}

      {data.pending.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <ShieldAlert className="h-5 w-5 text-amber-600" />
              Pending ({data.pending.length})
            </CardTitle>
            <CardDescription>
              Mutation tools that resolved to <code className="text-xs">Approve</code>{" "}
              autonomy. Approving emits on{" "}
              <code className="text-xs">_neurogrim/approval-resolutions</code>; the
              waiting agent unblocks on its next <code className="text-xs">await_approval</code>{" "}
              poll.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table data-testid="approvals-pending-table">
              <TableHeader>
                <TableRow>
                  <TableHead>Tool</TableHead>
                  <TableHead>Action type</TableHead>
                  <TableHead>Requested</TableHead>
                  <TableHead className="w-1">Action</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.pending.map((req) => (
                  <PendingRow
                    key={req.action_id}
                    request={req}
                    onApprove={() =>
                      resolve.mutate({ actionId: req.action_id, decision: "approve" })
                    }
                    onDeny={() =>
                      resolve.mutate({ actionId: req.action_id, decision: "deny" })
                    }
                    busy={resolve.isPending}
                  />
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}

      {data.recent_resolutions.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">
              Recent decisions ({data.recent_resolutions.length})
            </CardTitle>
            <CardDescription>
              Last {data.recent_resolutions.length} resolved approvals (newest first).
              Operator handle from <code className="text-xs">$NEUROGRIM_OPERATOR</code>{" "}
              at the dashboard server.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table data-testid="approvals-resolutions-table">
              <TableHeader>
                <TableRow>
                  <TableHead>Action ID</TableHead>
                  <TableHead>Decision</TableHead>
                  <TableHead>Operator</TableHead>
                  <TableHead>When</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.recent_resolutions.map((r) => (
                  <ResolutionRow key={`${r.action_id}-${r.decided_at}`} resolution={r} />
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
    <div className="space-y-6 p-6" data-testid="approvals-page">
      <header>
        <h1 className="text-2xl font-bold">Approvals</h1>
        <p className="text-sm text-muted-foreground mt-1">
          The autonomy gate's surface. Mutation tools that resolve to{" "}
          <code className="text-xs">Approve</code> land here; operator decisions
          unblock the waiting agent.
        </p>
      </header>
      {children}
    </div>
  );
}

function EmptyState() {
  return (
    <Card data-testid="approvals-empty">
      <CardHeader>
        <CardTitle className="text-lg">No approvals pending or recent</CardTitle>
        <CardDescription>
          The autonomy gate hasn't blocked any mutation tools yet, and no
          resolutions have been recorded.
        </CardDescription>
      </CardHeader>
      <CardContent className="text-sm space-y-2">
        <p>
          Background: when an MCP mutation tool (e.g.,{" "}
          <code className="text-xs">queue_publish</code>,{" "}
          <code className="text-xs">domain_new</code>) resolves to{" "}
          <code className="text-xs">Approve</code> autonomy, the call lands an
          entry on <code className="text-xs">_neurogrim/approvals</code> and
          this page shows the pending request with Approve/Deny buttons.
        </p>
        <p className="text-muted-foreground">
          See <code className="text-xs">neurogrim explain queues</code> for the
          full Pattern 2 (request/response coordination) flow.
        </p>
      </CardContent>
    </Card>
  );
}

function PendingRow({
  request,
  onApprove,
  onDeny,
  busy,
}: {
  request: ApprovalRequestView;
  onApprove: () => void;
  onDeny: () => void;
  busy: boolean;
}) {
  return (
    <TableRow data-testid={`approval-row-${request.action_id}`}>
      <TableCell>
        <div className="font-medium text-sm">{request.tool}</div>
        <div className="text-xs text-muted-foreground font-mono">
          {request.action_id}
        </div>
      </TableCell>
      <TableCell>
        <Badge variant="outline" className="capitalize">
          {request.action_type}
        </Badge>
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {formatTime(request.requested_at)}
      </TableCell>
      <TableCell>
        <div className="flex gap-2">
          <Button
            size="sm"
            variant="default"
            onClick={onApprove}
            disabled={busy}
            data-testid={`approve-button-${request.action_id}`}
          >
            <CheckCircle2 className="h-3.5 w-3.5 mr-1" />
            Approve
          </Button>
          <Button
            size="sm"
            variant="destructive"
            onClick={onDeny}
            disabled={busy}
            data-testid={`deny-button-${request.action_id}`}
          >
            <XCircle className="h-3.5 w-3.5 mr-1" />
            Deny
          </Button>
        </div>
      </TableCell>
    </TableRow>
  );
}

function ResolutionRow({ resolution }: { resolution: ApprovalResolutionView }) {
  return (
    <TableRow data-testid={`resolution-row-${resolution.action_id}`}>
      <TableCell className="text-xs font-mono text-muted-foreground">
        {resolution.action_id}
      </TableCell>
      <TableCell>
        <DecisionBadge decision={resolution.decision} />
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {resolution.operator ?? "—"}
      </TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {formatTime(resolution.decided_at)}
      </TableCell>
    </TableRow>
  );
}

function DecisionBadge({ decision }: { decision: string }) {
  switch (decision) {
    case "approve":
      return (
        <Badge variant="default" className="capitalize gap-1" data-testid="decision-approve">
          <CheckCircle2 className="h-3 w-3" />
          approved
        </Badge>
      );
    case "deny":
      return (
        <Badge variant="destructive" className="capitalize gap-1" data-testid="decision-deny">
          <XCircle className="h-3 w-3" />
          denied
        </Badge>
      );
    default:
      return (
        <Badge variant="outline" className="capitalize gap-1">
          <HelpCircle className="h-3 w-3" />
          {decision}
        </Badge>
      );
  }
}

function formatTime(iso: string): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

async function fetchApprovals(brainId: string): Promise<ApprovalsPageResponse> {
  const url = brainApi(brainId, "approvals");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as ApprovalsPageResponse;
}
