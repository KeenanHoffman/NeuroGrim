import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { HelpCircle, X } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Components } from "react-markdown";
import type { ExplainTopicResponse } from "@bindings/ExplainTopicResponse";

/**
 * S15-C-8 v2: inline-help "?" icon with rendered markdown.
 *
 * Click → modal that fetches `/api/explain/:topic` and renders the
 * markdown via `react-markdown` (with `remark-gfm` for GitHub-flavored
 * extensions: tables, fenced code, autolinks).
 *
 * **Anchor support:** topic markdown can include
 * `<!-- anchor: <kebab-id> -->` markers next to section headings.
 * When `anchor` prop matches, the rendered content is sliced to start
 * at that section. Anchor markers themselves are stripped from output.
 *
 * **Styling:** explicit Tailwind classes per element (no @tailwindcss/
 * typography plugin) so the modal stays self-contained and small.
 *
 * **v1 → v2 migration:** v1 used `<pre>` preformatted text; v2 renders
 * proper markdown. Code blocks get monospace + muted background, lists
 * indent properly, headings get a subtle hierarchy. Operators reading
 * help get the same content as `neurogrim explain <topic>` but
 * formatted for browser viewing instead of terminal viewing.
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
            <div className="text-sm" data-testid="help-modal-content">
              <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN_COMPONENTS}>
                {prepareContent(data.content, anchor)}
              </ReactMarkdown>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Component overrides for ReactMarkdown — keeps the modal styling
 * coherent with the rest of the dashboard (no plugin dependency).
 */
const MARKDOWN_COMPONENTS: Components = {
  h1: ({ children }) => <h1 className="text-lg font-bold mt-4 mb-2">{children}</h1>,
  h2: ({ children }) => <h2 className="text-base font-bold mt-4 mb-2">{children}</h2>,
  h3: ({ children }) => <h3 className="text-sm font-bold mt-3 mb-1">{children}</h3>,
  p: ({ children }) => <p className="my-2 leading-relaxed">{children}</p>,
  ul: ({ children }) => <ul className="list-disc list-outside ml-5 my-2 space-y-1">{children}</ul>,
  ol: ({ children }) => <ol className="list-decimal list-outside ml-5 my-2 space-y-1">{children}</ol>,
  li: ({ children }) => <li className="leading-relaxed">{children}</li>,
  code: ({ children, className }) => {
    // Inline code (no language class) → small inline span.
    // Block code (rendered inside <pre>) keeps default text styling.
    const isBlock = className?.startsWith("language-");
    if (isBlock) {
      return <code className={className}>{children}</code>;
    }
    return (
      <code className="px-1 py-0.5 rounded bg-muted text-xs font-mono">{children}</code>
    );
  },
  pre: ({ children }) => (
    <pre className="my-2 p-3 rounded bg-muted text-xs font-mono overflow-x-auto whitespace-pre">
      {children}
    </pre>
  ),
  a: ({ children, href }) => (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="text-primary underline hover:no-underline"
    >
      {children}
    </a>
  ),
  blockquote: ({ children }) => (
    <blockquote className="border-l-2 border-muted-foreground/30 pl-3 my-2 text-muted-foreground">
      {children}
    </blockquote>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto my-2">
      <table className="text-xs border-collapse">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-muted px-2 py-1 text-left font-bold">{children}</th>
  ),
  td: ({ children }) => <td className="border border-muted px-2 py-1">{children}</td>,
  hr: () => <hr className="my-4 border-muted" />,
};

/**
 * Prepare topic content for rendering: slice to anchor (if any) and
 * strip all `<!-- anchor: ... -->` markers from the visible output.
 */
export function prepareContent(content: string, anchor?: string): string {
  const sliced = sliceToAnchor(content, anchor);
  return stripAnchorMarkers(sliced);
}

/**
 * If `anchor` is provided AND the content has a marker matching it,
 * return the section starting from the marker. Otherwise return the
 * full content.
 *
 * Marker convention: `<!-- anchor: <id> -->` on its own line, just
 * above the section heading.
 */
export function sliceToAnchor(content: string, anchor?: string): string {
  if (!anchor) return content;
  const marker = `<!-- anchor: ${anchor} -->`;
  const idx = content.indexOf(marker);
  if (idx < 0) return content;
  return content.slice(idx);
}

/**
 * Strip all `<!-- anchor: ... -->` markers (including the trailing
 * newline) so they don't appear in the rendered output. Other HTML
 * comments are preserved (operators may legitimately want them
 * visible — the topic.md author can use `<!-- ... -->` comments to
 * leave authoring notes that they want rendered).
 */
export function stripAnchorMarkers(content: string): string {
  return content.replace(/<!--\s*anchor:\s*[^>]*-->\s*\n?/g, "");
}

async function fetchExplainTopic(topic: string): Promise<ExplainTopicResponse> {
  const url = `/api/explain/${encodeURIComponent(topic)}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return (await res.json()) as ExplainTopicResponse;
}
