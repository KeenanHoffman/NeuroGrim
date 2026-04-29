import { useQuery } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { AlertTriangle, CircleSlash, FileText } from "lucide-react";
import type { DashboardPagesConfig } from "@bindings/DashboardPagesConfig";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { brainApi, useBrainId } from "@/lib/useBrain";

/**
 * S15-C-6 v1: catchall renderer for operator-defined custom pages.
 *
 * Reads the v2 `dashboard-pages.json` config; renders the widget
 * list for the page named in the URL. Empty list → onboarding
 * placeholder. Unknown page name → 404-ish placeholder.
 *
 * **v1 scope:** the WidgetGrid is intentionally minimal — it
 * surfaces widget IDs + types as a list. Full widget rendering
 * (the actual gauge/chart/markdown/etc. components) reuses the
 * existing widget-catalog from the Overview page, which lands in
 * v2 once the dynamic-route + widget-gallery integration is
 * fleshed out.
 */
export function CustomPageRenderer() {
  const brainId = useBrainId();
  const params = useParams({ strict: false }) as { pageName?: string };
  const pageName = params.pageName ?? "";

  const { data, isLoading, error } = useQuery({
    queryKey: ["dashboard-pages", brainId],
    queryFn: () => fetchPages(brainId),
    refetchInterval: 30_000,
  });

  if (isLoading) {
    return <PageShell pageName={pageName}>Loading…</PageShell>;
  }
  if (error || !data) {
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
            {error instanceof Error ? error.message : "unknown error"}
          </CardContent>
        </Card>
      </PageShell>
    );
  }

  const widgets = data.pages[pageName];

  if (!widgets) {
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

  if (widgets.length === 0) {
    return (
      <PageShell pageName={pageName}>
        <Card data-testid="custom-page-empty">
          <CardHeader>
            <CardTitle className="text-lg flex items-center gap-2">
              <FileText className="h-5 w-5 text-muted-foreground" />
              Empty page
            </CardTitle>
            <CardDescription>
              This custom page has no widgets yet. Widget gallery
              integration (Add Widget UI) ships in S15-C-6 v2.
            </CardDescription>
          </CardHeader>
        </Card>
      </PageShell>
    );
  }

  return (
    <PageShell pageName={pageName}>
      <Card data-testid="custom-page-widgets">
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <FileText className="h-5 w-5" />
            Widgets ({widgets.length})
          </CardTitle>
          <CardDescription>
            v1 surfaces widget IDs + types only. Full rendering reuses
            the Overview page's widget catalog in v2.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <ul className="space-y-2 text-sm">
            {widgets.map((w) => (
              <li
                key={w.id}
                className="flex items-center gap-2 font-mono"
                data-testid={`custom-widget-${w.id}`}
              >
                <span className="text-muted-foreground">{w.widget_type}</span>
                <span className="text-xs text-muted-foreground">·</span>
                <span>{w.id}</span>
                {w.size && (
                  <span className="text-xs text-muted-foreground">
                    [{w.size}]
                  </span>
                )}
              </li>
            ))}
          </ul>
        </CardContent>
      </Card>
    </PageShell>
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
          Custom page (operator-defined). Manage via the Settings page's
          Custom pages tab.
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
