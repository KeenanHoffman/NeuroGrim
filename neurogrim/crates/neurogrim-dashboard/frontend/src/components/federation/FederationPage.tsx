import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Globe, Link2 } from "lucide-react";
import type { FederationResponse } from "@bindings/FederationResponse";
import type { PeerDto } from "@bindings/PeerDto";
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

async function fetchFederation(brainId: string): Promise<FederationResponse> {
  const url = brainApi(brainId, "federation");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as FederationResponse;
}

export function FederationPage() {
  const brainId = useBrainId();
  const { data, isLoading, error } = useQuery({
    queryKey: ["federation", brainId],
    queryFn: () => fetchFederation(brainId),
    refetchInterval: 30_000,
  });

  const [selected, setSelected] = useState<string | null>(null);

  if (isLoading) return <FederationSkeleton />;
  if (error || !data) {
    return (
      <Card className="border-destructive">
        <CardHeader>
          <CardTitle className="text-destructive">
            Failed to load federation
          </CardTitle>
        </CardHeader>
        <CardContent>
          <pre className="text-xs">{(error as Error)?.message ?? "Unknown error"}</pre>
        </CardContent>
      </Card>
    );
  }

  const selectedPeer = data.peers.find((p) => p.name === selected) ?? null;

  return (
    <div className="space-y-6">
      <SelfCard self={data.self_brain} schemaVersion={data.registry_schema_version} />

      {data.peers.length === 0 ? (
        <EmptyFederationCard />
      ) : (
        <>
          <TopologyCard
            selfLabel={data.self_brain.label}
            peers={data.peers}
            selected={selected}
            onSelect={setSelected}
          />
          <PeersTableCard
            peers={data.peers}
            selected={selected}
            onSelect={setSelected}
          />
          {selectedPeer && <PeerDetailCard peer={selectedPeer} />}
        </>
      )}
    </div>
  );
}

function SelfCard({
  self,
  schemaVersion,
}: {
  self: FederationResponse["self_brain"];
  schemaVersion: string;
}) {
  return (
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-4">
          <div>
            <CardTitle className="text-2xl flex items-center gap-2">
              <Globe className="h-5 w-5 text-muted-foreground" />
              {self.label}
            </CardTitle>
            <CardDescription className="font-mono text-xs">
              {self.project_root}
            </CardDescription>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="secondary">dashboard {self.version}</Badge>
            <Badge variant="outline">registry schema {schemaVersion}</Badge>
          </div>
        </div>
      </CardHeader>
    </Card>
  );
}

function EmptyFederationCard() {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg">No federation peers</CardTitle>
        <CardDescription>
          This Brain has no children declared in <code>config.children</code>.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="text-sm text-muted-foreground">
          Register a peer with{" "}
          <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
            neurogrim federation register --name &lt;peer&gt; --path &lt;path&gt;
          </code>
          .
        </div>
      </CardContent>
    </Card>
  );
}

interface TopologyCardProps {
  selfLabel: string;
  peers: PeerDto[];
  selected: string | null;
  onSelect: (name: string | null) => void;
}

/**
 * Hand-drawn SVG topology. Self on the left, peers stacked on the
 * right with connecting lines. Colors track peer status.
 *
 * Why no react-flow: at typical federation sizes (1-3 peers), a
 * full graph library is overkill. SVG keeps the bundle slim and the
 * dependency surface small. Phase 2.x can swap if interactive layout
 * (drag, zoom, edge labels) becomes worth its bundle weight.
 */
