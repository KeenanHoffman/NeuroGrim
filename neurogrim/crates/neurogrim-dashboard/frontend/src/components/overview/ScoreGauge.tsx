import {
  RadialBar,
  RadialBarChart,
  PolarAngleAxis,
  ResponsiveContainer,
} from "recharts";

interface ScoreGaugeProps {
  /** Unified score 0..=100. `null` = all-advisory Brain (no
   *  meaningful unified score; show N/A treatment instead). */
  score: number | null;
  /** Confidence 0..=100. Used for the inner ring. `null` mirrors
   *  the score=null all-advisory case. */
  confidence: number | null;
  /** Total scored domains; included in the centered subtitle. */
  domainCount: number;
}

/**
 * Composite radial-bar gauge showing unified score + confidence at
 * a glance. Recharts has no built-in gauge primitive, so the
 * shadcn-style approach is to build it from RadialBarChart with
 * tuned angles + traffic-light coloring.
 *
 * For all-advisory Brains (score === null), renders a muted
 * "observe-only posture" treatment instead of a 0% gauge —
 * matches the CLI's `agent --prose` framing, where 0/100 would be
 * misleading because nothing is weighted.
 */
export function ScoreGauge({ score, confidence, domainCount }: ScoreGaugeProps) {
  if (score === null) {
    return (
      <div className="flex h-64 flex-col items-center justify-center text-center">
        <div className="text-5xl font-bold text-muted-foreground">N/A</div>
        <div className="mt-2 text-sm text-muted-foreground">
          All-advisory Brain
        </div>
        <div className="mt-1 text-xs text-muted-foreground/70">
          observe-only posture · {domainCount} domains
        </div>
      </div>
    );
  }

  const scoreColor =
    score >= 75 ? "#10b981" : score >= 50 ? "#f59e0b" : "#ef4444"; // emerald / amber / red
  const confidenceShown = confidence ?? 0;

  const data = [
    { name: "score", value: score, fill: scoreColor },
  ];

  return (
    <div className="relative h-64 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <RadialBarChart
          innerRadius="70%"
          outerRadius="95%"
          data={data}
          startAngle={210}
          endAngle={-30}
          barSize={20}
        >
          <PolarAngleAxis
            type="number"
            domain={[0, 100]}
            angleAxisId={0}
            tick={false}
          />
          <RadialBar
            background={{ fill: "hsl(var(--muted))" }}
            dataKey="value"
            cornerRadius={10}
          />
        </RadialBarChart>
      </ResponsiveContainer>
      <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
        <div className="text-5xl font-bold" style={{ color: scoreColor }}>
          {score}
        </div>
        <div className="mt-1 text-xs uppercase tracking-wider text-muted-foreground">
          / 100
        </div>
        <div className="mt-3 text-xs text-muted-foreground">
          confidence {confidenceShown}%
        </div>
      </div>
    </div>
  );
}
