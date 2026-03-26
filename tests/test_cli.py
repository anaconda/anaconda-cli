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