function TopologyCard({
  selfLabel,
  peers,
  selected,
  onSelect,
}: TopologyCardProps) {
  const visiblePeers = peers;
  const peerCount = visiblePeers.length;

  // Layout constants. Width = 600, height grows with peer count.
  const W = 600;
  const NODE_W = 180;
  const NODE_H = 56;
  const ROW_H = 80;
  const H = Math.max(NODE_H + 40, peerCount * ROW_H + 40);

  const selfCx = NODE_W / 2 + 20;
  const selfCy = H / 2;
  const peerCx = W - NODE_W / 2 - 20;
  const peerYs = visiblePeers.map(
    (_, i) => 20 + NODE_H / 2 + i * ROW_H
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg">Topology</CardTitle>
        <CardDescription>
          One-hop view of declared peers. A2A peers were probed for liveness;
          subprocess peers are unprobed by design.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="overflow-x-auto">
          <svg
            viewBox={`0 0 ${W} ${H}`}
            className="w-full h-auto"
            data-testid="federation-topology"
          >
            {/* Edges from self → each peer */}
            {visiblePeers.map((p, i) => (
              <line
                key={`edge-${p.name}`}
                x1={selfCx + NODE_W / 2}
                y1={selfCy}
                x2={peerCx - NODE_W / 2}
                y2={peerYs[i]}
                stroke={edgeColor(p)}
                strokeWidth={selected === p.name ? 2 : 1}
                strokeDasharray={p.read_only ? "4 3" : undefined}
              />
            ))}

            {/* Self node */}
            <TopologyNode
              x={selfCx - NODE_W / 2}
              y={selfCy - NODE_H / 2}
              w={NODE_W}
              h={NODE_H}
              label={selfLabel}
              sublabel="self"
              accent="self"
              selected={false}
            />

            {/* Peer nodes */}
            {visiblePeers.map((p, i) => (
              <g
                key={`node-${p.name}`}
                onClick={() => onSelect(p.name === selected ? null : p.name)}
                style={{ cursor: "pointer" }}
                data-testid={`topology-node-${p.name}`}
              >
                <TopologyNode
                  x={peerCx - NODE_W / 2}
                  y={peerYs[i] - NODE_H / 2}
                  w={NODE_W}
                  h={NODE_H}
                  label={p.display_name}
                  sublabel={`${p.transport} · ${p.status.kind}`}
                  accent={statusAccent(p)}
                  selected={selected === p.name}
                />
              </g>
            ))}
          </svg>
        </div>
      </CardContent>
    </Card>
  );
}

type StatusAccent =
  | "self"
  | "alive"
  | "not-running"
  | "unhealthy"
  | "unreachable"
  | "unprobed"
  | "disabled";

interface TopologyNodeProps {
  x: number;
  y: number;
  w: number;
  h: number;
  label: string;
  sublabel: string;
  accent: StatusAccent;
  selected: boolean;
}

function TopologyNode({
  x,
  y,
  w,
  h,
  label,
  sublabel,
  accent,
  selected,
}: TopologyNodeProps) {
  const fill = {
    self: "hsl(var(--secondary))",
    alive: "rgb(16 185 129 / 0.12)",
    // unhealthy: process running but the well-known endpoint is
    // unresponsive — amber tone matches the "warning" badge variant
    // we use in the table.
    unhealthy: "rgb(245 158 11 / 0.12)",
    "not-running": "rgb(239 68 68 / 0.12)",
    unreachable: "rgb(239 68 68 / 0.12)",
    unprobed: "hsl(var(--muted))",
    disabled: "hsl(var(--muted) / 0.5)",
  }[accent];
  const stroke = {
    self: "hsl(var(--foreground) / 0.6)",
    alive: "rgb(16 185 129)",
    unhealthy: "rgb(245 158 11)",
    "not-running": "rgb(239 68 68)",
    unreachable: "rgb(239 68 68)",
    unprobed: "hsl(var(--border))",
    disabled: "hsl(var(--border))",
  }[accent];
  return (
    <>
      <rect
        x={x}
        y={y}
        width={w}
        height={h}
        rx={8}
        ry={8}
        fill={fill}
        stroke={stroke}
        strokeWidth={selected ? 2 : 1}
      />
      <text
        x={x + w / 2}
        y={y + h / 2 - 4}
        textAnchor="middle"
        fontSize="13"
        fontWeight="600"
        fill="hsl(var(--foreground))"
      >
        {truncate(label, 24)}
      </text>
      <text
        x={x + w / 2}
        y={y + h / 2 + 14}
        textAnchor="middle"
        fontSize="11"
        fill="hsl(var(--muted-foreground))"
      >
        {sublabel}
      </text>
    </>
  );
}

