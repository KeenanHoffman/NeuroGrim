import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Info,
  X,
  XCircle,
} from "lucide-react";

/**
 * Minimal toast notification primitive (v4.3 polish — Logs page
 * cross-page surfacing).
 *
 * The dashboard runs many pages; some events (peer crashes, agent
 * notifications) are interesting regardless of which page the
 * operator is on. Toasts surface them ambient-style without
 * stealing focus.
 *
 * **Design choices:**
 *
 * - **Top-right position** — industry convention; doesn't fight the
 *   left sidebar.
 * - **Max 3 visible** — when a 4th arrives, the oldest is auto-
 *   dismissed. Keeps the stack scannable; a flood of failures
 *   doesn't bury the most recent one.
 * - **8-second auto-dismiss** — long enough to read a one-line
 *   message; short enough that abandoned toasts don't pile up.
 * - **Click anywhere to dismiss** — the whole toast surface is the
 *   dismiss target; an explicit × is also there for screen readers.
 * - **`aria-live="polite"`** on the stack — assistive tech announces
 *   new toasts without interrupting the operator's current focus.
 * - **No persistence** — toasts are ephemeral; dismissing means
 *   "I saw it." Anything important enough to outlive the session
 *   already lives in a ledger (services.jsonl, approvals queue,
 *   etc.) the operator can revisit on the Logs page.
 *
 * **Severities:** `info` (default), `success`, `warning`, `error`.
 * Mapped to icon + color via [`severityStyle`].
 */

export type ToastSeverity = "info" | "success" | "warning" | "error";

export interface Toast {
  id: string;
  severity: ToastSeverity;
  title: string;
  body?: string;
}

interface ToastContextValue {
  toasts: Toast[];
  addToast: (
    severity: ToastSeverity,
    title: string,
    body?: string,
  ) => string;
  dismissToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

const AUTO_DISMISS_MS = 8000;
const MAX_VISIBLE = 3;

/**
 * Top-level provider. Wrap once at the shell so any descendant can
 * call [`useToast`] to dispatch.
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  // Track dismiss timers per-toast so click-dismiss can cancel
  // pending auto-dismiss.
  const timersRef = useRef<Map<string, number>>(new Map());

  // Cleanup all pending timers on unmount.
  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      for (const handle of timers.values()) {
        window.clearTimeout(handle);
      }
      timers.clear();
    };
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
    const handle = timersRef.current.get(id);
    if (handle !== undefined) {
      window.clearTimeout(handle);
      timersRef.current.delete(id);
    }
  }, []);

  const addToast = useCallback(
    (severity: ToastSeverity, title: string, body?: string): string => {
      const id = generateId();
      setToasts((prev) => {
        const next = [...prev, { id, severity, title, body }];
        // Trim to MAX_VISIBLE, dropping the oldest. Cancel any
        // pending auto-dismiss timers for the dropped entries.
        if (next.length > MAX_VISIBLE) {
          const dropped = next.splice(0, next.length - MAX_VISIBLE);
          for (const t of dropped) {
            const handle = timersRef.current.get(t.id);
            if (handle !== undefined) {
              window.clearTimeout(handle);
              timersRef.current.delete(t.id);
            }
          }
        }
        return next;
      });
      const handle = window.setTimeout(() => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
        timersRef.current.delete(id);
      }, AUTO_DISMISS_MS);
      timersRef.current.set(id, handle);
      return id;
    },
    [],
  );

  const value = useMemo<ToastContextValue>(
    () => ({ toasts, addToast, dismissToast }),
    [toasts, addToast, dismissToast],
  );

  return (
    <ToastContext.Provider value={value}>
      {children}
      <Toaster />
    </ToastContext.Provider>
  );
}

/**
 * Hook for any component that wants to dispatch a toast. Throws if
 * called outside [`ToastProvider`] — that signals a wiring bug
 * earlier than silently no-op'ing.
 */
export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast() must be called within a <ToastProvider>");
  }
  return ctx;
}

/**
 * The visible stack. Mounted automatically by [`ToastProvider`]; you
 * shouldn't need to render this directly. Top-right fixed
 * position; toasts stack vertically newest-on-bottom (matches
 * conventions like macOS Notifications, where newer items push
 * older ones up).
 */
function Toaster() {
  const ctx = useContext(ToastContext);
  if (!ctx) return null;
  const { toasts, dismissToast } = ctx;
  if (toasts.length === 0) return null;
  return (
    <div
      className="fixed top-4 right-4 z-[60] flex flex-col gap-2 max-w-sm pointer-events-none"
      aria-live="polite"
      aria-atomic="false"
      data-testid="toaster"
    >
      {toasts.map((t) => (
        <ToastCard
          key={t.id}
          toast={t}
          onDismiss={() => dismissToast(t.id)}
        />
      ))}
    </div>
  );
}

function ToastCard({
  toast,
  onDismiss,
}: {
  toast: Toast;
  onDismiss: () => void;
}) {
  const { Icon, ringClass, iconClass } = severityStyle(toast.severity);
  return (
    <div
      role="status"
      data-testid={`toast-${toast.id}`}
      data-severity={toast.severity}
      className={`pointer-events-auto rounded-lg border bg-background shadow-md px-4 py-3 flex items-start gap-3 cursor-pointer hover:bg-muted/40 transition-colors ${ringClass}`}
      onClick={onDismiss}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " " || e.key === "Escape") {
          e.preventDefault();
          onDismiss();
        }
      }}
      tabIndex={0}
    >
      <Icon className={`h-5 w-5 shrink-0 mt-0.5 ${iconClass}`} aria-hidden />
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium break-words">{toast.title}</div>
        {toast.body && (
          <div className="text-xs text-muted-foreground mt-1 break-words">
            {toast.body}
          </div>
        )}
      </div>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onDismiss();
        }}
        className="p-1 hover:bg-muted rounded shrink-0 -mr-1 -mt-1 text-muted-foreground hover:text-foreground"
        aria-label="Dismiss"
        data-testid={`toast-dismiss-${toast.id}`}
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

function severityStyle(severity: ToastSeverity): {
  Icon: typeof Info;
  ringClass: string;
  iconClass: string;
} {
  switch (severity) {
    case "success":
      return {
        Icon: CheckCircle2,
        ringClass: "border-emerald-500/40",
        iconClass: "text-emerald-600",
      };
    case "warning":
      return {
        Icon: AlertTriangle,
        ringClass: "border-amber-500/50",
        iconClass: "text-amber-600",
      };
    case "error":
      return {
        Icon: XCircle,
        ringClass: "border-destructive/60",
        iconClass: "text-destructive",
      };
    case "info":
    default:
      return {
        Icon: Info,
        ringClass: "border-border",
        iconClass: "text-muted-foreground",
      };
  }
}

function generateId(): string {
  // Sufficient for in-session uniqueness; toasts are ephemeral.
  return `toast-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}
