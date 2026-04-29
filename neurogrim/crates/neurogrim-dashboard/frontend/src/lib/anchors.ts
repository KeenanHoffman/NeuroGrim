/**
 * v3.5.0 — anchor-based deep links into the dashboard.
 *
 * Each Overview-page widget renders with `id="widget-<spec.id>"`,
 * and this module supplies the URL builder + scroll-into-view
 * helper. Agents (and humans) link directly to a specific widget
 * via `/brains/<id>/#widget-<widget-id>`.
 */

const WIDGET_PULSE_CLASS = "widget-pulse";

/**
 * Build a fully-qualified URL into a widget. The first arg is the
 * Brain id (URL-safe slug), the second is the widget id from the
 * layout's `WidgetSpec.id`. Output is a relative path so the
 * caller can prepend whatever origin they like.
 */
export function widgetAnchorUrl(brainId: string, widgetId: string): string {
  const brain = encodeURIComponent(brainId);
  const widget = encodeURIComponent(widgetId);
  return `/brains/${brain}/#widget-${widget}`;
}

/**
 * Read the current `window.location.hash` (e.g. `#widget-foo`),
 * extract the widget id, and scroll its DOM element into view +
 * apply a temporary highlight pulse.
 *
 * No-op in non-browser environments (SSR, vitest's jsdom is OK).
 * Idempotent — safe to call from a useEffect that re-runs on
 * hash change.
 */
export function applyHashAnchor(hash: string): void {
  if (!hash || !hash.startsWith("#widget-")) return;
  const widgetId = decodeURIComponent(hash.slice("#widget-".length));
  if (!widgetId) return;
  const targetId = `widget-${widgetId}`;
  const el = document.getElementById(targetId);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "start" });
  // Pulse highlight: add class, then remove after the animation
  // finishes (1.5s). Removing lets a subsequent re-anchor re-trigger
  // the pulse cleanly.
  el.classList.add(WIDGET_PULSE_CLASS);
  window.setTimeout(() => {
    el.classList.remove(WIDGET_PULSE_CLASS);
  }, 1500);
}

/**
 * Extract the widget id from a `#widget-<id>` hash, or `null` when
 * the hash is empty / shaped differently. Useful for tests and
 * agents that want to inspect the deep link without performing the
 * scroll.
 */
export function widgetIdFromHash(hash: string): string | null {
  if (!hash || !hash.startsWith("#widget-")) return null;
  const id = decodeURIComponent(hash.slice("#widget-".length));
  return id.length > 0 ? id : null;
}
