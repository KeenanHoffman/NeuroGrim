import { useQuery } from "@tanstack/react-query";
import type { DashboardLayoutResponse } from "@bindings/DashboardLayoutResponse";
import { brainApi, useBrainId } from "@/lib/useBrain";

/**
 * Fetches the per-brain dashboard layout. Falls back to a posture-
 * aware default on the server side when no `dashboard-layout.json`
 * exists; the response always carries a `widgets` array, never null.
 */
export function useDashboardLayout() {
  const brainId = useBrainId();
  return useQuery({
    queryKey: ["dashboard-layout", brainId],
    queryFn: async (): Promise<DashboardLayoutResponse> => {
      const url = brainApi(brainId, "dashboard-layout");
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${url} returned ${res.status}`);
      return (await res.json()) as DashboardLayoutResponse;
    },
    staleTime: 30_000,
  });
}

/**
 * Map a widget size token to a Tailwind grid-column-span class.
 * Layout uses a 12-column grid; widgets stack on small screens.
 */
export function widgetSpanClass(size: string): string {
  switch (size) {
    case "full":
      return "lg:col-span-12";
    case "half":
      return "lg:col-span-6";
    case "third":
      return "lg:col-span-4";
    case "quarter":
      return "lg:col-span-3";
    default:
      // Unknown size → full width as a safe fallback.
      return "lg:col-span-12";
  }
}
