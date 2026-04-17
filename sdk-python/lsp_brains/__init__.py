"""
lsp-brains: Python SDK for writing LSP Brains sensory tools.

Quick start::

    from lsp_brains import SensoryTool, Finding, run_server

    class MyTool(SensoryTool):
        name = "my-tool"
        domain = "my-domain"

        async def analyze(self, project_root: str) -> dict:
            return self.build_cmdb(
                score=75,
                findings=[Finding("All checks passed")],
            )

    if __name__ == "__main__":
        run_server(MyTool())
"""

from .sensory import SensoryTool, Finding, CmdbEnvelope
from .mcp import run_server
from .secret_provider import SecretProvider, SecretProviderSpec

__all__ = ["SensoryTool", "Finding", "CmdbEnvelope", "run_server", "SecretProvider", "SecretProviderSpec"]
__version__ = "0.1.0"
