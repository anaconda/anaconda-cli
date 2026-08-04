"""Integration tests for the 'ana org' command.

ana org forwards its arguments to the anaconda-cli package's `anaconda org`
subcommand (see src/tools/run.rs::run_tool_binary), which must be installed
separately via `ana tool install anaconda-cli` (or `ana bootstrap`). That
install takes real time (~1 minute, pulls ~200 packages via pixi), so the
cases here only cover what's reachable without it: help text (clap handles
--help itself, before any passthrough) and the "not installed" error path,
which is the first thing every real invocation hits in the isolated test
environment.
"""

from __future__ import annotations

from helpers import AnaRunner


class TestOrg:
    """Tests for 'ana org' command."""

    def test_org_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("org", "--help")
        assert result.returncode == 0
        assert "Interact with anaconda.org" in result.stdout
        assert "Usage: ana org" in result.stdout

    def test_org_short_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("org", "-h")
        assert result.returncode == 0
        assert "Interact with anaconda.org" in result.stdout

    def test_org_fails_when_anaconda_cli_not_installed(
        self, run_ana: AnaRunner
    ) -> None:
        result = run_ana("org", "whoami")
        assert result.returncode == 1
        assert "anaconda" in result.stderr.lower()
        assert "not found" in result.stderr.lower()
        assert "ana tool install anaconda-cli" in result.stderr

    def test_org_no_args_fails_when_anaconda_cli_not_installed(
        self, run_ana: AnaRunner
    ) -> None:
        """With no args, ana org still tries to invoke the proxied binary
        (there's no bare-command help to fall back to)."""
        result = run_ana("org")
        assert result.returncode == 1
        assert "not found" in result.stderr.lower()

    def test_org_forwards_hyphenated_args_without_local_parsing(
        self, run_ana: AnaRunner
    ) -> None:
        """Hyphenated args (allow_hyphen_values) reach the same "not
        installed" error rather than being misinterpreted as ana's own
        flags."""
        result = run_ana("org", "--token", "fake-token", "-c", "conda-forge")
        assert result.returncode == 1
        assert "not found" in result.stderr.lower()
