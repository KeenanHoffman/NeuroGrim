"""Tests for SensoryTool base class and CMDB envelope builder."""

import pytest
from lsp_brains import SensoryTool, Finding


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

    def test_plain_string_works(self):
        tool = EchoTool()
        envelope = tool.build_cmdb(score=50, findings=["plain string finding"])
        assert "plain string finding" in envelope["findings"]


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

    def test_findings_as_strings(self):
        findings = [Finding("Check 1"), Finding("Check 2", severity="warning")]
        env = self.tool.build_cmdb(score=60, findings=findings)
        assert env["findings"] == ["Check 1", "Check 2"]

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
        assert result["findings"] == ["All good"]
        assert result["exported_variables"]["echo:count"] == 1
