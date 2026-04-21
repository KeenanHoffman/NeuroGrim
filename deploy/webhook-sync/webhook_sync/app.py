"""FastAPI app — GitHub-style webhook receiver that keeps agent
workspaces in sync with their upstream repos.

Endpoints:

- `POST /webhooks/{agent}` — signed push → `git fetch && git reset`.
- `GET  /health`           — liveness probe.

Every real action yields one audit line to stdout. Field shape matches
`claude-proxy/audit.py`'s discipline: no payload content, only metadata
(timestamps, agent label, delivery id, action, duration, head sha,
error). JSON-lines, one event per line, so `jq` / Loki / Splunk can
all parse it without extra tooling.

Concurrency note: we hold a module-level `threading.Lock` around every
sqlite mutation. Workloads here are tiny (a few deliveries per minute,
each touching two small tables) so a single lock is simpler than a
connection pool and is easy to reason about.
"""

from __future__ import annotations

import json
import logging
import os
import threading
import time
from dataclasses import asdict
from pathlib import Path

from fastapi import FastAPI, HTTPException, Request, status

from . import signature
from .config import Config, ConfigError, load_config
from .storage import Storage
from .sync import sync_workspace, SyncResult

logger = logging.getLogger("webhook_sync")
audit = logging.getLogger("webhook_sync.audit")

# Serialize every sqlite mutation. Simple, correct, cheap at our scale.
_state_lock = threading.Lock()


def _emit_audit(event: dict) -> None:
    """Write one JSON-lines audit event. Strict allow-list: anything not
    in this function never reaches the log."""
    safe = {
        "ts": time.time(),
        "event": event.get("event"),
        "agent": event.get("agent"),
        "delivery_id": event.get("delivery_id"),
        "branch": event.get("branch"),
        "action": event.get("action"),
        "result": event.get("result"),
        "duration_s": event.get("duration_s"),
        "head_sha": event.get("head_sha"),
        # Error strings are allowed; they're our own diagnostic output,
        # not user-submitted payload content.
        "error": event.get("error"),
        "client": event.get("client"),
    }
    # Drop None-valued keys so audit lines stay compact.
    safe = {k: v for k, v in safe.items() if v is not None}
    audit.info(json.dumps(safe, separators=(",", ":")))


