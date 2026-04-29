import { Badge } from "@/components/ui/badge";
import {
  TrendingUp,
  TrendingDown,
  Minus,
  Activity,
  HelpCircle,
} from "lucide-react";

interface TrajectoryBadgeProps {
  /** Wire-format string from the OverviewResponse — one of
   *  "improving" | "degrading" | "stable" | "volatile" | "no-data". */
  trajectoryClass: string;
  /** Velocity (signed; positive = improving). Shown in parentheses
   *  when nonzero. */
  velocity: number;
  /** Number of score-history samples. Below the
   *  `min_samples_for_trend` threshold, classification is "no-data"
   *  even if samples > 0; this count is shown for context. */
  samples: number;
}

/**
 * Compact trajectory indicator: icon + label + velocity. Color
 * variant matches the classification semantically.
 *
 * Recharts isn't used here — a single sparkline would need ≥5
 * historical samples to be useful, and the wire-format already
 * encodes the classification verdict. The sparkline lands in
 * Phase 1.2's domain-detail page where per-domain history
 * matters.
 */
export function TrajectoryBadge({
  trajectoryClass,
  velocity,
  samples,
}: TrajectoryBadgeProps) {
  const { variant, icon: Icon, label } = (() => {
    switch (trajectoryClass) {
      case "improving":
        return { variant: "success" as const, icon: TrendingUp, label: "Improving" };
      case "degrading":
        return { variant: "danger" as const, icon: TrendingDown, label: "Degrading" };
      case "volatile":
        return { variant: "warning" as const, icon: Activity, label: "Volatile" };
      case "stable":
        return { variant: "secondary" as const, icon: Minus, label: "Stable" };
      default:
        return {
          variant: "outline" as const,
          icon: HelpCircle,
          label: "Insufficient data",
        };
    }
  })();

  const velocityText =
    Math.abs(velocity) < 0.05
      ? null
      : ` (${velocity > 0 ? "+" : ""}${velocity.toFixed(1)}/period)`;

  return (
    <div className="flex items-center gap-2">
      <Badge variant={variant} className="gap-1.5">
        <Icon className="h-3 w-3" />
        {label}
      </Badge>
      <span className="text-xs text-muted-foreground">
        {samples} sample{samples === 1 ? "" : "s"}
        {velocityText}
      </span>
    </div>
  );
}
