import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

export type ConnectionStatus = "connecting" | "live" | "offline" | "disabled";

/**
 * One Server-Sent Event from `/api/events`. Mirrors the
 * `DashboardEvent` enum in `events.rs`. Stringly-typed for
 * forward compat — unknown variants are ignored at the call
 * site.
 *
 * The "disabled" sentinel is sent by the backend when the
 * filesystem watcher failed to start (e.g. project root not
 * readable). We surface it as a status, not an error.
 */
export type DashboardEvent =
  | { kind: "registry_changed" }
  | { kind: "score_changed"; domain?: string | null }
  | { kind: "skill_invoked" }
  | { kind: "layout_changed" }
  // v3.5.0 service lifecycle events.
  | { kind: "service_starting"; peer_name: string; pid: number; port: number }
  | { kind: "service_started"; peer_name: string; pid: number; port: number }
  | { kind: "service_stopped"; peer_name: string; pid: number }
  | { kind: "service_failed"; peer_name: string; reason: string };

/**
 * Wire the dashboard to its live-update channel.
 *
 * Opens an EventSource on `/api/events`, parses each event, and
 * invalidates the relevant TanStack Query keys so the page refetches
 * just the data that could have changed. Backs off + reconnects on
 * disconnect.
 *
 * Returns the current connection status so the AppShell can render
 * a "live" indicator for operator visibility.
 */
export function useDashboardEvents(): ConnectionStatus {
  const queryClient = useQueryClient();
  const [status, setStatus] = useState<ConnectionStatus>("connecting");

  useEffect(() => {
    let es: EventSource | null = null;
    let reconnectTimer: number | null = null;
    let disposed = false;

    const handleEvent = (raw: string) => {
      try {
        const parsed = JSON.parse(raw);

        // The backend sends the literal string "disabled" (encoded
        // as JSON: '"disabled"') when the watcher failed to start —
        // surface that as a non-fatal status.
        if (parsed === "disabled") {
          setStatus("disabled");
          return;
        }

        // The Rust enum serializes via serde's external tagging; for
        // unit variants this is just the variant name string, for
        // struct variants it's `{ "score_changed": { domain } }`.
        // Normalize both shapes into a `{ kind, ...fields }` event.
        const event = normalize(parsed);
        if (!event) return;
        invalidate(queryClient, event);
      } catch {
        // Malformed events are ignored — the backend's serde
        // implementation is stable, so a parse failure indicates
        // version drift; polling will keep things current.
      }
    };

    const connect = () => {
      try {
        es = new EventSource("/api/events");
      } catch {
        scheduleReconnect();
        return;
      }
      es.onopen = () => {
        if (!disposed) setStatus("live");
      };
      es.onmessage = (e) => {
        if (!disposed) handleEvent(e.data);
      };
      es.onerror = () => {
        if (disposed) return;
        setStatus("offline");
        es?.close();
        es = null;
        scheduleReconnect();
      };
    };

    const scheduleReconnect = () => {
      if (disposed || reconnectTimer !== null) return;
      // Constant 3s backoff. SSE reconnects are usually a brief
      // server restart, so a short delay keeps the page feeling
      // live. Exponential backoff would be the right thing for
      // remote production but is overkill for a localhost dev tool.
      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = null;
        connect();
      }, 3000);
    };

    connect();

    return () => {
      disposed = true;
      if (reconnectTimer !== null) window.clearTimeout(reconnectTimer);
      if (es) es.close();
    };
  }, [queryClient]);

  return status;
}

function normalize(raw: unknown): DashboardEvent | null {
  if (typeof raw === "string") {
    // Unit variants: "registry_changed", "skill_invoked", "layout_changed".
    if (
      raw === "registry_changed" ||
      raw === "skill_invoked" ||
      raw === "layout_changed"
    ) {
      return { kind: raw };
    }
    return null;
  }
  if (raw && typeof raw === "object") {
    const obj = raw as Record<string, unknown>;
    if ("score_changed" in obj) {
      const inner = obj.score_changed as { domain?: string | null } | undefined;
      return {
        kind: "score_changed",
        domain: inner?.domain ?? null,
      };
    }
    if ("service_starting" in obj) {
      const inner = obj.service_starting as {
        peer_name: string;
        pid: number;
        port: number;
      };
      return { kind: "service_starting", ...inner };
    }
    if ("service_started" in obj) {
      const inner = obj.service_started as {
        peer_name: string;
        pid: number;
        port: number;
      };
      return { kind: "service_started", ...inner };
    }
    if ("service_stopped" in obj) {
      const inner = obj.service_stopped as {
        peer_name: string;
        pid: number;
      };
      return { kind: "service_stopped", ...inner };
    }
    if ("service_failed" in obj) {
      const inner = obj.service_failed as {
        peer_name: string;
        reason: string;
      };
      return { kind: "service_failed", ...inner };
    }
  }
  return null;
}

function invalidate(
  qc: ReturnType<typeof useQueryClient>,
  event: DashboardEvent
): void {
  switch (event.kind) {
    case "registry_changed":
      // Registry edits can affect every page that reads it.
      qc.invalidateQueries({ queryKey: ["overview"] });
      qc.invalidateQueries({ queryKey: ["domains"] });
      qc.invalidateQueries({ queryKey: ["domain-detail"] });
      qc.invalidateQueries({ queryKey: ["federation"] });
      // Skills page reads the skills directory, not the registry,
      // but a registry edit often comes alongside a `skill new` so
      // a refetch is cheap and the operator sees the new entry.
      qc.invalidateQueries({ queryKey: ["skills"] });
      break;
    case "score_changed":
      qc.invalidateQueries({ queryKey: ["overview"] });
      qc.invalidateQueries({ queryKey: ["domains"] });
      // Detail pages key by domain name, but invalidating without
      // an exact key match invalidates all `["domain-detail", *]`
      // entries — TanStack Query treats the queryKey as a prefix.
      qc.invalidateQueries({ queryKey: ["domain-detail"] });
      break;
    case "skill_invoked":
      qc.invalidateQueries({ queryKey: ["skills"] });
      // S15-C-2 v2: the Logs page surfaces invocation-ledger as a
      // source. Invalidate so the timeline reflects the new
      // invocation without waiting for the 30s refetch interval.
      qc.invalidateQueries({ queryKey: ["logs-invocations"] });
      break;
    case "layout_changed":
      // Operator (or agent) edited the per-Brain dashboard
      // layout JSON. Frontend invalidates so the Overview page
      // picks up the change without a manual refresh.
      qc.invalidateQueries({ queryKey: ["dashboard-layout"] });
      break;
    case "service_starting":
    case "service_started":
    case "service_stopped":
    case "service_failed":
      // Service-lifecycle events bubble to anyone watching the
      // federation page or the services list. Invalidating both
      // keys means the next render reflects the new state without
      // waiting for the 30s refetchInterval.
      qc.invalidateQueries({ queryKey: ["federation"] });
      qc.invalidateQueries({ queryKey: ["services"] });
      break;
  }
}
