"""
MCP server scaffolding for LSP Brains sensory tools.

This module wires a :class:`SensoryTool` into an MCP server using the
official Python MCP SDK. The server exposes a single tool named
``check_<domain>`` that the MotherBrain Brain engine discovers and invokes.

Usage::

    from lsp_brains import run_server
    from my_tool import MyTool

    if __name__ == "__main__":
        run_server(MyTool())
"""

from __future__ import annotations

import asyncio
import json
import logging
import sys
from typing import Any

from .sensory import SensoryTool
from .schemas import validate_cmdb_envelope, ValidationError

logger = logging.getLogger(__name__)


def run_server(tool: SensoryTool, *, validate: bool = True) -> None:
    """Start an MCP server for the given sensory tool (blocking, stdio transport).

    This is the main entry point for sensory tool binaries. It blocks until
    the MCP client disconnects (typically when the Brain process exits).

    The server exposes one MCP tool: ``check_<domain>``.
    The tool accepts ``{ "project_root": string }`` and returns the CMDB
    envelope as a JSON string in an MCP text content block.

    Args:
        tool: The sensory tool instance to serve.
        validate: If True (default), validates each CMDB envelope before
                  returning it. Set to False to skip validation for performance.

    Example::

        from lsp_brains import run_server
        from .tool import JiraHealthTool

        if __name__ == "__main__":
            run_server(JiraHealthTool())
    """
    try:
        from mcp.server import Server
        from mcp.server.stdio import stdio_server
        import mcp.types as types
    except ImportError as exc:
        raise RuntimeError(
            "The 'mcp' package is required. Install it with: pip install mcp"
        ) from exc

    server = Server(tool.name)

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        return [
            types.Tool(
                name=tool.mcp_tool_name,
                description=(
                    f"Analyze project health for the '{tool.domain}' domain. "
                    f"Returns a CMDB envelope with score, findings, and exported variables."
                ),
                inputSchema={
                    "type": "object",
                    "properties": {
                        "project_root": {
                            "type": "string",
                            "description": "Absolute path to the project root to analyze.",
                        }
                    },
                    "required": ["project_root"],
                },
            )
        ]

    @server.call_tool()
    async def call_tool(
        name: str, arguments: dict[str, Any]
    ) -> list[types.TextContent]:
        if name != tool.mcp_tool_name:
            raise ValueError(f"Unknown tool: {name}. Expected: {tool.mcp_tool_name}")

        project_root = arguments.get("project_root", ".")
        logger.info("Running %s for project_root=%s", tool.name, project_root)

        try:
            envelope = await tool.analyze(project_root)
        except Exception as exc:
            logger.error("Tool %s failed: %s", tool.name, exc)
            # Return a zero-score envelope rather than crashing the server.
            # The Brain will apply confidence decay naturally when it sees a
            # stale/low-confidence envelope next cycle.
            envelope = tool.build_cmdb(
                score=0,
                findings=[f"Tool error: {exc}"],
            )

        if validate:
            try:
                validate_cmdb_envelope(envelope)
            except ValidationError as exc:
                logger.warning("Envelope validation warning: %s", exc)

        return [types.TextContent(type="text", text=json.dumps(envelope, indent=2))]

    async def _serve() -> None:
        async with stdio_server() as (read_stream, write_stream):
            await server.run(
                read_stream,
                write_stream,
                server.create_initialization_options(),
            )

    asyncio.run(_serve())
