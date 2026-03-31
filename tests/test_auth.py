"""Integration tests for authentication commands."""

from __future__ import annotations

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
        """Login should retrieve and display API key."""
        result = run_ana("login", env=auth_env)

        assert result.returncode == 0
        # TODO: Replace this check with keyring verification once keyring storage is implemented
        assert MOCK_API_KEY in result.stdout

    def test_login_shows_creating_api_key_message(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Login should show message when creating API key."""
        result = run_ana("login", env=auth_env)

        assert result.returncode == 0
        assert "Creating API key..." in result.stdout
