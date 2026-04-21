"""Shared test fixtures: fake clock, tempdir bare repo, config + app factory."""

from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Callable

import pytest
from fastapi.testclient import TestClient

from webhook_sync.app import create_app
from webhook_sync.config import AgentCfg, Config
from webhook_sync.storage import Storage

TEST_SECRET = "test-secret-0123456789"


class FakeClock:
    """Deterministic clock for debounce + idempotency-window tests."""

    def __init__(self, start: float = 1_000_000.0) -> None:
        self.now = start

    def __call__(self) -> float:
        return self.now

    def advance(self, seconds: float) -> None:
        self.now += seconds


@pytest.fixture
def clock() -> FakeClock:
    return FakeClock()


@pytest.fixture
def workspace(tmp_path: Path) -> Path:
    """A fresh clone of a bare upstream repo. The `upstream` fixture is
    available separately for tests that want to push new commits to
    verify `git fetch` picks them up.
    """
    upstream = tmp_path / "upstream.git"
    workspace = tmp_path / "workspace"
    # `-b main` pins the bare repo's HEAD to refs/heads/main. Without
    # this, a Windows host whose `init.defaultBranch` is still `master`
    # creates a bare repo whose HEAD points at `master` — which we never
    # populate — and the subsequent `git clone` produces a workspace
    # with no checkout ("unknown revision HEAD" on rev-parse).
    _run_git(tmp_path, ["init", "--bare", "-b", "main", str(upstream)])

    seed = tmp_path / "seed"
    seed.mkdir()
    _run_git(seed, ["init", "-q", "-b", "main"])
    (seed / "README.md").write_text("seed\n")
    _configure_identity(seed)
    _run_git(seed, ["add", "README.md"])
    _run_git(seed, ["commit", "-q", "-m", "seed"])
    _run_git(seed, ["remote", "add", "origin", str(upstream)])
    _run_git(seed, ["push", "-q", "origin", "main"])

    _run_git(tmp_path, ["clone", "-q", str(upstream), str(workspace)])
    _configure_identity(workspace)
    return workspace


@pytest.fixture
def upstream(workspace: Path) -> Path:
    """Path to the bare upstream repo this workspace was cloned from."""
    # `git remote get-url origin` returns a real path (not a Windows-
    # translated one), so it round-trips cleanly.
    out = subprocess.check_output(
        ["git", "-C", str(workspace), "remote", "get-url", "origin"],
        text=True,
    ).strip()
    return Path(out)


@pytest.fixture
def make_app(tmp_path: Path, clock: FakeClock) -> Callable:
    """Factory that returns (TestClient, Storage, Config). Tests can
    override debounce / idempotency windows by passing kwargs.
    """

    def _build(
        *,
        workspace: Path | None = None,
        debounce_seconds: float = 5.0,
        idempotency_window_seconds: float = 86_400.0,
        branch: str = "main",
    ):
        ws = workspace if workspace is not None else tmp_path / "stubws"
        ws.mkdir(exist_ok=True)
        cfg = Config(
            host="127.0.0.1",
            port=4747,
            state_db=tmp_path / "state.sqlite",
            debounce_seconds=debounce_seconds,
            idempotency_window_seconds=idempotency_window_seconds,
            default_branch=branch,
        )
        cfg.agents["test-agent"] = AgentCfg(
            label="test-agent",
            workspace=ws,
            branch=branch,
            secret=TEST_SECRET,
        )
        storage = Storage(cfg.state_db, time_fn=clock)
        app = create_app(cfg, storage)
        return TestClient(app), storage, cfg

    return _build


# ---- helpers used by conftest itself ----


def _run_git(cwd: Path, argv: list[str]) -> None:
    subprocess.run(
        ["git"] + argv,
        cwd=cwd,
        check=True,
        capture_output=True,
    )


def _configure_identity(repo: Path) -> None:
    """Local-only identity so commits work in CI sandboxes without a
    global git config."""
    for k, v in [
        ("user.email", "test@example.invalid"),
        ("user.name", "webhook-sync-test"),
    ]:
        subprocess.run(
            ["git", "-C", str(repo), "config", k, v],
            check=True,
            capture_output=True,
        )
