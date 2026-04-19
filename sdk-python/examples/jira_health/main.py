"""
Jira ticket health sensory tool for LSP Brains.

Connects to a Jira instance and produces a CMDB envelope reflecting the
health of the project's issue tracker: open bugs, sprint completion,
P0/P1 bug counts, and average resolution time.

Configuration (via environment variables):
    JIRA_BASE_URL   Base URL of your Jira instance (e.g. https://myorg.atlassian.net)
    JIRA_EMAIL      Jira account email for Basic auth
    JIRA_API_TOKEN  Jira API token (generate at id.atlassian.com/manage-profile/security)
    JIRA_PROJECT    Jira project key to analyze (e.g. "MYPROJ")

Usage:
    # Run as a standalone MCP server (for use with NeuroGrim):
    python -m examples.jira_health

    # Or test directly:
    python -c "
    import asyncio
    from examples.jira_health.main import JiraHealthTool
    result = asyncio.run(JiraHealthTool().analyze('.'))
    import json; print(json.dumps(result, indent=2))
    "
"""

from __future__ import annotations

import asyncio
import os
from typing import Any

from lsp_brains import SensoryTool, Finding, run_server


class JiraHealthTool(SensoryTool):
    """Jira ticket health sensory tool.

    Produces a score (0-100) based on:
    - Open bug count (high open bugs → lower score)
    - P0/P1 critical bug count (each critical bug penalizes heavily)
    - Sprint completion rate (% of committed stories done by end of sprint)
    - Stale issue ratio (issues untouched for >30 days / total open)

    Score formula:
        base = 100
        base -= min(40, open_bugs * 2)      # up to -40 for open bugs
        base -= min(30, p0_p1_count * 10)   # up to -30 for critical bugs
        base -= stale_ratio * 20            # up to -20 for stale issues
        base += sprint_completion * 10      # up to +10 for sprint health
        score = clamp(base, 0, 100)
    """

    name = "jira-health"
    domain = "jira"

    async def analyze(self, project_root: str) -> dict[str, Any]:
        base_url = os.environ.get("JIRA_BASE_URL", "").rstrip("/")
        email = os.environ.get("JIRA_EMAIL", "")
        api_token = os.environ.get("JIRA_API_TOKEN", "")
        project = os.environ.get("JIRA_PROJECT", "")

        if not all([base_url, email, api_token, project]):
            return self._unconfigured_envelope()

        try:
            return await self._fetch_and_score(base_url, email, api_token, project)
        except Exception as exc:
            return self.build_cmdb(
                score=0,
                findings=[
                    Finding(f"Jira connection failed: {exc}", severity="critical"),
                    Finding("Set JIRA_BASE_URL, JIRA_EMAIL, JIRA_API_TOKEN, JIRA_PROJECT"),
                ],
            )

    async def _fetch_and_score(
        self, base_url: str, email: str, api_token: str, project: str
    ) -> dict[str, Any]:
        """Fetch Jira metrics and compute the CMDB envelope."""
        try:
            import httpx
        except ImportError:
            return self.build_cmdb(
                score=0,
                findings=[Finding("httpx not installed: pip install httpx", severity="critical")],
            )

        auth = (email, api_token)
        headers = {"Accept": "application/json"}

        async with httpx.AsyncClient(base_url=base_url, auth=auth, headers=headers) as client:
            # Open bugs
            open_bugs = await _jql_count(
                client, f'project = {project} AND issuetype = Bug AND status != Done'
            )
            # Critical bugs (P0/P1 or Highest/High priority)
            p0_p1 = await _jql_count(
                client,
                f'project = {project} AND issuetype = Bug AND status != Done '
                f'AND priority in (Highest, High, P0, P1)'
            )
            # Total open issues (for stale ratio)
            total_open = await _jql_count(
                client, f'project = {project} AND status != Done'
            )
            # Stale issues (not updated in 30 days)
            stale = await _jql_count(
                client,
                f'project = {project} AND status != Done '
                f'AND updated <= -30d'
            )
            # Active sprint completion
            sprint_done = await _jql_count(
                client,
                f'project = {project} AND sprint in openSprints() AND status = Done'
            )
            sprint_total = await _jql_count(
                client,
                f'project = {project} AND sprint in openSprints()'
            )

        stale_ratio = stale / max(total_open, 1)
        sprint_completion = sprint_done / max(sprint_total, 1)

        # Score computation
        score = 100
        score -= min(40, open_bugs * 2)
        score -= min(30, p0_p1 * 10)
        score -= int(stale_ratio * 20)
        score += int(sprint_completion * 10)
        score = max(0, min(100, score))

        findings = [
            Finding(f"Open bugs: {open_bugs}"),
            Finding(f"P0/P1 critical bugs: {p0_p1}", severity="critical" if p0_p1 > 0 else "info"),
            Finding(f"Total open issues: {total_open}"),
            Finding(f"Stale issues (>30d): {stale} ({stale_ratio:.0%})",
                    severity="warning" if stale_ratio > 0.3 else "info"),
            Finding(f"Sprint completion: {sprint_completion:.0%} ({sprint_done}/{sprint_total})"),
        ]

        exported_variables = {
            "jira:open_bug_count": open_bugs,
            "jira:p0_p1_count": p0_p1,
            "jira:total_open_issues": total_open,
            "jira:stale_issue_count": stale,
            "jira:stale_ratio_pct": round(stale_ratio * 100),
            "jira:sprint_completion_pct": round(sprint_completion * 100),
            "jira:has_critical_bugs": p0_p1 > 0,
        }

        return self.build_cmdb(
            score=score,
            findings=findings,
            exported_variables=exported_variables,
        )

    def _unconfigured_envelope(self) -> dict[str, Any]:
        return self.build_cmdb(
            score=0,
            findings=[
                Finding("Jira not configured — set environment variables:", severity="warning"),
                Finding("  JIRA_BASE_URL=https://myorg.atlassian.net"),
                Finding("  JIRA_EMAIL=you@example.com"),
                Finding("  JIRA_API_TOKEN=<api-token>"),
                Finding("  JIRA_PROJECT=MYPROJ"),
            ],
        )


async def _jql_count(client: Any, jql: str) -> int:
    """Return the total issue count for a JQL query."""
    resp = await client.get(
        "/rest/api/3/search",
        params={"jql": jql, "maxResults": 0, "fields": "summary"},
    )
    resp.raise_for_status()
    return resp.json().get("total", 0)


if __name__ == "__main__":
    run_server(JiraHealthTool())
