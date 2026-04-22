#!/usr/bin/env python3
"""Validate every .claude/skills/*/SKILL.md under D:/Brains: YAML parses
cleanly and `description + when_to_use` stays within 1,536 chars."""
import os
import sys

try:
    import yaml
except ImportError:
    print("pyyaml not installed", file=sys.stderr)
    sys.exit(2)

BUDGET = 1_536
ROOT = r"D:\Brains"

failures = []
over_budget = []
count = 0

for base, _, files in os.walk(ROOT):
    norm = base.replace("\\", "/")
    if ".claude/skills" not in norm:
        continue
    if "/archived/" in norm:
        continue
    if "SKILL.md" not in files:
        continue
    p = os.path.join(base, "SKILL.md")
    count += 1
    try:
        t = open(p, "r", encoding="utf-8").read()
        if not t.startswith("---"):
            failures.append((p, "no frontmatter"))
            continue
        end = t.find("\n---", 3)
        if end < 0:
            failures.append((p, "no closing frontmatter fence"))
            continue
        fm = yaml.safe_load(t[3:end])
        d = fm.get("description", "") or ""
        w = fm.get("when_to_use", "") or ""
        combined = (d + "\n\n" + w) if w else d
        n = len(combined)
        if n > BUDGET:
            over_budget.append((p, n))
    except Exception as e:
        failures.append((p, repr(e)[:120]))

print(f"Scanned: {count} SKILL.md files")
print(f"YAML failures: {len(failures)}")
print(f"Over-budget:   {len(over_budget)}")
for p, e in failures:
    print(f"  FAIL   {p}: {e}")
for p, n in over_budget:
    print(f"  BUDGET {p}: {n} chars")
sys.exit(1 if (failures or over_budget) else 0)
