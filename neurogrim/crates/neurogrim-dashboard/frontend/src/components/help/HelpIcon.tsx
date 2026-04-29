import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { HelpCircle, X } from "lucide-react";
import type { ExplainTopicResponse } from "@bindings/ExplainTopicResponse";

/**
 * S15-C-8 v1: inline-help "?" icon.
 *
 * Click → modal that fetches `/api/explain/:topic` and renders the
 * markdown content as preformatted text. Anchor support is best-
 * effort: when the topic content has `<!-- anchor: <id> -->` markers,
 * the modal scrolls to the relevant section.
 *
 * **v1 scope:** plain-text rendering. Future stories (C-8 v2) can
 * upgrade to a markdown renderer (`react-markdown`) for syntax-
 * highlighted code blocks + clickable links. For v1, the
 * pre-formatted view + the operator's familiarity with the
 * `neurogrim explain` CLI output is enough.
 *
 * **Anchor convention:** topic markdown can include
 * `<!-- anchor: <kebab-id> -->` lines next to section headings;
 * this component scrolls the modal body to that line on open.
 * Authoring more anchors across the 13 explain topics is a
 * gradual roll-out as Settings forms mature.
 */
export function HelpIcon({
  topic,
  anchor,
  ariaLabel,
}: {
  topic: string;
  anchor?: string;
  ariaLabel?: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="inline-flex items-center justify-center w-5 h-5 rounded-full text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
        aria-label={ariaLabel ?? `Help: ${topic}`}
        data-testid={`help-icon-${topic}${anchor ? "-" + anchor : ""}`}
      >
        <HelpCircle className="h-4 w-4" />
      </button>
      {open && <HelpModal topic={topic} anchor={anchor} onClose={() => setOpen(false)} />}
    </>
  );
}

function HelpModal({
  topic,
  anchor,
  onClose,
}: {
  topic: string;
  anchor?: string;
  onClose: () => void;
}) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["explain", topic],
    queryFn: () => fetchExplainTopic(topic),
  });

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      data-testid="help-modal-backdrop"
      onClick={onClose}
    >
      <div
        className="bg-background border rounded-lg shadow-lg max-w-2xl max-h-[80vh] w-full m-4 flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        data-testid={`help-modal-${topic}`}
      >
        <header className="flex items-center justify-between p-4 border-b">
          <div>
            <h2 className="text-lg font-bold">
              <code className="text-sm">neurogrim explain {topic}</code>
            </h2>
            {anchor && (
              <p className="text-xs text-muted-foreground mt-1">
                section: <code>{anchor}</code>
              </p>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1 hover:bg-muted rounded"
            aria-label="Close"
            data-testid="help-modal-close"
          >
            <X className="h-4 w-4" />
          </button>
        </header>
        <div className="flex-1 overflow-auto p-4">
          {isLoading && <div className="text-sm text-muted-foreground">Loading…</div>}
          {error && (
            <div className="text-sm text-destructive">
              Failed to load topic: {error instanceof Error ? error.message : "unknown"}
            </div>
          )}
          {data && (
            <pre
              className="text-xs whitespace-pre-wrap font-mono"
              data-testid="help-modal-content"
            >
              {sliceToAnchor(data.content, anchor)}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * If `anchor` is provided AND the content has a marker matching it,
 * return the section starting from the marker. Otherwise return the
 * full content.
 *
 * Marker convention: `<!-- anchor: <id> -->` on its own line, just
 * above the section heading.
 */
function sliceToAnchor(content: string, anchor?: string): string {
  if (!anchor) return content;
  const marker = `<!-- anchor: ${anchor} -->`;
  const idx = content.indexOf(marker);
  if (idx < 0) return content;
  return content.slice(idx);
}

async function fetchExplainTopic(topic: string): Promise<ExplainTopicResponse> {
  const url = `/api/explain/${encodeURIComponent(topic)}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as ExplainTopicResponse;
}
