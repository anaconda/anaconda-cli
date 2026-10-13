"""Integration tests for tool management."""

from __future__ import annotations

import platform
import shutil
import subprocess
import sys
from collections.abc import Callable
from pathlib import Path

import pytest
from helpers import AnaRunner

IS_WINDOWS = sys.platform == "win32"
PIXI_BIN = "pixi.exe" if IS_WINDOWS else "pixi"

# Maps Python (sys.platform, platform.machine()) values to installer filenames
# produced by src/installer/mod.rs::detect_target.
_MINICONDA_FILENAMES = {
    ("darwin", "arm64"): "Miniconda3-latest-MacOSX-arm64.sh",
    ("darwin", "x86_64"): "Miniconda3-latest-MacOSX-x86_64.sh",
    ("linux", "x86_64"): "Miniconda3-latest-Linux-x86_64.sh",
    ("linux", "aarch64"): "Miniconda3-latest-Linux-aarch64.sh",
    ("win32", "x86_64"): "Miniconda3-latest-Windows-x86_64.exe",
    ("win32", "amd64"): "Miniconda3-latest-Windows-x86_64.exe",
}


def _expected_miniconda_filename() -> str:
    """The installer filename ana would pick for the current platform."""
    key = (sys.platform, platform.machine().lower())
    filename = _MINICONDA_FILENAMES.get(key)
    if filename is None:
        raise RuntimeError(
            f"no known miniconda filename for platform {key}; "
            "update _MINICONDA_FILENAMES to match src/installer/mod.rs"
        )
    return filename


def _pixi_row(stdout: str) -> str:
    """Return the `ana tool list` table row for pixi."""
    return next(
        line
        for line in stdout.split("\n")
        if "│ pixi" in line.lower() and "anaconda" not in line.lower()
    )


def _make_fake_executable(directory: Path, name: str) -> Path:
    """Create a fake executable file named `name` in `directory`."""
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name
    path.write_text("#!/bin/sh\necho fake\n")
    path.chmod(0o755)
    return path


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

    def test_tool_download_no_args_shows_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool", "download")
        assert result.returncode == 0
        assert "miniconda" in result.stdout.lower()


class TestToolDownloadCommand:
    """Tests for 'ana tool download miniconda' subcommand.

    The happy path is exercised against the real
    ``https://repo.anaconda.com/miniconda/`` service (no mock, no
    ``ANA_INSTALLER_BASE_URL`` override) so the test reflects what a user
    actually experiences: a real manifest fetch and a real ~100-200 MB
    download. Failure-path/edge-case coverage (malformed manifests, checksum
    mismatches, 404s) belongs in Rust unit tests in src/installer/mod.rs,
    since those aren't safe or possible to reproduce against the live
    service.
    """

    def test_download_unknown_installer_errors(self, run_ana: AnaRunner) -> None:
        result = run_ana("tool", "download", "nonexistent-installer")
        assert result.returncode != 0
        assert "only miniconda is currently supported" in result.stderr.lower()
        assert "nonexistent-installer" in result.stderr

    def test_download_fails_when_destination_already_exists(
        self, run_ana: AnaRunner, tmp_path: Path
    ) -> None:
        """The pre-flight existence check runs before any network call, so this
        is safe to test without a real download."""
        existing = tmp_path / _expected_miniconda_filename()
        sentinel = b"do-not-overwrite"
        existing.write_bytes(sentinel)

        result = run_ana("tool", "download", "miniconda", cwd=tmp_path)
        assert result.returncode != 0
        assert "already exists" in result.stderr.lower()
        assert existing.read_bytes() == sentinel

    def test_download_miniconda_succeeds(
        self, run_ana: AnaRunner, tmp_path: Path
    ) -> None:
        """Happy path against the real service: download, verify checksum,
        report size, and print the run command. A second run against the same
        destination then refuses to overwrite."""
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path)

        assert result.returncode == 0, result.stderr

        filename = _expected_miniconda_filename()
        installer = tmp_path / filename
        assert installer.exists(), "installer should be written to the cwd"
        # Real installers are well over 50 MB; this guards against a stub,
        # truncated, or zero-byte download slipping past checksum verification.
        assert installer.stat().st_size > 50_000_000

        # No leftover temp file from the atomic rename. `download_and_verify`
        # writes to `dest.with_extension("tmp")`, which *replaces* the
        # extension (foo.sh -> foo.tmp) rather than appending.
        temp_path = installer.with_suffix(".tmp")
        assert not temp_path.exists(), f"leftover temp file: {temp_path.name}"

        assert filename in result.stdout
        assert "SHA256 verified." in result.stdout
        assert _expected_run_command(filename) in result.stdout

        # The manifest always reports a size for published installers, so the
        # " (X.X MB)" suffix should be present in both progress and success output.
        assert f"Downloading {filename} (" in result.stderr
        assert f"Downloaded {filename} (" in result.stdout

        # Re-running against the same destination refuses to overwrite.
        second = run_ana("tool", "download", "miniconda", cwd=tmp_path)
        assert second.returncode != 0
        assert "already exists" in second.stderr.lower()


