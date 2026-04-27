---
name: rogue-hat
description: Test fixture — declares an unknown vocabulary term.
forbidden_tools:
  - assassinate_prod
---

# rogue-hat

This fixture exists to exercise the closed-set vocabulary discipline (Q1):
`assassinate_prod` is not in the canonical 8-entry `ToolName` enum, so the schema
must reject it. The schema-conformance test asserts the failure mode is specifically
the `forbidden_tools` enum mismatch — distinct from missing-frontmatter, distinct
from `additionalProperties: false`.
