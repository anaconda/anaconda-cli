"""Integration tests for tool management and task running."""

from __future__ import annotations

import subprocess
from pathlib import Path
from textwrap import dedent

import pytest
from conftest import AnaRunner


class TestToolInstallHelp:
    """Tests for tool command help."""

    def test_tool_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool", "--help")
        assert result.returncode == 0
        assert "Manage tools" in result.stdout
        assert "install" in result.stdout

    def test_tool_no_args_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool")
        assert result.returncode == 0
        assert "Manage tools" in result.stdout
        assert "install" in result.stdout

    def test_tool_install_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool", "install", "--help")
        assert result.returncode == 0
        assert "Install a tool" in result.stdout


class TestToolInstallPixi:
    """Tests for 'ana tool install pixi' subcommand."""

    def test_tool_install_pixi(self, run_ana: AnaRunner, fake_home: Path) -> None:
        """Test that tool install pixi installs pixi to ~/.ana/tools."""
        result = run_ana("tool", "install", "pixi")
        assert result.returncode == 0
        assert "pixi" in result.stderr

        # Verify the tool directory was created
        tool_dir = fake_home / ".ana" / "tools" / "pixi"
        assert tool_dir.exists(), f"Tool directory not found: {tool_dir}"
        assert tool_dir.is_dir()

    def test_tool_install_pixi_creates_symlink(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool install creates a symlinked pixi binary in ~/.ana/bin."""
        result = run_ana("tool", "install", "pixi")
        assert result.returncode == 0

        bin_path = fake_home / ".ana" / "bin" / "pixi"
        assert bin_path.exists(), f"Binary not found: {bin_path}"
        assert bin_path.is_symlink(), f"Binary is not a symlink: {bin_path}"

    def test_tool_install_pixi_already_installed(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that running tool install twice shows already up to date."""
        # First run installs
        first_result = run_ana("tool", "install", "pixi")
        assert first_result.returncode == 0

        bin_path = fake_home / ".ana" / "bin" / "pixi"
        assert bin_path.exists()

        # Second run should indicate already up to date
        second_result = run_ana("tool", "install", "pixi")
        assert second_result.returncode == 0
        assert "up to date" in second_result.stderr.lower()

    def test_tool_install_pixi_binary_runs(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that the installed pixi binary runs."""
        result = run_ana("tool", "install", "pixi")
        assert result.returncode == 0

        bin_path = fake_home / ".ana" / "bin" / "pixi"
        assert bin_path.exists()

        proc = subprocess.run(
            [str(bin_path), "--version"],
            capture_output=True,
            text=True,
        )
        assert proc.returncode == 0
        assert "pixi" in proc.stdout.lower()

    def test_tool_install_unknown_tool(self, run_ana: AnaRunner) -> None:
        """Test that installing an unknown tool fails with error."""
        result = run_ana("tool", "install", "nonexistent-tool")
        assert result.returncode != 0
        assert "unknown tool" in result.stderr.lower()


class TestRunHelp:
    """Tests for run command help."""

    def test_run_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("run", "--help")
        assert result.returncode == 0
        assert "Run a task" in result.stdout


class TestRunTask:
    """Tests for 'ana run' command with pixi projects."""

    @pytest.fixture
    def pixi_project(self, fake_home: Path) -> Path:
        """Create a minimal pixi project with a hello task."""
        project_dir = fake_home / "project"
        project_dir.mkdir()

        pixi_toml = project_dir / "pixi.toml"
        pixi_toml.write_text(
            dedent(
                """\
                [project]
                name = "test-project"
                channels = ["https://repo.anaconda.com/pkgs/main"]
                platforms = ["osx-arm64", "osx-64", "linux-64"]

                [tasks]
                hello = "echo 'Hello from pixi!'"
                """
            )
        )
        return project_dir

    def test_run_auto_installs_tool(
        self, run_ana: AnaRunner, fake_home: Path, pixi_project: Path
    ) -> None:
        """Test that ana run auto-installs pixi if not present."""
        # Verify pixi is not installed
        bin_path = fake_home / ".ana" / "bin" / "pixi"
        assert not bin_path.exists()

        # Run a task - should auto-install pixi
        run_ana("run", "hello", cwd=pixi_project)

        # Check pixi was installed
        assert bin_path.exists(), "pixi should be auto-installed"

    def test_run_executes_task(
        self, run_ana: AnaRunner, fake_home: Path, pixi_project: Path
    ) -> None:
        """Test that ana run executes the task and produces output."""
        result = run_ana("run", "hello", cwd=pixi_project)
        assert result.returncode == 0
        assert "Hello from pixi!" in result.stdout

    def test_run_detects_project_type(
        self, run_ana: AnaRunner, fake_home: Path, pixi_project: Path
    ) -> None:
        """Test that ana run detects the pixi project type."""
        result = run_ana("run", "hello", cwd=pixi_project)
        assert (
            "Detected project type" in result.stderr or "pixi" in result.stderr.lower()
        )

    def test_run_no_project_fails(self, run_ana: AnaRunner, fake_home: Path) -> None:
        """Test that ana run fails when no project is detected."""
        empty_dir = fake_home / "empty"
        empty_dir.mkdir()

        result = run_ana("run", "hello", cwd=empty_dir)
        assert result.returncode != 0
        assert "no supported project" in result.stderr.lower()

    def test_run_no_task_fails(
        self, run_ana: AnaRunner, fake_home: Path, pixi_project: Path
    ) -> None:
        """Test that ana run fails when no task is specified."""
        result = run_ana("run", cwd=pixi_project)
        assert result.returncode != 0
