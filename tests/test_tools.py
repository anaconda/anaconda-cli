"""Integration tests for tool management."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from helpers import AnaRunner

IS_WINDOWS = sys.platform == "win32"
PIXI_BIN = "pixi.exe" if IS_WINDOWS else "pixi"


class TestToolHelp:
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

    def test_tool_download_subcommand_exists(self, run_ana: AnaRunner) -> None:
        """Verify the download subcommand is present in the binary."""
        result = run_ana("tool", "--help")
        assert result.returncode == 0
        assert "download" in result.stdout.lower()

    def test_tool_download_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool", "download", "--help")
        assert result.returncode == 0
        assert "miniconda" in result.stdout.lower()


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

        bin_path = fake_home / ".ana" / "bin" / PIXI_BIN
        assert bin_path.exists(), f"Binary not found: {bin_path}"
        tools_dir = fake_home / ".ana" / "tools"
        if IS_WINDOWS:
            shim_cfg = tools_dir / "shims.cfg"
            assert "pixi=pixi\\bin\\pixi.exe\r\n" in shim_cfg.read_text(newline="")
        else:
            src_file = tools_dir / "pixi" / "bin" / "pixi"
            assert bin_path.is_symlink(), f"Binary is not a symlink: {bin_path}"
            assert bin_path.samefile(src_file)

    def test_tool_install_pixi_already_installed(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that running tool install twice shows already up to date."""
        # First run installs
        first_result = run_ana("tool", "install", "pixi")
        assert first_result.returncode == 0

        bin_path = fake_home / ".ana" / "bin" / PIXI_BIN
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

        bin_path = fake_home / ".ana" / "bin" / PIXI_BIN
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

    def test_tool_list_shows_externally_managed_installers(
        self, run_ana: AnaRunner
    ) -> None:
        """Test that tool list shows the externally managed installers table."""
        result = run_ana("tool", "list")
        assert result.returncode == 0
        assert "Externally Managed Installers" in result.stdout
        assert "miniconda" in result.stdout
        assert "ana tool download miniconda" in result.stdout

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


class TestToolUpdate:
    """Tests for 'ana tool update' subcommand."""

    def test_tool_update_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool", "update", "--help")
        assert result.returncode == 0
        assert "Update all installed tools" in result.stdout

    def test_tool_update_no_tools_installed(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool update with no tools installed shows up to date."""
        result = run_ana("tool", "update")
        assert result.returncode == 0
        assert "up to date" in result.stderr.lower()

    def test_tool_update_updates_installed_tool(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool update updates an installed tool when lockfile hash changes."""
        # First install pixi
        install_result = run_ana("tool", "install", "pixi")
        assert install_result.returncode == 0

        # Verify hash file was created
        hash_file = fake_home / ".ana" / "tools" / "pixi" / ".lockfile-hash"
        assert hash_file.exists(), "Lockfile hash should be stored after install"

        # Corrupt the hash to simulate a lockfile change
        hash_file.write_text("fakehash")

        # Run tool update - should detect mismatch and update
        # Note: pixi has auto_update=false by default, so we must enable it via env
        update_result = run_ana("tool", "update", env={"ANA_AUTO_UPDATE_TOOLS": "true"})
        assert update_result.returncode == 0
        assert "pixi" in update_result.stderr.lower()

    def test_tool_update_skips_up_to_date_tools(
        self, run_ana: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool update skips tools that are already up to date."""
        # First install pixi
        install_result = run_ana("tool", "install", "pixi")
        assert install_result.returncode == 0

        # Run tool update - should show up to date
        update_result = run_ana("tool", "update")
        assert update_result.returncode == 0
        assert "up to date" in update_result.stderr.lower()


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
        bin_path = fake_home / ".ana" / "bin" / PIXI_BIN
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
        assert str(Path(".ana/bin/pixi")) in uninstall_result.stderr
        assert str(Path(".ana/tools/pixi")) in uninstall_result.stderr
