#!/usr/bin/env python3
"""Post-hoc patcher: fill in missing `when_to_use:` frontmatter fields
in already-migrated SKILL.md files. Looks for `**Trigger phrases:**`
(bold markdown) or `**When to read this:**` in the body and derives
when_to_use from the match. Falls back to a manual override dict for
skills where neither heuristic applies.

Preserves the body verbatim; only edits frontmatter.
"""
from __future__ import annotations
import os
import re
import sys
import textwrap

import yaml

ROOT = r"D:\Brains"

# For skills where no trigger phrases or when-to-read block exists in
# the body, provide an explicit override.
OVERRIDES = {
    "sync-ecosystem": (
        "You want to check whether the ecosystem Brain's view of its "
        "children is still accurate — before ecosystem-level reporting, "
        "before trajectory computation, or after NeuroGrim/LSP-Brains "
        "adds new domains, schemas, spec sections, or skills. Trigger "
        "phrases — \"sync ecosystem\", \"check children drift\", "
        "\"ecosystem registry stale\", \"refresh ecosystem view\"."
    ),
    "a2a": (
        "You are about to invoke, implement, or debug peer-Brain (A2A) "
        "communication — fractal composition (parent↔child) or dual-"
        "brain (local↔external). Trigger phrases — \"a2a\", \"peer "
        "brain\", \"invoke a peer\", \"fractal composition\", \"dual "
        "brain\", \"agent card\", \"beacon\", \"commune\", \"behold\"."
    ),
    "cli-mode": (
        "You are in a Claude Code session that omits the NeuroGrim MCP "
        "server (CLI-only mode) and need to invoke the Brain via "
        "`neurogrim score`, `neurogrim sensory <name>`, or similar Bash "
        "subcommands instead of MCP tools. Trigger phrases — \"cli "
        "mode\", \"neurogrim command\", \"score via bash\", \"mcp is "
        "off\", \"no brain tools\", \"bypass mcp\"."
    ),
    "peer-brain": (
        "You are configuring or running a NeuroGrim instance to serve "
        "as an A2A peer — agent card, port allocation, peer discovery, "
        "or dual-brain troubleshooting. Trigger phrases — \"run as a "
        "peer\", \"a2a serve\", \"agent card\", \"peer discovery\", "
        "\"port 842\", \"child brain\", \"external brain\"."
    ),
}


def extract_trigger_phrases_bold(body: str) -> str | None:
    """Extract `**Trigger phrases:**` block. Concatenates continuation
    lines until blank line."""
    lines = body.splitlines()
    for i, line in enumerate(lines):
        if line.lstrip().startswith("**Trigger phrases:**"):
            start = i
            end = i
            while end < len(lines) and lines[end].strip():
                end += 1
            para = " ".join(l.strip() for l in lines[start:end])
            return para.replace("**Trigger phrases:**", "").strip()
    return None


def extract_when_to_read(body: str) -> str | None:
    """Extract `**When to read this:**` paragraph."""
    lines = body.splitlines()
    for i, line in enumerate(lines):
        if line.lstrip().startswith("**When to read this:**"):
            start = i
            end = i
            while end < len(lines) and lines[end].strip():
                end += 1
            para = " ".join(l.strip() for l in lines[start:end])
            return para.replace("**When to read this:**", "").strip()
    return None


def folded_block_scalar(text: str, indent: int = 2) -> str:
    wrapped = textwrap.wrap(
        text, width=75 - indent, break_long_words=False, break_on_hyphens=False
    )
    if not wrapped:
        wrapped = [""]
    pad = " " * indent
    return ">-\n" + "\n".join(pad + l for l in wrapped)


def patch(path: str) -> str | None:
    """Return a reason for skipping, or None if patched successfully."""
    t = open(path, "r", encoding="utf-8").read()
    if not t.startswith("---"):
        return "no frontmatter"
    end = t.find("\n---", 3)
    fm_text = t[3:end]
    body = t[end + 4:]
    fm = yaml.safe_load(fm_text)
    if fm.get("when_to_use"):
        return "already has when_to_use"

    skill_id = fm.get("name", "")
    value = None
    if skill_id in OVERRIDES:
        value = OVERRIDES[skill_id]
    else:
        value = extract_trigger_phrases_bold(body)
        if not value:
            value = extract_when_to_read(body)
    if not value:
        return "no trigger-phrases match and no override"

    # Rebuild frontmatter preserving key order: insert when_to_use right
    # after description.
    block = folded_block_scalar(value)
    new_fm_text = fm_text.rstrip() + f"\nwhen_to_use: {block}\n"
    new_content = "---" + new_fm_text + "---" + body
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(new_content)
    return None


def main() -> int:
    patched, skipped = 0, 0
    for base, _, files in os.walk(ROOT):
        norm = base.replace("\\", "/")
        if ".claude/skills" not in norm or "/archived/" in norm:
            continue
        if "SKILL.md" not in files:
            continue
        p = os.path.join(base, "SKILL.md")
        t = open(p, "r", encoding="utf-8").read()
        end = t.find("\n---", 3)
        fm = yaml.safe_load(t[3:end])
        if fm.get("when_to_use"):
            continue
        reason = patch(p)
        if reason:
            print(f"SKIP  {p}: {reason}", file=sys.stderr)
            skipped += 1
        else:
            print(f"patch {p}")
            patched += 1
    print(f"patched: {patched}  skipped: {skipped}")
    return 0 if skipped == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
