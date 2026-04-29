import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { Brain, ChevronDown } from "lucide-react";
import type { BrainsListResponse } from "@bindings/BrainsListResponse";
import type { BrainListItemDto } from "@bindings/BrainListItemDto";

async function fetchBrains(): Promise<BrainsListResponse> {
  const res = await fetch("/api/brains");
  if (!res.ok) throw new Error(`/api/brains returned ${res.status}`);
  return (await res.json()) as BrainsListResponse;
}

/**
 * Sidebar Brain switcher — lists every Brain in the federation tree
 * with a tree-style indent so the host / direct children /
 * grandchildren are visually distinguishable. Selecting one
 * navigates to `/brains/<id>/` so the user lands on that Brain's
 * Overview.
 *
 * Renders a static label until the brain list resolves; this avoids
 * a flash of "no Brain" before the index redirect lands.
 */
export function BrainSelector() {
  const navigate = useNavigate();
  const params = useParams({ strict: false }) as { brainId?: string };
  const currentId = params.brainId;
  const { data, isLoading } = useQuery({
    queryKey: ["brains"],
    queryFn: fetchBrains,
    staleTime: 5 * 60_000,
  });

  if (isLoading || !data) {
    return (
      <span
        className="inline-flex items-center gap-2 text-xs text-muted-foreground"
        data-testid="brain-selector-loading"
      >
        <Brain className="h-3.5 w-3.5" />
        loading brains…
      </span>
    );
  }

  // Group entries by parent so we can render a depth-aware indent.
  // The list is already sorted self-first then by depth + id.
  return (
    <label
      className="block text-xs"
      data-testid="brain-selector"
      title="Switch the dashboard to view a different Brain in the federation tree."
    >
      <div className="flex items-center gap-1.5 text-muted-foreground mb-1">
        <Brain className="h-3.5 w-3.5" />
        Brain
        <ChevronDown className="h-3 w-3 opacity-60" />
      </div>
      <select
        value={currentId ?? data.self_id}
        onChange={(e) =>
          navigate({
            to: "/brains/$brainId",
            params: { brainId: e.target.value },
          })
        }
        data-testid="brain-selector-select"
        // Belt-and-suspenders: inline `color-scheme: dark light` lets
        // the browser's native dropdown panel match either theme, so
        // option text stays readable even if the page-level CSS rule
        // hasn't been applied yet (e.g., a stale cached bundle from
        // an older build).
        style={{ colorScheme: "dark light" }}
        className="w-full bg-transparent border border-border rounded px-2 py-1 text-foreground text-sm focus:outline-none focus:border-foreground/40"
      >
        {data.brains.map((b: BrainListItemDto) => (
          <option key={b.id} value={b.id}>
            {indent(b.depth)}
            {b.display_name}
            {b.id === data.self_id ? " (host)" : ""}
          </option>
        ))}
      </select>
    </label>
  );
}

function indent(depth: number): string {
  if (depth === 0) return "";
  // Use NBSPs so the indent survives in <option>'s text rendering;
  // a regular space gets collapsed by the browser.
  return "  ".repeat(depth) + "↳ ";
}
