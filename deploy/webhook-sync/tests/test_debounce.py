"""Debounce: rapid-fire pushes to the same repo trigger at most one
pull per window.

Realistic scenario: a squash-merge PR produces 2-3 push events in quick
succession. We want one pull, not three.
"""

import json
import subprocess

from webhook_sync import signature

from .conftest import TEST_SECRET


def _send(client, body_bytes, delivery_id):
    sig = signature.sign(body_bytes, TEST_SECRET)
    return client.post(
        "/webhooks/test-agent",
        content=body_bytes,
        headers={
            signature.SIGNATURE_HEADER: sig,
            "X-GitHub-Event": "push",
            "X-GitHub-Delivery": delivery_id,
            "Content-Type": "application/json",
        },
    )


def test_rapid_fire_pushes_trigger_one_pull(make_app, workspace, clock):
    client, _storage, _cfg = make_app(workspace=workspace, debounce_seconds=5.0)
    body = json.dumps({"ref": "refs/heads/main"}).encode()

    # Three distinct delivery IDs (distinct real pushes), zero clock
    # advance between them — classic "merge storm" shape.
    r1 = _send(client, body, "d-burst-001")
    r2 = _send(client, body, "d-burst-002")
    r3 = _send(client, body, "d-burst-003")

    assert r1.status_code == 200 and r1.json()["status"] == "ok"
    assert r2.status_code == 200 and r2.json()["status"] == "debounced"
    assert r3.status_code == 200 and r3.json()["status"] == "debounced"

    reset_events = _reset_events(workspace)
    assert len(reset_events) == 1, reset_events


def test_pull_acts_again_after_debounce_window(make_app, workspace, clock):
    client, _storage, cfg = make_app(workspace=workspace, debounce_seconds=5.0)
    body = json.dumps({"ref": "refs/heads/main"}).encode()

    r1 = _send(client, body, "d-dly-001")
    assert r1.json()["status"] == "ok"

    # Still within the window: debounced.
    clock.advance(cfg.debounce_seconds - 0.5)
    r2 = _send(client, body, "d-dly-002")
    assert r2.json()["status"] == "debounced"

    # Past the window: acts.
    clock.advance(1.0)
    r3 = _send(client, body, "d-dly-003")
    assert r3.json()["status"] == "ok"

    reset_events = _reset_events(workspace)
    assert len(reset_events) == 2, reset_events


def test_ignored_branch_does_not_count_toward_debounce(make_app, workspace):
    # A push to a feature branch is ignored — it must not also stamp the
    # debounce timer, otherwise the next main-branch push gets debounced
    # for a reason that has nothing to do with it.
    client, _storage, _cfg = make_app(workspace=workspace, debounce_seconds=5.0)
    feature_body = json.dumps({"ref": "refs/heads/feature/foo"}).encode()

    r_feature = _send(client, feature_body, "d-feat-001")
    assert r_feature.status_code == 200
    assert r_feature.json()["status"] == "ignored"

    # Immediately push to main — not debounced, because feature push never
    # stamped the debounce timer.
    main_body = json.dumps({"ref": "refs/heads/main"}).encode()
    r_main = _send(client, main_body, "d-main-001")
    assert r_main.json()["status"] == "ok"


def _reset_events(workspace):
    out = subprocess.check_output(
        ["git", "-C", str(workspace), "reflog", "--format=%gs"],
        text=True,
    ).strip().splitlines()
    return [line for line in out if "reset:" in line]
