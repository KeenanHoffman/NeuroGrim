import { useQuery } from "@tanstack/react-query";
import { Glasses } from "lucide-react";
import type { HatsResponse } from "@bindings/HatsResponse";
import type { HatDto } from "@bindings/HatDto";
import { useHat } from "@/lib/useHat";

async function fetchHats(): Promise<HatsResponse> {
  const res = await fetch("/api/hats");
  if (!res.ok) throw new Error(`/api/hats returned ${res.status}`);
  return (await res.json()) as HatsResponse;
}

/**
 * Hat-lens picker — a small `<select>` in the AppShell that lets
 * the operator switch perspective. Selecting a hat updates the
 * context value; pages re-fetch automatically because the hat is
 * part of their TanStack Query keys.
 *
 * When the registry has no hats declared (the synthetic "default"
 * is the only entry returned), the picker collapses to a static
 * label so we don't show a single-option dropdown that does
 * nothing.
 */
export function HatPicker() {
  const { hat, setHat } = useHat();
  const { data, isLoading } = useQuery({
    queryKey: ["hats"],
    queryFn: fetchHats,
    staleTime: 5 * 60_000,
  });

  if (isLoading || !data) {
    return (
      <span
        className="inline-flex items-center gap-2 text-xs text-muted-foreground"
        data-testid="hat-picker-loading"
      >
        <Glasses className="h-3.5 w-3.5" />
        loading hats…
      </span>
    );
  }

  // Only render a real picker when more than the synthetic
  // "default" entry is present.
  const hasDeclaredHats = data.hats.some((h) => !h.is_default);
  if (!hasDeclaredHats) {
    return (
      <span
        className="inline-flex items-center gap-2 text-xs text-muted-foreground"
        title="No hats declared in this Brain's registry."
        data-testid="hat-picker-empty"
      >
        <Glasses className="h-3.5 w-3.5" />
        no hats
      </span>
    );
  }

  const selected =
    data.hats.find((h) => h.name === hat) ??
    data.hats.find((h) => h.is_default)!;

  return (
    <label
      className="inline-flex items-center gap-2 text-xs"
      data-testid="hat-picker"
      title={selected.description || "Hat lens"}
    >
      <Glasses className="h-3.5 w-3.5 text-muted-foreground" />
      <select
        value={hat}
        onChange={(e) => setHat(e.target.value)}
        data-testid="hat-picker-select"
        className="bg-transparent border border-border rounded px-1.5 py-0.5 text-foreground focus:outline-none focus:border-foreground/40"
      >
        {data.hats.map((h: HatDto) => (
          <option key={h.name} value={h.name}>
            {h.is_default ? "default" : h.name}
          </option>
        ))}
      </select>
    </label>
  );
}
