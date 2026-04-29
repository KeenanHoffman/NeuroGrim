import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowUpDown, ChevronDown, ChevronUp, BookOpen, Search } from "lucide-react";
import type { SkillsResponse } from "@bindings/SkillsResponse";
import type { SkillDto } from "@bindings/SkillDto";
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

type SortKey = "name" | "format" | "invocation_count" | "last_invoked_at" | "hygiene_status";
type SortDir = "asc" | "desc";
type StatusFilter = "all" | "alive" | "dead" | "new" | "no-ledger";

async function fetchSkills(brainId: string): Promise<SkillsResponse> {
  const url = brainApi(brainId, "skills");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as SkillsResponse;
}

export function SkillsPage() {
  const brainId = useBrainId();
  const { data, isLoading, error } = useQuery({
    queryKey: ["skills", brainId],
    queryFn: () => fetchSkills(brainId),
    refetchInterval: 60_000,
  });

  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);

  const filtered = useMemo(() => {
    if (!data) return [];
    let rows = [...data.skills];
    if (statusFilter !== "all") {
      rows = rows.filter((s) => s.hygiene_status === statusFilter);
    }
    if (search.trim()) {
      const q = search.toLowerCase();
      rows = rows.filter(
        (s) =>
          s.name.toLowerCase().includes(q) ||
          s.description.toLowerCase().includes(q)
      );
    }
    rows.sort((a, b) => compareRows(a, b, sortKey));
    return sortDir === "asc" ? rows : rows.reverse();
  }, [data, statusFilter, search, sortKey, sortDir]);

  const counts = useMemo(() => {
    if (!data) return { all: 0, alive: 0, dead: 0, new: 0, "no-ledger": 0 };
    const c = { all: data.skills.length, alive: 0, dead: 0, new: 0, "no-ledger": 0 };
    for (const s of data.skills) {
      if (s.hygiene_status in c) {
        c[s.hygiene_status as keyof typeof c]++;
      }
    }
    return c;
  }, [data]);

  if (isLoading) return <SkillsSkeleton />;
  if (error || !data) {
    return (
      <Card className="border-destructive">
        <CardHeader>
          <CardTitle className="text-destructive">Failed to load skills</CardTitle>
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
      setSortDir(key === "invocation_count" || key === "last_invoked_at" ? "desc" : "asc");
    }
  };

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <div className="flex items-start justify-between gap-4 flex-wrap">
            <div>
              <CardTitle className="text-2xl flex items-center gap-2">
                <BookOpen className="h-5 w-5 text-muted-foreground" />
                Skills
              </CardTitle>
              <CardDescription>
                {data.skills.length} declared ·{" "}
                {data.ledger_present
                  ? `${data.total_invocations} invocations recorded · alive = invoked in last ${data.alive_window_days}d`
                  : "no invocation ledger yet"}
              </CardDescription>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <FilterChip
                label="All"
                count={counts.all}
                active={statusFilter === "all"}
                onClick={() => setStatusFilter("all")}
              />
              <FilterChip
                label="Alive"
                count={counts.alive}
                active={statusFilter === "alive"}
                tone="success"
                onClick={() => setStatusFilter("alive")}
              />
              <FilterChip
                label="Dead"
                count={counts.dead}
                active={statusFilter === "dead"}
                tone="warning"
                onClick={() => setStatusFilter("dead")}
              />
              <FilterChip
                label="New"
                count={counts.new}
                active={statusFilter === "new"}
                onClick={() => setStatusFilter("new")}
              />
              {counts["no-ledger"] > 0 && (
                <FilterChip
                  label="No-ledger"
                  count={counts["no-ledger"]}
                  active={statusFilter === "no-ledger"}
                  onClick={() => setStatusFilter("no-ledger")}
                />
              )}
            </div>
          </div>
        </CardHeader>
      </Card>

      {!data.ledger_present && <LedgerMissingBanner />}

      <Card>
        <CardContent className="pt-6">
          <div className="mb-4 flex items-center gap-2 max-w-md">
            <Search className="h-4 w-4 text-muted-foreground" />
            <input
              type="text"
              placeholder="Filter by name or description"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="flex-1 bg-transparent outline-none border-b border-border focus:border-foreground/40 text-sm py-1"
              data-testid="skills-search"
            />
          </div>

          <Table>
            <TableHeader>
              <TableRow>
                <SortableHead
                  current={sortKey}
                  sortKey="name"
                  dir={sortDir}
                  onClick={() => handleSort("name")}
                >
                  Name
                </SortableHead>
                <SortableHead
                  current={sortKey}
                  sortKey="format"
                  dir={sortDir}
                  onClick={() => handleSort("format")}
                  className="w-24"
                >
                  Format
                </SortableHead>
                <SortableHead
                  current={sortKey}
                  sortKey="hygiene_status"
                  dir={sortDir}
                  onClick={() => handleSort("hygiene_status")}
                  className="w-28"
                >
                  Status
                </SortableHead>
                <SortableHead
                  current={sortKey}
                  sortKey="invocation_count"
                  dir={sortDir}
                  onClick={() => handleSort("invocation_count")}
                  className="w-28 text-right"
                >
                  Invocations
                </SortableHead>
                <SortableHead
                  current={sortKey}
                  sortKey="last_invoked_at"
                  dir={sortDir}
                  onClick={() => handleSort("last_invoked_at")}
                  className="w-40"
                >
                  Last invoked
                </SortableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={5} className="text-center text-muted-foreground py-8">
                    No skills match the current filters.
                  </TableCell>
                </TableRow>
              ) : (
                filtered.map((s) => (
                  <SkillRow
                    key={s.name}
                    skill={s}
                    expanded={expanded === s.name}
                    onToggle={() =>
                      setExpanded(expanded === s.name ? null : s.name)
                    }
                  />
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}

function SkillRow({
  skill,
  expanded,
  onToggle,
}: {
  skill: SkillDto;
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <>
      <TableRow
        className="cursor-pointer"
        onClick={onToggle}
        data-testid={`skill-row-${skill.name}`}
        data-state={expanded ? "selected" : undefined}
      >
        <TableCell>
          <div className="font-medium font-mono text-sm">{skill.name}</div>
          <div className="text-xs text-muted-foreground truncate max-w-md">
            {skill.description || <em>(no description)</em>}
          </div>
        </TableCell>
        <TableCell className="text-xs font-mono">{skill.format}</TableCell>
        <TableCell>
          <StatusBadge status={skill.hygiene_status} />
        </TableCell>
        <TableCell className="text-right font-mono text-xs">
          {skill.invocation_count > 0 ? (
            <span
              title={`${skill.hard_invocations} hard (Skill-tool calls) + ${skill.soft_invocations} soft (SKILL.md reads); ${skill.recent_hard_invocations}h / ${skill.recent_soft_invocations}s within the alive window`}
            >
              <span className="font-semibold">{skill.invocation_count}</span>
              <span className="text-muted-foreground/80">
                {" ("}
                {skill.hard_invocations}h
                {" / "}
                {skill.soft_invocations}s
                {")"}
              </span>
            </span>
          ) : (
            <span className="text-muted-foreground">—</span>
          )}
        </TableCell>
        <TableCell className="text-xs font-mono text-muted-foreground">
          {skill.last_invoked_at ? formatRelative(skill.last_invoked_at) : "—"}
        </TableCell>
      </TableRow>
      {expanded && (
        <TableRow data-testid={`skill-row-${skill.name}-detail`}>
          <TableCell colSpan={5} className="bg-muted/30">
            <div className="space-y-3 py-2 text-sm">
              <div>
                <div className="text-xs uppercase tracking-wider text-muted-foreground">
                  Path
                </div>
                <div className="font-mono text-xs break-all">{skill.path}</div>
              </div>
              {skill.description && (
                <div>
                  <div className="text-xs uppercase tracking-wider text-muted-foreground">
                    Description
                  </div>
                  <div className="whitespace-pre-wrap text-foreground/90">
                    {skill.description}
                  </div>
                </div>
              )}
              {skill.invocation_count > 0 && (
                <div>
                  <div className="text-xs uppercase tracking-wider text-muted-foreground">
                    Invocations
                  </div>
                  <div className="text-xs">
                    <span className="font-semibold">{skill.invocation_count}</span>{" "}
                    total ·{" "}
                    <span title="Hard: explicit Skill-tool calls (slash commands or Skill(name=...)).">
                      {skill.hard_invocations} hard
                    </span>{" "}
                    ·{" "}
                    <span title="Soft: agent reads of the SKILL.md file via the Read tool. Captures the more common usage pattern where an agent follows skill guidance directly.">
                      {skill.soft_invocations} soft
                    </span>
                    {(skill.recent_hard_invocations > 0 ||
                      skill.recent_soft_invocations > 0) && (
                      <span className="text-muted-foreground">
                        {" "}
                        (recent: {skill.recent_hard_invocations}h /{" "}
                        {skill.recent_soft_invocations}s)
                      </span>
                    )}
                  </div>
                </div>
              )}
              {skill.last_invoked_at && (
                <div>
                  <div className="text-xs uppercase tracking-wider text-muted-foreground">
                    Last invoked
                  </div>
                  <div className="font-mono text-xs">{skill.last_invoked_at}</div>
                </div>
              )}
            </div>
          </TableCell>
        </TableRow>
      )}
    </>
  );
}

function StatusBadge({ status }: { status: string }) {
  const variant = (() => {
    switch (status) {
      case "alive":
        return "success" as const;
      case "dead":
        return "warning" as const;
      case "new":
        return "secondary" as const;
      case "no-ledger":
      default:
        return "outline" as const;
    }
  })();
  return (
    <Badge variant={variant} className="text-xs">
      {status}
    </Badge>
  );
}

function FilterChip({
  label,
  count,
  active,
  tone,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  tone?: "success" | "warning";
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={
        active
          ? `rounded-md px-3 py-1 text-xs font-medium border ${
              tone === "success"
                ? "bg-emerald-500/15 border-emerald-500/30 text-emerald-400"
                : tone === "warning"
                  ? "bg-amber-500/15 border-amber-500/30 text-amber-400"
                  : "bg-secondary border-border text-foreground"
            }`
          : "rounded-md px-3 py-1 text-xs font-medium border border-border text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
      }
      data-testid={`filter-${label.toLowerCase()}`}
    >
      {label} ({count})
    </button>
  );
}

function LedgerMissingBanner() {
  return (
    <Card className="border-amber-500/30 bg-amber-500/5">
      <CardContent className="pt-6">
        <div className="text-sm">
          <div className="font-medium text-amber-400 mb-1">
            Invocation ledger not yet wired up
          </div>
          <div className="text-muted-foreground">
            Wire the PostToolUse hook in <code>.claude/settings.local.json</code> to
            start recording skill invocations. Until then, every skill will be
            classified as <code>no-ledger</code>. See{" "}
            <code>docs/invocation-ledger.md</code> for setup.
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function SortableHead({
  children,
  current,
  sortKey,
  dir,
  onClick,
  className,
}: {
  children: React.ReactNode;
  current: SortKey;
  sortKey: SortKey;
  dir: SortDir;
  onClick: () => void;
  className?: string;
}) {
  const isActive = current === sortKey;
  const Icon = !isActive ? ArrowUpDown : dir === "asc" ? ChevronUp : ChevronDown;
  return (
    <TableHead className={className}>
      <button
        onClick={onClick}
        className="inline-flex items-center gap-1 hover:text-foreground transition-colors"
      >
        {children}
        <Icon className="h-3 w-3" />
      </button>
    </TableHead>
  );
}

function compareRows(a: SkillDto, b: SkillDto, key: SortKey): number {
  switch (key) {
    case "name":
      return a.name.localeCompare(b.name);
    case "format":
      return a.format.localeCompare(b.format);
    case "hygiene_status":
      return a.hygiene_status.localeCompare(b.hygiene_status);
    case "invocation_count":
      return a.invocation_count - b.invocation_count;
    case "last_invoked_at": {
      const aTs = a.last_invoked_at ? Date.parse(a.last_invoked_at) : 0;
      const bTs = b.last_invoked_at ? Date.parse(b.last_invoked_at) : 0;
      return aTs - bTs;
    }
  }
}

function formatRelative(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const now = Date.now();
  const diffSec = Math.round((now - t) / 1000);
  if (diffSec < 60) return "just now";
  const diffMin = Math.round(diffSec / 60);
  if (diffMin < 60) return `${diffMin} min ago`;
  const diffHr = Math.round(diffMin / 60);
  if (diffHr < 24) return `${diffHr} hr ago`;
  const diffDay = Math.round(diffHr / 24);
  if (diffDay < 30) return `${diffDay} day${diffDay === 1 ? "" : "s"} ago`;
  const diffMo = Math.round(diffDay / 30);
  if (diffMo < 12) return `${diffMo} mo ago`;
  const diffYr = Math.round(diffMo / 12);
  return `${diffYr} yr ago`;
}

function SkillsSkeleton() {
  return (
    <div className="animate-pulse space-y-6">
      <div className="h-24 rounded-lg bg-muted/50" />
      <div className="h-96 rounded-lg bg-muted/50" />
    </div>
  );
}
