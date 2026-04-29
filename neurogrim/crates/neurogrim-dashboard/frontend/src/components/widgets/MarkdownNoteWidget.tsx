import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

interface MarkdownNoteWidgetProps {
  title?: string | null;
  /** Markdown body from the widget's `config.content` field. */
  content: string;
}

/**
 * Free-text card. Renders a small subset of markdown manually
 * (bold, italic, inline code) — no external markdown library to
 * pull in for what's effectively a notice/explanation card.
 *
 * If we end up needing block-level markdown (lists, headings,
 * tables), we'd switch to `react-markdown` after the usual SCA
 * review.
 */
export function MarkdownNoteWidget({ title, content }: MarkdownNoteWidgetProps) {
  return (
    <Card className="h-full bg-muted/20">
      {title && (
        <CardHeader className="pb-2">
          <CardTitle className="text-base">{title}</CardTitle>
        </CardHeader>
      )}
      <CardContent className={title ? "pt-0" : "pt-6"}>
        <div
          className="text-sm text-foreground/90 leading-relaxed"
          // Render in a simple inline-formatting way. Operators
          // and agents writing layout JSON tend to use **bold**,
          // *italic*, and `code`; that's what we support inline
          // without a full parser.
          dangerouslySetInnerHTML={{ __html: renderInline(content) }}
        />
      </CardContent>
    </Card>
  );
}

/**
 * Render a small subset of markdown safely:
 * - HTML-escape the input first
 * - Then unescape **bold**, *italic*, and `code` patterns into
 *   their respective tags
 *
 * The escape-first approach prevents any `<script>` or other HTML
 * that an operator or agent might place in the content (intentional
 * or not).
 */
function renderInline(raw: string): string {
  // Step 1: HTML-escape everything.
  const escaped = raw
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");

  // Step 2: re-introduce the small set of allowed tags by
  // matching the markdown-like patterns. Order matters: code
  // first (so the contents aren't bold-parsed), then bold,
  // then italic.
  return escaped
    .replace(/`([^`]+)`/g, '<code class="rounded bg-muted px-1 py-0.5 text-xs">$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*]+)\*([^*]|$)/g, "$1<em>$2</em>$3")
    .replace(/\n\n/g, "<br/><br/>");
}
