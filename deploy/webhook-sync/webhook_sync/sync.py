"""Git sync operations for the webhook-sync service.

One entry point: `sync_workspace(workspace, branch)` runs
`git fetch origin` + `git reset --hard origin/<branch>` inside
`workspace`. That's the whole job — we trust GitHub delivered a real push
event (the HMAC check in `signature.py` already gated that) and bring the
agent's workspace cache into sync with the remote HEAD.

**Why hard reset and not pull:** the agent's workspace is a *cache*,
not a source of edits. Any local state (uncommitted changes from
developer fumbling, stale stash, detached HEAD) gets wiped. That's the
honest posture — the remote is the source of truth. If an operator
needed a workspace that preserves local state, they wouldn't be running
webhook-sync on it.

**Timeout:** each subprocess is bounded at `GIT_TIMEOUT_S` so a hung
network (slow fetch) or broken daemon doesn't freeze the webhook
handler. A second delivery for the same push will just pick up where
this one left off.

**No private-repo auth in v1.** Public repos or pre-authorized SSH
agents only. Documented non-goal — see README.md.
"""

from __future__ import annotations

import logging
import subprocess
from dataclasses import dataclass
from pathlib import Path

logger = logging.getLogger(__name__)

# 60s per git call. A cold fetch on a large repo over slow network can
# reasonably take this long; we don't want to cap so tight that a real
# sync gets falsely classified as hung.
GIT_TIMEOUT_S = 60.0


@dataclass
class SyncResult:
    success: bool
    head_sha: str | None
    duration_s: float
    error: str | None = None


def sync_workspace(workspace: str | Path, branch: str) -> SyncResult:
    """Hard-reset `workspace` to `origin/<branch>`.

    Returns a SyncResult regardless of outcome — failures are reported,
    not raised. The caller logs the audit line either way.
    """
    import time

    start = time.monotonic()
    workspace = Path(workspace)

    if not workspace.is_dir() or not (workspace / ".git").exists():
        elapsed = time.monotonic() - start
        return SyncResult(
            success=False,
            head_sha=None,
            duration_s=elapsed,
            error=f"workspace {workspace} is not a git repository",
        )

    try:
        _run_git(workspace, ["fetch", "origin"])
        _run_git(workspace, ["reset", "--hard", f"origin/{branch}"])
        head_sha = _run_git(workspace, ["rev-parse", "HEAD"]).strip()
        elapsed = time.monotonic() - start
        return SyncResult(
            success=True,
            head_sha=head_sha,
            duration_s=elapsed,
            error=None,
        )
    except subprocess.CalledProcessError as e:
        elapsed = time.monotonic() - start
        # Include stderr but trim it so an enormous stack trace doesn't
        # explode the audit log line.
        stderr = (e.stderr or b"").decode("utf-8", errors="replace")
        return SyncResult(
            success=False,
            head_sha=None,
            duration_s=elapsed,
            error=f"git {e.args}: {stderr.strip()[:500]}",
        )
    except subprocess.TimeoutExpired as e:
        elapsed = time.monotonic() - start
        return SyncResult(
            success=False,
            head_sha=None,
            duration_s=elapsed,
            error=f"git {e.args} timed out after {GIT_TIMEOUT_S}s",
        )


def _run_git(workspace: Path, argv: list[str]) -> str:
    """Run `git <argv>` inside `workspace` with the shared timeout.
    Returns stdout on success; raises subprocess.CalledProcessError on
    non-zero exit."""
    cmd = ["git", "-C", str(workspace)] + argv
    logger.debug("running %s", cmd)
    out = subprocess.run(
        cmd,
        capture_output=True,
        timeout=GIT_TIMEOUT_S,
        check=True,
    )
    return out.stdout.decode("utf-8", errors="replace")
