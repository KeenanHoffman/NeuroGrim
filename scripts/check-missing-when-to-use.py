#!/usr/bin/env python3
"""List every .claude/skills/*/SKILL.md missing a non-empty when_to_use."""
import os
import sys

import yaml

ROOT = r"D:\Brains"
missing = []
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
    if not fm.get("when_to_use"):
        missing.append(p)

for p in missing:
    print(p)
print(f"total: {len(missing)}")
