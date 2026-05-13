"""Integration tests for the install.ps1 script."""

from __future__ import annotations

import shutil
import stat
import subprocess
from pathlib import Path

import pytest
from helpers import IS_WINDOWS
from helpers import REPO_ROOT

SCRIPT_PATH = REPO_ROOT / "scripts" / "install.ps1"

# Prefer powershell (Windows PowerShell 5) for backwards compatibility testing
# Fall back to pwsh (PowerShell Core) on non-Windows
if shutil.which("powershell"):
    PWSH = "powershell"
elif shutil.which("pwsh"):
    PWSH = "pwsh"
else:
    pytest.skip("Tests require PowerShell.", allow_module_level=True)

BINARY_SUFFIX = ".exe" if IS_WINDOWS else ""


@pytest.fixture
def install_dir(ana_install_env_with_mock_server: dict[str, str]) -> Path:
    """Provide a temporary installation directory."""
    return Path(ana_install_env_with_mock_server["ANA_INSTALL_DIR"])


def run_script(
    *args: str,
    env: dict[str, str] | None = None,
    input: str | None = None,
) -> subprocess.CompletedProcess[str]:
    """Run the install script with given arguments."""
    return subprocess.run(
        [PWSH, "-ExecutionPolicy", "Bypass", "-File", str(SCRIPT_PATH), *args],
        capture_output=True,
        text=True,
        env=env,
        input=input,
    )


def get_user_path_env(env: dict[str, str] | None = None) -> str:
    """Get the user PATH environment variable on Windows."""
    result = subprocess.run(
        [
            PWSH,
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            '[Environment]::GetEnvironmentVariable("Path", "User")',
        ],
        capture_output=True,
        text=True,
        env=env,
    )
    return result.stdout.strip()


