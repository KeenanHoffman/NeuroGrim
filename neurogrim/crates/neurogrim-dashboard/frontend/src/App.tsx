import { OverviewPage } from "@/components/overview/OverviewPage";

/**
 * Phase 1.1: single-page render of the Overview. Phase 1.5 wraps
 * this in TanStack Router with the rest of the pages (Domains,
 * Federation, Skills) and a navigation shell.
 */
export default function App() {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <header className="border-b border-border">
        <div className="container max-w-7xl mx-auto px-6 py-4 flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold">NeuroGrim Dashboard</h1>
            <p className="text-xs text-muted-foreground">
              v3.4 Phase 1.1 — Overview
            </p>
          </div>
        </div>
      </header>
      <main className="container max-w-7xl mx-auto px-6 py-8">
        <OverviewPage />
      </main>
    </div>
  );
}
