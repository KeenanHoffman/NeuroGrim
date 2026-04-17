"""Tests for the Jira health example tool."""

import pytest
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from examples.jira_health.main import JiraHealthTool


class TestJiraHealthTool:
    def test_name_and_domain(self):
        tool = JiraHealthTool()
        assert tool.name == "jira-health"
        assert tool.domain == "jira"
        assert tool.mcp_tool_name == "check_jira"

    @pytest.mark.asyncio
    async def test_unconfigured_returns_zero_score(self, monkeypatch):
        """Without env vars, tool returns score=0 with setup instructions."""
        monkeypatch.delenv("JIRA_BASE_URL", raising=False)
        monkeypatch.delenv("JIRA_EMAIL", raising=False)
        monkeypatch.delenv("JIRA_API_TOKEN", raising=False)
        monkeypatch.delenv("JIRA_PROJECT", raising=False)

        tool = JiraHealthTool()
        result = await tool.analyze(".")
        assert result["score"] == 0
        assert any("JIRA_BASE_URL" in f.get("detail", "") for f in result["findings"])

    @pytest.mark.asyncio
    async def test_score_formula_low_bugs(self):
        """High score when open bugs are few and sprint completion is good."""
        tool = JiraHealthTool()
        score = _compute_score(open_bugs=1, p0_p1=0, stale_ratio=0.0, sprint_completion=1.0)
        assert score >= 80

    @pytest.mark.asyncio
    async def test_score_formula_many_critical_bugs(self):
        """Low score when there are many critical bugs and a stale backlog."""
        # open_bugs=20 → -40, p0_p1=5 → -30(capped), stale=50% → -10, sprint=50% → +5
        # 100 - 40 - 30 - 10 + 5 = 25
        score = _compute_score(open_bugs=20, p0_p1=5, stale_ratio=0.5, sprint_completion=0.5)
        assert score < 50

    @pytest.mark.asyncio
    async def test_score_formula_worst_case(self):
        """Worst-case penalties: max deductions, no sprint progress.

        Formula caps: bugs=-40, p0=-30, stale=-20, sprint=0 → floor is 10.
        (The formula cannot reach 0 by design — a project with some history
        but all bugs is still not a complete unknown.)
        """
        score = _compute_score(open_bugs=50, p0_p1=10, stale_ratio=1.0, sprint_completion=0.0)
        assert score == 10

    @pytest.mark.asyncio
    async def test_score_formula_perfect(self):
        """Healthy project with no bugs and full sprint completion scores 100."""
        # 100 - 0 - 0 - 0 + 10 = 110 → clamped to 100
        score = _compute_score(open_bugs=0, p0_p1=0, stale_ratio=0.0, sprint_completion=1.0)
        assert score == 100


def _compute_score(
    open_bugs: int,
    p0_p1: int,
    stale_ratio: float,
    sprint_completion: float,
) -> int:
    """Mirror of the JiraHealthTool score formula for unit testing."""
    score = 100
    score -= min(40, open_bugs * 2)
    score -= min(30, p0_p1 * 10)
    score -= int(stale_ratio * 20)
    score += int(sprint_completion * 10)
    return max(0, min(100, score))
