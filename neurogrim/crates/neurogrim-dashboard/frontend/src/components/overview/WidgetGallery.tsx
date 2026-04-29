import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { Plus, X } from "lucide-react";
import type { WidgetSpec } from "@bindings/WidgetSpec";
import type { OverviewResponse } from "@bindings/OverviewResponse";
import type { DomainsListResponse } from "@bindings/DomainsListResponse";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button-ish";
import { brainApi, useBrainId } from "@/lib/useBrain";
import { hatToQuery, useHat } from "@/lib/useHat";
import {
  WIDGET_CATALOG,
  type WidgetCatalogEntry,
} from "@/lib/widget-catalog";
import { WidgetDispatch } from "./OverviewPage";

/**
 * v3.5.0 — visual gallery of every widget type, rendered live
 * against the current Brain's data so an operator can pick by
 * appearance instead of by name.
 *
 * Rendering strategy:
 * - Fetch the same `/api/brains/:id/overview` payload the
 *   Overview page consumes; widgets that read from it
 *   (identity, score-gauge, strongest-signals,
 *   top-recommendations) render normally.
 * - Fetch the Brain's domain list so the `domain-card` preview
 *   gets a real domain name instead of an empty string. When the
 *   Brain has no domains, the gallery shows a helpful "preview
 *   unavailable" message but the operator can still add the
 *   widget and configure it.
 * - The other self-fetching widgets (`ports-panel`,
 *   `domain-card` once a domain is supplied) hit their own
 *   endpoints; gallery just renders them.
 *
 * Modal lifecycle: backdrop click and Escape both close.
 */
export function WidgetGallery({
  onClose,
  onPick,
}: {
  onClose: () => void;
  onPick: (type: string) => void;
}) {
  // Escape closes — keeps keyboard navigation working without a
  // full focus trap (overkill for v1).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const brainId = useBrainId();
  const { hat } = useHat();
  const queryHat = hatToQuery(hat);

  const overviewQ = useQuery({
    queryKey: ["overview", brainId, queryHat],
    queryFn: async () => {
      const base = brainApi(brainId, "overview");
      const url = queryHat
        ? `${base}?hat=${encodeURIComponent(queryHat)}`
        : base;
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${url} returned ${res.status}`);
      return (await res.json()) as OverviewResponse;
    },
  });

  const domainsQ = useQuery({
    queryKey: ["domains-list", brainId],
    queryFn: async () => {
      const url = brainApi(brainId, "domains");
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${url} returned ${res.status}`);
      return (await res.json()) as DomainsListResponse;
    },
    staleTime: 30_000,
  });

  const sampleDomain = domainsQ.data?.domains?.[0]?.name ?? "";
  const isLoading = overviewQ.isLoading || domainsQ.isLoading;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
      onClick={onClose}
      data-testid="widget-gallery"
      role="dialog"
      aria-label="Browse widgets"
    >
      <div
        className="bg-background border border-border rounded-lg shadow-xl max-w-5xl w-full max-h-[90vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between p-4 border-b border-border">
          <div>
            <h2 className="text-lg font-semibold">Browse widgets</h2>
            <p className="text-xs text-muted-foreground mt-0.5">
              Live previews rendered against this Brain's data. Click a
              card to add the widget to your layout.
            </p>
          </div>
          <Button
            onClick={onClose}
            variant="ghost"
            size="sm"
            data-testid="close-widget-gallery"
            aria-label="Close gallery"
            className="h-8 w-8 p-0"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>

        <div className="overflow-y-auto p-4 flex-1">
          {isLoading && (
            <div className="text-xs text-muted-foreground">
              Loading previews…
            </div>
          )}
          {overviewQ.error && (
            <div className="text-xs text-destructive mb-3">
              Failed to load overview:{" "}
              {(overviewQ.error as Error).message}
            </div>
          )}
          {overviewQ.data && (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {WIDGET_CATALOG.map((entry) => (
                <PreviewCard
                  key={entry.value}
                  entry={entry}
                  overview={overviewQ.data}
                  sampleDomain={sampleDomain}
                  onAdd={() => onPick(entry.value)}
                />
              ))}
            </div>
          )}
        </div>

        <div className="px-4 py-3 border-t border-border text-[11px] text-muted-foreground bg-muted/20">
          Previews use real data — what you see here is what'll appear on
          the Overview after you add it. Widgets needing per-instance
          config (e.g. <code className="font-mono">domain-card</code>)
          will need configuring before they render meaningfully.
        </div>
      </div>
    </div>
  );
}

function buildSampleSpec(
  entry: WidgetCatalogEntry,
  sampleDomain: string
): WidgetSpec {
  const cfg = { ...(entry.previewConfig ?? entry.defaultConfig) };
  // domain-card defaults to "" — override with a real domain name
  // so the preview actually renders against this Brain's data.
  if (entry.value === "domain-card" && sampleDomain) {
    (cfg as Record<string, unknown>).domain = sampleDomain;
  }
  return {
    id: `gallery-${entry.value}`,
    widget_type: entry.value,
    size: "third",
    title: null,
    config: cfg,
  };
}

function PreviewCard({
  entry,
  overview,
  sampleDomain,
  onAdd,
}: {
  entry: WidgetCatalogEntry;
  overview: OverviewResponse;
  sampleDomain: string;
  onAdd: () => void;
}) {
  const sampleSpec = buildSampleSpec(entry, sampleDomain);
  const previewBlocked =
    entry.value === "domain-card" && !sampleDomain;

  return (
    <Card data-testid={`gallery-card-${entry.value}`}>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm">{entry.label}</CardTitle>
        <CardDescription className="text-xs leading-snug">
          {entry.description}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div
          className="rounded border border-border bg-muted/20 p-2 max-h-72 overflow-hidden relative"
          data-testid={`gallery-preview-${entry.value}`}
        >
          {previewBlocked ? (
            <div className="text-xs text-muted-foreground p-4 text-center">
              This Brain has no declared domains yet — preview
              unavailable. You can still add the widget and configure
              its <code className="font-mono">domain</code> later.
            </div>
          ) : (
            <div className="pointer-events-none">
              <WidgetDispatch spec={sampleSpec} overview={overview} />
            </div>
          )}
        </div>
        <div className="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
          <span>
            default size:{" "}
            <code className="font-mono">{entry.defaultSize}</code>
          </span>
          <Button
            onClick={onAdd}
            size="sm"
            data-testid={`gallery-add-${entry.value}`}
          >
            <Plus className="h-3 w-3 mr-1.5" />
            Add to layout
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
