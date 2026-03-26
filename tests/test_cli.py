"""Integration tests for the ana CLI."""

from __future__ import annotations

import re

from conftest import AnaRunner


class TestHelp:
    """Tests for --help output."""

    def test_help_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("--help")
        assert result.returncode == 0
        assert "Usage: ana [command] [options]" in result.stdout

    def test_help_short_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("-h")
        assert result.returncode == 0
        assert "Usage: ana [command] [options]" in result.stdout

    def test_no_args_shows_help(self, run_ana: AnaRunner) -> None:
        result = run_ana()
        assert result.returncode == 0
        assert "Usage: ana [command] [options]" in result.stdout

    def test_help_shows_self_command(self, run_ana: AnaRunner) -> None:
        result = run_ana("--help")
        assert "self" in result.stdout
        assert "Manage the ana installation" in result.stdout

    def test_help_shows_options(self, run_ana: AnaRunner) -> None:
        result = run_ana("--help")
        assert "--version" in result.stdout
        assert "--help" in result.stdout


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
        assert "Usage: ana self <command>" in result.stdout

    def test_self_update_listed(self, run_ana: AnaRunner) -> None:
        result = run_ana("self")
        assert "update" in result.stdout


class TestSelfUpdateNoToken:
    """Tests for self update commands without GITHUB_TOKEN."""

    def test_update_check_without_token(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "update", "--check")
        # Should fail or show error about missing token
        assert "GITHUB_TOKEN" in result.stderr or result.returncode != 0

    def test_update_list_without_token(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "update", "--list")
        assert "GITHUB_TOKEN" in result.stderr or result.returncode != 0

    def test_update_without_token(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "update")
        assert "GITHUB_TOKEN" in result.stderr or result.returncode != 0


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
