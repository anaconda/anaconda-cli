"""Shared fixtures for integration tests."""

from __future__ import annotations

import http.server
import os
import socketserver
import subprocess
import threading
from collections.abc import Generator
from functools import partial
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from helpers import IS_WINDOWS
from helpers import REPO_ROOT
from helpers import AnaRunner
from mock_auth_server import MockAuthServer

if TYPE_CHECKING:
    from collections.abc import Generator


@pytest.fixture
def fake_home(tmp_path: Path) -> Path:
    """Provide a fake HOME directory for test isolation.

    Creates shell config files for testing profile modifications.
    """
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
def env_isolated(fake_home: Path) -> dict[str, str]:
    """Provide an isolated environment without ANA_* or GITHUB_TOKEN vars."""
    env = {
        key: val
        for key, val in os.environ.copy().items()
        if not key.startswith("ANA_") and key != "GITHUB_TOKEN"
    }
    if IS_WINDOWS:
        env["USERPROFILE"] = str(fake_home)
        # Rattler does not reliably detect the default cache for Windows tests
        env["RATTLER_CACHE_DIR"] = str(fake_home / "cache" / "rattler")
    else:
        env["HOME"] = str(fake_home)
    return env


@pytest.fixture(scope="session")
def ana_binary() -> Path | None:
    """Find the ana binary in standard locations.

    Search order:
    1. ANA_BINARY_PATH environment variable
    2. target/release/ana (release build)
    3. target/debug/ana (debug build)
    """
    if env_path := os.getenv("ANA_BINARY_PATH"):
        path = Path(env_path)
        if path.exists() and path.is_file():
            return path

    ana_bin = "ana.exe" if IS_WINDOWS else "ana"

    for subpath in ["target/release", "target/debug"]:
        binary = REPO_ROOT / subpath / ana_bin
        if binary.exists():
            return binary

    return None


@pytest.fixture
def run_ana(ana_binary: Path | None, env_isolated: dict[str, str]) -> AnaRunner:
    """Provide a function to run the ana binary with isolated environment."""
    if ana_binary is None:
        pytest.skip(
            "ana binary not found. Build with 'pixi run build-release' or set ANA_BINARY_PATH"
        )

    def _run(
        *args: str,
        env: dict[str, str] | None = None,
        input: str | None = None,
        cwd: Path | str | None = None,
    ) -> subprocess.CompletedProcess[str]:
        # Start with isolated environment and update with argument if available
        env = {**env_isolated, **(env or {})}
        return subprocess.run(
            [str(ana_binary), *args],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=env,
            input=input,
            cwd=cwd,
        )

    return _run


@pytest.fixture
def mock_auth_server() -> Generator[MockAuthServer, None, None]:
    """Run a mock authentication server for testing."""
    with MockAuthServer() as server:
        yield server


@pytest.fixture
def keyring_path(tmp_path: Path) -> Path:
    """Provide a temporary keyring file path."""
    return tmp_path / "keyring"


@pytest.fixture
def auth_env(
    env_isolated: dict[str, str],
    mock_auth_server: MockAuthServer,
    keyring_path: Path,
) -> dict[str, str]:
    """Environment configured to use mock auth server."""
    return {
        **env_isolated,
        "ANA_DOMAIN": mock_auth_server.domain,
        "ANA_KEYRING_PATH": str(keyring_path),
        "ANA_OPEN_BROWSER": "false",  # Don't try to open browser in tests
        "ANA_USE_HTTPS": "false",  # Use HTTP for mock server
    }


@pytest.fixture
def ana_install_env_isolated(fake_home: Path) -> dict[str, str]:
    """Provide an isolated environment that won't modify real shell configs."""
    env = {
        key: val for key, val in os.environ.copy().items() if not key.startswith("ANA_")
    }

    env["HOME"] = str(fake_home)
    env["ANA_INSTALL_DIR"] = str(fake_home / "local" / "bin")
    env["ANA_NO_PATH_UPDATE"] = "1"  # Extra safety
    return env


@pytest.fixture(scope="session")
def ana_binary_mock_server(
    tmp_path_factory: pytest.TempPathFactory,
) -> Generator[str, None, None]:
    """Start a local HTTP server to host mock binaries."""
    import hashlib

    # Create a simple mock binary script that responds to --version and --help
    mock_sh_script = """\
#!/bin/sh
case "$1" in
    --version) echo "0.0.0-mock" ;;
    --help) echo "Mock ana CLI for testing" ;;
    *) echo "mock ana" ;;
esac
"""

    # Windows batch script equivalent
    mock_cmd_script = """\
@echo off
if "%1"=="--version" (
    echo 0.0.0-mock
) else if "%1"=="--help" (
    echo Mock ana CLI for testing
) else (
    echo mock ana
)
"""

    supported_platforms = [
        "darwin-arm64",
        "darwin-x86_64",
        "linux-x86_64",
        "linux-aarch64",
        "windows-x86_64",
    ]

    executable_mode = 0o755  # rwxr-xr-x
    # Create mock binaries for different platforms
    root = tmp_path_factory.mktemp("mock_server")

    for platform in supported_platforms:
        is_windows_platform = platform.startswith("windows")
        # Hacky workaround: use .exe so that the install script works,
        # but tests need to rename the file to .cmd because Windows
        # expects .exe files to be PE files no matter the content
        suffix = ".exe" if is_windows_platform else ""
        binary = root / f"ana-{platform}{suffix}"

        binary_content = (
            mock_cmd_script.encode() if is_windows_platform else mock_sh_script.encode()
        )

        # Use write_bytes to avoid line ending conversion on Windows
        binary.write_bytes(binary_content)
        binary.chmod(executable_mode)

        # Create corresponding checksum file
        checksum = hashlib.sha256(binary_content).hexdigest()
        checksum_file = root / f"{binary.name}.sha256"
        checksum_file.write_text(f"{checksum}  {binary.name}\n")

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
def ana_install_env_with_mock_server(
    ana_install_env_isolated: dict[str, str],
    ana_binary_mock_server: str,
) -> dict[str, str]:
    """Provide isolated environment with mock server URL."""
    ana_install_env_isolated["ANA_BASE_URL"] = ana_binary_mock_server
    return ana_install_env_isolated
