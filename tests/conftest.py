"""Shared fixtures for integration tests."""

from __future__ import annotations

import os
import subprocess
from collections.abc import Callable
from collections.abc import Generator
from pathlib import Path

import pytest
from mock_auth_server import MockAuthServer


def assert_output_contains(text: str, *patterns: str) -> None:
    """Assert that patterns appear in text in order.

    Provides clear error messages showing:
    - Which pattern failed to match
    - What patterns matched before it
    - The relevant portion of text around the search position

    Args:
        text: The text to search (e.g., result.stderr)
        *patterns: Strings that should appear in order

    Example:
        assert_output_contains(
            result.stderr,
            "ACCOUNT",
            "username",
            "testuser",
            "SUBSCRIPTIONS",
        )
    """
    pos = 0
    matched = []

    for pattern in patterns:
        idx = text.find(pattern, pos)
        if idx < 0:
            # Build error message with context
            lines = text.splitlines()
            # Find which line we're at
            chars_seen = 0
            current_line = 0
            for i, line in enumerate(lines):
                if chars_seen + len(line) >= pos:
                    current_line = i
                    break
                chars_seen += len(line) + 1  # +1 for newline

            # Show context: a few lines before and after current position
            start_line = max(0, current_line - 2)
            end_line = min(len(lines), current_line + 5)
            context_lines = lines[start_line:end_line]
            context = "\n".join(
                f"  {'>' if i == current_line - start_line else ' '} {line}"
                for i, line in enumerate(context_lines)
            )

            matched_str = "\n  - ".join([""] + matched) if matched else " (none)"
            msg = (
                f"Pattern not found: {pattern!r}\n\n"
                f"Already matched:{matched_str}\n\n"
                f"Searching from line {current_line + 1}:\n{context}\n\n"
                f"Full output:\n{text}"
            )
            raise AssertionError(msg)

        matched.append(pattern)
        pos = idx + len(pattern)


def _find_repo_root() -> Path:
    """Find the repository root by looking for .git directory."""
    path = Path(__file__).resolve()
    for parent in path.parents:
        if (parent / ".git").exists():
            return parent
    raise RuntimeError("Could not find repository root")


REPO_ROOT = _find_repo_root()


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

    for subpath in ["target/release/ana", "target/debug/ana"]:
        binary = REPO_ROOT / subpath
        if binary.exists():
            return binary

    return None


AnaRunner = Callable[..., subprocess.CompletedProcess[str]]


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