def create_app(config: Config, storage: Storage) -> FastAPI:
    """Factory so tests can inject a fake config + storage without
    monkey-patching module-level globals."""

    app = FastAPI(
        title="neurogrim-webhook-sync",
        summary="Signed-push → git-sync for agent workspaces",
        version="0.1.0",
    )
    app.state.config = config
    app.state.storage = storage

    @app.get("/health")
    async def health() -> dict:
        return {
            "status": "ok",
            "agents": sorted(config.agents.keys()),
        }

    @app.post("/webhooks/{agent}")
    async def receive_webhook(agent: str, request: Request) -> dict:
        cfg = app.state.config
        store = app.state.storage
        if agent not in cfg.agents:
            # 404 — not 401 — because the name doesn't exist, not because
            # the signature was wrong. Don't confuse the caller.
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail=f"unknown agent {agent!r}",
            )
        agent_cfg = cfg.agents[agent]

        body = await request.body()
        provided_sig = request.headers.get(signature.SIGNATURE_HEADER)
        if not signature.verify(body, agent_cfg.secret, provided_sig):
            _emit_audit(
                {
                    "event": "webhook",
                    "agent": agent,
                    "result": "unauthorized",
                    "client": request.client.host if request.client else None,
                }
            )
            raise HTTPException(
                status_code=status.HTTP_401_UNAUTHORIZED,
                detail="invalid or missing signature",
            )

        event_type = request.headers.get("X-GitHub-Event", "").strip()
        delivery_id = request.headers.get("X-GitHub-Delivery", "").strip()

        # Ignore non-push events cleanly — a repo with `ping` or `issues`
        # webhooks enabled shouldn't make us crash.
        if event_type and event_type != "push":
            _emit_audit(
                {
                    "event": "webhook",
                    "agent": agent,
                    "delivery_id": delivery_id,
                    "action": event_type,
                    "result": "ignored-event-type",
                }
            )
            return {"status": "ignored", "reason": f"event type {event_type!r}"}

        # Parse the body as push payload. We only consult `ref` to
        # decide whether this push targets the configured branch.
        try:
            payload = json.loads(body.decode("utf-8"))
        except Exception as e:
            _emit_audit(
                {
                    "event": "webhook",
                    "agent": agent,
                    "delivery_id": delivery_id,
                    "result": "bad-json",
                    "error": str(e)[:200],
                }
            )
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail="body is not valid JSON",
            )

        ref = payload.get("ref") if isinstance(payload, dict) else None
        expected_ref = f"refs/heads/{agent_cfg.branch}"
        if ref is not None and ref != expected_ref:
            _emit_audit(
                {
                    "event": "webhook",
                    "agent": agent,
                    "delivery_id": delivery_id,
                    "branch": agent_cfg.branch,
                    "result": "ignored-branch",
                    "action": ref,
                }
            )
            return {"status": "ignored", "reason": f"ref {ref!r} != {expected_ref!r}"}

        # Idempotency: have we already acted on this delivery ID recently?
        with _state_lock:
            if store.delivery_seen(delivery_id, cfg.idempotency_window_seconds):
                _emit_audit(
                    {
                        "event": "webhook",
                        "agent": agent,
                        "delivery_id": delivery_id,
                        "branch": agent_cfg.branch,
                        "result": "idempotent-replay",
                    }
                )
                return {"status": "idempotent-replay"}

            # Debounce: rapid-fire pushes to the same repo collapse to one pull.
            since_last = store.seconds_since_last_pull(agent)
            if since_last is not None and since_last < cfg.debounce_seconds:
                # Record the delivery id so a retry of the SAME push doesn't
                # count against the debounce window, but don't pull.
                store.record_delivery(delivery_id, agent)
                _emit_audit(
                    {
                        "event": "webhook",
                        "agent": agent,
                        "delivery_id": delivery_id,
                        "branch": agent_cfg.branch,
                        "result": "debounced",
                        "duration_s": since_last,
                    }
                )
                return {
                    "status": "debounced",
                    "seconds_since_last_pull": since_last,
                    "debounce_seconds": cfg.debounce_seconds,
                }

            # Record the delivery BEFORE the pull so a crash mid-sync still
            # marks it seen — we don't want two webhooks retrying the same
            # delivery id to produce two pulls.
            store.record_delivery(delivery_id, agent)

        # Run the sync OUTSIDE the lock — git operations can take seconds
        # and we don't want to block unrelated deliveries.
        result: SyncResult = sync_workspace(agent_cfg.workspace, agent_cfg.branch)

        if result.success:
            with _state_lock:
                store.record_pull(agent)

        _emit_audit(
            {
                "event": "webhook",
                "agent": agent,
                "delivery_id": delivery_id,
                "branch": agent_cfg.branch,
                "action": "git-sync",
                "result": "ok" if result.success else "error",
                "duration_s": result.duration_s,
                "head_sha": result.head_sha,
                "error": result.error,
            }
        )

        if not result.success:
            # 500 so the sender retries — a transient git-fetch failure
            # shouldn't silently drop the update.
            raise HTTPException(
                status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
                detail=f"sync failed: {result.error}",
            )
        return {
            "status": "ok",
            "head_sha": result.head_sha,
            "duration_s": result.duration_s,
        }

    return app


def build_default_app() -> FastAPI:
    """Entry point for `uvicorn webhook_sync.app:app` — reads
    `WEBHOOK_SYNC_CONFIG` env for the TOML path, falls back to
    `/etc/webhook-sync/config.toml`."""
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    config_path = os.environ.get(
        "WEBHOOK_SYNC_CONFIG", "/etc/webhook-sync/config.toml"
    )
    try:
        cfg = load_config(config_path)
    except ConfigError as e:
        logger.error("config error: %s", e)
        raise
    storage = Storage(cfg.state_db)
    return create_app(cfg, storage)


# Uvicorn entry: `webhook_sync.app:app`
app = None  # populated below only when this module is imported by uvicorn
if os.environ.get("WEBHOOK_SYNC_AUTOSTART", "1") == "1":
    try:
        app = build_default_app()
    except Exception:  # pragma: no cover
        # Let uvicorn surface the error; tests import create_app directly.
        raise
