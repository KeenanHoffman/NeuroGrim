import { useQuery } from "@tanstack/react-query";
import { Glasses } from "lucide-react";
import type { HatsResponse } from "@bindings/HatsResponse";
import type { HatDto } from "@bindings/HatDto";
import { useHat } from "@/lib/useHat";
import { brainApi, useBrainId } from "@/lib/useBrain";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

async function fetchHats(brainId: string): Promise<HatsResponse> {
  const url = brainApi(brainId, "hats");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
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
  const brainId = useBrainId();
  const { hat, setHat } = useHat();
  const { data, isLoading } = useQuery({
    queryKey: ["hats", brainId],
    queryFn: () => fetchHats(brainId),
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
    <div
      className="text-xs space-y-1"
      data-testid="hat-picker"
      title={selected.description || "Hat lens"}
    >
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <Glasses className="h-3.5 w-3.5" />
        Hat
      </div>
      <Select value={hat} onValueChange={setHat}>
        <SelectTrigger
          className="w-full text-sm"
          data-testid="hat-picker-select"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {data.hats.map((h: HatDto) => (
            <SelectItem key={h.name} value={h.name}>
              {h.is_default ? "default" : h.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
