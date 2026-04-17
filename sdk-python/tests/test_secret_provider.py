"""Tests for SecretProvider and SecretProviderSpec."""

import textwrap
from pathlib import Path

import pytest
import yaml

from lsp_brains import SecretProvider, SecretProviderSpec


# ── Fixtures ──────────────────────────────────────────────────────────────────

class EnvProvider(SecretProvider):
    spec = SecretProviderSpec(
        name="test-env",
        description="Test env provider",
        reference_template="import os\n{env_var} = os.environ[\"{env_var}\"]",
        access_pattern="{env_var}",
    )


class VaultProvider(SecretProvider):
    spec = SecretProviderSpec(
        name="test-vault",
        description="Test Vault provider",
        reference_template=(
            "import hvac\n"
            "client = hvac.Client(url=\"{vault_url}\")\n"
            "{env_var} = client.secrets.kv.read_secret_version(path=\"{secret_path}\")"
        ),
        access_pattern="{mount}/{path}",
    )


# ── render_reference tests ─────────────────────────────────────────────────────

def test_render_reference_env():
    """Env provider renders correct os.environ access."""
    result = EnvProvider.render_reference({
        "env_var": "MY_API_KEY",
        "secret_path": "MY_API_KEY",
    })
    assert "MY_API_KEY" in result
    assert "os.environ" in result
    assert "{env_var}" not in result   # all tokens substituted


def test_render_reference_vault():
    """Vault provider substitutes env_var, secret_path, and vault_url."""
    result = VaultProvider.render_reference({
        "env_var": "DB_PASSWORD",
        "secret_path": "prod/db/password",
        "vault_url": "https://vault.internal",
    })
    assert "DB_PASSWORD" in result
    assert "prod/db/password" in result
    assert "https://vault.internal" in result
    assert "{" not in result   # no unsubstituted tokens


def test_render_reference_extra_template_vars():
    """Extra template_vars are substituted."""
    class CustomProvider(SecretProvider):
        spec = SecretProviderSpec(
            name="custom",
            description="Custom",
            reference_template="client.get(\"{namespace}\", \"{env_var}\")",
            access_pattern="{namespace}/{env_var}",
        )

    result = CustomProvider.render_reference({
        "env_var": "API_TOKEN",
        "template_vars": {"namespace": "prod"},
    })
    assert "API_TOKEN" in result
    assert "prod" in result


def test_render_reference_missing_spec_raises():
    """render_reference raises AttributeError if spec is not defined."""
    class NoSpec(SecretProvider):
        pass

    with pytest.raises(AttributeError, match="spec"):
        NoSpec.render_reference({"env_var": "X"})


# ── register() tests ──────────────────────────────────────────────────────────

def test_register_creates_manifest(tmp_path: Path):
    """register() creates .claude/secret-refs.yaml when it doesn't exist."""
    EnvProvider.register(project_root=str(tmp_path))

    manifest_path = tmp_path / ".claude" / "secret-refs.yaml"
    assert manifest_path.exists()

    with open(manifest_path) as fh:
        manifest = yaml.safe_load(fh)

    assert "providers" in manifest
    assert "test-env" in manifest["providers"]
    entry = manifest["providers"]["test-env"]
    assert entry["description"] == "Test env provider"
    assert "{env_var}" in entry["reference_template"]


def test_register_merges_existing(tmp_path: Path):
    """register() does not clobber existing manifest content."""
    # Seed manifest with an existing secret entry
    claude_dir = tmp_path / ".claude"
    claude_dir.mkdir()
    manifest_path = claude_dir / "secret-refs.yaml"
    initial = {
        "default_provider": "gcp",
        "secrets": {
            "my-secret": {
                "description": "Pre-existing secret",
                "env_var": "MY_SECRET",
                "provider": "gcp",
            }
        }
    }
    with open(manifest_path, "w") as fh:
        yaml.dump(initial, fh)

    # Register a new provider
    VaultProvider.register(project_root=str(tmp_path))

    with open(manifest_path) as fh:
        manifest = yaml.safe_load(fh)

    # Original content preserved
    assert manifest.get("default_provider") == "gcp"
    assert "my-secret" in manifest.get("secrets", {})

    # New provider added
    assert "test-vault" in manifest["providers"]


def test_register_upserts_existing_provider(tmp_path: Path):
    """register() overwrites an existing provider entry with the same name."""
    claude_dir = tmp_path / ".claude"
    claude_dir.mkdir()
    manifest_path = claude_dir / "secret-refs.yaml"
    existing = {
        "providers": {
            "test-env": {
                "description": "Old description",
                "reference_template": "old template",
                "access_pattern": "",
            }
        }
    }
    with open(manifest_path, "w") as fh:
        yaml.dump(existing, fh)

    EnvProvider.register(project_root=str(tmp_path))

    with open(manifest_path) as fh:
        manifest = yaml.safe_load(fh)

    assert manifest["providers"]["test-env"]["description"] == "Test env provider"
    assert "old template" not in manifest["providers"]["test-env"]["reference_template"]


def test_register_no_spec_raises(tmp_path: Path):
    """register() raises AttributeError if spec is not defined."""
    class BadProvider(SecretProvider):
        pass

    with pytest.raises(AttributeError, match="spec"):
        BadProvider.register(project_root=str(tmp_path))
