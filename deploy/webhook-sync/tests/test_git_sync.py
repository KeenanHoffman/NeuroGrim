"""Git-sync integration test.

Uses a tempdir bare repo as the upstream + a clone as the workspace
(both set up in conftest). Adds a commit to upstream through a side
clone, fires a webhook, then confirms the workspace's HEAD advances
to match upstream.

Proves end-to-end that the plumbing from `POST /webhooks/<agent>`
through `git fetch` + `git reset --hard` actually lands remote
commits in the agent's workspace.
"""

import json
import subprocess
from pathlib import Path

from webhook_sync import signature
from webhook_sync.sync import sync_workspace

from .conftest import TEST_SECRET


def _push_new_commit_to_upstream(upstream: Path, tmp_path: Path, message: str) -> str:
    """Create a throwaway clone, add a commit, push back to upstream.
    Returns the new HEAD sha of the upstream's `main`."""
    side = tmp_path / f"side-{message.replace(' ', '-')}"
    subprocess.run(
        ["git", "clone", "-q", str(upstream), str(side)],
        check=True, capture_output=True,
    )
    for k, v in [("user.email", "side@example.invalid"), ("user.name", "side")]:
        subprocess.run(["git", "-C", str(side), "config", k, v],
                       check=True, capture_output=True)
    (side / "file.txt").write_text(f"{message}\n")
    subprocess.run(["git", "-C", str(side), "add", "file.txt"],
                   check=True, capture_output=True)
    subprocess.run(["git", "-C", str(side), "commit", "-q", "-m", message],
                   check=True, capture_output=True)
    subprocess.run(["git", "-C", str(side), "push", "-q", "origin", "main"],
                   check=True, capture_output=True)
    sha = subprocess.check_output(
        ["git", "-C", str(side), "rev-parse", "HEAD"], text=True,
    ).strip()
    return sha


def _workspace_head(workspace: Path) -> str:
    return subprocess.check_output(
        ["git", "-C", str(workspace), "rev-parse", "HEAD"], text=True,
    ).strip()


def _send_push(client, ref="refs/heads/main", delivery_id="d-sync-001"):
    body = json.dumps({"ref": ref}).encode()
    sig = signature.sign(body, TEST_SECRET)
    return client.post(
        "/webhooks/test-agent",
        content=body,
        headers={
            signature.SIGNATURE_HEADER: sig,
            "X-GitHub-Event": "push",
            "X-GitHub-Delivery": delivery_id,
            "Content-Type": "application/json",
        },
    )


def test_webhook_advances_workspace_head(make_app, workspace, upstream, tmp_path):
    """A new commit lands in upstream; webhook fires; workspace advances."""
    client, _storage, _cfg = make_app(workspace=workspace)

    before = _workspace_head(workspace)
    new_sha = _push_new_commit_to_upstream(upstream, tmp_path, "remote commit 1")
    assert new_sha != before

    resp = _send_push(client)
    assert resp.status_code == 200, resp.text
    assert resp.json()["status"] == "ok"

    after = _workspace_head(workspace)
    assert after == new_sha, f"workspace HEAD didn't advance: {before=} {after=} {new_sha=}"
    assert resp.json()["head_sha"] == new_sha


def test_sync_workspace_direct_returns_head_sha(workspace):
    """Unit-level proof that `sync_workspace` itself returns the SHA —
    tests the module boundary without the FastAPI layer.
    """
    result = sync_workspace(workspace, "main")
    assert result.success
    assert result.head_sha is not None
    assert len(result.head_sha) == 40  # sha-1 hex


def test_sync_workspace_missing_repo_returns_error(tmp_path):
    """Graceful failure when the workspace isn't a git repo.
    Refusal — not a crash — is the contract."""
    not_a_repo = tmp_path / "notarepo"
    not_a_repo.mkdir()
    result = sync_workspace(not_a_repo, "main")
    assert not result.success
    assert "not a git repository" in (result.error or "")
