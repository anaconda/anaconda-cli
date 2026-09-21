"""Integration tests for the 'ana org' command.

ana org is a thin wrapper around the anaconda-cli package's `anaconda org`
subcommand (see src/anaconda_cli.rs). It calls ensure_tool first, so missing
anaconda-cli installs are created automatically (~1 minute, pulls ~200
packages via pixi). The `--help`/`-h` flags are forwarded to the wrapped tool
rather than rendered by ana.
"""

from __future__ import annotations

from pathlib import Path

from helpers import AnaRunner


class TestOrg:
    """Tests for 'ana org' command."""

    def test_org_help_auto_installs_and_forwards(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """--help triggers ensure_tool and is passed through to anaconda org."""
        result = run_ana("org", "--help")
        assert result.returncode == 0
        assert "anaconda org" in result.stdout

        tool_dir = fake_home / ".ana" / "tools" / "anaconda-cli"
        assert tool_dir.is_dir(), f"Tool directory not found: {tool_dir}"

    def test_org_short_help_is_forwarded(self, run_ana: AnaRunner) -> None:
        result = run_ana("org", "-h")
        assert result.returncode == 0
        assert "anaconda org" in result.stdout
