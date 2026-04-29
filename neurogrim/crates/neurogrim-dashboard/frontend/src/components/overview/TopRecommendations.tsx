import type { RecommendationDto } from "@bindings/RecommendationDto";
import { Badge } from "@/components/ui/badge";

interface TopRecommendationsProps {
  recommendations: RecommendationDto[];
}

/**
 * Top calls to action — what should an operator (or agent) do
 * next? Shows up to 3 entries, each with the domain it pertains
 * to, the gate tier badge, and a one-line description. The
 * `command` is rendered as a copyable code line — Phase 2.x can
 * make this a click-to-execute button (gated behind --allow-mutations).
 */
export function TopRecommendations({ recommendations }: TopRecommendationsProps) {
  if (recommendations.length === 0) {
    return (
      <div className="text-sm text-muted-foreground">
        Nothing pressing. The Brain has no urgent actions queued.
      </div>
    );
  }

  return (
    <ol className="space-y-4">
      {recommendations.map((r, i) => (
        <li key={`${r.domain}:${r.gate}:${i}`} className="space-y-1.5">
          <div className="flex items-center gap-2 text-sm">
            <Badge variant="outline" className="font-mono text-xs">
              {r.domain}
            </Badge>
            {r.gate && (
              <Badge variant="secondary" className="text-xs">
                {r.gate}
              </Badge>
            )}
          </div>
          {r.description && (
            <div className="text-sm text-foreground/90">{r.description}</div>
          )}
          {r.command && (
            <code className="block w-full overflow-x-auto rounded bg-muted/70 px-2 py-1 text-xs font-mono text-muted-foreground">
              → {r.command}
            </code>
          )}
        </li>
      ))}
    </ol>
  );
}
