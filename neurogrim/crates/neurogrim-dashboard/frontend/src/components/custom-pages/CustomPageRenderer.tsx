import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { AlertTriangle, CircleSlash, FileText } from "lucide-react";
import type { DashboardPagesConfig } from "@bindings/DashboardPagesConfig";
import type { OverviewResponse } from "@bindings/OverviewResponse";
import type { WidgetSpec } from "@bindings/WidgetSpec";
import { applyHashAnchor } from "@/lib/anchors";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  LayoutEditorToolbar,
  WidgetEditControls,
} from "@/components/overview/LayoutEditor";
import { WidgetDispatch } from "@/components/overview/OverviewPage";
import { hatToQuery, useHat } from "@/lib/useHat";
import { brainApi, useBrainId } from "@/lib/useBrain";
import { makeWidgetSpec } from "@/lib/widget-catalog";
import { widgetSpanClass } from "@/lib/useDashboardLayout";

/**
 * S15-C-6 v2: catchall renderer for operator-defined custom pages,
 * with widget gallery integration so operators compose pages
 * through the UI rather than editing dashboard-pages.json by hand.
 *
 * v1 surfaced a bare list of widget IDs + types — the page schema
 * was wired but operators couldn't fill it. v2 reuses the v3.4
 * `LayoutEditorToolbar` + `WidgetEditControls` + `WidgetDispatch`
 * machinery so a custom page renders + edits identically to the
 * Overview page. The only differences:
 *
 * - Save PUTs to `/api/brains/:id/dashboard-pages/:pageName/layout`
 *   (handled inside `LayoutEditorToolbar` via the `pageId` prop).
 * - No "Reset to default" button (custom pages have no posture-
 *   aware default to fall back to).
 * - The empty state shows a "Customize" call-to-action instead of
 *   a static placeholder.
 *
 * Edit mode requires `--allow-mutations`; if mutations are off the
 * server returns 403 and the toolbar surfaces the error inline.
 */
