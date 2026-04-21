"""Configuration loader for webhook-sync.

Same discipline as `claude-proxy/config.py`: a TOML file owns every knob;
secrets are held in environment variables and **referenced by name**
from the config. Rotation doesn't require a config-file edit; a stolen
config file doesn't disclose secrets.

Shape:

    [server]
    host = "0.0.0.0"    # bound inside the container
    port = 4747

    [data]
    state_db  = "/data/webhook-sync.sqlite"

    [defaults]
    branch = "main"
    debounce_seconds = 5
    idempotency_window_seconds = 86400    # 24h

    [agents.neurogrim-local]
    workspace  = "/workspaces/neurogrim-local"
    branch     = "main"                     # overrides [defaults.branch]
    secret_env = "NEUROGRIM_LOCAL_WEBHOOK_SECRET"

    [agents.neurogrim-external]
    workspace  = "/workspaces/neurogrim-external"
    secret_env = "NEUROGRIM_EXTERNAL_WEBHOOK_SECRET"

At load time we resolve `secret_env` → actual secret via `os.environ`.
A missing env var is a hard error during startup — better to refuse to
start than to run unauthenticated.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path

try:  # stdlib on 3.11+; backport for 3.10
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore[no-redef]


@dataclass(frozen=True)
class AgentCfg:
    label: str
    workspace: Path
    branch: str
    secret: str  # the actual secret, resolved from secret_env


@dataclass
class Config:
    host: str = "0.0.0.0"
    port: int = 4747
    state_db: Path = field(default_factory=lambda: Path("/data/webhook-sync.sqlite"))
    debounce_seconds: float = 5.0
    idempotency_window_seconds: float = 86_400.0
    default_branch: str = "main"
    agents: dict[str, AgentCfg] = field(default_factory=dict)


class ConfigError(RuntimeError):
    """Raised on bad / missing config; surfaces at startup, never in a request."""


def load_config(path: str | Path) -> Config:
    """Load and validate. Raises `ConfigError` on any issue.

    Separate from `Config()` default-construction so tests can build
    in-memory configs without writing TOML files.
    """
    path = Path(path)
    if not path.is_file():
        raise ConfigError(f"config file not found: {path}")

    raw = tomllib.loads(path.read_text(encoding="utf-8"))
    cfg = Config()

    server = raw.get("server", {})
    cfg.host = server.get("host", cfg.host)
    cfg.port = int(server.get("port", cfg.port))

    data = raw.get("data", {})
    cfg.state_db = Path(data.get("state_db", str(cfg.state_db)))

    defaults = raw.get("defaults", {})
    cfg.default_branch = defaults.get("branch", cfg.default_branch)
    cfg.debounce_seconds = float(defaults.get("debounce_seconds", cfg.debounce_seconds))
    cfg.idempotency_window_seconds = float(
        defaults.get("idempotency_window_seconds", cfg.idempotency_window_seconds)
    )

    agents_raw = raw.get("agents", {})
    if not isinstance(agents_raw, dict) or not agents_raw:
        raise ConfigError("config must declare at least one [agents.<label>] block")

    for label, spec in agents_raw.items():
        if not isinstance(spec, dict):
            raise ConfigError(f"[agents.{label}] must be a table")
        workspace = spec.get("workspace")
        if not workspace:
            raise ConfigError(f"[agents.{label}] missing required 'workspace'")
        secret_env = spec.get("secret_env")
        if not secret_env:
            raise ConfigError(f"[agents.{label}] missing required 'secret_env'")
        secret = os.environ.get(secret_env)
        if not secret:
            raise ConfigError(
                f"[agents.{label}] secret_env={secret_env!r} is not set in the environment "
                f"(refusing to start a webhook endpoint without a signing secret)"
            )
        cfg.agents[label] = AgentCfg(
            label=label,
            workspace=Path(workspace),
            branch=spec.get("branch", cfg.default_branch),
            secret=secret,
        )

    return cfg
