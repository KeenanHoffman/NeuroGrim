import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { ArrowRight } from "lucide-react";
import type { DomainDetailResponse } from "@bindings/DomainDetailResponse";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { brainApi, useBrainId } from "@/lib/useBrain";

interface DomainCardWidgetProps {
  /** Title override from the layout spec; falls back to display_name. */
  title?: string | null;
  /** Domain id from the widget's `config.domain` field. */
  domain: string;
}

/**
 * Single-domain stat card. Used heavily on the all-advisory default
 * layout where each `child-*` domain gets its own card showing the
 * live A2A-pulled score.
 *
 * Self-fetches its data — keeps each widget independent so a slow
 * sub-fetch doesn't block the whole layout.
 *
 * Click the card title to drill into the domain detail page; for
 * `child-*` domains, the detail page can take you further into
 * that child's full dashboard via the AppShell's brain switcher.
 */
export function DomainCardWidget({ title, domain }: DomainCardWidgetProps) {
  const brainId = useBrainId();
  const navigate = useNavigate();
  const { data, isLoading, error } = useQuery({
    queryKey: ["domain-detail", brainId, domain, null /* no hat */],
    queryFn: async (): Promise<DomainDetailResponse> => {
      const url = brainApi(brainId, `domains/${encodeURIComponent(domain)}`);
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${url} returned ${res.status}`);
      return (await res.json()) as DomainDetailResponse;
    },
    staleTime: 10_000,
  });

  if (isLoading) {
    return (
      <Card className="h-full animate-pulse">
        <CardHeader>
          <div className="h-5 w-24 rounded bg-muted/50" />
        </CardHeader>
        <CardContent>
          <div className="h-12 w-16 rounded bg-muted/50" />
        </CardContent>
      </Card>
    );
  }

  if (error || !data) {
    return (
      <Card className="h-full border-destructive/50">
        <CardHeader>
          <CardTitle className="text-base">{title ?? domain}</CardTitle>
          <CardDescription className="font-mono text-xs">{domain}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="text-xs text-muted-foreground">
            {(error as Error)?.message ?? "Unable to load."}
          </div>
        </CardContent>
      </Card>
    );
  }

  const score = data.effective_score;
  const colorClass =
    score >= 75
      ? "text-emerald-400"
      : score >= 50
        ? "text-amber-400"
        : "text-red-400";

  return (
    <Card
      className="h-full cursor-pointer hover:border-foreground/30 transition-colors"
      onClick={() =>
        navigate({
          to: "/brains/$brainId/domains/$name",
          params: { brainId, name: domain },
        })
      }
    >
      <CardHeader className="pb-2">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <CardTitle className="text-base truncate">
              {title ?? data.display_name}
            </CardTitle>
            <CardDescription className="font-mono text-xs truncate">
              {domain}
            </CardDescription>
          </div>
          <ArrowRight className="h-4 w-4 text-muted-foreground/60 shrink-0" />
        </div>
      </CardHeader>
      <CardContent className="space-y-2">
        <div className="flex items-baseline gap-2">
          <div className={`text-3xl font-semibold ${colorClass}`}>{score}</div>
          <div className="text-xs text-muted-foreground">/ 100</div>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
          {data.weight === 0 ? (
            <Badge variant="outline" className="text-[10px]">
              advisory
            </Badge>
          ) : (
            <Badge variant="secondary" className="text-[10px]">
              weight {data.weight.toFixed(2)}
            </Badge>
          )}
          <span>{data.confidence}% conf</span>
          {data.trajectory_class !== "no-data" && (
            <span>· {data.trajectory_class}</span>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
