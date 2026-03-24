"""Integration tests for the install.sh script."""

from __future__ import annotations

import subprocess
from pathlib import Path

import pytest


def _find_repo_root() -> Path:
    """Find the repository root by looking for .git directory."""
    path = Path(__file__).resolve()
    for parent in path.parents:
        if (parent / ".git").exists():
            return parent
    raise RuntimeError("Could not find repository root")


REPO_ROOT = _find_repo_root()
SCRIPT_PATH = REPO_ROOT / "scripts" / "install.sh"


def run_script(
    *args: str,
    env: dict[str, str] | None = None,
    input: str | None = None,
) -> subprocess.CompletedProcess[str]:
    """Run the install script with given arguments."""
    return subprocess.run(
        ["sh", str(SCRIPT_PATH), *args],
        capture_output=True,
        text=True,
        env=env,
        input=input,
    )


class TestHelp:
    """Tests for --help output."""

    def test_help_short_flag(self) -> None:
        result = run_script("-h")
        assert result.returncode == 0
        assert "Usage: install.sh [OPTIONS]" in result.stdout
        assert "Install the ana CLI tool." in result.stdout

    def test_help_long_flag(self) -> None:
        result = run_script("--help")
        assert result.returncode == 0
        assert "Usage: install.sh [OPTIONS]" in result.stdout

    def test_help_shows_all_options(self) -> None:
        result = run_script("--help")
        assert "--install-dir" in result.stdout
        assert "--version" in result.stdout
        assert "--verify-checksum" in result.stdout
        assert "--no-path-update" in result.stdout
        assert "--token" in result.stdout
        assert "--force" in result.stdout
        assert "--help" in result.stdout

    def test_help_shows_environment_variables(self) -> None:
        result = run_script("--help")
        assert "ANA_INSTALL_DIR" in result.stdout
        assert "ANA_VERSION" in result.stdout
        assert "ANA_VERIFY_CHECKSUM" in result.stdout
        assert "ANA_NO_PATH_UPDATE" in result.stdout
        assert "ANA_FORCE_INSTALL" in result.stdout
        assert "GITHUB_TOKEN" in result.stdout

    def test_help_shows_examples(self) -> None:
        result = run_script("--help")
        assert "Examples:" in result.stdout
        assert "curl" in result.stdout

    @pytest.mark.parametrize(
        "expected",
        [
            pytest.param("~/.local/bin", id="install-dir"),
            pytest.param("latest", id="version"),
            pytest.param(
                "true",
                id="verify-checksum",
                marks=pytest.mark.xfail(
                    reason="Checksum verification disabled until .sha256 files published"
                ),
            ),
        ],
    )
    def test_help_shows_defaults(self, expected: str) -> None:
        result = run_script("--help")
        assert f"default: {expected})" in result.stdout