function PeersTableCard({
  peers,
  selected,
  onSelect,
}: {
  peers: PeerDto[];
  selected: string | null;
  onSelect: (name: string | null) => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg">Peers ({peers.length})</CardTitle>
        <CardDescription>Click a row for details + Agent Card excerpt.</CardDescription>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead className="w-32">Transport</TableHead>
              <TableHead className="w-28">Status</TableHead>
              <TableHead className="w-20 text-right">Weight</TableHead>
              <TableHead className="w-24">Posture</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {peers.map((p) => (
              <TableRow
                key={p.name}
                data-testid={`peer-row-${p.name}`}
                className="cursor-pointer"
                data-state={selected === p.name ? "selected" : undefined}
                onClick={() => onSelect(p.name === selected ? null : p.name)}
              >
                <TableCell>
                  <div className="font-medium">{p.display_name}</div>
                  <div className="font-mono text-xs text-muted-foreground">
                    {p.name}
                  </div>
                </TableCell>
                <TableCell className="text-xs font-mono">
                  {p.transport}
                </TableCell>
                <TableCell>
                  <StatusBadge peer={p} />
                </TableCell>
                <TableCell className="text-right font-mono text-xs">
                  {p.weight.toFixed(2)}
                </TableCell>
                <TableCell>
                  {p.read_only ? (
                    <Badge variant="outline" className="text-xs">
                      read-only
                    </Badge>
                  ) : (
                    <Badge variant="secondary" className="text-xs">
                      contributing
                    </Badge>
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function StatusBadge({ peer }: { peer: PeerDto }) {
  const variant = (() => {
    switch (peer.status.kind) {
      case "alive":
        return "success" as const;
      case "unhealthy":
        // TCP open but the well-known endpoint isn't serving — the
        // process is up, just not behaving. Warning tone reflects
        // "something's wrong here, take a look" rather than the
        // harder "this is offline" of `not-running`.
        return "warning" as const;
      case "not-running":
      case "unreachable":
        return "danger" as const;
      case "disabled":
        return "outline" as const;
      case "unprobed":
      default:
        return "secondary" as const;
    }
  })();
  return (
    <Badge variant={variant} className="text-xs" title={peer.status.message}>
      {peer.status.kind}
    </Badge>
  );
}

function PeerDetailCard({ peer }: { peer: PeerDto }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-lg flex items-center gap-2">
          <Link2 className="h-4 w-4 text-muted-foreground" />
          {peer.display_name}
        </CardTitle>
        <CardDescription className="font-mono text-xs">{peer.name}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 text-sm">
        <DetailGrid>
          <Detail label="Transport" value={peer.transport} />
          <Detail label="Status" value={peer.status.kind} />
          <Detail label="Weight" value={peer.weight.toFixed(2)} />
          <Detail
            label="Posture"
            value={peer.read_only ? "read-only" : "contributing"}
          />
          {peer.a2a_endpoint && (
            <Detail label="A2A endpoint" value={peer.a2a_endpoint} mono span />
          )}
          {peer.brain_path && (
            <Detail label="Brain path" value={peer.brain_path} mono span />
          )}
          {peer.status.message && (
            <Detail label="Status detail" value={peer.status.message} span />
          )}
        </DetailGrid>

        {peer.agent_card && (
          <div className="rounded border border-border bg-muted/30 p-4">
            <div className="mb-2 text-xs uppercase tracking-wider text-muted-foreground">
              Agent Card
            </div>
            <DetailGrid>
              <Detail label="ID" value={peer.agent_card.id} mono />
              <Detail label="Name" value={peer.agent_card.name} />
              <Detail label="Version" value={peer.agent_card.version} />
              <Detail
                label="Interface"
                value={peer.agent_card.interface_version}
              />
              <Detail
                label="Schema"
                value={peer.agent_card.schema_version}
              />
              <Detail
                label="Protocol"
                value={peer.agent_card.transport_protocol}
                mono
              />
              {peer.agent_card.topology_role && (
                <Detail label="Role" value={peer.agent_card.topology_role} />
              )}
              {peer.agent_card.topology_parent_id && (
                <Detail
                  label="Parent"
                  value={peer.agent_card.topology_parent_id}
                  mono
                />
              )}
            </DetailGrid>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function DetailGrid({ children }: { children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 md:grid-cols-3">
      {children}
    </div>
  );
}

function Detail({
  label,
  value,
  mono,
  span,
}: {
  label: string;
  value: string;
  mono?: boolean;
  span?: boolean;
}) {
  return (
    <div className={span ? "sm:col-span-2 md:col-span-3" : ""}>
      <div className="text-xs uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <div className={mono ? "font-mono text-xs break-all" : ""}>{value}</div>
    </div>
  );
}

function FederationSkeleton() {
  return (
    <div className="animate-pulse space-y-6">
      <div className="h-24 rounded-lg bg-muted/50" />
      <div className="h-64 rounded-lg bg-muted/50" />
      <div className="h-48 rounded-lg bg-muted/50" />
    </div>
  );
}

function statusAccent(p: PeerDto): Exclude<StatusAccent, "self"> {
  if (!p.enabled) return "disabled";
  switch (p.status.kind) {
    case "alive":
      return "alive";
    case "unhealthy":
      return "unhealthy";
    case "not-running":
      return "not-running";
    case "unreachable":
      return "unreachable";
    case "disabled":
      return "disabled";
    case "unprobed":
    default:
      return "unprobed";
  }
}

function edgeColor(p: PeerDto): string {
  if (!p.enabled) return "hsl(var(--border))";
  switch (p.status.kind) {
    case "alive":
      return "rgb(16 185 129 / 0.7)";
    case "unhealthy":
      return "rgb(245 158 11 / 0.6)";
    case "not-running":
    case "unreachable":
      return "rgb(239 68 68 / 0.6)";
    default:
      return "hsl(var(--border))";
  }
}

function truncate(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
}
