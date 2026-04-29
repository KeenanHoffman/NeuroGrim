import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { Brain } from "lucide-react";
import type { BrainsListResponse } from "@bindings/BrainsListResponse";
import type { BrainListItemDto } from "@bindings/BrainListItemDto";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

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
 * Built on the Radix-based Select primitive (components/ui/select)
 * rather than a native `<select>`: Chromium's native combobox
 * panel ignores option:hover styling and uses a bright OS-default
 * highlight that fights the dashboard's muted palette. The Radix
 * portal renders as plain DOM so every state honors our CSS.
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

  const handleChange = (id: string) => {
    navigate({ to: "/brains/$brainId", params: { brainId: id } });
  };

  return (
    <div className="text-xs space-y-1" data-testid="brain-selector">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <Brain className="h-3.5 w-3.5" />
        Brain
      </div>
      <Select value={currentId ?? data.self_id} onValueChange={handleChange}>
        <SelectTrigger
          className="w-full text-sm"
          data-testid="brain-selector-select"
          title="Switch the dashboard to view a different Brain in the federation tree."
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {data.brains.map((b: BrainListItemDto) => (
            <SelectItem key={b.id} value={b.id}>
              <span className="font-mono text-muted-foreground/70">
                {indent(b.depth)}
              </span>
              {b.display_name}
              {b.id === data.self_id && (
                <span className="ml-1.5 text-muted-foreground/70">(host)</span>
              )}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

function indent(depth: number): string {
  if (depth === 0) return "";
  // Use NBSPs so the indent is preserved verbatim in the rendered
  // text (browsers collapse normal spaces).
  return "  ".repeat(depth) + "↳ ";
}
