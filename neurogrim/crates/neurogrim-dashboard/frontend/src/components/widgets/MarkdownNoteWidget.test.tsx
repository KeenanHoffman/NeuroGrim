import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { MarkdownNoteWidget } from "./MarkdownNoteWidget";

describe("MarkdownNoteWidget", () => {
  it("renders the title when provided", () => {
    render(<MarkdownNoteWidget title="Heads up" content="hello" />);
    expect(screen.getByText("Heads up")).toBeInTheDocument();
    expect(screen.getByText("hello")).toBeInTheDocument();
  });

  it("renders without a title block when title is null", () => {
    render(<MarkdownNoteWidget title={null} content="just body" />);
    expect(screen.getByText("just body")).toBeInTheDocument();
  });

  it("renders **bold** as <strong>", () => {
    render(<MarkdownNoteWidget content="prefix **highlight** suffix" />);
    const strong = screen.getByText("highlight");
    expect(strong.tagName).toBe("STRONG");
  });

  it("renders *italic* as <em>", () => {
    render(<MarkdownNoteWidget content="some *emphasis* text" />);
    const em = screen.getByText("emphasis");
    expect(em.tagName).toBe("EM");
  });

  it("renders `code` as <code>", () => {
    render(<MarkdownNoteWidget content="run `npm install` first" />);
    const code = screen.getByText("npm install");
    expect(code.tagName).toBe("CODE");
  });

  it("escapes HTML in input — no XSS via <script>", () => {
    const malicious = '<script>alert("xss")</script>';
    const { container } = render(<MarkdownNoteWidget content={malicious} />);
    // The content is escaped: it shows as TEXT inside a div, not
    // as a real <script> element.
    expect(container.querySelector("script")).toBeNull();
    // The escaped form must appear as literal text — Testing
    // Library matches escaped HTML entities ("&lt;script&gt;...")
    // when reading textContent.
    expect(container.textContent).toContain('<script>alert("xss")</script>');
  });

  it("escapes HTML attribute injection attempts", () => {
    // Even if someone tries to slip in onerror handlers, they
    // should be visible as text, not executed.
    const malicious = '<img src=x onerror="alert(1)">';
    const { container } = render(<MarkdownNoteWidget content={malicious} />);
    expect(container.querySelector("img")).toBeNull();
  });
});
