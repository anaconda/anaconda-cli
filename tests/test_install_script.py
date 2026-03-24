"""Integration tests for the install.sh script."""

from __future__ import annotations

import http.server
import os
import socketserver
import subprocess
import stat
import sys
import threading
from functools import partial
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from collections.abc import Generator

# Skip all tests in this module on Windows
pytestmark = pytest.mark.skipif(
    sys.platform == "win32",
    reason="install.sh is a shell script that only runs on Linux/macOS",
)


def _find_repo_root() -> Path:
    """Find the repository root by looking for .git directory."""
    path = Path(__file__).resolve()
    for parent in path.parents:
        if (parent / ".git").exists():
            return parent
    raise RuntimeError("Could not find repository root")


REPO_ROOT = _find_repo_root()
SCRIPT_PATH = REPO_ROOT / "scripts" / "install.sh"

# Create a simple mock binary script that responds to --version and --help
MOCK_BINARY_SCRIPT = """\
#!/bin/sh
case "$1" in
    --version) echo "0.0.0-mock" ;;
    --help) echo "Mock ana CLI for testing" ;;
    *) echo "mock ana" ;;
esac
"""
EXECUTABLE_MODE = 0o755  # rwxr-xr-x
SUPPORTED_PLATFORMS = ["darwin-arm64", "darwin-x86_64", "linux-x86_64", "linux-aarch64"]


@pytest.fixture
def install_dir(tmp_path: Path) -> Path:
    """Provide a temporary installation directory."""
    d = tmp_path / "bin"
    d.mkdir()
    return d


@pytest.fixture
def fake_home(tmp_path: Path) -> Path:
    """Provide a fake HOME directory to isolate shell profile modifications."""
    home = tmp_path / "home"
    home.mkdir()
    # Create shell config files
    (home / ".bashrc").touch()
    (home / ".zshrc").touch()
    fish_config = home / ".config" / "fish"
    fish_config.mkdir(parents=True)
    (fish_config / "config.fish").touch()
    return home


@pytest.fixture
def env_isolated(fake_home: Path, install_dir: Path) -> dict[str, str]:
    """Provide an isolated environment that won't modify real shell configs."""
    env = {
        key: val for key, val in os.environ.copy().items() if not key.startswith("ANA_")
    }

    env["HOME"] = str(fake_home)
    env["ANA_INSTALL_DIR"] = str(install_dir)
    env["ANA_NO_PATH_UPDATE"] = "1"  # Extra safety
    return env


@pytest.fixture(scope="session")
def mock_server(tmp_path_factory: pytest.TempPathFactory) -> Generator[str, None, None]:
    """Start a local HTTP server to host mock binaries."""
    import hashlib

    # Create mock binaries for different platforms
    root = tmp_path_factory.mktemp("mock_server")
    for platform in SUPPORTED_PLATFORMS:
        binary = root / f"ana-{platform}"
        binary.write_text(MOCK_BINARY_SCRIPT)
        binary.chmod(EXECUTABLE_MODE)
        # Create corresponding checksum file
        checksum = hashlib.sha256(MOCK_BINARY_SCRIPT.encode()).hexdigest()
        checksum_file = root / f"ana-{platform}.sha256"
        checksum_file.write_text(f"{checksum}  ana-{platform}\n")

    class QuietHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
        """HTTP request handler that suppresses logging."""

        def log_message(self, format: str, *args: object) -> None:
            pass  # Suppress logging

    handler = partial(QuietHTTPRequestHandler, directory=str(root))

    # Use port 0 to let the OS pick an available port
    with socketserver.TCPServer(("127.0.0.1", 0), handler) as server:
        port = server.server_address[1]
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()

        yield f"http://127.0.0.1:{port}"

        server.shutdown()


@pytest.fixture
def env_with_mock_server(
    env_isolated: dict[str, str],
    mock_server: str,
) -> dict[str, str]:
    """Provide isolated environment with mock server URL."""
    env_isolated["ANA_BASE_URL"] = mock_server
    return env_isolated


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

    def test_missing_token_argument(self) -> None:
        result = run_script("--token")
        assert result.returncode == 1
        assert "Missing argument" in result.stderr

    def test_short_flags_work(self) -> None:
        # -h is tested above, test -d and -v require more setup
        # Just verify -h works as a smoke test for short flags
        result = run_script("-h")
        assert result.returncode == 0


# TODO(mattkram): Remove this test class once we don't need GitHub tokens
class TestGithubTokenEnvVar:
    """Tests for environment variable handling."""

    def test_github_token_env_var(self, env_isolated: dict[str, str]) -> None:
        """Test that GITHUB_TOKEN is recognized."""
        env_isolated["GITHUB_TOKEN"] = "test_token_12345"
        # This will fail because the token is fake, but we can check it tried to use it
        result = run_script(env=env_isolated)
        # Should try to use the token (will fail at API call)
        assert result.returncode == 1
        assert "GitHub API" in result.stderr or "Download failed" in result.stderr

    def test_github_token_missing_errors(self, env_isolated: dict[str, str]) -> None:
        """Test that missing GitHub token produces an error."""
        # Ensure no token is available
        env_isolated.pop("GITHUB_TOKEN", None)
        # Set PATH to only include essential system paths (no gh CLI)
        env_isolated["PATH"] = "/usr/bin:/bin"

        result = run_script(env=env_isolated)
        assert result.returncode == 1
        assert "GitHub token" in result.stderr or "token" in result.stderr.lower()


