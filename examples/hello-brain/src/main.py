"""Minimal example module for the hello-brain example project.

The point of this file isn't to be interesting — it's to be SOMETHING
for the Brain's sensors to score against. Real projects substitute
their own code here.
"""
from __future__ import annotations


def greet(name: str) -> str:
    """Return a greeting. Deliberately trivial."""
    if not name:
        raise ValueError("name must be non-empty")
    return f"Hello, {name}!"


if __name__ == "__main__":
    print(greet("world"))
