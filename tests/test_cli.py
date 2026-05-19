"""Integration tests for the ana CLI."""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

import pytest
from helpers import AnaRunner

IS_WINDOWS = sys.platform == "win32"
ANACONDA_BIN = "anaconda.exe" if IS_WINDOWS else "anaconda"


class TestHelp:
    """Tests for --help output."""

    def test_help_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("--help")
        assert result.returncode == 0
        assert "Manage your" in result.stdout

    def test_help_short_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("-h")
        assert result.returncode == 0
        assert "Manage your" in result.stdout

    def test_no_args_shows_help(self, run_ana: AnaRunner) -> None:
        result = run_ana()
        assert result.returncode == 0
        assert "Manage your" in result.stdout

    def test_help_shows_version_in_header(self, run_ana: AnaRunner) -> None:
        result = run_ana("--help")
        # Header should be "ana {version}" on first line (with optional indent and dev suffix)
        first_line = result.stdout.split("\n")[0]
        assert re.match(r"\s*ana \d+\.\d+\.\d+(\.dev\d+)?", first_line)

    def test_help_shows_self_command(self, run_ana: AnaRunner) -> None:
        result = run_ana("--help")
        assert "self" in result.stdout
        assert "Manage the ana installation" in result.stdout

    def test_help_shows_options(self, run_ana: AnaRunner) -> None:
        result = run_ana("--help")
        assert "-V, --version" in result.stdout
        assert "-h, --help" in result.stdout


class TestSubcommandHelp:
    """Tests for --help on subcommands."""

    def test_bootstrap_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("bootstrap", "--help")
        assert result.returncode == 0
        assert "Install the Anaconda CLI" in result.stdout
        assert "Usage: ana bootstrap" in result.stdout

    def test_config_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("config", "--help")
        assert result.returncode == 0
        assert "Show current configuration" in result.stdout
        assert "Usage: ana config" in result.stdout

    def test_login_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("login", "--help")
        assert result.returncode == 0
        assert "Log in to Anaconda" in result.stdout
        assert "Usage: ana login" in result.stdout

    def test_logout_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("logout", "--help")
        assert result.returncode == 0
        assert "Log out from Anaconda" in result.stdout
        assert "Usage: ana logout" in result.stdout

    def test_whoami_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("whoami", "--help")
        assert result.returncode == 0
        assert "Display information about the logged-in user" in result.stdout
        assert "Usage: ana whoami" in result.stdout

    def test_self_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "--help")
        assert result.returncode == 0
        assert "Manage the ana installation" in result.stdout
        assert "Usage: ana self" in result.stdout
        assert "COMMANDS" in result.stdout
        assert "update" in result.stdout

    def test_auth_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("auth", "--help")
        assert result.returncode == 0
        assert "Authentication commands" in result.stdout
        assert "Usage: ana auth" in result.stdout
        assert "COMMANDS" in result.stdout
        assert "api-key" in result.stdout

    def test_org_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("org", "--help")
        assert result.returncode == 0
        assert "Interact with anaconda.org" in result.stdout
        assert "Usage: ana org" in result.stdout


class TestVersion:
    """Tests for --version output."""

    def test_version_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("--version")
        assert result.returncode == 0
        assert result.stdout.strip()  # Not empty

    def test_version_short_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("-V")
        assert result.returncode == 0
        assert result.stdout.strip()  # Not empty

    def test_version_format(self, run_ana: AnaRunner) -> None:
        result = run_ana("--version")
        assert result.returncode == 0
        version = result.stdout.strip()
        # Should match semver pattern (possibly with dev suffix like .dev0)
        assert re.match(r"\d+\.\d+\.\d+", version)


class TestSelfCommand:
    """Tests for 'ana self' subcommand."""

    def test_self_shows_usage(self, run_ana: AnaRunner) -> None:
        result = run_ana("self")
        assert result.returncode == 0
        assert "Usage: ana self" in result.stdout

    def test_self_shows_update_command(self, run_ana: AnaRunner) -> None:
        result = run_ana("self")
        assert "update" in result.stdout
        assert "Manage your ana version" in result.stdout


