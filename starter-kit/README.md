---
date: 2026-06-30
status: current
anchored-to: none
front-door: false
---

# Starter Kit — Archived

The PowerShell starter kit has been moved to:

**`D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\`**

## Why

The starter kit was the v1 reference implementation (PowerShell + `.ps1` sensory tools).
It is superseded by the Rust reference implementation under `neurogrim/` and the Python
SDK under `sdk-python/`. The archive preserves the historical implementation as read-only
reference material — it is **not maintained** and should not be used for new adoptions.

## What Replaces It

| Need | New home |
|------|----------|
| Quickstart | `README.md` at repo root — shows `cargo build` + `neurogrim sensory` flow |
| Spec | [`LSP-Brains/spec/LSP-BRAINS-SPEC.md`](https://github.com/KeenanHoffman/LSP-Brains/blob/main/spec/LSP-BRAINS-SPEC.md) (v2.1+; sibling submodule) |
| Write a custom sensory tool | `sdk-python/README.md` — Python SDK with `lsp-brains` package |
| Built-in sensory tools | `neurogrim/crates/neurogrim-sensory/` (Rust) |
| Registry template | `.claude/brain-registry.json` at repo root (the Meta Brain registry) |
| Tutorial | [`whitepaper/WHITEPAPER.md`](../whitepaper/WHITEPAPER.md) — methodology narrative |

## Historical Reference

If you need to read the archived PowerShell code (e.g., to understand the v1 pattern or
migrate a legacy deployment), it is at `D:\Brains\archive\Moth-er-Br-AI-n\starter-kit\`.