def _expected_run_command(filename: str) -> str:
    """The run command ana prints for the given installer filename."""
    if filename.endswith(".exe"):
        return f'start "" ".\\{filename}"'
    return f"bash ./{filename}"


def _assert_no_artifacts(tmp_path: Path) -> None:
    """No installer or leftover temp file should exist in the working dir."""
    filename = _expected_miniconda_filename()
    assert not (tmp_path / filename).exists(), "installer should not be written"
    assert not (tmp_path / filename).with_suffix(".tmp").exists(), (
        "temp file should not be left behind"
    )


class TestToolDownloadNegative:
    """Negative and edge cases for 'ana tool download miniconda'.

    Complements the positive coverage in ``TestToolDownloadCommand`` by driving
    the failure branches of the download flow (manifest errors, missing/empty
    checksums, checksum mismatch, installer download failure) and asserting the
    process exits non-zero, reports a useful error, and leaves no artifacts.
    """

    def test_download_fails_when_manifest_missing(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """A 404 on .files.json is reported and nothing is downloaded."""
        base_url = miniconda_server_factory(write_manifest=False, installer_bytes=None)

        env = {"ANA_INSTALLER_BASE_URL": base_url}
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path, env=env)

        assert result.returncode != 0
        assert "manifest request failed" in result.stderr.lower()
        _assert_no_artifacts(tmp_path)

    def test_download_fails_when_manifest_malformed(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """An unparseable .files.json is reported and nothing is downloaded."""
        base_url = miniconda_server_factory(manifest_body="not valid json")

        env = {"ANA_INSTALLER_BASE_URL": base_url}
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path, env=env)

        assert result.returncode != 0
        assert "failed to parse manifest" in result.stderr.lower()
        _assert_no_artifacts(tmp_path)

    def test_download_fails_when_filename_not_in_manifest(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """A manifest without our platform's key is a stale-map error."""
        base_url = miniconda_server_factory(manifest_body="{}")

        env = {"ANA_INSTALLER_BASE_URL": base_url}
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path, env=env)

        assert result.returncode != 0
        assert "not in manifest" in result.stderr.lower()
        assert _expected_miniconda_filename() in result.stderr
        _assert_no_artifacts(tmp_path)

    def test_download_fails_when_sha256_missing(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """A manifest entry with no sha256 refuses to download unverified."""
        base_url = miniconda_server_factory(sha256=None)

        env = {"ANA_INSTALLER_BASE_URL": base_url}
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path, env=env)

        assert result.returncode != 0
        assert "refusing unverified download" in result.stderr.lower()
        _assert_no_artifacts(tmp_path)

    def test_download_fails_when_sha256_empty(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """An empty sha256 string is treated the same as a missing one."""
        base_url = miniconda_server_factory(sha256="")

        env = {"ANA_INSTALLER_BASE_URL": base_url}
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path, env=env)

        assert result.returncode != 0
        assert "refusing unverified download" in result.stderr.lower()
        _assert_no_artifacts(tmp_path)

    def test_download_fails_on_checksum_mismatch(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """A checksum that doesn't match the bytes aborts and cleans up."""
        base_url = miniconda_server_factory(sha256="0" * 64)

        env = {"ANA_INSTALLER_BASE_URL": base_url}
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path, env=env)

        assert result.returncode != 0
        assert "checksum mismatch" in result.stderr.lower()
        # Neither the final installer nor the temp file may remain.
        _assert_no_artifacts(tmp_path)

    def test_download_fails_when_installer_missing(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """A 404 on the installer (manifest OK) is reported, no artifacts."""
        # Valid manifest, but the installer file itself is absent.
        base_url = miniconda_server_factory(installer_bytes=None)

        env = {"ANA_INSTALLER_BASE_URL": base_url}
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path, env=env)

        assert result.returncode != 0
        assert "download request failed" in result.stderr.lower()
        _assert_no_artifacts(tmp_path)

    def test_download_succeeds_without_size_in_manifest(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """Size is optional: a manifest without it still downloads cleanly."""
        # size=None omits the field; sha256 still defaults to the correct hash.
        base_url = miniconda_server_factory(size=None)

        env = {"ANA_INSTALLER_BASE_URL": base_url}
        result = run_ana("tool", "download", "miniconda", cwd=tmp_path, env=env)

        assert result.returncode == 0, result.stderr
        filename = _expected_miniconda_filename()
        assert (tmp_path / filename).read_bytes() == MOCK_MINICONDA_INSTALLER_CONTENT
        # No size suffix when the manifest entry omits size.
        assert f"Downloading {filename}\n" in result.stderr

    def test_download_accepts_base_url_with_trailing_slash_variants(
        self,
        run_ana: AnaRunner,
        tmp_path: Path,
        miniconda_server_factory: Callable[..., str],
    ) -> None:
        """The base URL is normalized: trailing/absent slashes both work."""
        base_url = miniconda_server_factory()

        for variant in (base_url, base_url.rstrip("/")):
            target = tmp_path / variant.replace("/", "_").replace(":", "")
            target.mkdir()
            env = {"ANA_INSTALLER_BASE_URL": variant}
            result = run_ana("tool", "download", "miniconda", cwd=target, env=env)
            assert result.returncode == 0, f"{variant}: {result.stderr}"
            assert (target / _expected_miniconda_filename()).exists()


class TestToolInstallPixi:
    """Tests for 'ana tool install pixi' subcommand."""

    def test_tool_install_pixi(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool install pixi installs pixi to ~/.ana/tools."""
        result = run_ana_logged_in("tool", "install", "pixi")
        assert result.returncode == 0
        assert "pixi" in result.stderr

        # Verify the tool directory was created
        tool_dir = fake_home / ".ana" / "tools" / "pixi"
        assert tool_dir.exists(), f"Tool directory not found: {tool_dir}"
        assert tool_dir.is_dir()

    def test_tool_install_pixi_creates_symlink(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool install creates a symlinked pixi binary in ~/.ana/bin."""
        result = run_ana_logged_in("tool", "install", "pixi")
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
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that running tool install twice shows already up to date."""
        # First run installs
        first_result = run_ana_logged_in("tool", "install", "pixi")
        assert first_result.returncode == 0

        bin_path = fake_home / ".ana" / "bin" / PIXI_BIN
        assert bin_path.exists()

        # Second run should indicate already up to date
        second_result = run_ana_logged_in("tool", "install", "pixi")
        assert second_result.returncode == 0
        assert "up to date" in second_result.stderr.lower()

    def test_tool_install_pixi_binary_runs(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that the installed pixi binary runs."""
        result = run_ana_logged_in("tool", "install", "pixi")
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

    def test_tool_install_unknown_tool(self, run_ana_logged_in: AnaRunner) -> None:
        """Test that installing an unknown tool fails with error."""
        result = run_ana_logged_in("tool", "install", "nonexistent-tool")
        assert result.returncode != 0
        assert "unknown tool" in result.stderr.lower()

    @pytest.mark.skipif(IS_WINDOWS, reason="Windows uses shims.cfg, not symlinks")
    def test_tool_install_cleans_up_broken_symlink(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Manually deleting the tool dir leaves a broken symlink behind;
        re-running install should clean it up before recreating it. See
        CLI-526."""
        result = run_ana_logged_in("tool", "install", "pixi")
        assert result.returncode == 0

        tool_dir = fake_home / ".ana" / "tools" / "pixi"
        bin_path = fake_home / ".ana" / "bin" / PIXI_BIN
        shutil.rmtree(tool_dir)
        assert bin_path.is_symlink()
        assert not bin_path.exists(), "symlink should now be broken"

        result = run_ana_logged_in("tool", "install", "pixi")
        assert result.returncode == 0
        assert bin_path.is_symlink()
        assert bin_path.exists(), "symlink should be recreated and valid"


class TestToolList:
    """Tests for 'ana tool list' subcommand."""

    def test_tool_list_help(self, run_ana_logged_in: AnaRunner) -> None:
        result = run_ana_logged_in("tool", "list", "--help")
        assert result.returncode == 0
        assert "List available tools" in result.stdout

    def test_tool_list_shows_tools(self, run_ana_logged_in: AnaRunner) -> None:
        """Test that tool list shows available tools."""
        result = run_ana_logged_in("tool", "list")
        assert result.returncode == 0
        assert "Name" in result.stdout
        assert "Installed" in result.stdout
        assert "Binaries" in result.stdout
        assert "pixi" in result.stdout
        assert "anaconda-cli" in result.stdout

    def test_tool_list_shows_externally_managed_installers(
        self, run_ana_logged_in: AnaRunner
    ) -> None:
        """Test that tool list shows the externally managed installers table."""
        result = run_ana_logged_in("tool", "list")
        assert result.returncode == 0
        assert "Externally Managed Installers" in result.stdout
        assert "miniconda" in result.stdout
        assert "ana tool download miniconda" in result.stdout

    def test_tool_list_shows_installed_status(
        self, run_ana_logged_in: AnaRunner, fake_home: Path, tmp_path: Path
    ) -> None:
        """Test that tool list correctly shows installation status."""
        # Before install, should show ✗. Use an empty PATH so a pixi on the
        # host machine isn't reported as an external install.
        empty_bin = tmp_path / "empty-bin"
        empty_bin.mkdir()
        result_before = run_ana_logged_in("tool", "list", env={"PATH": str(empty_bin)})
        assert result_before.returncode == 0
        pixi_line_before = _pixi_row(result_before.stdout)
        assert "✗" in pixi_line_before

        # Install pixi
        install_result = run_ana_logged_in("tool", "install", "pixi")
        assert install_result.returncode == 0

        # After install, should show ✓
        result_after = run_ana_logged_in("tool", "list")
        assert result_after.returncode == 0
        pixi_line_after = _pixi_row(result_after.stdout)
        assert "✓" in pixi_line_after

    def test_tool_list_detects_external_install(
        self, run_ana_logged_in: AnaRunner, tmp_path: Path
    ) -> None:
        """A pixi installed outside ana (e.g. via pixi.sh/install.sh into
        ~/.pixi/bin) should be reported as external, not as missing. See
        GitHub issue #423."""
        external_pixi = _make_fake_executable(tmp_path / ".pixi" / "bin", PIXI_BIN)

        result = run_ana_logged_in(
            "tool", "list", env={"PATH": str(external_pixi.parent)}
        )
        assert result.returncode == 0
        pixi_line = _pixi_row(result.stdout)
        assert "external" in pixi_line
        assert "✗" not in pixi_line
        assert "not managed by ana" in result.stdout
        assert str(external_pixi) in result.stdout

    def test_tool_list_ignores_ana_bin_dir_on_path(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Binaries in ~/.ana/bin are ana's own links and must never be
        reported as an external install."""
        _make_fake_executable(fake_home / ".ana" / "bin", PIXI_BIN)

        result = run_ana_logged_in(
            "tool", "list", env={"PATH": str(fake_home / ".ana" / "bin")}
        )
        assert result.returncode == 0
        pixi_line = _pixi_row(result.stdout)
        assert "external" not in pixi_line
        assert "not managed by ana" not in result.stdout

    @pytest.mark.skipif(IS_WINDOWS, reason="Windows uses shims.cfg, not symlinks")
    def test_tool_list_cleans_up_broken_symlink(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Manually deleting the tool dir leaves a broken symlink behind;
        `tool list` should clean it up rather than leaving it dangling. See
        CLI-526."""
        install_result = run_ana_logged_in("tool", "install", "pixi")
        assert install_result.returncode == 0

        tool_dir = fake_home / ".ana" / "tools" / "pixi"
        bin_path = fake_home / ".ana" / "bin" / PIXI_BIN
        shutil.rmtree(tool_dir)
        assert bin_path.is_symlink()
        assert not bin_path.exists(), "symlink should now be broken"

        result = run_ana_logged_in("tool", "list")
        assert result.returncode == 0
        assert not bin_path.is_symlink(), "broken symlink should be removed"


class TestToolUpdate:
    """Tests for 'ana tool update' subcommand."""

    def test_tool_update_help(self, run_ana_logged_in: AnaRunner) -> None:
        result = run_ana_logged_in("tool", "update", "--help")
        assert result.returncode == 0
        assert "Update all installed tools" in result.stdout

    def test_tool_update_no_tools_installed(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool update with no tools installed shows up to date."""
        result = run_ana_logged_in("tool", "update")
        assert result.returncode == 0
        assert "up to date" in result.stderr.lower()

    def test_tool_update_updates_installed_tool(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool update updates an installed tool when lockfile hash changes."""
        # First install pixi
        install_result = run_ana_logged_in("tool", "install", "pixi")
        assert install_result.returncode == 0

        # Verify hash file was created
        hash_file = fake_home / ".ana" / "tools" / "pixi" / ".lockfile-hash"
        assert hash_file.exists(), "Lockfile hash should be stored after install"

        # Corrupt the hash to simulate a lockfile change
        hash_file.write_text("fakehash")

        # Run tool update - should detect mismatch and update
        # Note: pixi has auto_update=false by default, so we must enable it via env
        update_result = run_ana_logged_in(
            "tool", "update", env={"ANA_AUTO_UPDATE_TOOLS": "true"}
        )
        assert update_result.returncode == 0
        assert "pixi" in update_result.stderr.lower()

    def test_tool_update_skips_up_to_date_tools(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool update skips tools that are already up to date."""
        # First install pixi
        install_result = run_ana_logged_in("tool", "install", "pixi")
        assert install_result.returncode == 0

        # Run tool update - should show up to date
        update_result = run_ana_logged_in("tool", "update")
        assert update_result.returncode == 0
        assert "up to date" in update_result.stderr.lower()


class TestToolUninstall:
    """Tests for 'ana tool uninstall' subcommand."""

    def test_tool_uninstall_help(self, run_ana_logged_in: AnaRunner) -> None:
        result = run_ana_logged_in("tool", "uninstall", "--help")
        assert result.returncode == 0
        assert "Uninstall a tool" in result.stdout

    def test_tool_uninstall_unknown_tool(self, run_ana_logged_in: AnaRunner) -> None:
        """Test that uninstalling an unknown tool fails with error."""
        result = run_ana_logged_in("tool", "uninstall", "nonexistent-tool")
        assert result.returncode != 0
        assert "unknown tool" in result.stderr.lower()

    def test_tool_uninstall_not_installed(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that uninstalling a tool that isn't installed is a no-op."""
        result = run_ana_logged_in("tool", "uninstall", "pixi", "--yes")
        assert result.returncode == 0
        assert "not installed" in result.stderr.lower()

    def test_tool_uninstall_pixi(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that tool uninstall removes the tool and cleans up."""
        # First install
        install_result = run_ana_logged_in("tool", "install", "pixi")
        assert install_result.returncode == 0

        tool_dir = fake_home / ".ana" / "tools" / "pixi"
        bin_path = fake_home / ".ana" / "bin" / PIXI_BIN
        assert tool_dir.exists()
        assert bin_path.exists()

        # Then uninstall (with --yes to skip prompt)
        uninstall_result = run_ana_logged_in("tool", "uninstall", "pixi", "--yes")
        assert uninstall_result.returncode == 0
        assert "Successfully uninstalled" in uninstall_result.stderr

        # Verify removal
        assert not tool_dir.exists(), "Tool directory should be removed"
        assert not bin_path.exists(), "Symlink should be removed"

    def test_tool_uninstall_shows_what_will_be_removed(
        self, run_ana_logged_in: AnaRunner, fake_home: Path
    ) -> None:
        """Test that uninstall shows what will be deleted before prompting."""
        # First install
        install_result = run_ana_logged_in("tool", "install", "pixi")
        assert install_result.returncode == 0

        # Run uninstall with --yes and check output
        uninstall_result = run_ana_logged_in("tool", "uninstall", "pixi", "--yes")
        assert uninstall_result.returncode == 0
        assert "The following will be removed:" in uninstall_result.stderr
        assert str(Path(".ana/bin/pixi")) in uninstall_result.stderr
        assert str(Path(".ana/tools/pixi")) in uninstall_result.stderr
