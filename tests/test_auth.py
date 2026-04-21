"""Integration tests for authentication commands."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from helpers import AnaRunner
from helpers import assert_output_contains
from mock_auth_server import MockAuthServer


class TestLogin:
    """Tests for 'ana login' command."""

    @pytest.mark.parametrize("args", [["login"], ["auth", "login"]])
    def test_login_creates_keyring(
        self,
        args: list[str],
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
    ) -> None:
        """Login should create keyring file with API key."""
        result = run_ana(*args, env=auth_env)

        assert result.returncode == 0
        assert_output_contains(
            result.stderr,
            "visit:",  # Message varies: "visit:" or "scan the QR code or visit:"
            "Authentication complete",
            "API key stored in keyring",
            "Logged in as",
            "test@example.com",
            "expires",
        )
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


class TestLogout:
    """Tests for 'ana logout' command."""

    @pytest.mark.parametrize("args", [["logout"], ["auth", "logout"]])
    def test_logout_removes_key(
        self,
        args: list[str],
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
        result = run_ana(*args, env=auth_env)

        assert result.returncode == 0
        assert f"Logged out of {mock_auth_server.domain}" in result.stderr
        # Keyring should be deleted when empty
        assert not keyring_path.exists()

    def test_logout_when_not_logged_in(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Logout when not logged in should warn and succeed."""
        result = run_ana("logout", env=auth_env)

        assert result.returncode == 0
        assert "Not logged in" in result.stderr


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
        # API key is a JWT (header.payload.signature format)
        assert result.stdout.strip().count(".") == 2

    def test_api_key_when_not_logged_in(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        mock_auth_server: MockAuthServer,
    ) -> None:
        """Api-key should show helpful message when not logged in."""
        result = run_ana("auth", "api-key", env=auth_env)

        assert result.returncode == 0
        assert "not logged in" in result.stderr
        assert "ana login" in result.stderr

    def test_api_key_output_is_clean(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Api-key output should be just the key (for piping)."""
        run_ana("login", env=auth_env)
        result = run_ana("auth", "api-key", env=auth_env)

        # Should be just the key with a newline (no extra output)
        lines = result.stdout.strip().split("\n")
        assert len(lines) == 1
        # API key is a JWT
        assert lines[0].count(".") == 2


class TestAuthHelp:
    """Tests for 'ana auth' help output."""

    def test_auth_shows_help(self, run_ana: AnaRunner) -> None:
        """'ana auth' should show auth command help."""
        result = run_ana("auth")

        assert result.returncode == 0
        assert "Authentication commands" in result.stdout
        assert "Usage: ana auth <command>" in result.stdout

    def test_auth_shows_subcommands(self, run_ana: AnaRunner) -> None:
        """Auth help should list subcommands."""
        result = run_ana("auth")

        assert "api-key" in result.stdout
        assert "login" in result.stdout
        assert "logout" in result.stdout
        assert "whoami" in result.stdout


class TestWhoami:
    """Tests for 'ana whoami' command."""

    @pytest.mark.parametrize("args", [["whoami"], ["auth", "whoami"]])
    def test_whoami_when_logged_in(
        self,
        args: list[str],
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
    ) -> None:
        """Whoami should display user info when logged in."""
        run_ana("login", env=auth_env)
        result = run_ana(*args, env=auth_env)

        assert result.returncode == 0
        assert_output_contains(
            result.stderr,
            "ACCOUNT",
            "Test User",
            "testuser",
            "test@example.com",
            "SUBSCRIPTIONS",
            "Test Organization",
            "2030-01-01",
            "TOKEN",
            str(keyring_path),
        )

    def test_whoami_when_not_logged_in(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Whoami should show helpful message when not logged in."""
        result = run_ana("whoami", env=auth_env)

        assert result.returncode == 0
        assert_output_contains(
            result.stderr,
            "not logged in",
            "ana login",
        )

    def test_whoami_json_flag(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Whoami with --json should output raw JSON."""
        run_ana("login", env=auth_env)
        result = run_ana("whoami", "--json", env=auth_env)

        assert result.returncode == 0
        # Should be valid JSON
        import json

        data = json.loads(result.stdout)
        assert "passport" in data
        assert "profile" in data["passport"]


class TestLoginApiKey:
    """Tests for 'ana login --api-key' option."""

    @pytest.mark.parametrize(
        "args", [["login", "--api-key"], ["auth", "login", "--api-key"]]
    )
    def test_login_api_key_reads_from_stdin_when_no_value(
        self,
        args: list[str],
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """--api-key without value should read from stdin when piped."""
        # When stdin is piped, reads API key directly (no interactive prompt)
        result = run_ana(*args, env=auth_env, input=f"{mock_auth_server.api_key}\n")

        assert result.returncode == 0
        # Should show success messages
        assert_output_contains(
            result.stderr,
            "Token stored in system keyring",
            "Logged in as",
            "test@example.com",
            "expires",
        )
        assert keyring_path.exists()

        # Verify API key was stored correctly
        api_key_result = run_ana("auth", "api-key", env=auth_env)
        assert api_key_result.stdout.strip() == mock_auth_server.api_key

    @pytest.mark.parametrize(
        "args",
        [
            # --api-key=<value> style
            ["login", "--api-key={}"],
            ["auth", "login", "--api-key={}"],
            # --api-key <value> style (space-separated)
            ["login", "--api-key", "{}"],
            ["auth", "login", "--api-key", "{}"],
        ],
    )
    def test_login_api_key_with_value(
        self,
        args: list[str],
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """--api-key=<value> and --api-key <value> should use provided value directly."""
        # Insert the API key into the args
        formatted_args = [arg.format(mock_auth_server.api_key) for arg in args]
        result = run_ana(*formatted_args, env=auth_env)

        assert result.returncode == 0
        # Should NOT prompt for API key (no "API key:" in output)
        assert "API key:" not in result.stderr
        # Should show success messages
        assert_output_contains(
            result.stderr,
            "Token stored in system keyring",
            "Logged in as",
            "test@example.com",
            "expires",
        )
        assert keyring_path.exists()

        # Verify API key was stored correctly
        api_key_result = run_ana("auth", "api-key", env=auth_env)
        assert api_key_result.stdout.strip() == mock_auth_server.api_key

    def test_login_api_key_stored_correctly(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """API key provided via --api-key should be stored and retrievable."""
        run_ana("login", f"--api-key={mock_auth_server.api_key}", env=auth_env)

        # Verify API key is retrievable via `ana auth api-key`
        result = run_ana("auth", "api-key", env=auth_env)
        assert result.returncode == 0
        assert result.stdout.strip() == mock_auth_server.api_key

    @pytest.mark.parametrize(
        "args",
        [
            # --api-key without value reads from stdin
            ["login", "--api-key"],
            ["auth", "login", "--api-key"],
            # --api-key - explicitly reads from stdin (Unix convention)
            ["login", "--api-key", "-"],
            ["auth", "login", "--api-key", "-"],
        ],
    )
    def test_login_api_key_from_stdin_with_flag(
        self,
        args: list[str],
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """API key can be piped via stdin with --api-key flag."""
        # Simulate piping: provide API key via stdin
        result = run_ana(*args, env=auth_env, input=f"{mock_auth_server.api_key}\n")

        assert result.returncode == 0
        # Should show success messages
        assert_output_contains(
            result.stderr,
            "Token stored in system keyring",
            "Logged in as",
            "test@example.com",
            "expires",
        )
        assert keyring_path.exists()

        # Verify API key was stored correctly
        api_key_result = run_ana("auth", "api-key", env=auth_env)
        assert api_key_result.stdout.strip() == mock_auth_server.api_key

    def test_login_api_key_invalid_token_format(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
    ) -> None:
        """Malformed API key (not a valid JWT) should show error."""
        result = run_ana("login", "--api-key=invalid-not-a-jwt", env=auth_env)

        assert result.returncode != 0
        # Should show some kind of error (exact message depends on implementation)
        assert "error" in result.stderr.lower() or "invalid" in result.stderr.lower()

    def test_login_api_key_when_already_logged_in_requires_force(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """--api-key when already logged in (piped) should require --force."""
        # First login via device flow
        run_ana("login", env=auth_env)

        # Get the original API key
        original_key = run_ana("auth", "api-key", env=auth_env).stdout.strip()

        # Try to login with --api-key (stdin is piped due to subprocess)
        result = run_ana("login", f"--api-key={mock_auth_server.api_key}", env=auth_env)

        assert result.returncode == 0
        # Should warn and tell user to use --force
        assert "Already logged in" in result.stderr
        assert "--force" in result.stderr
        # Original key should still be in place
        api_key_result = run_ana("auth", "api-key", env=auth_env)
        assert api_key_result.stdout.strip() == original_key

    def test_login_api_key_force_overwrites(
        self,
        run_ana: AnaRunner,
        auth_env: dict[str, str],
        keyring_path: Path,
        mock_auth_server: MockAuthServer,
    ) -> None:
        """--api-key --force should overwrite without confirmation."""
        # First login via device flow
        run_ana("login", env=auth_env)

        # Login with --api-key --force (no stdin input needed)
        result = run_ana(
            "login", f"--api-key={mock_auth_server.api_key}", "--force", env=auth_env
        )

        assert result.returncode == 0
        # Should NOT prompt for confirmation
        assert "overwrite" not in result.stderr.lower()
        assert "[y/N]" not in result.stderr
        # Should show success
        assert_output_contains(
            result.stderr,
            "Token stored in system keyring",
            "Logged in as",
        )

        # Verify API key was overwritten
        api_key_result = run_ana("auth", "api-key", env=auth_env)
        assert api_key_result.stdout.strip() == mock_auth_server.api_key


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
                "ANA_DOMAIN": server1.domain,
                "ANA_KEYRING_PATH": str(keyring_path),
                "ANA_OPEN_BROWSER": "false",
                "ANA_USE_HTTPS": "false",
            }
            env2 = {
                **env_isolated,
                "ANA_DOMAIN": server2.domain,
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
