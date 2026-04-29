import { useQuery } from "@tanstack/react-query";
import type { OverviewResponse } from "@bindings/OverviewResponse";
import type { WidgetSpec } from "@bindings/WidgetSpec";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { BrainIdentityCard } from "./BrainIdentityCard";
import { ScoreGauge } from "./ScoreGauge";
import { TrajectoryBadge } from "./TrajectoryBadge";
import { StrongestSignals } from "./StrongestSignals";
import { TopRecommendations } from "./TopRecommendations";
import { DomainCardWidget } from "@/components/widgets/DomainCardWidget";
import { MarkdownNoteWidget } from "@/components/widgets/MarkdownNoteWidget";
import { hatToQuery, useHat } from "@/lib/useHat";
import { brainApi, useBrainId } from "@/lib/useBrain";
import { useDashboardLayout, widgetSpanClass } from "@/lib/useDashboardLayout";

async function fetchOverview(
  brainId: string,
  hat: string | null
): Promise<OverviewResponse> {
  const base = brainApi(brainId, "overview");
  const url = hat ? `${base}?hat=${encodeURIComponent(hat)}` : base;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as OverviewResponse;
}

/**
 * Phase B: the Overview page now renders from a per-Brain layout
 * (the `/api/brains/:id/dashboard-layout` endpoint). Widgets are
 * dispatched by `widget_type`; unknown types render as a
 * placeholder card so a forward-compatible layout (authored
 * against a future widget vocabulary) doesn't blank the page.
 *
 * Each widget reads from the shared `OverviewResponse` (fetched
 * once at the page level) when possible, or self-fetches its own
 * data (e.g., DomainCardWidget). Self-fetching widgets keep
 * their independence so a slow sub-call doesn't stall the rest
 * of the layout.
 */
export function OverviewPage() {
  const brainId = useBrainId();
  const { hat } = useHat();
  const queryHat = hatToQuery(hat);
  const overviewQ = useQuery({
    queryKey: ["overview", brainId, queryHat],
    queryFn: () => fetchOverview(brainId, queryHat),
  });
  const layoutQ = useDashboardLayout();

  if (overviewQ.isLoading || layoutQ.isLoading) {
    return <OverviewSkeleton />;
  }

  if (overviewQ.error || !overviewQ.data) {
    return (
      <Card className="border-destructive">
        <CardHeader>
          <CardTitle className="text-destructive">
            Failed to load Brain state
          </CardTitle>
        </CardHeader>
        <CardContent>
          <pre className="text-xs">
            {(overviewQ.error as Error)?.message ?? "Unknown error"}
          </pre>
        </CardContent>
      </Card>
    );
  }

  // Layout fetch failure is non-fatal — render the canonical
  // weighted layout as a fallback so the page stays usable even
  // if the layout endpoint is misbehaving.
  const layout = layoutQ.data;
  const widgets: WidgetSpec[] = layout?.widgets ?? FALLBACK_WIDGETS;

  return (
    <div className="space-y-3">
      {layout?.is_default && (
        <div
          className="text-xs text-muted-foreground/80 flex items-center justify-between"
          data-testid="default-layout-banner"
        >
          <span>
            Showing the default layout for this Brain. Save a custom one to
            <code className="mx-1 rounded bg-muted px-1.5 py-0.5">
              .claude/brain/dashboard-layout.json
            </code>
            to override.
          </span>
        </div>
      )}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
        {widgets.map((w) => (
          <div
            key={w.id}
            className={`${widgetSpanClass(w.size)} col-span-1`}
            data-widget-type={w.widget_type}
            data-widget-id={w.id}
          >
            <WidgetDispatch spec={w} overview={overviewQ.data} />
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Renders the right widget for the given spec. Unknown widget
 * types render as a placeholder so a forward-compatible layout
 * doesn't blank the page.
 */
function WidgetDispatch({
  spec,
  overview,
}: {
  spec: WidgetSpec;
  overview: OverviewResponse;
}) {
  switch (spec.widget_type) {
    case "identity":
      return <BrainIdentityCard overview={overview} />;
    case "score-gauge":
      return (
        <Card className="h-full">
          <CardHeader>
            <CardTitle className="text-lg">
              {spec.title ?? "Unified Score"}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <ScoreGauge
              score={overview.score}
              confidence={overview.confidence}
              domainCount={overview.domain_count}
            />
            <div className="mt-4 border-t border-border pt-4">
              <TrajectoryBadge
                trajectoryClass={overview.trajectory_class}
                velocity={overview.trajectory_velocity}
                samples={overview.trajectory_samples}
              />
            </div>
          </CardContent>
        </Card>
      );
    case "strongest-signals":
      return (
        <Card className="h-full">
          <CardHeader>
            <CardTitle className="text-lg">
              {spec.title ?? "Strongest Signals"}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <StrongestSignals signals={overview.strongest_signals} />
          </CardContent>
        </Card>
      );
    case "top-recommendations":
      return (
        <Card className="h-full">
          <CardHeader>
            <CardTitle className="text-lg">
              {spec.title ?? "Top Calls to Action"}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <TopRecommendations recommendations={overview.top_recommendations} />
          </CardContent>
        </Card>
      );
    case "domain-card": {
      const cfg = spec.config as { domain?: string };
      if (!cfg?.domain) {
        return <UnknownWidget reason="domain-card requires config.domain" />;
      }
      return <DomainCardWidget title={spec.title} domain={cfg.domain} />;
    }
    case "markdown-note": {
      const cfg = spec.config as { content?: string };
      if (!cfg?.content) {
        return (
          <UnknownWidget reason="markdown-note requires config.content" />
        );
      }
      return <MarkdownNoteWidget title={spec.title} content={cfg.content} />;
    }
    default:
      return (
        <UnknownWidget
          reason={`Unknown widget type: ${spec.widget_type}`}
        />
      );
  }
}

function UnknownWidget({ reason }: { reason: string }) {
  return (
    <Card className="h-full border-dashed border-muted-foreground/40">
      <CardContent className="pt-6">
        <div className="text-xs text-muted-foreground italic">{reason}</div>
      </CardContent>
    </Card>
  );
}

/**
 * Hard-coded fallback when the layout endpoint fails entirely.
 * Mirrors the default weighted layout. Kept in TS rather than
 * fetched so the page can render even if /api/brains is down.
 */
const FALLBACK_WIDGETS: WidgetSpec[] = [
  {
    id: "fb-identity",
    widget_type: "identity",
    size: "full",
    title: null,
    config: {},
  },
  {
    id: "fb-gauge",
    widget_type: "score-gauge",
    size: "third",
    title: null,
    config: {},
  },
  {
    id: "fb-signals",
    widget_type: "strongest-signals",
    size: "third",
    title: null,
    config: {},
  },
  {
    id: "fb-recs",
    widget_type: "top-recommendations",
    size: "third",
    title: null,
    config: {},
  },
];

function OverviewSkeleton() {
  return (
    <div className="space-y-6 animate-pulse">
      <div className="h-32 rounded-lg bg-muted/50" />
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div className="h-80 rounded-lg bg-muted/50" />
        <div className="h-80 rounded-lg bg-muted/50" />
        <div className="h-80 rounded-lg bg-muted/50" />
      </div>
    </div>
  );
}
