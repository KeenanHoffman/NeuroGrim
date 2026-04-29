import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { OverviewResponse } from "@bindings/OverviewResponse";
import type { WidgetSpec } from "@bindings/WidgetSpec";
import { applyHashAnchor } from "@/lib/anchors";
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
import { PortsPanelWidget } from "@/components/widgets/PortsPanelWidget";
import { makeWidgetSpec } from "@/lib/widget-catalog";
import {
  LayoutEditorToolbar,
  WidgetEditControls,
} from "./LayoutEditor";
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
 * Phase B: the Overview page renders from a per-Brain layout
 * (the `/api/brains/:id/dashboard-layout` endpoint). Widgets are
 * dispatched by `widget_type`; unknown types render as a
 * placeholder card so a forward-compatible layout (authored
 * against a future widget vocabulary) doesn't blank the page.
 *
 * Edit mode (added in v3.4 publish-prep slice 2):
 * - Operator clicks "Customize" → toolbar appears with Save /
 *   Reset / Done + an "Add widget" picker.
 * - Each widget gets per-instance controls: ↑ ↓ ✕ + size, plus
 *   inline config editors for `domain-card` and `markdown-note`.
 * - Edits live in local state until Save; navigating away
 *   without saving discards changes.
 * - Save → PUT, server fires LayoutChanged via SSE, the layout
 *   query invalidates and re-fetches.
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

  const [isEditing, setIsEditing] = useState(false);
  // Local widget state for in-flight edits. When not editing, this
  // mirrors the server's layout. When editing, edits go here and
  // are PUT'd on Save.
  const [draft, setDraft] = useState<WidgetSpec[] | null>(null);

  // Sync draft from server data when not editing or when brain changes.
  useEffect(() => {
    if (!isEditing && layoutQ.data) {
      setDraft(layoutQ.data.widgets);
    }
  }, [isEditing, layoutQ.data]);

  // Reset edit mode if the brain changes underneath us.
  useEffect(() => {
    setIsEditing(false);
  }, [brainId]);

  // v3.5.0 anchor links: when a `#widget-<id>` hash is in the URL
  // on first paint, smooth-scroll to the matching widget and
  // pulse-highlight it briefly. Re-runs on hashchange so an
  // operator clicking a deep-link from elsewhere on the same page
  // also triggers the scroll. The 100ms delay gives the layout
  // grid a chance to lay out before we measure scroll positions.
  useEffect(() => {
    let mounted = true;
    const trigger = () => {
      if (!mounted) return;
      window.setTimeout(() => {
        if (mounted) applyHashAnchor(window.location.hash);
      }, 100);
    };
    trigger();
    window.addEventListener("hashchange", trigger);
    return () => {
      mounted = false;
      window.removeEventListener("hashchange", trigger);
    };
  }, [brainId]);

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

  const layout = layoutQ.data;
  const serverWidgets = layout?.widgets ?? FALLBACK_WIDGETS;
  // While editing, the page renders from `draft`; otherwise from
  // the server's layout directly.
  const widgets: WidgetSpec[] = isEditing && draft ? draft : serverWidgets;

  // Mutators for edit-mode operations on the local draft.
  const updateAt = (idx: number, next: WidgetSpec) => {
    if (!draft) return;
    const copy = [...draft];
    copy[idx] = next;
    setDraft(copy);
  };
  const removeAt = (idx: number) => {
    if (!draft) return;
    setDraft(draft.filter((_, i) => i !== idx));
  };
  const moveAt = (idx: number, delta: -1 | 1) => {
    if (!draft) return;
    const next = idx + delta;
    if (next < 0 || next >= draft.length) return;
    const copy = [...draft];
    [copy[idx], copy[next]] = [copy[next], copy[idx]];
    setDraft(copy);
  };

  return (
    <div className="space-y-3">
      <LayoutEditorToolbar
        isEditing={isEditing}
        setIsEditing={setIsEditing}
        widgets={widgets}
        setWidgets={setDraft}
      />

      {!isEditing && layout?.is_default && (
        <div
          className="text-xs text-muted-foreground/80 flex items-center justify-between"
          data-testid="default-layout-banner"
        >
          <span>
            Showing the default layout for this Brain.{" "}
            <button
              onClick={() => setIsEditing(true)}
              className="underline-offset-4 hover:underline text-foreground"
              data-testid="customize-from-banner"
            >
              Customize
            </button>{" "}
            to author your own.
          </span>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
        {widgets.map((w, idx) => (
          <div
            key={w.id}
            // v3.5.0 anchor links: id="widget-<spec.id>" lets agents
            // link directly via /brains/<id>/#widget-<id>. See
            // `lib/anchors.ts` for the URL builder.
            id={`widget-${w.id}`}
            className={`${widgetSpanClass(w.size)} col-span-1 scroll-mt-4`}
            data-widget-type={w.widget_type}
            data-widget-id={w.id}
          >
            {isEditing && (
              <WidgetEditControls
                index={idx}
                widgetCount={widgets.length}
                widget={w}
                onMove={(delta) => moveAt(idx, delta)}
                onResize={(size) => updateAt(idx, { ...w, size })}
                onRemove={() => removeAt(idx)}
                onReset={() => {
                  // Replace the widget's size + title + config with
                  // the type's defaults; preserve `id` so React keys
                  // and any anchor links pointing at this slot stay
                  // stable.
                  const fresh = makeWidgetSpec(w.widget_type, w.id);
                  updateAt(idx, fresh);
                }}
                onConfigChange={(field, value) => {
                  if (field === "title") {
                    updateAt(idx, { ...w, title: value || null });
                    return;
                  }
                  const cfg =
                    typeof w.config === "object" && w.config !== null
                      ? (w.config as Record<string, unknown>)
                      : {};
                  if (field === "count") {
                    const n = parseInt(value, 10);
                    if (!isNaN(n) && n > 0) {
                      updateAt(idx, {
                        ...w,
                        config: { ...cfg, count: n },
                      });
                    }
                    return;
                  }
                  updateAt(idx, { ...w, config: { ...cfg, [field]: value } });
                }}
              />
            )}
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
 *
 * Exported so the v3.5+ `WidgetGallery` can render live previews
 * of every catalog entry against the current Brain's data
 * without duplicating the dispatch wiring.
 */
export function WidgetDispatch({
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
    case "ports-panel":
      return <PortsPanelWidget title={spec.title} />;
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
