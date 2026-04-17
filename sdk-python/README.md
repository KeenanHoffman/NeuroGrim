# lsp-brains

Python SDK for writing [LSP Brains](https://github.com/keenanHoffmanSparq/LSP-Brains) sensory tools.

## Quick start

```python
from lsp_brains import SensoryTool, Finding, run_server

class MyTool(SensoryTool):
    name = "my-tool"
    domain = "my-domain"

    async def analyze(self, project_root: str) -> dict:
        return self.build_cmdb(
            score=75,
            findings=[Finding("All checks passed")],
            exported_variables={"my-domain:healthy": True},
        )

if __name__ == "__main__":
    run_server(MyTool())
```

## Installation

```bash
pip install lsp-brains
```

## Concepts

A **sensory tool** produces a CMDB envelope — a JSON object with a health score (0-100),
findings, and exported variables. The MotherBrain engine discovers your tool via MCP,
invokes it, and uses the score for confidence-weighted domain scoring.

See `examples/jira_health/main.py` for a complete example.
