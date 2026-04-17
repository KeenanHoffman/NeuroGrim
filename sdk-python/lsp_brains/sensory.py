"""
SensoryTool base class and CMDB envelope builder.

The CMDB envelope is the data contract between sensory tools and the Brain.
Every sensory tool produces a CMDB envelope: a JSON object containing a score
(0-100), a timestamp, a list of findings, and optional exported variables.

The Brain reads these envelopes to compute domain scores and populate
cross-domain correlation variables.
"""

from __future__ import annotations

import abc
import json
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any


@dataclass
class Finding:
    """A single observation produced by a sensory tool.

    Serialized into the CMDB envelope as an object matching
    `cmdb-envelope-v1.schema.json` — `{name, status, points, detail}`.

    Args:
        message: Human-readable observation (e.g. "Open bugs: 12"). Maps to
                 the schema's ``detail`` field.
        severity: Severity hint ("info", "warning", "critical"). Maps to the
                  schema's ``status`` field. The Brain uses its own threshold
                  rules; this is advisory.
        name: Explicit identifier for the finding. If omitted, the SDK
              auto-derives one (``finding-000``, ``finding-001``, ...) when
              serialized.
        points: Score contribution of this finding. Defaults to 0. Only
                meaningful for sensors whose scoring model attributes points
                to individual findings.
    """

    message: str
    severity: str = "info"
    name: str | None = None
    points: int = 0

    def __str__(self) -> str:
        return self.message


@dataclass
class CmdbEnvelope:
    """Validated CMDB envelope ready for serialization.

    Do not construct this directly — use :meth:`SensoryTool.build_cmdb`.
    """

    score: int
    updated_at: str
    meta: dict[str, Any]
    findings: list[dict[str, Any]] = field(default_factory=list)
    exported_variables: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        """Return a plain dict suitable for JSON serialization."""
        d: dict[str, Any] = {
            "meta": self.meta,
            "score": self.score,
            "updated_at": self.updated_at,
            "findings": self.findings,
        }
        if self.exported_variables:
            d["exported_variables"] = self.exported_variables
        return d

    def to_json(self, indent: int = 2) -> str:
        """Serialize to a JSON string."""
        return json.dumps(self.to_dict(), indent=indent)


class SensoryTool(abc.ABC):
    """Base class for all LSP Brains sensory tools.

    Subclass this to create a custom sensory tool. The Brain discovers
    and invokes your tool via MCP, expecting a CMDB envelope in response.

    Required class attributes:
        name (str): Tool identifier used in logging and the MCP tool name.
                    Conventionally kebab-case (e.g. "jira-health").
        domain (str): The brain-registry.json domain key this tool updates
                      (e.g. "jira"). The MCP tool will be named
                      ``check_<domain>`` (with hyphens replaced by underscores).

    Example::

        class JiraHealthTool(SensoryTool):
            name = "jira-health"
            domain = "jira"

            async def analyze(self, project_root: str) -> dict:
                open_bugs = await fetch_open_bugs()
                score = max(0, 100 - open_bugs * 5)
                return self.build_cmdb(
                    score=score,
                    findings=[Finding(f"Open bugs: {open_bugs}")],
                    exported_variables={"jira:open_bug_count": open_bugs},
                )
    """

    #: Tool name for logging and identification. Override in subclass.
    name: str = "unnamed-tool"

    #: Domain key in brain-registry.json. The MCP tool will be ``check_<domain>``.
    domain: str = "unnamed"

    @abc.abstractmethod
    async def analyze(self, project_root: str) -> dict[str, Any]:
        """Run the health analysis and return a CMDB envelope dict.

        Implementations should call :meth:`build_cmdb` to produce the envelope.

        Args:
            project_root: Absolute path to the project being analyzed.

        Returns:
            A CMDB envelope dict (use ``self.build_cmdb(...)``).
        """

    def build_cmdb(
        self,
        score: int,
        findings: list[Finding | str] | None = None,
        exported_variables: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Build a validated CMDB envelope dict.

        This is the primary way to produce output from a sensory tool.
        Call this at the end of :meth:`analyze` with your computed score.

        Args:
            score: Health score 0-100. Higher is healthier.
            findings: List of :class:`Finding` objects or plain strings
                      describing what was observed.
            exported_variables: Dictionary of ``domain:variable_name`` →
                                 value pairs. These become available as
                                 cross-domain correlation variables in the Brain.
                                 Use the ``domain:`` prefix to namespace them
                                 (e.g. ``"jira:open_bug_count": 12``).

        Returns:
            A dict conforming to the CMDB envelope v1 schema.

        Raises:
            ValueError: If score is outside the [0, 100] range.
        """
        if not 0 <= score <= 100:
            raise ValueError(f"score must be in [0, 100], got {score}")

        now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

        finding_objs: list[dict[str, Any]] = []
        if findings:
            for idx, f in enumerate(findings):
                if isinstance(f, dict):
                    finding_objs.append(f)
                elif isinstance(f, str):
                    finding_objs.append(
                        {"name": f"finding-{idx:03d}", "status": "info", "points": 0, "detail": f}
                    )
                else:
                    finding_objs.append(
                        {
                            "name": f.name if f.name is not None else f"finding-{idx:03d}",
                            "status": f.severity,
                            "points": f.points,
                            "detail": f.message,
                        }
                    )

        envelope = CmdbEnvelope(
            score=score,
            updated_at=now,
            meta={
                "updated_by": self.name,
                "updated_at": now,
                "source": "sensory-tool",
                "schema_version": "1",
            },
            findings=finding_objs,
            exported_variables=exported_variables or {},
        )
        return envelope.to_dict()

    @property
    def mcp_tool_name(self) -> str:
        """MCP tool name derived from domain (e.g. 'jira' → 'check_jira')."""
        return f"check_{self.domain.replace('-', '_')}"
