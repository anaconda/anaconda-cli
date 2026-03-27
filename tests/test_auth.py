"""Integration tests for authentication commands."""

from __future__ import annotations

import json
from pathlib import Path

from conftest import AnaRunner
from mock_auth_server import MOCK_API_KEY


class TestLogin:
    """Tests for 'ana login' command."""

    def test_login_shows_verification_url(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Login should display verification URL to user."""
        result = run_ana("login", env=auth_env)

        assert result.returncode == 0
        assert "To authenticate, visit:" in result.stdout

    def test_login_shows_success_message(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Login should show success message after authentication."""
        result = run_ana("login", env=auth_env)

        assert result.returncode == 0
        assert "Successfully authenticated!" in result.stdout

    def test_login_retrieves_api_key(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Login should store API key in keyring."""
        import base64

        result = run_ana("login", env=auth_env)

        assert result.returncode == 0
        # Verify API key is stored in keyring file
        keyring_path = Path(auth_env["HOME"]) / ".ana" / "keyring"
        assert keyring_path.exists(), "Keyring file should be created"
        keyring_data = json.loads(keyring_path.read_text())
        domain = auth_env["ANA_DOMAIN"]
        # Keyring format: {"Anaconda Cloud": {"domain": "base64-encoded-credential"}}
        assert "Anaconda Cloud" in keyring_data
        assert domain in keyring_data["Anaconda Cloud"]
        credential = json.loads(
            base64.b64decode(keyring_data["Anaconda Cloud"][domain])
        )
        assert credential["api_key"] == MOCK_API_KEY

    def test_login_shows_creating_api_key_message(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Login should show message when creating API key."""
        result = run_ana("login", env=auth_env)

        assert result.returncode == 0
        assert "Creating API key..." in result.stdout