# Skip static hosting tests if Cloudflare credentials aren't available
# (the endpoint is protected by Cloudflare Zero Trust)
requires_cloudflare = pytest.mark.skipif(
    not os.environ.get("CF_ACCESS_CLIENT_ID")
    or not os.environ.get("CF_ACCESS_CLIENT_SECRET"),
    reason="Cloudflare credentials not available (CF_ACCESS_CLIENT_ID, CF_ACCESS_CLIENT_SECRET)",
)


@requires_cloudflare
class TestSelfUpdateStaticHosting:
    """Tests for self update with static hosting."""

    def test_update_list(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "update", "--list")
        assert result.returncode == 0
        # Should show version list
        assert "v0.0" in result.stderr

    def test_update_check_shows_status(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "update", "--check")
        assert result.returncode == 0
        # Should show either "Update available" or "Already up to date"
        assert "Update available" in result.stderr or "UP TO DATE" in result.stderr


class TestSelfUpdateGitHubFallback:
    """Tests for self update with GitHub fallback."""

    @pytest.fixture(scope="class")
    def self_update_env(self):
        env = {"ANA_SELF_UPDATE_URL": "github"}
        if token := os.environ.get("GITHUB_TOKEN"):
            env["GITHUB_TOKEN"] = token
        return env

    def test_update_list(
        self, run_ana: AnaRunner, self_update_env: dict[str, str]
    ) -> None:
        result = run_ana(
            "self",
            "update",
            "--list",
            env=self_update_env,
        )
        assert result.returncode == 0
        # Should show version list
        assert "v0.0" in result.stderr

    def test_update_check_shows_status(
        self, run_ana: AnaRunner, self_update_env: dict[str, str]
    ) -> None:
        result = run_ana(
            "self",
            "update",
            "--check",
            env=self_update_env,
        )
        assert result.returncode == 0
        # Should show either "Update available" or "Already up to date"
        assert "Update available" in result.stderr or "UP TO DATE" in result.stderr


class TestConfig:
    """Tests for 'ana config' subcommand."""

    def test_config_shows_table(self, run_ana: AnaRunner) -> None:
        result = run_ana("config")
        assert result.returncode == 0
        # Should be a unicode table
        assert "┌" in result.stdout
        assert "└" in result.stdout

    def test_config_shows_headers(self, run_ana: AnaRunner) -> None:
        result = run_ana("config")
        assert result.returncode == 0
        assert "Setting" in result.stdout
        assert "Value" in result.stdout

    def test_config_shows_all_settings(self, run_ana: AnaRunner) -> None:
        result = run_ana("config")
        assert result.returncode == 0
        assert "domain" in result.stdout
        assert "client_id" in result.stdout
        assert "ssl_verify" in result.stdout
        assert "open_browser" in result.stdout

    def test_config_shows_default_values(self, run_ana: AnaRunner) -> None:
        result = run_ana("config")
        assert result.returncode == 0
        assert "anaconda.com" in result.stdout
        assert "true" in result.stdout  # ssl_verify and open_browser defaults

    def test_config_respects_env_override(self, run_ana: AnaRunner) -> None:
        result = run_ana("config", env={"ANA_DOMAIN": "custom.example.com"})
        assert result.returncode == 0
        assert "custom.example.com" in result.stdout


class TestLogin:
    """Tests for 'ana login' subcommand."""

    def test_help_shows_login_command(self, run_ana: AnaRunner) -> None:
        result = run_ana("--help")
        assert result.returncode == 0
        assert "login" in result.stdout
        assert "Log in to Anaconda" in result.stdout


