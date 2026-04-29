import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  HelpIcon,
  prepareContent,
  sliceToAnchor,
  stripAnchorMarkers,
} from "./HelpIcon";

function renderWithClient(node: React.ReactNode) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={qc}>{node}</QueryClientProvider>);
}

function mockExplainFetch(content: string) {
  global.fetch = vi.fn().mockImplementation(async (url: string) => {
    if (url.includes("/api/explain/")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({
          name: "scoring",
          path: "/data/explain/scoring.md",
          content,
        }),
        text: async () => "",
      } as Response;
    }
    return {
      ok: false,
      status: 404,
      json: async () => ({ error: "not found" }),
      text: async () => "",
    } as Response;
  });
}

describe("HelpIcon", () => {
  it("renders a help button with accessible label", () => {
    renderWithClient(<HelpIcon topic="scoring" />);
    expect(
      screen.getByTestId("help-icon-scoring"),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Help: scoring")).toBeInTheDocument();
  });

  it("uses anchor in test id when provided", () => {
    renderWithClient(<HelpIcon topic="scoring" anchor="confidence" />);
    expect(
      screen.getByTestId("help-icon-scoring-confidence"),
    ).toBeInTheDocument();
  });

  it("respects custom ariaLabel", () => {
    renderWithClient(
      <HelpIcon topic="scoring" ariaLabel="What is a domain weight?" />,
    );
    expect(
      screen.getByLabelText("What is a domain weight?"),
    ).toBeInTheDocument();
  });

  it("opens modal on click", async () => {
    mockExplainFetch("# Scoring\nHello.");
    renderWithClient(<HelpIcon topic="scoring" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring"));
    expect(
      await screen.findByTestId("help-modal-scoring"),
    ).toBeInTheDocument();
  });

  it("renders markdown headings as <h1>/<h2>", async () => {
    mockExplainFetch("# Title\n\n## Subsection\n\nbody");
    renderWithClient(<HelpIcon topic="scoring" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring"));
    const h1 = await screen.findByText("Title");
    expect(h1.tagName).toBe("H1");
    const h2 = screen.getByText("Subsection");
    expect(h2.tagName).toBe("H2");
  });

  it("renders inline code as <code>", async () => {
    mockExplainFetch("Run `neurogrim score` to inspect.");
    renderWithClient(<HelpIcon topic="scoring" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring"));
    const code = await screen.findByText("neurogrim score");
    expect(code.tagName).toBe("CODE");
  });

  it("renders fenced code blocks inside <pre>", async () => {
    mockExplainFetch("```\nfoo = bar\n```\n");
    renderWithClient(<HelpIcon topic="scoring" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring"));
    const content = await screen.findByTestId("help-modal-content");
    expect(content.querySelector("pre")).not.toBeNull();
  });

  it("does not render anchor markers as visible text", async () => {
    mockExplainFetch(
      "# Title\n<!-- anchor: hidden-id -->\n## Section\nbody\n",
    );
    renderWithClient(<HelpIcon topic="scoring" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring"));
    const content = await screen.findByTestId("help-modal-content");
    expect(content.textContent).not.toContain("anchor: hidden-id");
    expect(content.textContent).not.toContain("<!--");
  });

  it("scopes content to anchor section when anchor prop given", async () => {
    mockExplainFetch(
      "# Top\n\nintro paragraph.\n\n<!-- anchor: deep -->\n## Deep section\n\nrelevant body\n",
    );
    renderWithClient(<HelpIcon topic="scoring" anchor="deep" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring-deep"));
    const content = await screen.findByTestId("help-modal-content");
    // "intro paragraph" lives before the anchor — should be sliced out.
    expect(content.textContent).not.toContain("intro paragraph");
    expect(content.textContent).toContain("Deep section");
    expect(content.textContent).toContain("relevant body");
  });

  it("shows the section indicator in the header when anchor is set", async () => {
    mockExplainFetch("<!-- anchor: confidence -->\n## Confidence\nbody");
    renderWithClient(<HelpIcon topic="scoring" anchor="confidence" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring-confidence"));
    await screen.findByTestId("help-modal-scoring");
    expect(screen.getByText(/section:/i)).toBeInTheDocument();
  });

  it("renders the topic command in the modal header", async () => {
    mockExplainFetch("body");
    renderWithClient(<HelpIcon topic="scoring" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring"));
    await screen.findByTestId("help-modal-scoring");
    expect(
      screen.getByText(/neurogrim explain scoring/i),
    ).toBeInTheDocument();
  });

  it("closes the modal when backdrop clicked", async () => {
    mockExplainFetch("body");
    renderWithClient(<HelpIcon topic="scoring" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring"));
    await screen.findByTestId("help-modal-scoring");
    fireEvent.click(screen.getByTestId("help-modal-backdrop"));
    expect(screen.queryByTestId("help-modal-scoring")).toBeNull();
  });

  it("closes the modal when X clicked", async () => {
    mockExplainFetch("body");
    renderWithClient(<HelpIcon topic="scoring" />);
    fireEvent.click(screen.getByTestId("help-icon-scoring"));
    await screen.findByTestId("help-modal-scoring");
    fireEvent.click(screen.getByTestId("help-modal-close"));
    expect(screen.queryByTestId("help-modal-scoring")).toBeNull();
  });

  it("shows error state when fetch fails", async () => {
    global.fetch = vi.fn().mockImplementation(async () => {
      return {
        ok: false,
        status: 500,
        json: async () => ({ error: "boom" }),
        text: async () => "",
      } as Response;
    });
    renderWithClient(<HelpIcon topic="missing" />);
    fireEvent.click(screen.getByTestId("help-icon-missing"));
    expect(
      await screen.findByText(/failed to load topic/i),
    ).toBeInTheDocument();
  });
});

describe("sliceToAnchor", () => {
  it("returns full content when no anchor", () => {
    const content = "line a\nline b";
    expect(sliceToAnchor(content)).toBe(content);
  });

  it("returns full content when anchor not found", () => {
    const content = "line a\nline b";
    expect(sliceToAnchor(content, "missing")).toBe(content);
  });

  it("slices to the anchor when found", () => {
    const content =
      "intro\n<!-- anchor: section1 -->\n## Section 1\nbody\n";
    const result = sliceToAnchor(content, "section1");
    expect(result.startsWith("<!-- anchor: section1 -->")).toBe(true);
    expect(result).toContain("Section 1");
    expect(result).not.toContain("intro");
  });

  it("preserves later content beyond the matched anchor", () => {
    const content =
      "<!-- anchor: a -->\n## A\n\n<!-- anchor: b -->\n## B\n";
    const result = sliceToAnchor(content, "a");
    expect(result).toContain("## A");
    expect(result).toContain("## B");
  });
});

describe("stripAnchorMarkers", () => {
  it("removes a single marker plus the trailing newline", () => {
    const content = "<!-- anchor: foo -->\n## Foo\nbody";
    expect(stripAnchorMarkers(content)).toBe("## Foo\nbody");
  });

  it("removes multiple markers", () => {
    const content =
      "<!-- anchor: a -->\n## A\n<!-- anchor: b -->\n## B\n";
    const stripped = stripAnchorMarkers(content);
    expect(stripped).not.toContain("anchor:");
    expect(stripped).toContain("## A");
    expect(stripped).toContain("## B");
  });

  it("preserves unrelated HTML comments", () => {
    const content =
      "<!-- anchor: real -->\n<!-- normal comment -->\n## Section";
    const stripped = stripAnchorMarkers(content);
    expect(stripped).not.toContain("anchor: real");
    expect(stripped).toContain("normal comment");
  });

  it("is a no-op when no markers present", () => {
    const content = "## Heading\n\nparagraph\n";
    expect(stripAnchorMarkers(content)).toBe(content);
  });
});

describe("prepareContent", () => {
  it("returns content unchanged when no anchor and no markers", () => {
    const content = "## Heading\nbody\n";
    expect(prepareContent(content)).toBe(content);
  });

  it("strips markers even when no anchor passed", () => {
    const content = "<!-- anchor: foo -->\n## Foo\n";
    const result = prepareContent(content);
    expect(result).not.toContain("anchor:");
    expect(result).toContain("## Foo");
  });

  it("slices to anchor and strips remaining markers", () => {
    const content =
      "intro\n<!-- anchor: a -->\n## A\n<!-- anchor: b -->\n## B\n";
    const result = prepareContent(content, "a");
    expect(result).not.toContain("intro");
    expect(result).not.toContain("anchor:");
    expect(result).toContain("## A");
    expect(result).toContain("## B");
  });
});
