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
    # Bundled copy of the canonical cmdb-envelope-v1 schema for offline
    # validation. Must agree with `LSP-Brains/schemas/cmdb-envelope-v1.schema.json`
    # field-for-field. Keep this file in lock-step with the canonical one; any
    # divergence is drift that spec-impl-alignment will surface eventually.
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "LSP Brains CMDB Envelope v1",
    "description": (
        "Output contract for sensory tools. Every sensory tool produces "
        "a CMDB envelope that the Brain reads to compute domain scores."
    ),
    "type": "object",
    "required": ["meta", "score", "updated_at"],
    "additionalProperties": True,
    "properties": {
        "meta": {
            "type": "object",
            "required": ["schema_version", "updated_at", "updated_by"],
            "additionalProperties": False,
            "properties": {
                "schema_version": {
                    "type": "string",
                    "description": "CMDB schema version. Current: '1'.",
                    "const": "1",
                },
                "updated_at": {
                    "type": "string",
                    "format": "date-time",
                    "description": "ISO 8601 UTC timestamp of when the tool ran.",
                },
                "updated_by": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Name of the sensory tool that produced this envelope.",
                },
                "source": {
                    "type": "string",
                    "description": "Human-readable description of the observed source (optional).",
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
