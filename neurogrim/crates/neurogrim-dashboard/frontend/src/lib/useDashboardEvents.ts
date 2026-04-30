import { useEffect, useRef, useState } from "react";
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
  | { kind: "service_failed"; peer_name: string; reason: string }
  // v4.3 S15-C-2 v3 — Logs-page sources surface as live events.
  | { kind: "publish_gate_ledger_appended" }
  | { kind: "approval_resolved" }
  | { kind: "notification_published" }
  | { kind: "services_log_appended" }
  | { kind: "queue_config_changed" };

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
 *
 * **Optional `onEvent` callback** — fires for every parsed event
 * AFTER cache invalidation. Used by AppShell to dispatch toasts
 * (and is the extension point for any future "react to a specific
 * event without a query refetch" consumer). Stable identity is the
 * caller's responsibility — wrap in `useCallback` when the toast
 * dispatcher you pass closes over hook results.
 */
export function useDashboardEvents(
  onEvent?: (event: DashboardEvent) => void,
): ConnectionStatus {
  const queryClient = useQueryClient();
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  // Latch the latest callback so the EventSource handler always
  // reads the current value without re-binding on every render.
  // Without the ref, passing an inline `(e) => addToast(...)` would
  // tear down + re-open the SSE connection on every render of the
  // parent.
  const onEventRef = useRef(onEvent);
  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

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
        // Notify the optional listener AFTER invalidation so any
        // toast it dispatches reflects the post-invalidation state
        // (the queries the operator might click through to are
        // already invalidated by the time they read the toast).
        onEventRef.current?.(event);
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
    // Unit variants emit as bare strings via serde's external
    // tagging (no inner data). v4.3 S15-C-2 v3 added three more.
    if (
      raw === "registry_changed" ||
      raw === "skill_invoked" ||
      raw === "layout_changed" ||
      raw === "publish_gate_ledger_appended" ||
      raw === "approval_resolved" ||
      raw === "notification_published" ||
      raw === "services_log_appended" ||
      raw === "queue_config_changed"
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
      // v4.3 S15-C-3 expansion: score-history is a Logs page source.
      // Invalidate so a new snapshot lands in the timeline within
      // ~1s instead of waiting for the 30s refetch.
      qc.invalidateQueries({ queryKey: ["logs-score-history"] });
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
    // v4.3 S15-C-2 v3 — Logs-page sources gain live SSE
    // invalidation. The Logs page already refetches every 30s;
    // these invalidations pull the next render forward to ~1s.
    case "publish_gate_ledger_appended":
      // Logs page's publish-gates source + the dedicated
      // Publish Gates page both read this ledger.
      qc.invalidateQueries({ queryKey: ["logs-publish-gates"] });
      qc.invalidateQueries({ queryKey: ["publish-gates"] });
      break;
    case "approval_resolved":
      // Approvals page + Logs page's approvals source both
      // consume the same approval-resolutions JSONL.
      qc.invalidateQueries({ queryKey: ["approvals"] });
      qc.invalidateQueries({ queryKey: ["logs-approvals"] });
      break;
    case "notification_published":
      qc.invalidateQueries({ queryKey: ["logs-notifications"] });
      break;
    case "services_log_appended":
      // S15-C-3 expansion follow-on: services.jsonl rows trigger
      // both the Logs page's services source AND the Federation /
      // Services page (the operator's "fleet view"). The federation
      // page already invalidates on service_started/stopped — the
      // file-watcher event is a defense-in-depth path for out-of-
      // band edits.
      qc.invalidateQueries({ queryKey: ["logs-services"] });
      qc.invalidateQueries({ queryKey: ["services"] });
      break;
    case "queue_config_changed":
      // S13 follow-on hot-reload: the bus has already swapped its
      // in-memory config server-side. Frontend invalidates the
      // Settings page's queue-config viewer query so the displayed
      // YAML reflects the new file content.
      qc.invalidateQueries({ queryKey: ["config-file"] });
      break;
  }
}
