"""SQLite-backed state for the webhook-sync service.

Two tables:

- `deliveries(delivery_id, received_at, agent)` — per-delivery idempotency.
  Duplicate `X-GitHub-Delivery` UUIDs within the configured window return
  200 without re-pulling. Outside the window, old rows may be purged.

- `pulls(agent, last_pull_at)` — per-agent debounce. Rapid-fire pushes to
  the same repo trigger at most one pull per `debounce_seconds` window.

Both kept in one file so the operator has one thing to back up / rotate.
The service will auto-create the file + schema on first open, like the
a2a `token_store` in neurogrim-a2a.

Time is injected as a callable so tests can feed fake clocks without
monkey-patching stdlib.
"""

from __future__ import annotations

import sqlite3
import time
from pathlib import Path
from typing import Callable

TimeFn = Callable[[], float]


class Storage:
    def __init__(self, db_path: str | Path, *, time_fn: TimeFn = time.time) -> None:
        self._db_path = str(db_path)
        self._time_fn = time_fn
        # `check_same_thread=False` lets FastAPI's threadpool executor share
        # the connection; our access is serialized by a module-level lock
        # in `app.py`. For higher concurrency, upgrade to a connection pool.
        self._conn = sqlite3.connect(self._db_path, check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode = WAL")
        self._init_schema()

    def _init_schema(self) -> None:
        self._conn.executescript(
            """
            CREATE TABLE IF NOT EXISTS deliveries (
                delivery_id TEXT PRIMARY KEY,
                received_at REAL NOT NULL,
                agent       TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_deliveries_received_at
                ON deliveries(received_at);

            CREATE TABLE IF NOT EXISTS pulls (
                agent         TEXT PRIMARY KEY,
                last_pull_at  REAL NOT NULL
            );
            """
        )
        self._conn.commit()

    # -------------------- delivery idempotency --------------------

    def delivery_seen(self, delivery_id: str, within_seconds: float) -> bool:
        """Return True iff we've seen this delivery_id within the last
        `within_seconds`. Caller interprets True as "skip, but 200."
        """
        if not delivery_id:
            return False
        now = self._time_fn()
        cutoff = now - within_seconds
        row = self._conn.execute(
            "SELECT received_at FROM deliveries WHERE delivery_id = ?",
            (delivery_id,),
        ).fetchone()
        if row is None:
            return False
        return row[0] >= cutoff

    def record_delivery(self, delivery_id: str, agent: str) -> None:
        if not delivery_id:
            return
        now = self._time_fn()
        self._conn.execute(
            "INSERT OR REPLACE INTO deliveries "
            "(delivery_id, received_at, agent) VALUES (?, ?, ?)",
            (delivery_id, now, agent),
        )
        self._conn.commit()

    def purge_deliveries_older_than(self, seconds: float) -> int:
        cutoff = self._time_fn() - seconds
        cur = self._conn.execute(
            "DELETE FROM deliveries WHERE received_at < ?",
            (cutoff,),
        )
        self._conn.commit()
        return cur.rowcount

    # -------------------- per-agent debounce --------------------

    def seconds_since_last_pull(self, agent: str) -> float | None:
        """Seconds since the last successful pull for `agent`, or None if
        we've never pulled. Used by the debounce check.
        """
        row = self._conn.execute(
            "SELECT last_pull_at FROM pulls WHERE agent = ?",
            (agent,),
        ).fetchone()
        if row is None:
            return None
        return self._time_fn() - row[0]

    def record_pull(self, agent: str) -> None:
        now = self._time_fn()
        self._conn.execute(
            "INSERT OR REPLACE INTO pulls (agent, last_pull_at) VALUES (?, ?)",
            (agent, now),
        )
        self._conn.commit()

    # -------------------- lifecycle --------------------

    def close(self) -> None:
        self._conn.close()
