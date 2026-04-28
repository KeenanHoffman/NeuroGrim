import { useQuery } from "@tanstack/react-query";

interface HealthResponse {
  ok: boolean;
  registry_path: string;
  version: string;
}

async function fetchHealth(): Promise<HealthResponse> {
  const res = await fetch("/api/health");
  if (!res.ok) throw new Error(`/api/health returned ${res.status}`);
  return res.json();
}

/**
 * Phase 0.3 placeholder app — proves end-to-end:
 *   Rust route → JSON → React fetch → render.
 *
 * Phase 1.x replaces this with the routed app shell + 5 pages.
 */
export default function App() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["health"],
    queryFn: fetchHealth,
  });

  return (
    <div className="min-h-screen bg-background text-foreground p-12">
      <div className="container max-w-3xl mx-auto">
        <h1 className="text-4xl font-bold mb-2">NeuroGrim Dashboard</h1>
        <p className="text-muted-foreground mb-8">
          v3.4 boot — Phase 0.3 end-to-end smoke. Rust API ↔ React, working.
        </p>

        <div className="rounded-lg border border-border bg-card p-6 shadow">
          <h2 className="text-xl font-semibold mb-4">/api/health</h2>
          {isLoading && (
            <p className="text-muted-foreground">Loading…</p>
          )}
          {error && (
            <p className="text-destructive">
              Error: {(error as Error).message}
            </p>
          )}
          {data && (
            <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2 text-sm">
              <dt className="text-muted-foreground">ok</dt>
              <dd className="font-mono">{String(data.ok)}</dd>
              <dt className="text-muted-foreground">version</dt>
              <dd className="font-mono">{data.version}</dd>
              <dt className="text-muted-foreground">registry_path</dt>
              <dd className="font-mono break-all">{data.registry_path}</dd>
            </dl>
          )}
        </div>

        <p className="text-xs text-muted-foreground mt-8">
          Phase 1 will replace this with the real dashboard.
        </p>
      </div>
    </div>
  );
}
