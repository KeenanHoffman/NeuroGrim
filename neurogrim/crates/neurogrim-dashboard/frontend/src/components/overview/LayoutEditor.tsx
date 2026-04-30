import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  Bold,
  Code as CodeIcon,
  Italic,
  LayoutGrid,
  Minus,
  Pencil,
  Plus,
  RotateCcw,
  Save,
  X,
} from "lucide-react";
import type { WidgetSpec } from "@bindings/WidgetSpec";
import type { DomainsListResponse } from "@bindings/DomainsListResponse";
import { Button } from "@/components/ui/button-ish";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { brainApi, useBrainId } from "@/lib/useBrain";
import {
  WIDGET_CATALOG,
  makeWidgetSpec,
  widgetMeta,
} from "@/lib/widget-catalog";
import { WidgetGallery } from "./WidgetGallery";

/**
 * Edit-mode toolbar + per-widget controls for the Overview page's
 * widget layout. Layout-edits are operator preference (not Brain
 * state), so this surface is ungated — no `--allow-mutations`
 * flag required.
 *
 * Operations:
 * - **Add widget**: picker dropdown + "+ Add" button. Inserts a
 *   new widget at the end with reasonable defaults (third-width).
 * - **Reorder**: ↑ / ↓ buttons on each widget. Trivial array
 *   swap; simpler than drag-drop and works without an extra dep.
 * - **Resize**: per-widget size dropdown (full / half / third /
 *   quarter). Updates immediately in the local edit state.
 * - **Remove**: ✕ on each widget.
 * - **Save**: PUT /api/brains/:id/dashboard-layout. Server
 *   broadcasts a LayoutChanged SSE event; this component's
 *   own queries get invalidated and re-fetch.
 * - **Reset to default**: DELETE the layout file. Server then
 *   serves the posture-aware default again.
 *
 * In edit mode, each widget renders inside a thin dashed-border
 * shell (rendered by the parent OverviewPage) and the widget
 * itself is wrapped with these controls.
 */

interface LayoutEditorProps {
  /** Whether the editor toolbar shows. Driven by the parent
   *  page; the parent also passes the current layout's widgets. */
  isEditing: boolean;
  setIsEditing: (v: boolean) => void;
  widgets: WidgetSpec[];
  /** Local widget state — owned by parent so the rendered
   *  Overview can use the in-flight edits without round-tripping
   *  through the server. */
  setWidgets: (next: WidgetSpec[]) => void;
  /**
   * **S15-C-6 v2:** when set, the toolbar saves to the per-custom-page
   * endpoint (`PUT /api/brains/:id/dashboard-pages/:pageId/layout`)
   * instead of the legacy single-page Overview endpoint
   * (`PUT /api/brains/:id/dashboard-layout`). The Reset-to-default
   * button is hidden because custom pages have no posture-aware
   * default to fall back to.
   */
  pageId?: string;
}

// Widget catalog (WIDGET_CATALOG) + helpers (widgetMeta,
// defaultConfigFor, defaultSizeFor, makeWidgetSpec) live in
// @/lib/widget-catalog so the gallery can share them without a
// circular import.

