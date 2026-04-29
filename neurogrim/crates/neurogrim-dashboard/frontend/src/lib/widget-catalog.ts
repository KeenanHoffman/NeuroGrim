/**
 * Single source of truth for the v3.5+ widget catalog.
 *
 * Both `LayoutEditor` (Add picker, default-config helpers) and
 * `WidgetGallery` (live-preview cards) read from here so they
 * stay in lock-step. Keep in alignment with:
 * - The dispatcher in `components/overview/OverviewPage.tsx`
 * - The catalog table in `crates/neurogrim-mcp/data/explain/dashboard-layouts.md`
 */

import type { WidgetSpec } from "@bindings/WidgetSpec";

export type WidgetSize = "full" | "half" | "third" | "quarter";

export interface WidgetCatalogEntry {
  /** Wire-format `widget_type` value. */
  value: string;
  /** Human-readable label used in pickers + gallery cards. */
  label: string;
  /** One-sentence description shown to operators. */
  description: string;
  /** Idiomatic default size when an operator first adds the widget. */
  defaultSize: WidgetSize;
  /** Default config object — used by Add and Reset-to-defaults. */
  defaultConfig: Record<string, unknown>;
  /**
   * Sample config used by the gallery's live-preview rendering when
   * the widget needs config that an operator would otherwise have
   * to fill in (e.g. `domain-card` needs a domain name). Falls back
   * to `defaultConfig` when omitted.
   *
   * For widgets whose preview depends on data fetched from the
   * Brain (e.g. a real domain name), the gallery overrides this
   * dynamically before rendering.
   */
  previewConfig?: Record<string, unknown>;
}

export const WIDGET_CATALOG: WidgetCatalogEntry[] = [
  {
    value: "identity",
    label: "Identity card",
    description:
      "Brain identity: project label, root, domain count, weighted/advisory split, federation peer count. Always lead with this.",
    defaultSize: "full",
    defaultConfig: {},
  },
  {
    value: "score-gauge",
    label: "Score gauge",
    description:
      "Radial gauge of the unified score + trajectory badge. Renders 'N/A · observe-only' for all-advisory Brains.",
    defaultSize: "third",
    defaultConfig: {},
  },
  {
    value: "strongest-signals",
    label: "Strongest signals",
    description:
      "Top N domains by effective score. Useful as a quick 'what's healthy' callout.",
    defaultSize: "third",
    defaultConfig: { count: 3 },
  },
  {
    value: "top-recommendations",
    label: "Top recommendations",
    description:
      "Top N gates / calls to action. Surfaces the operator's next moves.",
    defaultSize: "third",
    defaultConfig: { count: 3 },
  },
  {
    value: "domain-card",
    label: "Domain card",
    description:
      "Single-domain stat card (score, weight, confidence, trajectory). Click drills into the domain detail page. Requires a 'domain' config — pick from this Brain's declared domains.",
    defaultSize: "third",
    defaultConfig: { domain: "" },
    previewConfig: { domain: "" },
  },
  {
    value: "markdown-note",
    label: "Markdown note",
    description:
      "Free-text card for framing notes, posture explanations, or short directives. Inline **bold**, *italic*, `code` rendered safely.",
    defaultSize: "full",
    defaultConfig: { content: "Edit me." },
    previewConfig: {
      content:
        "**Sample** _markdown_ with `inline code`. Use this widget for framing notes, posture explanations, or short directives.",
    },
  },
  {
    value: "ports-panel",
    label: "Ports panel",
    description:
      "Surfaces this project's persisted port allocation (.claude/brain/ports.json) plus a live 'is this port currently bound?' indicator.",
    defaultSize: "third",
    defaultConfig: {},
  },
];

/** Look up a catalog entry by `widget_type`. */
export function widgetMeta(type: string): WidgetCatalogEntry | undefined {
  return WIDGET_CATALOG.find((t) => t.value === type);
}

/** Default config object for a widget type, or `{}` for unknown types. */
export function defaultConfigFor(type: string): Record<string, unknown> {
  const meta = widgetMeta(type);
  // Spread to avoid sharing the catalog's object across widgets.
  return { ...(meta?.defaultConfig ?? {}) };
}

/** Default size for a widget type. */
export function defaultSizeFor(type: string): WidgetSize {
  return widgetMeta(type)?.defaultSize ?? "third";
}

/**
 * Build a fresh `WidgetSpec` for a widget type. Used when adding a
 * new widget (from the picker or the gallery) and when resetting
 * an existing widget's config to defaults.
 *
 * `id` is caller-supplied; the catalog has no opinion on identity.
 */
export function makeWidgetSpec(type: string, id: string): WidgetSpec {
  return {
    id,
    widget_type: type,
    size: defaultSizeFor(type),
    title: null,
    config: defaultConfigFor(type),
  };
}
