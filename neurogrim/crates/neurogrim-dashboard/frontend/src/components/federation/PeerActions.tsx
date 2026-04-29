import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Play, Square, Loader2, AlertTriangle } from "lucide-react";
import type { PeerDto } from "@bindings/PeerDto";
import type { StartPeerResponse } from "@bindings/StartPeerResponse";
import type { StopPeerResponse } from "@bindings/StopPeerResponse";
import type { ServiceErrorDto } from "@bindings/ServiceErrorDto";
import { Button } from "@/components/ui/button-ish";
import { brainApi, useBrainId } from "@/lib/useBrain";
import { useHealth } from "@/lib/useHealth";

/**
 * v3.5.0 — Start / Stop controls for an A2A peer.
 *
 * Visible only when:
 * - `mutations_allowed` is true (server started with --allow-mutations)
 * - the peer's transport is "a2a"
 *
 * In any other case the component renders nothing — the dashboard
 * remains read-only-equivalent for that peer.
 *
 * Optimistic UI: clicking Start flips local state to "starting"
 * immediately; the readiness watcher emits SSE events that
 * invalidate the federation query so the StatusBadge updates
 * within ~5s. On synchronous spawn failure (422 port-conflict,
 * 409 already-running, etc.) the error is surfaced inline.
 */
export function PeerActions({ peer }: { peer: PeerDto }) {
  // CRITICAL: every hook must run on every render — Rules of Hooks.
  // The earlier (broken) version returned null BEFORE calling
  // useMutation, which crashed with React #310 ("Rendered more
  // hooks during this render than during the previous render")
  // when useHealth flipped from loading=true to loading=false on
  // the second render. All hook calls now happen first; the
  // visibility decision is the LAST thing the function does.
  const { mutationsAllowed, loading: healthLoading } = useHealth();
  const brainId = useBrainId();
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);

  const startMutation = useMutation({
    mutationFn: async () => {
      const url = brainApi(brainId, `peers/${encodeURIComponent(peer.name)}/start`);
      const res = await fetch(url, { method: "POST" });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as Partial<ServiceErrorDto>;
        throw new Error(body.error ?? `${url} returned ${res.status}`);
      }
      return (await res.json()) as StartPeerResponse;
    },
    onMutate: () => {
      setError(null);
    },
    onError: (e: Error) => {
      setError(e.message);
    },
    onSettled: () => {
      // Federation refetch will re-render once the SSE event
      // arrives, but invalidate immediately so the optimistic
      // refetch happens.
      void queryClient.invalidateQueries({ queryKey: ["federation"] });
    },
  });

  const stopMutation = useMutation({
    mutationFn: async () => {
      const url = brainApi(brainId, `peers/${encodeURIComponent(peer.name)}/stop`);
      const res = await fetch(url, { method: "POST" });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as Partial<ServiceErrorDto>;
        throw new Error(body.error ?? `${url} returned ${res.status}`);
      }
      return (await res.json()) as StopPeerResponse;
    },
    onMutate: () => {
      setError(null);
    },
    onError: (e: Error) => {
      setError(e.message);
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: ["federation"] });
    },
  });

  // Clear error after 6s — it's a transient hint, not a sticky
  // log. The federation refetch will reveal the real state.
  useEffect(() => {
    if (!error) return;
    const t = window.setTimeout(() => setError(null), 6000);
    return () => window.clearTimeout(t);
  }, [error]);

  // ALL hooks above this line. Visibility-gating returns now safe.
  if (healthLoading) return null;
  if (!mutationsAllowed) return null;
  if (peer.transport !== "a2a") return null;

  const isStarting = startMutation.isPending;
  const isStopping = stopMutation.isPending;
  const isAlive = peer.status.kind === "alive";
  const inFlight = isStarting || isStopping;

  return (
    <div
      className="rounded border border-border bg-muted/20 p-4"
      data-testid={`peer-actions-${peer.name}`}
    >
      <div className="mb-2 text-xs uppercase tracking-wider text-muted-foreground">
        Lifecycle
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="default"
          size="sm"
          onClick={() => startMutation.mutate()}
          disabled={inFlight || isAlive}
          data-testid={`peer-start-${peer.name}`}
        >
          {isStarting ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : (
            <Play className="h-3 w-3" />
          )}
          <span className="ml-2">Start</span>
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => stopMutation.mutate()}
          disabled={inFlight || !isAlive}
          data-testid={`peer-stop-${peer.name}`}
        >
          {isStopping ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : (
            <Square className="h-3 w-3" />
          )}
          <span className="ml-2">Stop</span>
        </Button>
        <span className="ml-1 text-xs text-muted-foreground">
          {isAlive
            ? "Service is running. Click Stop to terminate."
            : "Service is not running under this dashboard. Click Start to spawn."}
        </span>
      </div>
      {error && (
        <div
          className="mt-3 flex items-start gap-2 rounded bg-destructive/10 p-2 text-xs text-destructive"
          data-testid={`peer-actions-error-${peer.name}`}
          role="alert"
        >
          <AlertTriangle className="h-3 w-3 shrink-0 translate-y-0.5" />
          <span className="break-words">{error}</span>
        </div>
      )}
    </div>
  );
}
