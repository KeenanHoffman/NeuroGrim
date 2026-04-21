"""Idempotency: duplicate X-GitHub-Delivery IDs must not cause duplicate pulls.

GitHub retries deliveries on 5xx / timeout. If we didn't de-duplicate
we'd run `git reset --hard` repeatedly on the same workspace — wasteful
but not corrupting. Still, the contract says we skip, so pin it.
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


def test_duplicate_delivery_id_is_idempotent_replay(make_app, workspace):
    client, _storage, _cfg = make_app(workspace=workspace)
    body = json.dumps({"ref": "refs/heads/main"}).encode()

    # First delivery triggers a real pull.
    r1 = _send(client, body, "d-dup-001")
    assert r1.status_code == 200, r1.text
    assert r1.json()["status"] == "ok"

    # Second delivery with the same ID returns 200 but reports replay —
    # no new pull happens. We verify via `git reflog`: reset events land
    # there, so the count is a reliable "how many times did reset run"
    # probe.
    r2 = _send(client, body, "d-dup-001")
    assert r2.status_code == 200, r2.text
    assert r2.json()["status"] == "idempotent-replay"

    reflog = subprocess.check_output(
        ["git", "-C", str(workspace), "reflog", "--format=%gs"],
        text=True,
    ).strip().splitlines()
    # At most one "reset" entry from our sync call. `clone` and the initial
    # checkout also write reflog entries, so we only count reset events.
    reset_events = [line for line in reflog if "reset:" in line]
    assert len(reset_events) == 1, reflog


def test_different_delivery_ids_both_act(make_app, workspace, clock):
    # Two distinct delivery IDs representing two real pushes. Both should
    # pull — there's no reason to collapse them (that's debounce's job).
    # We bypass debounce by advancing the clock past the window.
    client, _storage, cfg = make_app(workspace=workspace, debounce_seconds=1.0)
    body = json.dumps({"ref": "refs/heads/main"}).encode()

    r1 = _send(client, body, "d-distinct-001")
    assert r1.status_code == 200
    assert r1.json()["status"] == "ok"

    # Advance past the debounce window so the second one isn't debounced.
    clock.advance(cfg.debounce_seconds + 0.1)

    r2 = _send(client, body, "d-distinct-002")
    assert r2.status_code == 200
    assert r2.json()["status"] == "ok"


def test_idempotency_window_expires(make_app, workspace, clock):
    # After the idempotency window passes, an old delivery ID may re-fire.
    # This matches GitHub's at-least-once semantics: far-future replays
    # aren't blocked forever.
    client, _storage, cfg = make_app(
        workspace=workspace,
        idempotency_window_seconds=60.0,
        debounce_seconds=0.0,  # disable debounce for this test
    )
    body = json.dumps({"ref": "refs/heads/main"}).encode()

    r1 = _send(client, body, "d-window-001")
    assert r1.status_code == 200 and r1.json()["status"] == "ok"

    # Within the window: replay.
    r2 = _send(client, body, "d-window-001")
    assert r2.json()["status"] == "idempotent-replay"

    # Advance the fake clock past the window.
    clock.advance(cfg.idempotency_window_seconds + 1.0)

    # Same delivery ID now acts again (the row is outside the window).
    r3 = _send(client, body, "d-window-001")
    assert r3.status_code == 200 and r3.json()["status"] == "ok"
