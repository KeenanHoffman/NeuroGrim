import type { DomainSignalDto } from "@bindings/DomainSignalDto";

interface StrongestSignalsProps {
  signals: DomainSignalDto[];
}

/**
 * Top 3 highest-effective-score domains. Each row shows the
 * humanized name, effective score (color-coded), and confidence.
 * For an all-advisory Brain this is "what's looking healthiest at
 * a glance" rather than "what's driving the score" — the framing
 * is identical to CLI's `agent --prose` Strongest Signals section.
 */
export function StrongestSignals({ signals }: StrongestSignalsProps) {
  if (signals.length === 0) {
    return (
      <div className="text-sm text-muted-foreground">
        No domains scored yet.
      </div>
    );
  }

  return (
    <ul className="space-y-3">
      {signals.map((s) => {
        const color =
          s.effective_score >= 75
            ? "text-emerald-400"
            : s.effective_score >= 50
              ? "text-amber-400"
              : "text-red-400";
        return (
          <li
            key={s.name}
            className="flex items-center justify-between gap-3 text-sm"
          >
            <span className="truncate">{s.display_name}</span>
            <span className="flex shrink-0 items-baseline gap-3 font-mono text-xs text-muted-foreground">
              <span className={`text-base font-semibold ${color}`}>
                {s.effective_score}
              </span>
              <span>conf {s.confidence}%</span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}
