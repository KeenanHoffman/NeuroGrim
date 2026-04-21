"""HMAC signature verification tests.

The plan's adversarial review highlighted "broken signature check = anyone
can force a pull." These tests pin that defense.
"""

import json

from webhook_sync import signature

from .conftest import TEST_SECRET


def test_sign_verify_roundtrip():
    body = b'{"ref":"refs/heads/main"}'
    sig = signature.sign(body, TEST_SECRET)
    assert sig.startswith("sha256=")
    assert signature.verify(body, TEST_SECRET, sig)


def test_verify_rejects_tampered_body():
    original = b'{"ref":"refs/heads/main"}'
    sig = signature.sign(original, TEST_SECRET)
    tampered = b'{"ref":"refs/heads/main","injected":true}'
    assert not signature.verify(tampered, TEST_SECRET, sig)


def test_verify_rejects_wrong_secret():
    body = b'{"ref":"refs/heads/main"}'
    sig = signature.sign(body, TEST_SECRET)
    assert not signature.verify(body, "a-different-secret", sig)


def test_verify_rejects_missing_header():
    body = b'x'
    assert not signature.verify(body, TEST_SECRET, None)
    assert not signature.verify(body, TEST_SECRET, "")


def test_verify_rejects_missing_prefix():
    body = b'x'
    mac = signature.sign(body, TEST_SECRET)
    # Strip the "sha256=" prefix — the header is invalid without it.
    raw_hex = mac.split("=", 1)[1]
    assert not signature.verify(body, TEST_SECRET, raw_hex)


def test_verify_rejects_wrong_length_digest():
    # An attacker who truncates the digest must not trick compare_digest.
    body = b'x'
    short = "sha256=" + "0" * 10
    assert not signature.verify(body, TEST_SECRET, short)


def test_webhook_endpoint_401s_without_signature(make_app):
    client, _storage, _cfg = make_app()
    body = b'{"ref":"refs/heads/main"}'
    resp = client.post("/webhooks/test-agent", content=body)
    assert resp.status_code == 401


def test_webhook_endpoint_401s_on_tampered_body(make_app):
    client, _storage, _cfg = make_app()
    original = b'{"ref":"refs/heads/main"}'
    sig = signature.sign(original, TEST_SECRET)
    tampered = b'{"ref":"refs/heads/main","extra":true}'
    resp = client.post(
        "/webhooks/test-agent",
        content=tampered,
        headers={
            signature.SIGNATURE_HEADER: sig,
            "X-GitHub-Event": "push",
            "X-GitHub-Delivery": "d-tamper-001",
            "Content-Type": "application/json",
        },
    )
    assert resp.status_code == 401


def test_webhook_endpoint_404s_unknown_agent(make_app):
    client, _storage, _cfg = make_app()
    body = json.dumps({"ref": "refs/heads/main"}).encode()
    sig = signature.sign(body, TEST_SECRET)
    resp = client.post(
        "/webhooks/nobody",
        content=body,
        headers={signature.SIGNATURE_HEADER: sig},
    )
    assert resp.status_code == 404
