"""
JSON schema validation for CMDB envelopes and agent output.

Schemas are embedded directly so the package works without filesystem lookups.
"""

from __future__ import annotations

import json
from typing import Any

try:
    import jsonschema
    _HAS_JSONSCHEMA = True
except ImportError:
    _HAS_JSONSCHEMA = False


CMDB_ENVELOPE_V1: dict[str, Any] = {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "LSP Brains CMDB Envelope v1",
    "description": (
        "Output contract for sensory tools. Every sensory tool produces "
        "a CMDB envelope that the Brain reads to compute domain scores."
    ),
    "type": "object",
    "required": ["score", "updated_at", "meta"],
    "additionalProperties": True,
    "properties": {
        "meta": {
            "type": "object",
            "required": ["updated_by", "updated_at", "source", "schema_version"],
            "properties": {
                "updated_by": {
                    "type": "string",
                    "description": "Name of the sensory tool that produced this envelope.",
                },
                "updated_at": {
                    "type": "string",
                    "format": "date-time",
                    "description": "ISO 8601 UTC timestamp of when the tool ran.",
                },
                "source": {
                    "type": "string",
                    "description": "Always 'sensory-tool' for MCP-produced envelopes.",
                },
                "schema_version": {
                    "type": "string",
                    "description": "Envelope schema version. Currently '1'.",
                },
            },
        },
        "score": {
            "type": "integer",
            "minimum": 0,
            "maximum": 100,
            "description": "Domain health score. 0 = unhealthy, 100 = fully healthy.",
        },
        "updated_at": {
            "type": "string",
            "format": "date-time",
            "description": (
                "Top-level timestamp the Brain uses for confidence decay. "
                "Must match meta.updated_at."
            ),
        },
        "findings": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "status": {"type": "string"},
                    "points": {"type": "integer"},
                    "detail": {"type": "string"},
                },
            },
            "description": (
                "Array of structured observations. Each finding matches the "
                "cmdb-envelope-v1 schema shape (name, status, points, detail)."
            ),
        },
        "exported_variables": {
            "type": "object",
            "description": (
                "Cross-domain correlation variables. Keys should follow the "
                "'domain:variable_name' convention. Values must be scalar "
                "(boolean, integer, number, or string)."
            ),
            "additionalProperties": {
                "type": ["boolean", "integer", "number", "string"],
            },
        },
    },
}


class ValidationError(Exception):
    """Raised when a CMDB envelope fails schema validation."""


def validate_cmdb_envelope(envelope: dict[str, Any]) -> None:
    """Validate a CMDB envelope dict against the v1 schema.

    Args:
        envelope: The dict to validate (as returned by :meth:`SensoryTool.build_cmdb`).

    Raises:
        ValidationError: If the envelope does not conform to the schema.
        RuntimeError: If the ``jsonschema`` package is not installed.
    """
    if not _HAS_JSONSCHEMA:
        raise RuntimeError(
            "jsonschema is required for validation. "
            "Install it with: pip install jsonschema"
        )
    try:
        jsonschema.validate(envelope, CMDB_ENVELOPE_V1)
    except jsonschema.ValidationError as exc:
        raise ValidationError(f"CMDB envelope validation failed: {exc.message}") from exc


def cmdb_schema_json(indent: int = 2) -> str:
    """Return the CMDB envelope v1 schema as a JSON string."""
    return json.dumps(CMDB_ENVELOPE_V1, indent=indent)
