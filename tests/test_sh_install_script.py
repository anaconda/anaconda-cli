"""Integration tests for the install.sh script."""

from __future__ import annotations

import shutil
import stat
import subprocess
from pathlib import Path

import pytest
from helpers import IS_WINDOWS
from helpers import REPO_ROOT

SCRIPT_PATH = REPO_ROOT / "scripts" / "install.sh"

if IS_WINDOWS and not shutil.which("sh"):
    pytest.skip("Tests only work in bash shell on Windows.", allow_module_level=True)

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
        assert "--no-verify-checksum" in result.stdout
        assert "--no-path-update" in result.stdout
        assert "--channel" in result.stdout
        assert "--force" in result.stdout
        assert "--help" in result.stdout

    def test_help_shows_environment_variables(self) -> None:
        result = run_script("--help")
        assert "ANA_INSTALL_DIR" in result.stdout
        assert "ANA_VERSION" in result.stdout
        assert "ANA_VERIFY_CHECKSUM" in result.stdout
        assert "ANA_NO_PATH_UPDATE" in result.stdout
        assert "ANA_FORCE_INSTALL" in result.stdout
        assert "ANA_BASE_URL" in result.stdout
        assert "ANA_CHANNEL" in result.stdout

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


class TestArgumentParsing:
    """Tests for CLI argument parsing."""

    def test_unknown_option_errors(self) -> None:
        result = run_script("--unknown-option")
        assert result.returncode == 1
        assert "Unknown option: --unknown-option" in result.stderr

    def test_unexpected_argument_errors(self) -> None:
        result = run_script("unexpected_arg")
        assert result.returncode == 1
        assert "Unexpected argument: unexpected_arg" in result.stderr

    def test_missing_install_dir_argument(self) -> None:
        result = run_script("--install-dir")
        assert result.returncode == 1
        assert "Missing argument" in result.stderr

    def test_missing_version_argument(self) -> None:
        result = run_script("--version")
        assert result.returncode == 1
        assert "Missing argument" in result.stderr

    def test_short_flags_work(self) -> None:
        # -h is tested above, test -d and -v require more setup
        # Just verify -h works as a smoke test for short flags
        result = run_script("-h")
        assert result.returncode == 0


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

        assert result.returncode == 0
        assert "Installing ana for" in result.stdout
        # Check for the message and binary name (path separators vary by platform)
        assert "Installed ana to" in result.stdout
        assert f"ana{BINARY_SUFFIX}" in result.stdout
        assert "Done!" in result.stdout

        # Verify binary exists and is executable
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
            "--install-dir",
            str(install_dir),
            env=ana_install_env_with_mock_server,
        )

        assert result.returncode == 0
        assert (install_dir / f"ana{BINARY_SUFFIX}").exists()

    def test_checksum_verification_disabled_warning(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test that checksum verification disabled warning is shown."""
        result = run_script(
            "--no-verify-checksum", env=ana_install_env_with_mock_server
        )

        assert result.returncode == 0
        assert (
            "Checksum verification disabled" in result.stderr
            or "Checksum verification disabled" in result.stdout
        )

    def test_checksum_verification_invalid_value_errors(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test that invalid ANA_VERIFY_CHECKSUM values raise an error."""
        ana_install_env_with_mock_server["ANA_VERIFY_CHECKSUM"] = "blargh"
        result = run_script(env=ana_install_env_with_mock_server)

        assert result.returncode == 1
        assert "Invalid ANA_VERIFY_CHECKSUM" in result.stderr


class TestForceInstall:
    """Tests for --force flag behavior."""

    def test_overwrite_without_force_fails_non_tty(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test that overwriting without --force fails in non-TTY mode."""
        # First install
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        # Try to install again without --force
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 1
        assert "already exists" in result.stderr
        assert "--force" in result.stderr

    def test_overwrite_with_force_succeeds(
        self,
        ana_install_env_with_mock_server: dict[str, str],
    ) -> None:
        """Test that overwriting with --force succeeds."""
        # First install
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        # Second install with --force
        result = run_script("--force", env=ana_install_env_with_mock_server)
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


class TestShellProfileUpdate:
    """Tests for shell profile modification."""

    def test_no_path_update_flag(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        fake_home: Path,
    ) -> None:
        """Test --no-path-update prevents shell profile modification."""
        del ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"]

        zshrc_before = (fake_home / ".zshrc").read_text()

        result = run_script("--no-path-update", env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        zshrc_after = (fake_home / ".zshrc").read_text()
        assert zshrc_before == zshrc_after

    def test_no_path_update_env_var(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        fake_home: Path,
    ) -> None:
        """Test ANA_NO_PATH_UPDATE prevents shell profile modification."""
        ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"] = "1"

        zshrc_before = (fake_home / ".zshrc").read_text()

        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        zshrc_after = (fake_home / ".zshrc").read_text()
        assert zshrc_before == zshrc_after

    def test_path_update_modifies_profile(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        fake_home: Path,
        install_dir: Path,
    ) -> None:
        """Test that path update modifies the shell profile."""
        del ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"]
        # Set SHELL to zsh for predictable behavior
        ana_install_env_with_mock_server["SHELL"] = "/bin/zsh"

        zshrc = fake_home / ".zshrc"
        zshrc_before = zshrc.read_text()

        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0

        zshrc_after = zshrc.read_text()
        assert zshrc_before != zshrc_after
        assert str(install_dir) in zshrc_after
        assert "export PATH=" in zshrc_after

    def test_path_update_idempotent(
        self,
        ana_install_env_with_mock_server: dict[str, str],
        fake_home: Path,
    ) -> None:
        """Test that running install twice doesn't duplicate PATH entry."""
        del ana_install_env_with_mock_server["ANA_NO_PATH_UPDATE"]
        ana_install_env_with_mock_server["SHELL"] = "/bin/zsh"

        # First install
        result = run_script(env=ana_install_env_with_mock_server)
        assert result.returncode == 0
        zshrc_after_first = (fake_home / ".zshrc").read_text()

        # Second install (--force to overwrite existing binary)
        result = run_script("--force", env=ana_install_env_with_mock_server)
        assert result.returncode == 0
        zshrc_after_second = (fake_home / ".zshrc").read_text()

        # Should be the same (no duplicate entries)
        assert zshrc_after_first == zshrc_after_second


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

        binary = Path(
            ana_install_env_with_mock_server["ANA_INSTALL_DIR"], f"ana{BINARY_SUFFIX}"
        )
        binary = self._get_mock_binary(install_dir)
        result = subprocess.run(
            [str(binary), "--help"],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert "Mock ana CLI for testing" in result.stdout
