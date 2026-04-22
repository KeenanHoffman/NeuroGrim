#!/usr/bin/env python3
"""Migrate a single legacy .claude/skills/<name>.md file to the plugin
format .claude/skills/<name>/SKILL.md with YAML frontmatter.

Extracts:
  - description: the "**When to use this skill:**" (or similar) lead
    paragraph, prefix stripped.
  - when_to_use: the "Trigger phrases:" line + continuation, prefix
    stripped.

Both are written as folded block scalars (>-) so embedded colons and
quotes don't break YAML.

The original body is preserved verbatim — the frontmatter is additive
metadata the Claude Code skill index uses for routing; it does not
replace the body. Agents navigating to the file with Read still see
identical content.

Usage: migrate-skill-to-plugin.py <legacy.md>
  Writes: <dirname>/<stem>/SKILL.md (next to the original)
  Removes: <legacy.md>

Run against the byte-identical canonical copy (e.g. NeuroGrim's), then
`cp` the resulting SKILL.md to peer Brains and `rm` their legacy copies.
"""
from __future__ import annotations
import os
import re
import sys
import textwrap

LEAD_MARKERS = [
    "**When to use this skill:**",
    "**When to read this:**",
    "**Use this skill when:**",
    "**When to reach for this skill:**",
]


def extract_lead_paragraph(body: str) -> str | None:
    """Find the lead paragraph. Prefer the one starting with a
    LEAD_MARKER. Fall back to the first non-empty paragraph after the
    H1 title."""
    lines = body.splitlines()

    # First pass: look for LEAD_MARKER start.
    for i, line in enumerate(lines):
        if any(line.lstrip().startswith(m) for m in LEAD_MARKERS):
            start = i
            end = i
            while end < len(lines) and lines[end].strip():
                end += 1
            para = " ".join(l.strip() for l in lines[start:end])
            for m in LEAD_MARKERS:
                if para.startswith(m):
                    return para[len(m):].strip()
            return para

    # Fallback: first non-empty paragraph after `# Title`.
    # Skip past the H1.
    i = 0
    while i < len(lines) and not lines[i].lstrip().startswith("# "):
        i += 1
    if i >= len(lines):
        return None
    i += 1  # past title
    # Skip blank lines.
    while i < len(lines) and not lines[i].strip():
        i += 1
    if i >= len(lines):
        return None
    start = i
    while i < len(lines) and lines[i].strip():
        # Exclude frontmatter-like lines from the paragraph.
        if any(lines[i].lstrip().startswith(t) for t in
               ("Role:", "Hat:", "Protocol:", "Domain:",
                "Trigger phrases:", "Methodology-step:", "Governs:",
                "Scope:", "---")):
            break
        i += 1
    return " ".join(l.strip() for l in lines[start:i]) or None


def extract_trigger_phrases(body: str) -> str | None:
    """Find `Trigger phrases:` block. Concatenate continuation lines
    until we hit a blank line, `Methodology-step:`, `Domain:`, or `---`.
    Return the raw phrase-list text (no prefix)."""
    lines = body.splitlines()
    i = 0
    while i < len(lines):
        if lines[i].lstrip().startswith("Trigger phrases:"):
            break
        i += 1
    if i >= len(lines):
        return None
    collected = [lines[i].split("Trigger phrases:", 1)[1].strip()]
    i += 1
    stop_tokens = (
        "Methodology-step:",
        "Domain:",
        "Protocol:",
        "Role:",
        "Hat:",
        "---",
    )
    while i < len(lines):
        s = lines[i].strip()
        if not s:
            break
        if any(s.startswith(t) for t in stop_tokens):
            i += 1
            continue
        collected.append(s)
        i += 1
    return " ".join(collected)


def folded_block_scalar(text: str, indent: int = 2) -> str:
    """Render text as a YAML folded block scalar (>-).
    Wraps at ~75 chars so the result is readable."""
    wrapped = textwrap.wrap(text, width=75 - indent, break_long_words=False,
                            break_on_hyphens=False)
    if not wrapped:
        wrapped = [""]
    pad = " " * indent
    body_lines = "\n".join(pad + l for l in wrapped)
    return ">-\n" + body_lines


def migrate(legacy_path: str) -> str:
    """Migrate a legacy .md file in place. Returns the new SKILL.md path."""
    legacy_path = os.path.abspath(legacy_path)
    dirname = os.path.dirname(legacy_path)
    stem = os.path.basename(legacy_path)
    if not stem.endswith(".md"):
        raise ValueError(f"expected .md file, got {legacy_path!r}")
    name = stem[:-3]

    body = open(legacy_path, "r", encoding="utf-8").read()
    description = extract_lead_paragraph(body)
    when_to_use = extract_trigger_phrases(body)

    if description is None:
        raise RuntimeError(
            f"{legacy_path}: no lead paragraph (expected one of {LEAD_MARKERS})"
        )

    frontmatter = f"---\nname: {name}\ndescription: {folded_block_scalar(description)}\n"
    if when_to_use:
        frontmatter += f"when_to_use: {folded_block_scalar(when_to_use)}\n"
    frontmatter += "---\n\n"

    new_dir = os.path.join(dirname, name)
    os.makedirs(new_dir, exist_ok=True)
    new_path = os.path.join(new_dir, "SKILL.md")

    with open(new_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(frontmatter + body)

    os.unlink(legacy_path)
    return new_path


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    for p in argv[1:]:
        try:
            new_path = migrate(p)
            print(f"migrated: {p} -> {new_path}")
        except Exception as e:
            print(f"FAILED:   {p}: {e}", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
