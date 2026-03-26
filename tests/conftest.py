"""Shared fixtures for integration tests."""

from __future__ import annotations

import os
import subprocess
from collections.abc import Callable
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


@pytest.fixture(scope="session")
def run_ana(ana_binary: Path | None) -> AnaRunner:
    """Provide a function to run the ana binary."""
    if ana_binary is None:
        pytest.skip(
            "ana binary not found. Build with 'pixi run build-release' or set ANA_BINARY_PATH"
        )

    def _run(
        *args: str,
        env: dict[str, str] | None = None,
        input: str | None = None,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(ana_binary), *args],
            capture_output=True,
            text=True,
            env=env,
            input=input,
        )

    return _run