class TestInstallation:
    """Tests for installation using mock server."""

    def test_successful_install(
        self,
        env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test successful installation of a specific version."""
        result = run_script(env=env_with_mock_server)

        expected_binary = install_dir / "ana"

        assert result.returncode == 0
        assert "Installing ana for" in result.stdout
        assert f"Installed ana to {expected_binary}" in result.stdout
        assert "Done!" in result.stdout

        # Verify binary exists and is executable
        assert expected_binary.exists()
        assert expected_binary.stat().st_mode & stat.S_IXUSR

    def test_install_with_cli_options(
        self,
        env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test installation using CLI options."""
        # Remove env vars to test CLI takes precedence
        del env_with_mock_server["ANA_INSTALL_DIR"]

        result = run_script(
            "--install-dir",
            str(install_dir),
            env=env_with_mock_server,
        )

        assert result.returncode == 0
        assert (install_dir / "ana").exists()

    def test_checksum_verification_disabled_warning(
        self,
        env_with_mock_server: dict[str, str],
    ) -> None:
        """Test that checksum verification disabled warning is shown."""
        result = run_script("--no-verify-checksum", env=env_with_mock_server)

        assert result.returncode == 0
        assert (
            "Checksum verification disabled" in result.stderr
            or "Checksum verification disabled" in result.stdout
        )


class TestForceInstall:
    """Tests for --force flag behavior."""

    def test_overwrite_without_force_fails_non_tty(
        self,
        env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test that overwriting without --force fails in non-TTY mode."""
        # First install
        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0

        # Try to install again without --force
        result = run_script(env=env_with_mock_server)
        assert result.returncode == 1
        assert "already exists" in result.stderr
        assert "--force" in result.stderr

    def test_overwrite_with_force_succeeds(
        self,
        env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test that overwriting with --force succeeds."""
        # First install
        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0

        # Second install with --force
        result = run_script("--force", env=env_with_mock_server)
        assert result.returncode == 0

    def test_force_via_env_var(
        self,
        env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test ANA_FORCE_INSTALL environment variable."""
        env_with_mock_server["ANA_FORCE_INSTALL"] = "1"

        # First install
        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0

        # Second install (should succeed due to env var)
        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0


class TestShellProfileUpdate:
    """Tests for shell profile modification."""

    def test_no_path_update_flag(
        self,
        env_with_mock_server: dict[str, str],
        fake_home: Path,
    ) -> None:
        """Test --no-path-update prevents shell profile modification."""
        del env_with_mock_server["ANA_NO_PATH_UPDATE"]

        zshrc_before = (fake_home / ".zshrc").read_text()

        result = run_script("--no-path-update", env=env_with_mock_server)
        assert result.returncode == 0

        zshrc_after = (fake_home / ".zshrc").read_text()
        assert zshrc_before == zshrc_after

    def test_no_path_update_env_var(
        self,
        env_with_mock_server: dict[str, str],
        fake_home: Path,
    ) -> None:
        """Test ANA_NO_PATH_UPDATE prevents shell profile modification."""
        env_with_mock_server["ANA_NO_PATH_UPDATE"] = "1"

        zshrc_before = (fake_home / ".zshrc").read_text()

        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0

        zshrc_after = (fake_home / ".zshrc").read_text()
        assert zshrc_before == zshrc_after

    def test_path_update_modifies_profile(
        self,
        env_with_mock_server: dict[str, str],
        fake_home: Path,
        install_dir: Path,
    ) -> None:
        """Test that path update modifies the shell profile."""
        del env_with_mock_server["ANA_NO_PATH_UPDATE"]
        # Set SHELL to zsh for predictable behavior
        env_with_mock_server["SHELL"] = "/bin/zsh"

        zshrc = fake_home / ".zshrc"
        zshrc_before = zshrc.read_text()

        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0

        zshrc_after = zshrc.read_text()
        assert zshrc_before != zshrc_after
        assert str(install_dir) in zshrc_after
        assert "export PATH=" in zshrc_after

    def test_path_update_idempotent(
        self,
        env_with_mock_server: dict[str, str],
        fake_home: Path,
        install_dir: Path,
    ) -> None:
        """Test that running install twice doesn't duplicate PATH entry."""
        del env_with_mock_server["ANA_NO_PATH_UPDATE"]
        env_with_mock_server["SHELL"] = "/bin/zsh"

        # First install
        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0
        zshrc_after_first = (fake_home / ".zshrc").read_text()

        # Second install (--force to overwrite existing binary)
        result = run_script("--force", env=env_with_mock_server)
        assert result.returncode == 0
        zshrc_after_second = (fake_home / ".zshrc").read_text()

        # Should be the same (no duplicate entries)
        assert zshrc_after_first == zshrc_after_second


class TestBinaryVerification:
    """Tests to verify the installed mock binary works."""

    def test_installed_binary_runs(
        self,
        env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test that the installed binary actually runs."""
        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0

        # Run the installed binary
        binary = install_dir / "ana"
        result = subprocess.run(
            [str(binary), "--version"],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert "0.0.0-mock" in result.stdout

    def test_installed_binary_help(
        self,
        env_with_mock_server: dict[str, str],
        install_dir: Path,
    ) -> None:
        """Test that the installed binary shows help."""
        result = run_script(env=env_with_mock_server)
        assert result.returncode == 0

        binary = install_dir / "ana"
        result = subprocess.run(
            [str(binary), "--help"],
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        assert "Mock ana CLI for testing" in result.stdout
