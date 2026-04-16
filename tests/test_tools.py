"""Integration tests for tool management."""

from __future__ import annotations

import subprocess
from pathlib import Path

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


class TestToolList:
    """Tests for 'ana tool list' subcommand."""

    def test_tool_list_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool", "list", "--help")
        assert result.returncode == 0
        assert "List available tools" in result.stdout

    def test_tool_list_shows_tools(self, run_ana: AnaRunner) -> None:
        """Test that tool list shows available tools."""
        result = run_ana("tool", "list")
        assert result.returncode == 0
        assert "Name" in result.stdout
        assert "Installed" in result.stdout
        assert "Binaries" in result.stdout
        assert "pixi" in result.stdout
        assert "anaconda-cli" in result.stdout

    def test_tool_list_shows_installed_status(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool list correctly shows installation status."""
        # Before install, should show ✗
        result_before = run_ana("tool", "list")
        assert result_before.returncode == 0
        # pixi should show as not installed (find line with pixi but not anaconda)
        lines_before = result_before.stdout.split("\n")
        pixi_line_before = [
            line
            for line in lines_before
            if "pixi" in line.lower() and "anaconda" not in line.lower()
        ][0]
        assert "✗" in pixi_line_before

        # Install pixi
        install_result = run_ana("tool", "install", "pixi")
        assert install_result.returncode == 0

        # After install, should show ✓
        result_after = run_ana("tool", "list")
        assert result_after.returncode == 0
        lines_after = result_after.stdout.split("\n")
        pixi_line_after = [
            line
            for line in lines_after
            if "pixi" in line.lower() and "anaconda" not in line.lower()
        ][0]
        assert "✓" in pixi_line_after


class TestToolUninstall:
    """Tests for 'ana tool uninstall' subcommand."""

    def test_tool_uninstall_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool", "uninstall", "--help")
        assert result.returncode == 0
        assert "Uninstall a tool" in result.stdout

    def test_tool_uninstall_unknown_tool(self, run_ana: AnaRunner) -> None:
        """Test that uninstalling an unknown tool fails with error."""
        result = run_ana("tool", "uninstall", "nonexistent-tool")
        assert result.returncode != 0
        assert "unknown tool" in result.stderr.lower()

    def test_tool_uninstall_not_installed(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that uninstalling a tool that isn't installed is a no-op."""
        result = run_ana("tool", "uninstall", "pixi", "--yes")
        assert result.returncode == 0
        assert "not installed" in result.stderr.lower()

    def test_tool_uninstall_pixi(self, run_ana: AnaRunner, fake_home: Path) -> None:
        """Test that tool uninstall removes the tool and cleans up."""
        # First install
        install_result = run_ana("tool", "install", "pixi")
        assert install_result.returncode == 0

        tool_dir = fake_home / ".ana" / "tools" / "pixi"
        bin_path = fake_home / ".ana" / "bin" / "pixi"
        assert tool_dir.exists()
        assert bin_path.exists()

        # Then uninstall (with --yes to skip prompt)
        uninstall_result = run_ana("tool", "uninstall", "pixi", "--yes")
        assert uninstall_result.returncode == 0
        assert "Successfully uninstalled" in uninstall_result.stderr

        # Verify removal
        assert not tool_dir.exists(), "Tool directory should be removed"
        assert not bin_path.exists(), "Symlink should be removed"

    def test_tool_uninstall_shows_what_will_be_removed(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that uninstall shows what will be deleted before prompting."""
        # First install
        install_result = run_ana("tool", "install", "pixi")
        assert install_result.returncode == 0

        # Run uninstall with --yes and check output
        uninstall_result = run_ana("tool", "uninstall", "pixi", "--yes")
        assert uninstall_result.returncode == 0
        assert "The following will be removed:" in uninstall_result.stderr
        assert ".ana/bin/pixi" in uninstall_result.stderr
        assert ".ana/tools/pixi" in uninstall_result.stderr


class TestToolInstallConda:
    """Tests for 'ana tool install conda' subcommand."""

    def test_tool_install_conda(self, run_ana: AnaRunner, fake_home: Path) -> None:
        """Test that tool install conda installs conda to ~/.ana/tools."""
        result = run_ana("tool", "install", "conda")
        assert result.returncode == 0
        assert "conda" in result.stderr

        # Verify the tool directory was created
        tool_dir = fake_home / ".ana" / "tools" / "conda"
        assert tool_dir.exists(), f"Tool directory not found: {tool_dir}"
        assert tool_dir.is_dir()

    def test_tool_install_conda_creates_wrapper_symlink(
        self, run_ana: AnaRunner, fake_home: Path, ana_binary: Path
    ) -> None:
        """Test that tool install creates a wrapper symlink that points to ana."""
        result = run_ana("tool", "install", "conda")
        assert result.returncode == 0
        assert "(wrapper)" in result.stderr

        bin_path = fake_home / ".ana" / "bin" / "conda"
        assert bin_path.exists(), f"Binary not found: {bin_path}"
        assert bin_path.is_symlink(), f"Binary is not a symlink: {bin_path}"

        # Verify the symlink points to the ana binary, not to the actual conda
        target = bin_path.resolve()
        assert target == ana_binary.resolve(), (
            f"Wrapper symlink should point to ana binary, "
            f"got {target} instead of {ana_binary.resolve()}"
        )


class TestCondaWrapper:
    """Tests for conda wrapper functionality."""

    @pytest.fixture(scope="class")
    def conda_home(
        self, tmp_path_factory: pytest.TempPathFactory, request: pytest.FixtureRequest
    ) -> Path:
        """Create a shared home directory for all tests in this class."""
        home = tmp_path_factory.mktemp("conda_home")
        # Create shell config files
        (home / ".bashrc").touch()
        (home / ".zshrc").touch()
        fish_config = home / ".config" / "fish"
        fish_config.mkdir(parents=True)
        (fish_config / "config.fish").touch()

        def cleanup():
            # Use system rm for faster cleanup of many files (conda installs ~127 packages)
            subprocess.run(["rm", "-rf", str(home)], check=False)

        request.addfinalizer(cleanup)
        return home

    @pytest.fixture(scope="class")
    def conda_env(self, conda_home: Path) -> dict[str, str]:
        """Environment isolated for conda tests."""
        import os

        env = {
            key: val
            for key, val in os.environ.copy().items()
            if not key.startswith("ANA_") and key != "GITHUB_TOKEN"
        }
        env["HOME"] = str(conda_home)
        return env

    @pytest.fixture(scope="class")
    def conda_wrapper(
        self, ana_binary: Path, conda_home: Path, conda_env: dict[str, str]
    ) -> Path:
        """Install conda once and return the wrapper binary path."""
        result = subprocess.run(
            [str(ana_binary), "tool", "install", "conda"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert result.returncode == 0, f"Failed to install conda: {result.stderr}"
        wrapper = conda_home / ".ana" / "bin" / "conda"
        assert wrapper.exists()
        return wrapper

    def test_conda_wrapper_activate_intercepted(
        self, conda_wrapper: Path, conda_env: dict[str, str]
    ) -> None:
        """Test that conda activate is intercepted with helpful message."""
        proc = subprocess.run(
            [str(conda_wrapper), "activate"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert proc.returncode == 1
        assert "not available via ana" in proc.stderr
        assert "conda shell" in proc.stderr
        assert "conda-spawn" in proc.stderr

    def test_conda_wrapper_deactivate_intercepted(
        self, conda_wrapper: Path, conda_env: dict[str, str]
    ) -> None:
        """Test that conda deactivate is intercepted with helpful message."""
        proc = subprocess.run(
            [str(conda_wrapper), "deactivate"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert proc.returncode == 1
        assert "not available via ana" in proc.stderr
        assert "conda shell" in proc.stderr

    def test_conda_wrapper_init_intercepted(
        self, conda_wrapper: Path, conda_env: dict[str, str]
    ) -> None:
        """Test that conda init is intercepted with helpful message."""
        proc = subprocess.run(
            [str(conda_wrapper), "init"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert proc.returncode == 1
        assert "not needed with ana" in proc.stderr
        assert "~/.ana/bin" in proc.stderr

    def test_conda_wrapper_version_passes_through(
        self, conda_wrapper: Path, conda_env: dict[str, str]
    ) -> None:
        """Test that conda --version passes through to real conda."""
        proc = subprocess.run(
            [str(conda_wrapper), "--version"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert proc.returncode == 0
        assert "conda" in proc.stdout.lower()

    def test_conda_wrapper_info_passes_through(
        self, conda_wrapper: Path, conda_env: dict[str, str]
    ) -> None:
        """Test that conda info passes through to real conda."""
        proc = subprocess.run(
            [str(conda_wrapper), "info"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert proc.returncode == 0
        assert "conda version" in proc.stdout.lower()

    def test_conda_wrapper_help_passes_through(
        self, conda_wrapper: Path, conda_env: dict[str, str]
    ) -> None:
        """Test that conda --help passes through to real conda."""
        proc = subprocess.run(
            [str(conda_wrapper), "--help"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert proc.returncode == 0
        # conda --help outputs to stdout
        assert "conda" in proc.stdout.lower()

    def test_conda_wrapper_shell_alias_for_spawn(
        self, conda_wrapper: Path, conda_env: dict[str, str]
    ) -> None:
        """Test that conda shell is an alias for conda spawn."""
        proc = subprocess.run(
            [str(conda_wrapper), "shell", "--help"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert proc.returncode == 0
        # Should show spawn help (shell is aliased to spawn)
        assert "spawn" in proc.stdout.lower()
        assert "activate conda environments" in proc.stdout.lower()

    def test_conda_environment_is_frozen(
        self, conda_wrapper: Path, conda_env: dict[str, str], conda_home: Path
    ) -> None:
        """Test that the conda environment is frozen and blocks direct installs."""
        # Verify the frozen marker file exists
        frozen_path = conda_home / ".ana" / "tools" / "conda" / "conda-meta" / "frozen"
        assert frozen_path.exists(), "Frozen marker file should exist"

        # Verify it contains the expected message
        frozen_content = frozen_path.read_text()
        assert "managed by ana" in frozen_content

        # Test that conda install to base is blocked
        proc = subprocess.run(
            [str(conda_wrapper), "install", "-n", "base", "numpy", "-y"],
            capture_output=True,
            text=True,
            env=conda_env,
        )
        assert proc.returncode != 0
        assert (
            "frozen" in proc.stderr.lower() or "EnvironmentIsFrozenError" in proc.stderr
        )

    def test_conda_condarc_configured(
        self, conda_wrapper: Path, conda_env: dict[str, str], conda_home: Path
    ) -> None:
        """Test that .condarc is configured with default channels and permanent packages."""
        condarc_path = conda_home / ".ana" / "tools" / "conda" / ".condarc"
        assert condarc_path.exists(), ".condarc file should exist"

        condarc_content = condarc_path.read_text()
        # Verify default_channels are configured
        assert "https://repo.anaconda.com/pkgs/main" in condarc_content
        assert "https://repo.anaconda.com/pkgs/r" in condarc_content
        # Verify auto_activate_base is disabled
        assert "auto_activate_base: false" in condarc_content.lower()
        # Verify permanent packages are configured (for conda self reset)
        assert "self_permanent_packages" in condarc_content
        assert "anaconda-anon-usage" in condarc_content
        assert "anaconda-auth" in condarc_content
        assert "conda-spawn" in condarc_content
