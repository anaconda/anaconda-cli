"""Integration tests for the conda package build of ana.

Builds the anaconda-cli conda package (the `conda-package` feature build:
no self-update, no tool management, binaries resolved from $CONDA_PREFIX),
installs it into a real conda environment with its full dependency tree
resolved from repo.anaconda.com, and smoke-tests the packaged binary.

The package build and environment creation are session-scoped and reused
across tests. To skip the build and test a pre-built package, set
ANA_CONDA_PACKAGE_PATH to the .conda file, or run `pixi run build-conda`
first (output/ is reused).

Note: the environment install is network-dependent and slow (it downloads
the full anaconda-* dependency tree on first run).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from collections.abc import Generator
from pathlib import Path

import pytest
from helpers import IS_WINDOWS
from helpers import REPO_ROOT

PACKAGE_NAME = "anaconda-cli"


def _find_built_package() -> Path | None:
    """Find an existing anaconda-cli package in the rattler-build output dir."""
    output_dir = REPO_ROOT / "output"
    if not output_dir.is_dir():
        return None
    packages = sorted(
        output_dir.glob(f"*/{PACKAGE_NAME}-*.conda"),
        key=lambda p: p.stat().st_mtime,
    )
    return packages[-1] if packages else None


def _build_package() -> None:
    """Build the conda package via the pixi task (binary + rattler-build)."""
    if shutil.which("pixi"):
        cmd = ["pixi", "run", "build-conda"]
    elif shutil.which("rattler-build") and shutil.which("cargo"):
        with_version = REPO_ROOT / "scripts" / "with_version.py"
        subprocess.run(
            [
                sys.executable,
                str(with_version),
                "cargo",
                "build",
                "--release",
                "--no-default-features",
                "--features",
                "conda-package",
                "--target-dir",
                "target/conda-package",
            ],
            cwd=REPO_ROOT,
            check=True,
        )
        cmd = [
            sys.executable,
            str(with_version),
            "rattler-build",
            "build",
            "--recipe",
            "conda.recipe",
        ]
    else:
        pytest.skip("building the conda package requires pixi or rattler-build + cargo")

    subprocess.run(cmd, cwd=REPO_ROOT, check=True)


@pytest.fixture(scope="session")
def conda_package() -> Path:
    """Provide the path to the anaconda-cli .conda package, building if needed."""
    if env_path := os.getenv("ANA_CONDA_PACKAGE_PATH"):
        path = Path(env_path)
        if path.is_file():
            return path
        pytest.fail(f"ANA_CONDA_PACKAGE_PATH does not exist: {path}")

    package = _find_built_package()
    if package is None:
        _build_package()
        package = _find_built_package()
    if package is None:
        pytest.fail("conda package build succeeded but no .conda found in output/")
    return package


@pytest.fixture(scope="session")
def conda_env_prefix(
    conda_package: Path, tmp_path_factory: pytest.TempPathFactory
) -> Generator[Path, None, None]:
    """Install the package with its full dependency tree into a fresh env.

    The local rattler-build output dir serves as the channel for
    anaconda-cli itself; run dependencies resolve from the anaconda-cloud
    and pkgs/main channels.
    """
    conda = shutil.which("conda")
    if conda is None:
        pytest.skip("conda is required to install the package into an environment")

    prefix = tmp_path_factory.mktemp("conda-env") / "env"
    local_channel = conda_package.parent.parent.as_uri()
    result = subprocess.run(
        [
            conda,
            "create",
            "-y",
            "-p",
            str(prefix),
            "-c",
            local_channel,
            "-c",
            "anaconda-cloud",
            # anaconda-repo-cli is only published under the dev label
            "-c",
            "anaconda-cloud/label/dev",
            "-c",
            "https://repo.anaconda.com/pkgs/main",
            PACKAGE_NAME,
        ],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        pytest.fail(
            f"conda create failed for {conda_package}:\n{result.stdout}\n{result.stderr}"
        )

    yield prefix

    if IS_WINDOWS:
        subprocess.run(["cmd", "/c", "rmdir", "/s", "/q", str(prefix)], check=False)
    else:
        shutil.rmtree(prefix, ignore_errors=True)


@pytest.fixture(scope="session")
def run_packaged_ana(
    conda_env_prefix: Path, tmp_path_factory: pytest.TempPathFactory
) -> Generator:
    """Run the packaged ana binary with CONDA_PREFIX pointed at the test env."""
    home = tmp_path_factory.mktemp("conda-home")
    binary = conda_env_prefix / "bin" / ("ana.exe" if IS_WINDOWS else "ana")
    if not binary.exists():
        pytest.fail(f"packaged binary not found at {binary}")

    env = {key: val for key, val in os.environ.items() if not key.startswith("ANA_")}
    env["CONDA_PREFIX"] = str(conda_env_prefix)
    env["ANA_ENABLE_TELEMETRY"] = "false"
    env["RUST_LOG"] = "off"
    if IS_WINDOWS:
        env["USERPROFILE"] = str(home)
    else:
        env["HOME"] = str(home)
    env_base = env

    def _run(
        *args: str, env: dict[str, str | None] | None = None
    ) -> subprocess.CompletedProcess[str]:
        # Overlay env vars; a value of None removes the variable
        merged = {**env_base, **(env or {})}
        merged = {k: v for k, v in merged.items() if v is not None}
        return subprocess.run(
            [str(binary), *args],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=merged,
            timeout=60,
        )

    yield _run


class TestCondaPackage:
    """Smoke tests for the packaged conda build of ana."""

    def test_version(self, run_packaged_ana, conda_package: Path) -> None:
        result = run_packaged_ana("--version")
        assert result.returncode == 0
        version = conda_package.name.removeprefix(f"{PACKAGE_NAME}-").split("-")[0]
        assert version in result.stdout

    def test_help(self, run_packaged_ana) -> None:
        result = run_packaged_ana("--help")
        assert result.returncode == 0
        assert "Usage" in result.stdout

    def test_self_update_unavailable(self, run_packaged_ana) -> None:
        result = run_packaged_ana("self", "update")
        assert result.returncode == 1
        assert "Self-update is not available" in result.stderr

    def test_tool_install_unavailable(self, run_packaged_ana) -> None:
        result = run_packaged_ana("tool", "install", "pixi")
        assert result.returncode == 1
        assert "Tool management is not available" in result.stderr

    def test_tool_uninstall_unavailable(self, run_packaged_ana) -> None:
        result = run_packaged_ana("tool", "uninstall", "pixi")
        assert result.returncode == 1
        assert "Tool management is not available" in result.stderr

    def test_tool_list_works(self, run_packaged_ana) -> None:
        result = run_packaged_ana("tool", "list")
        assert result.returncode == 0
        # Installation status comes from the env's conda-meta entries
        cli_row = next(
            line for line in result.stdout.splitlines() if "anaconda-cli" in line
        )
        assert "✓" in cli_row

    def test_works_without_conda_prefix(self, run_packaged_ana) -> None:
        """The prefix is derived from the executable location, so invoking
        ana by absolute path without an activated environment works."""
        result = run_packaged_ana("tool", "list", env={"CONDA_PREFIX": None})
        assert result.returncode == 0
        cli_row = next(
            line for line in result.stdout.splitlines() if "anaconda-cli" in line
        )
        assert "✓" in cli_row

    def test_org_proxies_to_installed_anaconda(self, run_packaged_ana) -> None:
        """ana org locates and executes the anaconda binary from a run dep.

        The proxied command itself may fail (no login); what matters is that
        binary resolution succeeded rather than erroring with 'not found'.
        """
        result = run_packaged_ana("org", "whoami")
        assert "not found at" not in result.stderr
        assert "Could not determine conda environment prefix" not in result.stderr

    def test_run_deps_installed(self, conda_env_prefix: Path) -> None:
        """All run dependencies from the recipe are installed in the env."""
        conda_meta = conda_env_prefix / "conda-meta"
        for dep in [
            "anaconda-audit",
            "anaconda-auth",
            "anaconda-client",
            "anaconda-env-log",
            "anaconda-mcp",
            "anaconda-repo-cli",
        ]:
            assert any(conda_meta.glob(f"{dep}-*.json")), f"{dep} not installed"

    def test_anaconda_binary_available(self, conda_env_prefix: Path) -> None:
        """The `anaconda` binary that ana proxies to comes from a run dep."""
        bin_subdir = "Scripts" if IS_WINDOWS else "bin"
        binary = "anaconda.exe" if IS_WINDOWS else "anaconda"
        assert (conda_env_prefix / bin_subdir / binary).exists()
