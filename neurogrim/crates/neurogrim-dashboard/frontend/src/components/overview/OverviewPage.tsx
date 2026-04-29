import { useQuery } from "@tanstack/react-query";
import type { OverviewResponse } from "@bindings/OverviewResponse";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { BrainIdentityCard } from "./BrainIdentityCard";
import { ScoreGauge } from "./ScoreGauge";
import { TrajectoryBadge } from "./TrajectoryBadge";
import { StrongestSignals } from "./StrongestSignals";
import { TopRecommendations } from "./TopRecommendations";
import { hatToQuery, useHat } from "@/lib/useHat";

async function fetchOverview(hat: string | null): Promise<OverviewResponse> {
  const url = hat
    ? `/api/overview?hat=${encodeURIComponent(hat)}`
    : "/api/overview";
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as OverviewResponse;
}

export function OverviewPage() {
  const { hat } = useHat();
  const queryHat = hatToQuery(hat);
  const { data, isLoading, error } = useQuery({
    queryKey: ["overview", queryHat],
    queryFn: () => fetchOverview(queryHat),
  });

  if (isLoading) {
    return <OverviewSkeleton />;
  }

  if (error || !data) {
    return (
      <Card className="border-destructive">
        <CardHeader>
          <CardTitle className="text-destructive">
            Failed to load Brain state
          </CardTitle>
        </CardHeader>
        <CardContent>
          <pre className="text-xs">{(error as Error)?.message ?? "Unknown error"}</pre>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-6">
      <BrainIdentityCard overview={data} />

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
        {/* Score gauge — left column, 1/3 width on desktop */}
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Unified Score</CardTitle>
          </CardHeader>
          <CardContent>
            <ScoreGauge
              score={data.score}
              confidence={data.confidence}
              domainCount={data.domain_count}
            />
            <div className="mt-4 border-t border-border pt-4">
              <TrajectoryBadge
                trajectoryClass={data.trajectory_class}
                velocity={data.trajectory_velocity}
                samples={data.trajectory_samples}
              />
            </div>
          </CardContent>
        </Card>

        {/* Strongest signals — middle column */}
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Strongest Signals</CardTitle>
          </CardHeader>
          <CardContent>
            <StrongestSignals signals={data.strongest_signals} />
          </CardContent>
        </Card>

        {/* Top recommendations — right column */}
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">Top Calls to Action</CardTitle>
          </CardHeader>
          <CardContent>
            <TopRecommendations recommendations={data.top_recommendations} />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function OverviewSkeleton() {
  return (
    <div className="space-y-6 animate-pulse">
      <div className="h-32 rounded-lg bg-muted/50" />
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div className="h-80 rounded-lg bg-muted/50" />
        <div className="h-80 rounded-lg bg-muted/50" />
        <div className="h-80 rounded-lg bg-muted/50" />
      </div>
    </div>
  );
}
