"""HMAC-SHA256 verification for GitHub-style webhooks.

GitHub signs each delivery with `X-Hub-Signature-256: sha256=<hex>` where
the MAC is computed over the raw request body using the per-repo webhook
secret. We match that exactly — no GitLab/Bitbucket variants yet.

Constant-time comparison via `hmac.compare_digest` so timing attackers
can't distinguish "no signature" from "wrong signature" from
"tampered payload."
"""

from __future__ import annotations

import hmac
from hashlib import sha256

SIGNATURE_HEADER = "X-Hub-Signature-256"
SIGNATURE_PREFIX = "sha256="


def verify(body: bytes, secret: str, signature_header: str | None) -> bool:
    """Return True iff `signature_header` is a valid HMAC-SHA256 of `body`
    under `secret`.

    Safe to call with `signature_header=None` or an empty/malformed string —
    returns False without throwing.
    """
    if not signature_header:
        return False
    if not signature_header.startswith(SIGNATURE_PREFIX):
        return False
    provided_hex = signature_header[len(SIGNATURE_PREFIX) :].strip()
    if not provided_hex:
        return False
    computed = hmac.new(secret.encode("utf-8"), body, sha256).hexdigest()
    # compare_digest tolerates unequal-length inputs but for safety we also
    # pre-check — it's a fast short-circuit against random garbage.
    if len(computed) != len(provided_hex):
        return False
    return hmac.compare_digest(computed, provided_hex)


def sign(body: bytes, secret: str) -> str:
    """Produce the signature header value the tests use to mint valid requests.

    Not called from the request path — the webhook sender (GitHub in
    production; our tests in local) is the side that signs.
    """
    mac = hmac.new(secret.encode("utf-8"), body, sha256).hexdigest()
    return f"{SIGNATURE_PREFIX}{mac}"
