"""Tests for SensoryTool base class and CMDB envelope builder."""

import json
from pathlib import Path

import jsonschema
import pytest
from lsp_brains import SensoryTool, Finding


# Schema lives one repo over (the LSP-Brains submodule sibling). We tolerate
# both monorepo layouts: ecosystem-root adjacent, or missing (skip marker).
_SCHEMA_PATH_CANDIDATES = [
    Path(__file__).resolve().parents[3] / "LSP-Brains" / "schemas" / "cmdb-envelope-v1.schema.json",
    Path(__file__).resolve().parents[2] / "schemas" / "cmdb-envelope-v1.schema.json",
]


def _load_cmdb_schema() -> dict | None:
    for p in _SCHEMA_PATH_CANDIDATES:
        if p.is_file():
            return json.loads(p.read_text())
    return None


class EchoTool(SensoryTool):
    """Minimal concrete tool for testing."""
    name = "echo-tool"
    domain = "echo"

    async def analyze(self, project_root: str) -> dict:
        return self.build_cmdb(
            score=80,
            findings=[Finding("All good")],
            exported_variables={"echo:count": 1},
        )


class TestFinding:
    def test_str_conversion(self):
        f = Finding("Open bugs: 5", severity="warning")
        assert str(f) == "Open bugs: 5"

    def test_plain_string_becomes_object(self):
        tool = EchoTool()
        envelope = tool.build_cmdb(score=50, findings=["plain string finding"])
        assert envelope["findings"][0]["detail"] == "plain string finding"
        assert envelope["findings"][0]["status"] == "info"
        assert envelope["findings"][0]["points"] == 0
        assert envelope["findings"][0]["name"] == "finding-000"

    def test_finding_emits_object_with_schema_fields(self):
        tool = EchoTool()
        envelope = tool.build_cmdb(
            score=50, findings=[Finding("msg", severity="warning")]
        )
        obj = envelope["findings"][0]
        assert set(obj.keys()) == {"name", "status", "points", "detail"}
        assert obj["detail"] == "msg"
        assert obj["status"] == "warning"
        assert obj["points"] == 0
        assert obj["name"] == "finding-000"

    def test_finding_explicit_name_and_points_preserved(self):
        tool = EchoTool()
        envelope = tool.build_cmdb(
            score=50,
            findings=[Finding("msg", severity="critical", name="readme-check", points=-10)],
        )
        obj = envelope["findings"][0]
        assert obj["name"] == "readme-check"
        assert obj["points"] == -10
        assert obj["status"] == "critical"


class TestBuildCmdb:
    def setup_method(self):
        self.tool = EchoTool()

    def test_score_in_envelope(self):
        env = self.tool.build_cmdb(score=75)
        assert env["score"] == 75

    def test_score_zero(self):
        env = self.tool.build_cmdb(score=0)
        assert env["score"] == 0

    def test_score_100(self):
        env = self.tool.build_cmdb(score=100)
        assert env["score"] == 100

    def test_score_out_of_range_raises(self):
        with pytest.raises(ValueError, match="score must be in"):
            self.tool.build_cmdb(score=101)

    def test_score_negative_raises(self):
        with pytest.raises(ValueError, match="score must be in"):
            self.tool.build_cmdb(score=-1)

    def test_meta_fields_populated(self):
        env = self.tool.build_cmdb(score=50)
        assert env["meta"]["updated_by"] == "echo-tool"
        assert env["meta"]["source"] == "sensory-tool"
        assert env["meta"]["schema_version"] == "1"
        assert "updated_at" in env["meta"]

    def test_updated_at_iso8601(self):
        env = self.tool.build_cmdb(score=50)
        # Should end with Z (UTC)
        assert env["updated_at"].endswith("Z")
        assert "T" in env["updated_at"]

    def test_findings_as_objects(self):
        findings = [Finding("Check 1"), Finding("Check 2", severity="warning")]
        env = self.tool.build_cmdb(score=60, findings=findings)
        assert [f["detail"] for f in env["findings"]] == ["Check 1", "Check 2"]
        assert [f["status"] for f in env["findings"]] == ["info", "warning"]

    def test_envelope_validates_against_schema(self):
        schema = _load_cmdb_schema()
        if schema is None:
            pytest.skip("cmdb-envelope-v1 schema not found on disk (expected when SDK is checked out standalone)")
        findings = [
            Finding("info finding"),
            Finding("warn finding", severity="warning"),
            Finding("crit finding", severity="critical", name="explicit-name", points=-5),
        ]
        env = self.tool.build_cmdb(
            score=77, findings=findings, exported_variables={"echo:thing": 1}
        )
        jsonschema.validate(env, schema)

    def test_empty_findings_omitted_from_list(self):
        env = self.tool.build_cmdb(score=60)
        assert env["findings"] == []

    def test_exported_variables_included(self):
        env = self.tool.build_cmdb(
            score=70,
            exported_variables={"echo:count": 42, "echo:healthy": True},
        )
        assert env["exported_variables"]["echo:count"] == 42
        assert env["exported_variables"]["echo:healthy"] is True

    def test_empty_exported_variables_not_included(self):
        env = self.tool.build_cmdb(score=50)
        assert "exported_variables" not in env

    def test_mcp_tool_name_simple(self):
        assert EchoTool().mcp_tool_name == "check_echo"

    def test_mcp_tool_name_with_hyphens(self):
        class HyphenTool(SensoryTool):
            name = "git-health"
            domain = "git-health"
            async def analyze(self, project_root): ...
        assert HyphenTool().mcp_tool_name == "check_git_health"


class TestAnalyze:
    @pytest.mark.asyncio
    async def test_analyze_returns_valid_envelope(self):
        tool = EchoTool()
        result = await tool.analyze(".")
        assert result["score"] == 80
        assert result["findings"][0]["detail"] == "All good"
        assert result["exported_variables"]["echo:count"] == 1
