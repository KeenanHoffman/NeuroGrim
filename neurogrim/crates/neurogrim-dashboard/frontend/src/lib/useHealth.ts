import { useQuery } from "@tanstack/react-query";
import type { HealthResponse } from "@bindings/HealthResponse";

/**
 * Fetch `/api/health` once on mount. The result is treated as
 * effectively immutable — the dashboard restarts when
 * `--allow-mutations` flips, so `mutations_allowed` doesn't change
 * during a session.
 *
 * Returned as a normalized object so component callers don't have
 * to handle the loading/error case for what is essentially a
 * boolean read. While the request is in flight, `mutationsAllowed`
 * is `false` (safe default — buttons stay hidden).
 */
export function useHealth(): {
  loading: boolean;
  mutationsAllowed: boolean;
  version: string;
  registryPath: string;
} {
  const { data, isLoading } = useQuery({
    queryKey: ["health"],
    queryFn: async () => {
      const res = await fetch("/api/health");
      if (!res.ok) throw new Error(`/api/health returned ${res.status}`);
      return (await res.json()) as HealthResponse;
    },
    staleTime: Infinity,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
  });
  return {
    loading: isLoading,
    mutationsAllowed: data?.mutations_allowed ?? false,
    version: data?.version ?? "",
    registryPath: data?.registry_path ?? "",
  };
}