export function LayoutEditorToolbar({
  isEditing,
  setIsEditing,
  widgets,
  setWidgets,
  pageId,
}: LayoutEditorProps) {
  const brainId = useBrainId();
  const queryClient = useQueryClient();
  const [adding, setAdding] = useState<string>("domain-card");
  const [galleryOpen, setGalleryOpen] = useState(false);

  const saveMutation = useMutation({
    mutationFn: async (next: WidgetSpec[]) => {
      // Custom pages PUT to dashboard-pages/:pageId/layout; the
      // legacy Overview endpoint stays as dashboard-layout.
      const url = pageId
        ? `${brainApi(brainId, "dashboard-pages")}/${encodeURIComponent(pageId)}/layout`
        : brainApi(brainId, "dashboard-layout");
      const res = await fetch(url, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ widgets: next }),
      });
      if (!res.ok) throw new Error(`PUT ${url} returned ${res.status}`);
      return res.json();
    },
    onSuccess: () => {
      // Invalidate the right query — Overview's layout cache is
      // separate from the multi-page config cache.
      queryClient.invalidateQueries({
        queryKey: pageId
          ? ["dashboard-pages", brainId]
          : ["dashboard-layout", brainId],
      });
      setIsEditing(false);
    },
  });

  const resetMutation = useMutation({
    mutationFn: async () => {
      // Reset is only meaningful for the Overview's posture-aware
      // default; custom pages have no default to fall back to. The
      // button is hidden when pageId is set, so this branch is
      // unreachable in that case — kept as a defensive guard.
      if (pageId) throw new Error("reset not supported on custom pages");
      const url = brainApi(brainId, "dashboard-layout");
      const res = await fetch(url, { method: "DELETE" });
      if (!res.ok) throw new Error(`DELETE ${url} returned ${res.status}`);
      return res.json();
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["dashboard-layout", brainId],
      });
      setIsEditing(false);
    },
  });

  if (!isEditing) {
    return (
      <div className="flex items-center justify-end" data-testid="edit-mode-off">
        <Button
          onClick={() => setIsEditing(true)}
          variant="ghost"
          data-testid="enter-edit-mode"
        >
          <Pencil className="h-4 w-4 mr-1.5" />
          Customize
        </Button>
      </div>
    );
  }

  const appendWidget = (type: string) => {
    const id = `w-${Date.now().toString(36)}`;
    setWidgets([...widgets, makeWidgetSpec(type, id)]);
  };

  const addWidget = () => {
    appendWidget(adding);
  };

  const handleGalleryPick = (type: string) => {
    appendWidget(type);
    setGalleryOpen(false);
    // Match the picker selection to the gallery pick so the
    // description panel below the dropdown reflects the new widget.
    setAdding(type);
  };

  const addingMeta = widgetMeta(adding);

  return (
    <div
      className="rounded-md border border-border bg-muted/30 p-3"
      data-testid="edit-mode-on"
    >
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-xs font-medium text-muted-foreground mr-1">
          Customizing layout —
        </span>

        <div className="flex items-center gap-1.5">
          <span className="text-xs text-muted-foreground">Add</span>
          <Select value={adding} onValueChange={setAdding}>
            <SelectTrigger
              className="w-56 h-8 text-xs"
              data-testid="add-widget-type"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {WIDGET_CATALOG.map((t) => (
                <SelectItem key={t.value} value={t.value}>
                  {t.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            onClick={addWidget}
            variant="ghost"
            data-testid="add-widget"
            size="sm"
          >
            <Plus className="h-4 w-4" />
          </Button>
          <Button
            onClick={() => setGalleryOpen(true)}
            variant="ghost"
            size="sm"
            data-testid="open-widget-gallery"
            aria-label="Browse widgets with live previews"
          >
            <LayoutGrid className="h-4 w-4 mr-1.5" />
            Browse
          </Button>
        </div>

        <div className="flex-1" />

      <Button
        onClick={() => saveMutation.mutate(widgets)}
        disabled={saveMutation.isPending}
        data-testid="save-layout"
        size="sm"
      >
        <Save className="h-4 w-4 mr-1.5" />
        {saveMutation.isPending ? "Saving…" : "Save"}
      </Button>
      {!pageId && (
        <Button
          onClick={() => {
            if (
              window.confirm(
                "Reset to the posture-aware default? Your saved layout will be deleted."
              )
            ) {
              resetMutation.mutate();
            }
          }}
          disabled={resetMutation.isPending}
          variant="ghost"
          data-testid="reset-layout"
          size="sm"
        >
          <RotateCcw className="h-4 w-4 mr-1.5" />
          Reset
        </Button>
      )}
      <Button
          onClick={() => setIsEditing(false)}
          variant="ghost"
          data-testid="exit-edit-mode"
          size="sm"
        >
          <X className="h-4 w-4 mr-1.5" />
          Done
        </Button>
      </div>

      {addingMeta && (
        <div
          className="mt-2 text-xs text-muted-foreground"
          data-testid="add-widget-description"
        >
          <span className="font-medium text-foreground">{addingMeta.label}</span>
          {" — "}
          {addingMeta.description}
        </div>
      )}

      {(saveMutation.error || resetMutation.error) && (
        <div className="mt-2 text-xs text-destructive">
          {(saveMutation.error as Error)?.message ??
            (resetMutation.error as Error)?.message}
        </div>
      )}

      {galleryOpen && (
        <WidgetGallery
          onClose={() => setGalleryOpen(false)}
          onPick={handleGalleryPick}
        />
      )}
    </div>
  );
}

/**
 * Per-widget edit controls — overlaid on top of the widget
 * (rendered inline by the parent dispatcher).
 *
 * `onConfigChange` carries the changed field. Title goes on
 * `WidgetSpec.title`; everything else goes inside
 * `WidgetSpec.config`. Numeric values like `count` come through
 * as strings and are parsed at the parent's update handler.
 */
export function WidgetEditControls({
  index,
  widgetCount,
  widget,
  onMove,
  onResize,
  onRemove,
  onReset,
  onConfigChange,
}: {
  index: number;
  widgetCount: number;
  widget: WidgetSpec;
  onMove: (delta: -1 | 1) => void;
  onResize: (next: string) => void;
  onRemove: () => void;
  onReset: () => void;
  onConfigChange: (
    field: "domain" | "content" | "title" | "count",
    value: string
  ) => void;
}) {
  const cfg = widget.config as {
    domain?: string;
    content?: string;
    count?: number;
  };
  const meta = widgetMeta(widget.widget_type);
  const hasConfigEditor =
    widget.widget_type === "domain-card" ||
    widget.widget_type === "markdown-note" ||
    widget.widget_type === "strongest-signals" ||
    widget.widget_type === "top-recommendations";

  return (
    <div
      className="rounded-md border border-dashed border-foreground/30 p-2 mb-2 bg-background"
      data-testid={`widget-edit-${widget.id}`}
    >
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="font-mono text-muted-foreground" title={meta?.description}>
          {widget.widget_type}
        </span>

        <Button
          onClick={() => onMove(-1)}
          disabled={index === 0}
          variant="ghost"
          size="sm"
          data-testid={`move-up-${widget.id}`}
          className="h-7 w-7 p-0"
          aria-label="Move up"
        >
          <ArrowUp className="h-3.5 w-3.5" />
        </Button>
        <Button
          onClick={() => onMove(1)}
          disabled={index === widgetCount - 1}
          variant="ghost"
          size="sm"
          data-testid={`move-down-${widget.id}`}
          className="h-7 w-7 p-0"
          aria-label="Move down"
        >
          <ArrowDown className="h-3.5 w-3.5" />
        </Button>

        <span className="text-muted-foreground">size</span>
        <Select value={widget.size} onValueChange={onResize}>
          <SelectTrigger
            className="w-24 h-7 text-xs"
            data-testid={`size-${widget.id}`}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="full">full</SelectItem>
            <SelectItem value="half">half</SelectItem>
            <SelectItem value="third">third</SelectItem>
            <SelectItem value="quarter">quarter</SelectItem>
          </SelectContent>
        </Select>

        <div className="flex-1" />

        <Button
          onClick={() => {
            if (
              window.confirm(
                `Reset ${widget.widget_type} to its default config?\nKeeps the widget; resets size, title, and config to the type's defaults.`
              )
            ) {
              onReset();
            }
          }}
          variant="ghost"
          size="sm"
          data-testid={`reset-${widget.id}`}
          className="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
          aria-label="Reset this widget to defaults"
          title="Reset this widget to defaults"
        >
          <RotateCcw className="h-3.5 w-3.5" />
        </Button>
        <Button
          onClick={onRemove}
          variant="ghost"
          size="sm"
          data-testid={`remove-${widget.id}`}
          className="h-7 w-7 p-0 text-destructive hover:text-destructive"
          aria-label="Remove widget"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>

      {/* Inline config editors for the widget types that need
          per-instance config. Other types (identity, gauge,
          ports-panel) take no config. */}
      {widget.widget_type === "domain-card" && (
        <DomainCardConfig
          widgetId={widget.id}
          domain={cfg.domain ?? ""}
          onChange={(v) => onConfigChange("domain", v)}
        />
      )}

      {(widget.widget_type === "strongest-signals" ||
        widget.widget_type === "top-recommendations") && (
        <CountStepperConfig
          widgetId={widget.id}
          count={typeof cfg.count === "number" ? cfg.count : 3}
          onChange={(n) => onConfigChange("count", String(n))}
        />
      )}
      {widget.widget_type === "markdown-note" && (
        <MarkdownNoteConfig
          widgetId={widget.id}
          title={widget.title ?? ""}
          content={cfg.content ?? ""}
          onTitleChange={(v) => onConfigChange("title", v)}
          onContentChange={(v) => onConfigChange("content", v)}
        />
      )}

      {hasConfigEditor && (
        <div
          className="mt-2 text-[10px] uppercase tracking-wider text-muted-foreground/70"
          data-testid={`live-preview-hint-${widget.id}`}
          aria-hidden="true"
        >
          ↓ Live preview below — edits apply instantly
        </div>
      )}
    </div>
  );
}

/**
 * Numeric stepper for `count` config on strongest-signals /
 * top-recommendations. Range: 1..=20 (matches reasonable UI
 * density on the cards). Use −/+ buttons for click access; the
 * inner input also accepts direct keyboard entry.
 */
function CountStepperConfig({
  widgetId,
  count,
  onChange,
}: {
  widgetId: string;
  count: number;
  onChange: (n: number) => void;
}) {
  const MIN = 1;
  const MAX = 20;
  const clamp = (n: number) => Math.max(MIN, Math.min(MAX, n));

  return (
    <div className="mt-2 flex items-center gap-1.5 text-xs">
      <span className="text-muted-foreground">count</span>
      <Button
        onClick={() => onChange(clamp(count - 1))}
        disabled={count <= MIN}
        variant="ghost"
        size="sm"
        data-testid={`count-decrement-${widgetId}`}
        className="h-7 w-7 p-0"
        aria-label="Decrement count"
      >
        <Minus className="h-3 w-3" />
      </Button>
      <input
        type="number"
        min={MIN}
        max={MAX}
        value={count}
        onChange={(e) => {
          const n = parseInt(e.target.value, 10);
          if (!isNaN(n)) onChange(clamp(n));
        }}
        data-testid={`count-input-${widgetId}`}
        className="w-14 bg-transparent border border-border rounded px-1.5 py-0.5 font-mono text-center"
      />
      <Button
        onClick={() => onChange(clamp(count + 1))}
        disabled={count >= MAX}
        variant="ghost"
        size="sm"
        data-testid={`count-increment-${widgetId}`}
        className="h-7 w-7 p-0"
        aria-label="Increment count"
      >
        <Plus className="h-3 w-3" />
      </Button>
      <span className="text-muted-foreground/70">
        ({MIN}–{MAX})
      </span>
    </div>
  );
}

/**
 * markdown-note config editor — title input plus a textarea with
 * a thin rich-text toolbar (B / I / `code`) that wraps the
 * current selection in the corresponding markdown delimiters.
 * No selection → inserts the delimiters at the cursor and
 * positions it between them.
 */
function MarkdownNoteConfig({
  widgetId,
  title,
  content,
  onTitleChange,
  onContentChange,
}: {
  widgetId: string;
  title: string;
  content: string;
  onTitleChange: (next: string) => void;
  onContentChange: (next: string) => void;
}) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const wrapSelection = (delimiter: string) => {
    const ta = textareaRef.current;
    if (!ta) return;
    const start = ta.selectionStart ?? content.length;
    const end = ta.selectionEnd ?? content.length;
    const before = content.slice(0, start);
    const selected = content.slice(start, end);
    const after = content.slice(end);
    const wrapped = `${before}${delimiter}${selected}${delimiter}${after}`;
    onContentChange(wrapped);
    // Restore the cursor: place it inside the delimiters when
    // there was no selection, or keep the wrapped range selected
    // when there was. Defer until after the parent re-renders.
    requestAnimationFrame(() => {
      const t = textareaRef.current;
      if (!t) return;
      if (start === end) {
        const cursor = start + delimiter.length;
        t.setSelectionRange(cursor, cursor);
      } else {
        t.setSelectionRange(
          start + delimiter.length,
          end + delimiter.length
        );
      }
      t.focus();
    });
  };

  return (
    <div className="mt-2 space-y-1.5 text-xs">
      <div className="flex items-center gap-1.5">
        <span className="text-muted-foreground w-12">title</span>
        <input
          type="text"
          value={title}
          onChange={(e) => onTitleChange(e.target.value)}
          placeholder="(no title)"
          data-testid={`config-title-${widgetId}`}
          className="flex-1 bg-transparent border border-border rounded px-1.5 py-0.5"
        />
      </div>
      <div className="flex items-start gap-1.5">
        <span className="text-muted-foreground w-12 pt-1">content</span>
        <div className="flex-1 flex flex-col gap-1">
          <div
            className="flex items-center gap-1"
            data-testid={`markdown-toolbar-${widgetId}`}
          >
            <Button
              onClick={() => wrapSelection("**")}
              variant="ghost"
              size="sm"
              data-testid={`md-bold-${widgetId}`}
              className="h-6 w-6 p-0"
              aria-label="Bold (wraps selection in **)"
              title="Bold (wraps selection in **)"
            >
              <Bold className="h-3 w-3" />
            </Button>
            <Button
              onClick={() => wrapSelection("*")}
              variant="ghost"
              size="sm"
              data-testid={`md-italic-${widgetId}`}
              className="h-6 w-6 p-0"
              aria-label="Italic (wraps selection in *)"
              title="Italic (wraps selection in *)"
            >
              <Italic className="h-3 w-3" />
            </Button>
            <Button
              onClick={() => wrapSelection("`")}
              variant="ghost"
              size="sm"
              data-testid={`md-code-${widgetId}`}
              className="h-6 w-6 p-0"
              aria-label="Inline code (wraps selection in backticks)"
              title="Inline code (wraps selection in backticks)"
            >
              <CodeIcon className="h-3 w-3" />
            </Button>
            <span className="ml-1 text-[10px] text-muted-foreground/70">
              Select text and click a button to wrap
            </span>
          </div>
          <textarea
            ref={textareaRef}
            value={content}
            onChange={(e) => onContentChange(e.target.value)}
            data-testid={`config-content-${widgetId}`}
            rows={3}
            className="bg-transparent border border-border rounded px-1.5 py-1 resize-y"
          />
        </div>
      </div>
    </div>
  );
}

/**
 * domain-card config editor — fetches the Brain's declared
 * domains and presents a dropdown so operators don't have to
 * remember the kebab-case name. Falls back to a free-text input
 * (with a hint) when the domains query fails — the operator can
 * still author a layout if the API is briefly unavailable.
 */
function DomainCardConfig({
  widgetId,
  domain,
  onChange,
}: {
  widgetId: string;
  domain: string;
  onChange: (next: string) => void;
}) {
  const brainId = useBrainId();
  const { data, isLoading, error } = useQuery({
    queryKey: ["domains-list", brainId],
    queryFn: async () => {
      const url = brainApi(brainId, "domains");
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${url} returned ${res.status}`);
      return (await res.json()) as DomainsListResponse;
    },
    staleTime: 30_000,
  });

  const options = data?.domains ?? [];

  // If the picked domain isn't currently in the list (e.g. operator
  // typed it before; domain has since been removed) keep it as an
  // "ad-hoc" option so the layout doesn't silently lose its value.
  const knownNames = new Set(options.map((d) => d.name));
  const showAdhoc = domain.length > 0 && !knownNames.has(domain);

  if (error) {
    return (
      <div className="mt-2 space-y-1 text-xs">
        <div className="flex items-center gap-1.5">
          <span className="text-muted-foreground">domain</span>
          <input
            type="text"
            value={domain}
            onChange={(e) => onChange(e.target.value)}
            placeholder="e.g. test-health"
            data-testid={`config-domain-${widgetId}`}
            className="flex-1 bg-transparent border border-border rounded px-1.5 py-0.5 font-mono"
          />
        </div>
        <div className="text-amber-500">
          Couldn't load domain list ({(error as Error).message}); enter the kebab-case name manually.
        </div>
      </div>
    );
  }

  return (
    <div className="mt-2 flex items-center gap-1.5 text-xs">
      <span className="text-muted-foreground">domain</span>
      <Select
        value={domain || ""}
        onValueChange={onChange}
        disabled={isLoading}
      >
        <SelectTrigger
          className="flex-1 h-7 text-xs font-mono"
          data-testid={`config-domain-${widgetId}`}
        >
          <SelectValue
            placeholder={isLoading ? "Loading domains…" : "Pick a domain"}
          />
        </SelectTrigger>
        <SelectContent>
          {showAdhoc && (
            <SelectItem value={domain} className="font-mono">
              {domain}{" "}
              <span className="text-muted-foreground">(unknown — kept as-is)</span>
            </SelectItem>
          )}
          {options.map((d) => (
            <SelectItem key={d.name} value={d.name} className="font-mono">
              {d.name}{" "}
              {d.weight > 0 ? (
                <span className="text-muted-foreground">
                  · weighted ({d.weight.toFixed(2)})
                </span>
              ) : (
                <span className="text-muted-foreground">· advisory</span>
              )}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
