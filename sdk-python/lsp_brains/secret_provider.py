"""
SecretProvider — extensible secret reference provider for the LSP Brains SDK.

Subclass :class:`SecretProvider` to define a custom secret manager integration,
then call :meth:`SecretProvider.register` to write it into the project's
``.claude/secret-refs.yaml`` manifest under the ``providers:`` key.

The Rust sensory tool reads the manifest and merges custom providers with its
built-in set (gcp, aws, azure, vault, env), so your provider becomes available
to all agents reading the Brain context.

Example::

    from lsp_brains import SecretProvider, SecretProviderSpec

    class MyVaultProvider(SecretProvider):
        spec = SecretProviderSpec(
            name="my-vault",
            description="Internal HashiCorp Vault with AppRole auth",
            reference_template=(
                "import hvac, os\\n"
                "client = hvac.Client(url=\\"{vault_url}\\", token=os.environ[\\"VAULT_TOKEN\\"])\\n"
                "{env_var} = client.secrets.kv.v2.read_secret_version("
                "path=\\"{secret_path}\\")[\\"data\\"][\\"data\\"][\\"value\\"]"
            ),
            access_pattern="{mount}/{path}",
        )

    # Writes provider into .claude/secret-refs.yaml
    MyVaultProvider.register(project_root=".")

Template substitution tokens (used in ``reference_template``):

    ``{env_var}``      — the environment variable name (e.g. ``DATABASE_PASSWORD``)
    ``{secret_path}``  — the secret's path in the manager (from the manifest entry)
    ``{vault_url}``    — the Vault/Azure endpoint URL (from the manifest entry)
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import yaml
    _YAML_AVAILABLE = True
except ImportError:
    _YAML_AVAILABLE = False


@dataclass
class SecretProviderSpec:
    """Specification for a custom secret provider.

    Args:
        name: Unique provider identifier (kebab-case, e.g. ``"my-vault"``).
              Used as the ``provider:`` value in secret entries.
        description: Human-readable description of this provider.
        reference_template: Python code template for accessing a secret.
              Use ``{env_var}``, ``{secret_path}``, and ``{vault_url}`` as
              substitution tokens. The rendered result appears in
              ``secret_catalog[].reference_pattern`` in the CMDB.
        access_pattern: Documents the expected shape of ``secret_path`` for
              this provider (e.g. ``"{mount}/{path}"``). Informational only.
    """

    name: str
    description: str
    reference_template: str
    access_pattern: str = ""


class SecretProvider:
    """Base class for custom secret provider implementations.

    Override :attr:`spec` with a :class:`SecretProviderSpec` instance and call
    :meth:`register` to write the provider into the project manifest.

    The provider is then available in the Rust sensory tool alongside the
    built-ins (gcp, aws, azure, vault, env) — no Rust changes required.
    """

    #: Provider specification. Override in subclass.
    spec: SecretProviderSpec

    @classmethod
    def register(cls, project_root: str = ".") -> None:
        """Write this provider's spec into ``.claude/secret-refs.yaml``.

        If the file does not exist it is created. Existing content is
        preserved — only the ``providers.<name>`` entry is upserted.

        Args:
            project_root: Path to the project root that contains ``.claude/``.

        Raises:
            ImportError: If ``pyyaml`` is not installed.
            AttributeError: If the subclass has not defined :attr:`spec`.
        """
        if not _YAML_AVAILABLE:
            raise ImportError(
                "pyyaml is required for SecretProvider.register(). "
                "Install it with: pip install pyyaml"
            )

        if not hasattr(cls, "spec") or not isinstance(cls.spec, SecretProviderSpec):
            raise AttributeError(
                f"{cls.__name__} must define a `spec` class attribute "
                "of type SecretProviderSpec."
            )

        manifest_path = Path(project_root) / ".claude" / "secret-refs.yaml"

        # Load existing manifest or start from scratch
        if manifest_path.exists():
            with open(manifest_path, encoding="utf-8") as fh:
                manifest: dict[str, Any] = yaml.safe_load(fh) or {}
        else:
            manifest = {}

        # Upsert providers section
        if "providers" not in manifest or manifest["providers"] is None:
            manifest["providers"] = {}

        manifest["providers"][cls.spec.name] = {
            "description": cls.spec.description,
            "reference_template": cls.spec.reference_template,
            "access_pattern": cls.spec.access_pattern,
        }

        # Write back — preserve existing structure
        manifest_path.parent.mkdir(parents=True, exist_ok=True)
        with open(manifest_path, "w", encoding="utf-8") as fh:
            yaml.dump(manifest, fh, default_flow_style=False, sort_keys=False, allow_unicode=True)

        print(f"[secret-refs] Registered provider '{cls.spec.name}' in {manifest_path}")

    @classmethod
    def render_reference(cls, secret_ref: dict[str, Any]) -> str:
        """Render the reference template for a specific secret entry.

        This mirrors the substitution the Rust sensory tool performs when
        building ``secret_catalog``. Useful for testing or offline rendering.

        Args:
            secret_ref: A secret entry dict (keys: ``env_var``, ``secret_path``,
                        ``vault_url``, plus any custom ``template_vars``).

        Returns:
            The rendered reference code as a string.
        """
        if not hasattr(cls, "spec"):
            raise AttributeError(f"{cls.__name__} must define a `spec` attribute.")

        rendered = cls.spec.reference_template
        substitutions = {
            "{env_var}":     secret_ref.get("env_var", "SECRET_VALUE"),
            "{secret_path}": secret_ref.get("secret_path", ""),
            "{vault_url}":   secret_ref.get("vault_url", ""),
        }
        # Allow arbitrary extra tokens via template_vars
        for key, value in secret_ref.get("template_vars", {}).items():
            substitutions[f"{{{key}}}"] = str(value)

        for token, value in substitutions.items():
            rendered = rendered.replace(token, value)

        return rendered
