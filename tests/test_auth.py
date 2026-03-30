"""Integration tests for authentication commands."""

from __future__ import annotations

import json
from pathlib import Path

from conftest import AnaRunner
from mock_auth_server import MOCK_API_KEY
from mock_auth_server import MOCK_USER
from mock_auth_server import MockAuthServer


class TestLogin:
    """Tests for 'ana login' command."""

    def test_login_creates_keyring(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
    ) -> None:
        """Login should create keyring file with API key."""
        result = run_ana("login", env=auth_env)

        assert result.returncode == 0
        assert "To authenticate, visit:" in result.stdout
        assert "Successfully authenticated!" in result.stdout
        assert "API key saved to" in result.stdout
        assert keyring_path.exists()

    def test_login_keyring_format(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """Login should create keyring in anaconda-auth compatible format."""
        run_ana("login", env=auth_env)

        keyring_data = json.loads(keyring_path.read_text())
        assert "Anaconda Cloud" in keyring_data
        assert mock_auth_server.domain in keyring_data["Anaconda Cloud"]

    def test_login_via_auth_subcommand(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
    ) -> None:
        """'ana auth login' should work the same as 'ana login'."""
        result = run_ana("auth", "login", env=auth_env)

        assert result.returncode == 0
        assert "Successfully authenticated!" in result.stdout
        assert keyring_path.exists()


class TestLogout:
    """Tests for 'ana logout' command."""

    def test_logout_removes_key(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """Logout should remove API key from keyring."""
        # First login
        run_ana("login", env=auth_env)
        assert keyring_path.exists()

        # Then logout
        result = run_ana("logout", env=auth_env)

        assert result.returncode == 0
        assert f"Logged out from {mock_auth_server.domain}" in result.stdout
        # Keyring should be deleted when empty
        assert not keyring_path.exists()

    def test_logout_when_not_logged_in(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Logout when not logged in should succeed silently."""
        result = run_ana("logout", env=auth_env)

        assert result.returncode == 0

    def test_logout_via_auth_subcommand(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """'ana auth logout' should work the same as 'ana logout'."""
        run_ana("login", env=auth_env)
        result = run_ana("auth", "logout", env=auth_env)

        assert result.returncode == 0
        assert f"Logged out from {mock_auth_server.domain}" in result.stdout


# TODO: Update once we validate what we want output to look like


class TestWhoami:
    """Tests for 'ana whoami' command."""

    def test_whoami_when_logged_in(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        mock_auth_server: MockAuthServer,
    ) -> None:
        """Whoami should display user info when logged in."""
        run_ana("login", env=auth_env)
        result = run_ana("whoami", env=auth_env)

        assert result.returncode == 0
        assert f"Your info ({mock_auth_server.domain}):" in result.stdout
        assert MOCK_USER["user"]["username"] in result.stdout
        assert MOCK_USER["user"]["email"] in result.stdout

    def test_whoami_when_not_logged_in(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        mock_auth_server: MockAuthServer,
    ) -> None:
        """Whoami should show helpful message when not logged in."""
        result = run_ana("whoami", env=auth_env)

        assert result.returncode == 0
        assert f"Not logged in to {mock_auth_server.domain}" in result.stdout
        assert "Run `ana login` to authenticate." in result.stdout

    def test_whoami_via_auth_subcommand(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        mock_auth_server: MockAuthServer,
    ) -> None:
        """'ana auth whoami' should work the same as 'ana whoami'."""
        run_ana("login", env=auth_env)
        result = run_ana("auth", "whoami", env=auth_env)

        assert result.returncode == 0
        assert f"Your info ({mock_auth_server.domain}):" in result.stdout


class TestApiKey:
    """Tests for 'ana auth api-key' command."""

    def test_api_key_when_logged_in(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Api-key should print the API key when logged in."""
        run_ana("login", env=auth_env)
        result = run_ana("auth", "api-key", env=auth_env)

        assert result.returncode == 0
        assert MOCK_API_KEY in result.stdout

    def test_api_key_when_not_logged_in(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        mock_auth_server: MockAuthServer,
    ) -> None:
        """Api-key should show helpful message when not logged in."""
        result = run_ana("auth", "api-key", env=auth_env)

        assert result.returncode == 0
        assert f"Not logged in to {mock_auth_server.domain}" in result.stdout
        assert "Run `ana login` to authenticate." in result.stdout

    def test_api_key_output_is_clean(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Api-key output should be just the key (for piping)."""
        run_ana("login", env=auth_env)
        result = run_ana("auth", "api-key", env=auth_env)

        # Should be just the key with a newline
        assert result.stdout.strip() == MOCK_API_KEY


class TestAuthHelp:
    """Tests for 'ana auth' help output."""

    def test_auth_shows_help(self, run_ana: AnaRunner) -> None:
        """'ana auth' should show auth command help."""
        result = run_ana("auth")

        assert result.returncode == 0
        assert "Authentication commands" in result.stdout
        assert "Usage: ana auth <command>" in result.stdout

    def test_auth_shows_all_subcommands(self, run_ana: AnaRunner) -> None:
        """Auth help should list all subcommands."""
        result = run_ana("auth")

        assert "api-key" in result.stdout
        assert "login" in result.stdout
        assert "logout" in result.stdout
        assert "whoami" in result.stdout


class TestMultipleDomains:
    """Tests for multi-domain keyring support."""

    def test_login_to_multiple_domains(
        self,
        run_ana: AnaRunner,
        env_isolated: dict[str, str],
        keyring_path: Path,
    ) -> None:
        """Should be able to login to multiple domains."""
        with MockAuthServer() as server1, MockAuthServer() as server2:
            env1 = {
                **env_isolated,
                "ANA_AUTH_DOMAIN": server1.domain,
                "ANA_KEYRING_PATH": str(keyring_path),
                "ANA_OPEN_BROWSER": "false",
                "ANA_USE_HTTPS": "false",
            }
            env2 = {
                **env_isolated,
                "ANA_AUTH_DOMAIN": server2.domain,
                "ANA_KEYRING_PATH": str(keyring_path),
                "ANA_OPEN_BROWSER": "false",
                "ANA_USE_HTTPS": "false",
            }

            # Login to both
            run_ana("login", env=env1)
            run_ana("login", env=env2)

            # Both should be in keyring
            keyring_data = json.loads(keyring_path.read_text())
            assert server1.domain in keyring_data["Anaconda Cloud"]
            assert server2.domain in keyring_data["Anaconda Cloud"]

            # Logout from one, other should remain
            run_ana("logout", env=env1)
            keyring_data = json.loads(keyring_path.read_text())
            assert server1.domain not in keyring_data["Anaconda Cloud"]
            assert server2.domain in keyring_data["Anaconda Cloud"]

            # Logout from other, keyring file deleted since no more credentials stored
            run_ana("logout", env=env2)
            assert not keyring_path.exists()
