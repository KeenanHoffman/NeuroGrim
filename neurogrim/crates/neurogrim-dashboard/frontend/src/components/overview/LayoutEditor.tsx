import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  Pencil,
  Plus,
  RotateCcw,
  Save,
  X,
} from "lucide-react";
import type { WidgetSpec } from "@bindings/WidgetSpec";
import { Button } from "@/components/ui/button-ish";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { brainApi, useBrainId } from "@/lib/useBrain";

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
}

const WIDGET_TYPES: { value: string; label: string }[] = [
  { value: "identity", label: "Identity card" },
  { value: "score-gauge", label: "Score gauge" },
  { value: "strongest-signals", label: "Strongest signals" },
  { value: "top-recommendations", label: "Top recommendations" },
  { value: "domain-card", label: "Domain card" },
  { value: "markdown-note", label: "Markdown note" },
];

export function LayoutEditorToolbar({
  isEditing,
  setIsEditing,
  widgets,
  setWidgets,
}: LayoutEditorProps) {
  const brainId = useBrainId();
  const queryClient = useQueryClient();
  const [adding, setAdding] = useState<string>("domain-card");

  const saveMutation = useMutation({
    mutationFn: async (next: WidgetSpec[]) => {
      const url = brainApi(brainId, "dashboard-layout");
      const res = await fetch(url, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ widgets: next }),
      });
      if (!res.ok) throw new Error(`PUT ${url} returned ${res.status}`);
      return res.json();
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["dashboard-layout", brainId],
      });
      setIsEditing(false);
    },
  });

  const resetMutation = useMutation({
    mutationFn: async () => {
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

  const addWidget = () => {
    const id = `w-${Date.now().toString(36)}`;
    const isMarkdown = adding === "markdown-note";
    const isDomainCard = adding === "domain-card";
    setWidgets([
      ...widgets,
      {
        id,
        widget_type: adding,
        size: adding === "identity" || isMarkdown ? "full" : "third",
        title: null,
        config: isMarkdown
          ? { content: "Edit me." }
          : isDomainCard
            ? { domain: "" } // operator must fill in
            : {},
      },
    ]);
  };

  return (
    <div
      className="rounded-md border border-border bg-muted/30 p-3 flex flex-wrap items-center gap-3"
      data-testid="edit-mode-on"
    >
      <span className="text-xs font-medium text-muted-foreground mr-1">
        Customizing layout —
      </span>

      <div className="flex items-center gap-1.5">
        <span className="text-xs text-muted-foreground">Add</span>
        <Select value={adding} onValueChange={setAdding}>
          <SelectTrigger
            className="w-44 h-8 text-xs"
            data-testid="add-widget-type"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {WIDGET_TYPES.map((t) => (
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
      <Button
        onClick={() => setIsEditing(false)}
        variant="ghost"
        data-testid="exit-edit-mode"
        size="sm"
      >
        <X className="h-4 w-4 mr-1.5" />
        Done
      </Button>

      {(saveMutation.error || resetMutation.error) && (
        <div className="basis-full text-xs text-destructive">
          {(saveMutation.error as Error)?.message ??
            (resetMutation.error as Error)?.message}
        </div>
      )}
    </div>
  );
}

/**
 * Per-widget edit controls — overlaid on top of the widget
 * (rendered inline by the parent dispatcher).
 */
export function WidgetEditControls({
  index,
  widgetCount,
  widget,
  onMove,
  onResize,
  onRemove,
  onConfigChange,
}: {
  index: number;
  widgetCount: number;
  widget: WidgetSpec;
  onMove: (delta: -1 | 1) => void;
  onResize: (next: string) => void;
  onRemove: () => void;
  onConfigChange: (
    field: "domain" | "content" | "title",
    value: string
  ) => void;
}) {
  const cfg = widget.config as { domain?: string; content?: string };
  return (
    <div
      className="rounded-md border border-dashed border-foreground/30 p-2 mb-2 bg-background"
      data-testid={`widget-edit-${widget.id}`}
    >
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="font-mono text-muted-foreground">
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
          strongest-signals, top-recs) take no config. */}
      {widget.widget_type === "domain-card" && (
        <div className="mt-2 flex items-center gap-1.5 text-xs">
          <span className="text-muted-foreground">domain</span>
          <input
            type="text"
            value={cfg.domain ?? ""}
            onChange={(e) => onConfigChange("domain", e.target.value)}
            placeholder="e.g. test-health"
            data-testid={`config-domain-${widget.id}`}
            className="flex-1 bg-transparent border border-border rounded px-1.5 py-0.5 font-mono"
          />
        </div>
      )}
      {widget.widget_type === "markdown-note" && (
        <div className="mt-2 space-y-1.5 text-xs">
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground w-12">title</span>
            <input
              type="text"
              value={widget.title ?? ""}
              onChange={(e) => onConfigChange("title", e.target.value)}
              placeholder="(no title)"
              data-testid={`config-title-${widget.id}`}
              className="flex-1 bg-transparent border border-border rounded px-1.5 py-0.5"
            />
          </div>
          <div className="flex items-start gap-1.5">
            <span className="text-muted-foreground w-12 pt-1">content</span>
            <textarea
              value={cfg.content ?? ""}
              onChange={(e) => onConfigChange("content", e.target.value)}
              data-testid={`config-content-${widget.id}`}
              rows={3}
              className="flex-1 bg-transparent border border-border rounded px-1.5 py-1 resize-y"
            />
          </div>
        </div>
      )}
    </div>
  );
}