export function CustomPageRenderer() {
  const brainId = useBrainId();
  const { hat } = useHat();
  const queryHat = hatToQuery(hat);
  const params = useParams({ strict: false }) as { pageName?: string };
  const pageName = params.pageName ?? "";

  const pagesQ = useQuery({
    queryKey: ["dashboard-pages", brainId],
    queryFn: () => fetchPages(brainId),
    refetchInterval: 30_000,
  });

  // Overview data backs the score-gauge / strongest-signals /
  // top-recommendations / identity widgets. Custom pages may include
  // any of these, so fetch the same data the Overview page does.
  const overviewQ = useQuery({
    queryKey: ["overview", brainId, queryHat],
    queryFn: () => fetchOverview(brainId, queryHat),
  });

  const [isEditing, setIsEditing] = useState(false);
  // Local widget draft — edits live here until Save round-trips.
  const [draft, setDraft] = useState<WidgetSpec[] | null>(null);

  // Sync draft from server data when not editing or when the page
  // changes underneath us. Mirrors the Overview page pattern.
  useEffect(() => {
    if (!isEditing && pagesQ.data) {
      const widgets = pagesQ.data.pages[pageName] ?? [];
      setDraft(widgets);
    }
  }, [isEditing, pagesQ.data, pageName]);

  // Reset edit mode if the brain or page changes underneath us.
  useEffect(() => {
    setIsEditing(false);
  }, [brainId, pageName]);

  // Hash-anchor smooth-scroll parity with Overview (v3.5+ deep
  // links). Operators sharing a "see your <widget>" URL across
  // pages should land at the right spot.
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
  }, [brainId, pageName]);

  if (pagesQ.isLoading) {
    return <PageShell pageName={pageName}>Loading…</PageShell>;
  }
  if (pagesQ.error || !pagesQ.data) {
    return (
      <PageShell pageName={pageName}>
        <Card>
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-destructive" />
              Failed to load custom page
            </CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">
            {pagesQ.error instanceof Error
              ? pagesQ.error.message
              : "unknown error"}
          </CardContent>
        </Card>
      </PageShell>
    );
  }

  const serverWidgets = pagesQ.data.pages[pageName];

  if (serverWidgets === undefined) {
    return (
      <PageShell pageName={pageName}>
        <Card data-testid="custom-page-not-found">
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <CircleSlash className="h-5 w-5 text-muted-foreground" />
              Page not found
            </CardTitle>
            <CardDescription>
              No custom page named <code className="text-xs">{pageName}</code>{" "}
              is declared in this Brain's <code className="text-xs">dashboard-pages.json</code>.
            </CardDescription>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">
            Create one via the Settings page, or via{" "}
            <code className="text-xs">
              POST /api/brains/{brainId}/dashboard-pages/{pageName || "&lt;name&gt;"}
            </code>{" "}
            (gated by <code className="text-xs">--allow-mutations</code>).
          </CardContent>
        </Card>
      </PageShell>
    );
  }

  // While editing, render from the draft; otherwise from the
  // server's view. Mirrors the Overview page's edit-mode pattern.
  const widgets: WidgetSpec[] =
    isEditing && draft ? draft : serverWidgets;

  // Mutators on the local draft.
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

  const empty = widgets.length === 0;

  return (
    <PageShell pageName={pageName}>
      <LayoutEditorToolbar
        isEditing={isEditing}
        setIsEditing={setIsEditing}
        widgets={widgets}
        setWidgets={setDraft}
        pageId={pageName}
      />

      {!isEditing && empty && (
        <Card data-testid="custom-page-empty">
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <FileText className="h-5 w-5 text-muted-foreground" />
              Empty page
            </CardTitle>
            <CardDescription>
              This custom page has no widgets yet. Click{" "}
              <strong>Customize</strong> above to add some.
            </CardDescription>
          </CardHeader>
        </Card>
      )}

      {/* The widget grid. Empty in non-edit mode (the placeholder
          card above takes its place); in edit mode renders even
          when empty so the per-widget controls become available
          as soon as the operator adds a widget. */}
      {(!empty || isEditing) && (
        <div
          className="grid grid-cols-1 lg:grid-cols-12 gap-6"
          data-testid="custom-page-widgets"
        >
          {widgets.map((w, idx) => (
            <div
              key={w.id}
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
                    updateAt(idx, {
                      ...w,
                      config: { ...cfg, [field]: value },
                    });
                  }}
                />
              )}
              {/* Render the widget. WidgetDispatch needs an
                  OverviewResponse for some widget types (gauge,
                  identity, signals, recs); skip those when
                  overview hasn't loaded yet, render the rest. */}
              {overviewQ.data ? (
                <WidgetDispatch spec={w} overview={overviewQ.data} />
              ) : (
                <WidgetSkeleton />
              )}
            </div>
          ))}
        </div>
      )}
    </PageShell>
  );
}

function WidgetSkeleton() {
  return (
    <Card className="h-full border-dashed border-muted-foreground/30">
      <CardContent className="pt-6">
        <div className="text-xs text-muted-foreground italic">Loading…</div>
      </CardContent>
    </Card>
  );
}

function PageShell({
  pageName,
  children,
}: {
  pageName: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-6 p-6" data-testid="custom-page">
      <header>
        <h1 className="text-2xl font-bold">{pageName}</h1>
        <p className="text-sm text-muted-foreground mt-1">
          Custom page (operator-defined). Click <strong>Customize</strong> to
          add widgets; manage existing pages via the Settings page's Custom
          pages tab.
        </p>
      </header>
      {children}
    </div>
  );
}

async function fetchPages(brainId: string): Promise<DashboardPagesConfig> {
  const url = brainApi(brainId, "dashboard-pages");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as DashboardPagesConfig;
}

async function fetchOverview(
  brainId: string,
  hat: string | null,
): Promise<OverviewResponse> {
  const base = brainApi(brainId, "overview");
  const url = hat ? `${base}?hat=${encodeURIComponent(hat)}` : base;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as OverviewResponse;
}