class TestBootstrap:
    """Tests for 'ana bootstrap' subcommand."""

    def test_bootstrap_installs_anaconda_cli(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that bootstrap installs anaconda-cli to ~/.ana/tools."""
        result = run_ana("bootstrap")
        assert result.returncode == 0
        assert "anaconda-cli" in result.stderr

        # Verify the tool directory was created
        tool_dir = fake_home / ".ana" / "tools" / "anaconda-cli"
        assert tool_dir.exists(), f"Tool directory not found: {tool_dir}"
        assert tool_dir.is_dir()

    def test_bootstrap_creates_symlinked_binary(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that bootstrap creates a symlinked anaconda binary in ~/.ana/bin."""
        result = run_ana("bootstrap")
        assert result.returncode == 0

        # Verify the symlinked binary exists
        bin_path = fake_home / ".ana" / "bin" / ANACONDA_BIN
        assert bin_path.exists(), f"Binary not found: {bin_path}"
        tools_dir = fake_home / ".ana" / "tools"
        if IS_WINDOWS:
            shim_cfg = tools_dir / "shims.cfg"
            assert (
                "anaconda=anaconda-cli\\Scripts\\anaconda.exe\r\n"
                in shim_cfg.read_text(newline="")
            )
        else:
            src_file = tools_dir / "anaconda-cli" / "bin" / "anaconda"
            assert bin_path.is_symlink(), f"Binary is not a symlink: {bin_path}"
            assert bin_path.samefile(src_file)

    def test_bootstrap_already_installed(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that running bootstrap twice shows already installed message."""
        # First run installs
        first_result = run_ana("bootstrap")
        assert first_result.returncode == 0

        # Verify installation exists
        bin_path = fake_home / ".ana" / "bin" / ANACONDA_BIN
        assert bin_path.exists()

        # Second run should indicate already installed
        second_result = run_ana("bootstrap")
        assert second_result.returncode == 0
        assert "already installed" in second_result.stderr

    def test_bootstrap_anaconda_binary_runs(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that the installed anaconda binary runs and outputs help."""
        result = run_ana("bootstrap")
        assert result.returncode == 0

        # Run the installed anaconda binary
        bin_path = fake_home / ".ana" / "bin" / ANACONDA_BIN
        assert bin_path.exists()

        proc = subprocess.run(
            [str(bin_path), "--help"],
            capture_output=True,
            text=True,
        )
        assert proc.returncode == 0
        assert "anaconda" in proc.stdout.lower() or "usage" in proc.stdout.lower()


class TestArgumentErrors:
    """Tests for CLI argument parsing and error handling."""

    def test_unknown_command(self, run_ana: AnaRunner) -> None:
        result = run_ana("foobar")
        assert result.returncode == 1
        assert "Unknown command: foobar" in result.stderr

    def test_unknown_self_command(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "foobar")
        assert result.returncode == 1
        assert "Unknown self command: foobar" in result.stderr


class TestHelpWithArguments:
    """Tests for --help when arguments are provided before the flag."""

    def test_feature_enable_help_with_argument(self, run_ana: AnaRunner) -> None:
        """Test that 'ana feature enable main-x --help' shows help, not an error."""
        with_arg = run_ana("feature", "enable", "main-x", "--help")
        without_arg = run_ana("feature", "enable", "--help")

        assert with_arg.returncode == 0
        assert without_arg.returncode == 0
        assert with_arg.stdout == without_arg.stdout

    def test_feature_disable_help_with_argument(self, run_ana: AnaRunner) -> None:
        """Test that 'ana feature disable main-x --help' shows help, not an error."""
        with_arg = run_ana("feature", "disable", "main-x", "--help")
        without_arg = run_ana("feature", "disable", "--help")

        assert with_arg.returncode == 0
        assert without_arg.returncode == 0
        assert with_arg.stdout == without_arg.stdout

    def test_tool_install_help_with_argument(self, run_ana: AnaRunner) -> None:
        """Test that 'ana tool install some-tool --help' shows help, not an error."""
        with_arg = run_ana("tool", "install", "some-tool", "--help")
        without_arg = run_ana("tool", "install", "--help")

        assert with_arg.returncode == 0
        assert without_arg.returncode == 0
        assert with_arg.stdout == without_arg.stdout


class TestBinaryFeatures:
    """Tests for binary feature configuration."""

    def test_sentry_not_enabled_by_default(self, ana_binary: Path | None) -> None:
        """Verify that Sentry is not compiled into the default binary.

        The diagnostics feature (Sentry) should be opt-in, not default.
        This test checks that the binary does not contain Sentry-specific strings
        that would only be present if the sentry crate is compiled in.
        """
        if ana_binary is None:
            pytest.skip("ana binary not found")

        binary_content = ana_binary.read_bytes()

        # These strings are unique to the Sentry SDK and only appear when compiled with diagnostics
        sentry_indicators = [
            b"[sentry] initialized",  # Sentry initialization log message
            b"X-Sentry-Auth",  # Sentry auth header
            b"sentry-transport",  # Sentry transport module
        ]

        for indicator in sentry_indicators:
            assert indicator not in binary_content, (
                f"Binary contains Sentry indicator '{indicator.decode()}' - "
                "diagnostics feature should not be enabled by default"
            )