class TestHelp:
    """Tests for -Help output."""

    def test_help_flag(self) -> None:
        # PowerShell's Get-Help requires different invocation
        result = subprocess.run(
            [
                PWSH,
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                f"Get-Help '{SCRIPT_PATH}' -Full",
            ],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert "SYNOPSIS" in result.stdout
        assert "Install" in result.stdout

    def test_help_shows_all_parameters(self) -> None:
        result = subprocess.run(
            [
                PWSH,
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                f"Get-Help '{SCRIPT_PATH}' -Full",
            ],
            capture_output=True,
            text=True,
        )
        assert "-InstallDir" in result.stdout
        assert "-Version" in result.stdout
        assert "-NoVerifyChecksum" in result.stdout
        assert "-NoPathUpdate" in result.stdout
        assert "-NoBootstrap" in result.stdout
        assert "-Force" in result.stdout

    def test_help_shows_environment_variables(self) -> None:
        result = subprocess.run(
            [
                PWSH,
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                f"Get-Help '{SCRIPT_PATH}' -Full",
            ],
            capture_output=True,
            text=True,
        )
        assert "ANA_INSTALL_DIR" in result.stdout
        assert "ANA_VERSION" in result.stdout
        assert "ANA_VERIFY_CHECKSUM" in result.stdout
        assert "ANA_NO_PATH_UPDATE" in result.stdout
        assert "ANA_FORCE_INSTALL" in result.stdout
        assert "ANA_BASE_URL" in result.stdout
        assert "ANA_CHANNEL" in result.stdout

    def test_help_shows_examples(self) -> None:
        result = subprocess.run(
            [
                PWSH,
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                f"Get-Help '{SCRIPT_PATH}' -Full",
            ],
            capture_output=True,
            text=True,
        )
        assert "EXAMPLE" in result.stdout


class TestArgumentParsing:
    """Tests for CLI argument parsing."""

    def test_unknown_option_errors(self) -> None:
        result = run_script("-UnknownOption")
        assert result.returncode != 0
        assert "parameter" in result.stderr.lower() or "UnknownOption" in result.stderr


class TestInstallation:
    """Tests for installation using mock server."""

    def test_successful_install(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test successful installation of a specific version."""
        result = run_script(env=ana_install_env_with_mock_server)

        expected_binary = install_dir / f"ana{BINARY_SUFFIX}"

        assert result.returncode == 0, (
            f"stderr: {result.stderr}\nstdout: {result.stdout}"
        )
        assert "Installing ana for" in result.stdout
        assert "Installed ana to" in result.stdout
        assert f"ana{BINARY_SUFFIX}" in result.stdout

        # Verify binary exists
        assert expected_binary.exists()
        if not IS_WINDOWS:
            assert expected_binary.stat().st_mode & stat.S_IXUSR

    def test_install_with_cli_options(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test installation using CLI options."""
        # Remove env vars to test CLI takes precedence
        del ana_install_env_with_mock_server["ANA_INSTALL_DIR"]

        result = run_script(
            "-InstallDir",
            str(install_dir),
            env=ana_install_env_with_mock_server,
        )

        assert result.returncode == 0, f"stderr: {result.stderr}"
        assert (install_dir / f"ana{BINARY_SUFFIX}").exists()

    def test_checksum_verification_disabled_warning(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test that checksum verification disabled warning is shown."""
        result = run_script("-NoVerifyChecksum", env=ana_install_env_with_mock_server)

        assert result.returncode == 0
        assert (
            "Checksum verification disabled" in result.stderr
            or "Checksum verification disabled" in result.stdout
        )


class TestForceInstall:
    """Tests for -Force flag behavior."""

    def test_overwrite_without_force_prompts(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test that overwriting without -Force prompts or fails."""
        # First install
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        # Try to install again without -Force (provide 'n' as input)
        result = run_script(env=ana_install_env_with_mock_server, input="n\n")
        assert result.returncode != 0
        assert "already exists" in result.stdout or "cancelled" in result.stderr.lower()

    def test_overwrite_with_force_succeeds(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test that overwriting with -Force succeeds."""
        # First install
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        # Second install with -Force
        result = run_script("-Force", env=ana_install_env_with_mock_server)
        assert result.returncode == 0

    def test_force_via_env_var(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test ANA_FORCE_INSTALL environment variable."""
        ana_install_env_with_mock_server["ANA_FORCE_INSTALL"] = "1"

        # First install
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        # Second install (should succeed due to env var)
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0


class TestPathUpdateWindows:
    """Tests for Windows PATH modification."""

    @pytest.mark.skipif(not IS_WINDOWS, reason="Windows-specific PATH update")
    def test_no_path_update_flag(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test -NoPathUpdate prevents PATH modification."""
        del ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"]

        path_before = get_user_path_env(ana_install_env_with_mock_server)

        result = run_script("-NoPathUpdate", env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        path_after = get_user_path_env(ana_install_env_with_mock_server)
        assert path_before == path_after
        assert "Adding ana installation to PATH" not in result.stdout

    @pytest.mark.skipif(not IS_WINDOWS, reason="Windows-specific PATH update")
    def test_no_path_update_env_var(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test ANA_NO_PATH_UPDATE prevents PATH modification."""
        ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"] = "1"

        path_before = get_user_path_env(ana_install_env_with_mock_server)

        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        path_after = get_user_path_env(ana_install_env_with_mock_server)
        assert path_before == path_after

    @pytest.mark.skipif(not IS_WINDOWS, reason="Windows-specific PATH update")
    def test_path_update_adds_to_path(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test that PATH is updated on Windows."""
        del ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"]

        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0
        assert "Added" in result.stdout and "PATH" in result.stdout

        # Verify install dir is in user PATH
        user_path = get_user_path_env(ana_install_env_with_mock_server)
        assert str(install_dir) in user_path


class TestPSProfileUpdate:
    """Tests for PowerShell profile modification on non-Windows."""

    @staticmethod
    def _get_ps_profile(fake_home: Path) -> Path:
        """Get the PowerShell profile path within the fake home directory."""
        ps_profile = (
            fake_home / ".config" / "powershell" / "Microsoft.PowerShell_profile.ps1"
        )
        ps_profile.parent.mkdir(parents=True, exist_ok=True)
        ps_profile.touch()
        return ps_profile

    @pytest.mark.skipif(
        IS_WINDOWS, reason="PowerShell profile update is for non-Windows"
    )
    def test_no_path_update_flag(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        fake_home: Path,
    ) -> None:
        """Test -NoPathUpdate prevents PowerShell profile modification."""
        del ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"]

        ps_profile = self._get_ps_profile(fake_home)
        profile_before = ps_profile.read_text()

        result = run_script("-NoPathUpdate", env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        profile_after = ps_profile.read_text()
        assert profile_before == profile_after

    @pytest.mark.skipif(
        IS_WINDOWS, reason="PowerShell profile update is for non-Windows"
    )
    def test_no_path_update_env_var(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        fake_home: Path,
    ) -> None:
        """Test ANA_NO_PATH_UPDATE prevents PowerShell profile modification."""
        ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"] = "1"

        ps_profile = self._get_ps_profile(fake_home)
        profile_before = ps_profile.read_text()

        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        profile_after = ps_profile.read_text()
        assert profile_before == profile_after

    @pytest.mark.skipif(
        IS_WINDOWS, reason="PowerShell profile update is for non-Windows"
    )
    def test_path_update_modifies_profile(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        fake_home: Path,
        install_dir: Path,
    ) -> None:
        """Test that path update modifies the PowerShell profile."""
        del ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"]

        ps_profile = self._get_ps_profile(fake_home)
        profile_before = ps_profile.read_text()

        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        profile_after = ps_profile.read_text()
        assert profile_before != profile_after
        assert str(install_dir) in profile_after
        assert "$env:PATH" in profile_after

    @pytest.mark.skipif(
        IS_WINDOWS, reason="PowerShell profile update is for non-Windows"
    )
    def test_path_update_idempotent(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        fake_home: Path,
    ) -> None:
        """Test that running install twice doesn't duplicate PATH entry."""
        del ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"]

        ps_profile = self._get_ps_profile(fake_home)

        # First install
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0
        profile_after_first = ps_profile.read_text()

        # Second install (-Force to overwrite existing binary)
        result = run_script("-Force", env=ana_install_env_with_mock_server)
        assert result.returncode == 0
        profile_after_second = ps_profile.read_text()

        # Should be the same (no duplicate entries)
        assert profile_after_first == profile_after_second


class TestBinaryVerification:
    """Tests to verify the installed mock binary works."""

    @staticmethod
    def _get_mock_binary(install_dir: Path) -> Path:
        """Get the mock binary path, renaming .exe files to .cmd outside sh.

        Windows shells expect .exe files to be PE files and will refuse to
        execute other scripts disguised as .exe.
        """
        binary = install_dir / f"ana{BINARY_SUFFIX}"
        if not IS_WINDOWS:
            return binary
        cmd_file = binary.with_suffix(".cmd")
        binary.rename(cmd_file)
        return cmd_file

    def test_installed_binary_runs(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test that the installed binary actually runs."""
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        # Run the installed binary
        binary = self._get_mock_binary(install_dir)
        result = subprocess.run(
            [str(binary), "--version"],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert "0.0.0-mock" in result.stdout

    def test_installed_binary_help(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test that the installed binary shows help."""
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        binary = self._get_mock_binary(install_dir)
        result = subprocess.run(
            [str(binary), "--help"],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert "Mock ana CLI for testing" in result.stdout
