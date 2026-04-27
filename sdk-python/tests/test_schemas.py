"""Tests for CMDB envelope schema validation."""

import pytest
from lsp_brains.schemas import validate_cmdb_envelope, ValidationError, cmdb_schema_json
from lsp_brains import SensoryTool, Finding


class SimpleTool(SensoryTool):
    name = "test-tool"
    domain = "test"

    async def analyze(self, project_root: str) -> dict:
        return self.build_cmdb(score=50)


def make_valid_envelope(**overrides):
    """Build a minimal valid CMDB envelope."""
    import datetime
    now = datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")
    base = {
        "meta": {
            "updated_by": "test-tool",
            "updated_at": now,
            "source": "sensory-tool",
            "schema_version": "1",
        },
        "score": 50,
        "updated_at": now,
        "findings": [],
    }
    base.update(overrides)
    return base


class TestValidateCmdbEnvelope:
    def test_valid_envelope_passes(self):
        env = make_valid_envelope()
        validate_cmdb_envelope(env)  # Should not raise

    def test_score_zero_passes(self):
        env = make_valid_envelope(score=0)
        validate_cmdb_envelope(env)

    def test_score_100_passes(self):
        env = make_valid_envelope(score=100)
        validate_cmdb_envelope(env)

    def test_score_above_100_fails(self):
        env = make_valid_envelope(score=101)
        with pytest.raises(ValidationError, match="validation failed"):
            validate_cmdb_envelope(env)

    def test_score_below_0_fails(self):
        env = make_valid_envelope(score=-1)
        with pytest.raises(ValidationError, match="validation failed"):
            validate_cmdb_envelope(env)

    def test_missing_score_fails(self):
        env = make_valid_envelope()
        del env["score"]
        with pytest.raises(ValidationError):
            validate_cmdb_envelope(env)

    def test_missing_meta_fails(self):
        env = make_valid_envelope()
        del env["meta"]
        with pytest.raises(ValidationError):
            validate_cmdb_envelope(env)

    def test_missing_updated_at_fails(self):
        env = make_valid_envelope()
        del env["updated_at"]
        with pytest.raises(ValidationError):
            validate_cmdb_envelope(env)

    def test_with_exported_variables(self):
        env = make_valid_envelope(
            exported_variables={"jira:open_bug_count": 5, "jira:has_critical_bugs": True}
        )
        validate_cmdb_envelope(env)  # Should not raise

    def test_tool_produced_envelope_is_valid(self):
        """Envelope produced by SensoryTool.build_cmdb passes schema validation."""
        import asyncio
        tool = SimpleTool()
        env = tool.build_cmdb(
            score=75,
            findings=[Finding("Test finding")],
            exported_variables={"test:value": 42},
        )
        validate_cmdb_envelope(env)  # Should not raise

    # E-B2-1 C2/C3: optional `confidence` field at envelope root.
    # Mirrors NeuroGrim's schema_conformance.rs tests, in-lockstep with
    # the canonical schema at LSP-Brains/schemas/cmdb-envelope-v1.schema.json.
    def test_envelope_with_confidence_passes(self):
        """Envelope WITH explicit confidence in [0,100] validates."""
        env = make_valid_envelope(confidence=82)
        validate_cmdb_envelope(env)  # Should not raise

    def test_envelope_with_confidence_zero_passes(self):
        """Envelope confidence=0 is honored (sensor explicitly recorded zero)."""
        env = make_valid_envelope(confidence=0)
        validate_cmdb_envelope(env)

    def test_envelope_with_confidence_100_passes(self):
        """Envelope confidence=100 (max) validates."""
        env = make_valid_envelope(confidence=100)
        validate_cmdb_envelope(env)

    def test_confidence_above_100_fails(self):
        """Defensive: confidence > 100 fails schema validation."""
        env = make_valid_envelope(confidence=150)
        with pytest.raises(ValidationError, match="validation failed"):
            validate_cmdb_envelope(env)

    def test_confidence_below_0_fails(self):
        """Defensive: confidence < 0 fails schema validation."""
        env = make_valid_envelope(confidence=-10)
        with pytest.raises(ValidationError, match="validation failed"):
            validate_cmdb_envelope(env)


class TestCmdbSchemaJson:
    def test_returns_valid_json(self):
        import json
        schema_str = cmdb_schema_json()
        schema = json.loads(schema_str)
        assert schema["title"] == "LSP Brains CMDB Envelope v1"
        assert "score" in schema["properties"]
