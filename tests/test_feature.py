"""Integration tests for the 'ana feature' command."""

from __future__ import annotations

import subprocess
from pathlib import Path

import pytest
from helpers import AnaRunner
from mock_auth_server import MockAuthServer

MAIN_X_CHANNEL = "https://repo.anaconda.cloud/repo/main-x"


def is_conda_available() -> bool:
    """Check if conda is available in PATH."""
    try:
        result = subprocess.run(
            ["conda", "--version"],
            capture_output=True,
            text=True,
        )
        return result.returncode == 0
    except FileNotFoundError:
        return False


# Skip all tests in this module if conda is not available
pytestmark = pytest.mark.skipif(
    not is_conda_available(),
    reason="conda not available in PATH",
)


@pytest.fixture
def conda_isolated_env(tmp_path: Path, env_isolated: dict[str, str]) -> dict[str, str]:
    """Provide an environment with isolated conda configuration.

    Uses CONDARC environment variable to point conda at an isolated config file,
    preventing tests from modifying the user's actual conda configuration.
    """
    condarc_path = tmp_path / ".condarc"
    condarc_path.write_text("channels:\n  - defaults\n")

    return {
        **env_isolated,
        "CONDARC": str(condarc_path),
    }


@pytest.fixture
def feature_env(
    conda_isolated_env: dict[str, str],
    mock_auth_server: MockAuthServer,
    keyring_path: Path,
) -> dict[str, str]:
    """Environment for feature tests: isolated conda + mock auth server."""
    return {
        **conda_isolated_env,
        "ANA_DOMAIN": mock_auth_server.domain,
        "ANA_KEYRING_PATH": str(keyring_path),
        "ANA_OPEN_BROWSER": "false",
        "ANA_USE_HTTPS": "false",
    }


@pytest.fixture
def run_conda(conda_isolated_env: dict[str, str]) -> AnaRunner:
    """Provide a function to run conda commands with isolated configuration."""

    def _run(*args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["conda", *args],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=conda_isolated_env,
        )

    return _run


@pytest.fixture
def run_ana_feature(
    ana_binary: Path | None,
    feature_env: dict[str, str],
) -> AnaRunner:
    """Provide a function to run ana with isolated conda + mock auth."""
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
        merged_env = {**feature_env, **(env or {})}
        return subprocess.run(
            [str(ana_binary), *args],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=merged_env,
            input=input,
            cwd=cwd,
        )

    return _run


def get_channels(env: dict[str, str]) -> list[str]:
    """Get the list of configured channels from conda."""
    result = subprocess.run(
        ["conda", "config", "--show", "channels"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
    )
    assert result.returncode == 0, f"conda config failed: {result.stderr}"

    channels = []
    for line in result.stdout.splitlines():
        line = line.strip()
        if line.startswith("- "):
            channels.append(line[2:])
    return channels


class TestFeatureHelp:
    """Tests for 'ana feature --help'."""

    def test_feature_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("feature", "--help")
        assert result.returncode == 0
        assert "Enable or disable Anaconda features" in result.stdout

    def test_feature_shows_subcommands(self, run_ana: AnaRunner) -> None:
        result = run_ana("feature", "--help")
        assert result.returncode == 0
        assert "enable" in result.stdout
        assert "disable" in result.stdout

    def test_feature_enable_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("feature", "enable", "--help")
        assert result.returncode == 0
        assert "Enable a feature" in result.stdout

    def test_feature_disable_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("feature", "disable", "--help")
        assert result.returncode == 0
        assert "Disable a feature" in result.stdout


class TestFeatureUnknown:
    """Tests for unknown feature names."""

    def test_enable_unknown_feature(self, run_ana: AnaRunner) -> None:
        result = run_ana("feature", "enable", "unknown-feature", "-f")
        assert result.returncode != 0
        assert "Unknown feature" in result.stderr

    def test_disable_unknown_feature(self, run_ana: AnaRunner) -> None:
        result = run_ana("feature", "disable", "unknown-feature", "-f")
        assert result.returncode != 0
        assert "Unknown feature" in result.stderr


class TestMainXEnable:
    """Tests for 'ana feature enable main-x'."""

    def test_enable_main_x_adds_channel(
        self,
        run_ana_feature: AnaRunner,
        feature_env: dict[str, str],
    ) -> None:
        """Enabling main-x should add the main-x channel to conda config."""
        # Verify main-x is not in channels initially
        initial_channels = get_channels(feature_env)
        assert MAIN_X_CHANNEL not in initial_channels

        # Login first via mock server (device flow auto-completes)
        login_result = run_ana_feature("login")
        assert login_result.returncode == 0, f"Login failed: {login_result.stderr}"

        # Enable main-x with force flag (skip confirmation)
        result = run_ana_feature("feature", "enable", "main-x", "-f")
        assert result.returncode == 0, f"Enable failed: {result.stderr}"

        # Verify main-x channel was added
        final_channels = get_channels(feature_env)
        assert MAIN_X_CHANNEL in final_channels

    def test_enable_main_x_idempotent(
        self,
        run_ana_feature: AnaRunner,
        feature_env: dict[str, str],
    ) -> None:
        """Enabling main-x when already enabled should succeed with 'already enabled' message."""
        # Pre-configure main-x channel
        condarc_path = Path(feature_env["CONDARC"])
        condarc_path.write_text(f"channels:\n  - {MAIN_X_CHANNEL}\n  - defaults\n")

        # Login first
        login_result = run_ana_feature("login")
        assert login_result.returncode == 0

        # Try to enable main-x again
        result = run_ana_feature("feature", "enable", "main-x", "-f")
        assert result.returncode == 0
        assert "already enabled" in result.stderr.lower()

    def test_enable_main_x_requires_login(
        self,
        ana_binary: Path | None,
        conda_isolated_env: dict[str, str],
    ) -> None:
        """Enabling main-x without login should trigger login flow."""
        if ana_binary is None:
            pytest.skip("ana binary not found")

        # Run without auth - no mock server, no keyring
        result = subprocess.run(
            [str(ana_binary), "feature", "enable", "main-x", "-f"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env={
                **conda_isolated_env,
                "ANA_OPEN_BROWSER": "false",
            },
        )

        # Should fail or try to start login
        assert (
            "login" in result.stderr.lower() or "not logged in" in result.stderr.lower()
        )


class TestMainXDisable:
    """Tests for 'ana feature disable main-x'."""

    def test_disable_main_x_removes_channel(
        self,
        run_ana_feature: AnaRunner,
        feature_env: dict[str, str],
    ) -> None:
        """Disabling main-x should remove the main-x channel from conda config."""
        # Pre-configure main-x channel
        condarc_path = Path(feature_env["CONDARC"])
        condarc_path.write_text(f"channels:\n  - {MAIN_X_CHANNEL}\n  - defaults\n")

        # Verify main-x is in channels
        initial_channels = get_channels(feature_env)
        assert MAIN_X_CHANNEL in initial_channels

        # Disable main-x with force flag
        result = run_ana_feature("feature", "disable", "main-x", "-f")
        assert result.returncode == 0, f"Disable failed: {result.stderr}"

        # Verify main-x channel was removed
        final_channels = get_channels(feature_env)
        assert MAIN_X_CHANNEL not in final_channels

    def test_disable_main_x_not_enabled(
        self,
        run_ana_feature: AnaRunner,
        feature_env: dict[str, str],
    ) -> None:
        """Disabling main-x when not enabled should succeed with appropriate message."""
        # Verify main-x is not in channels
        initial_channels = get_channels(feature_env)
        assert MAIN_X_CHANNEL not in initial_channels

        result = run_ana_feature("feature", "disable", "main-x", "-f")
        assert result.returncode == 0
        assert "not enabled" in result.stderr.lower()

    def test_disable_main_x_preserves_other_channels(
        self,
        run_ana_feature: AnaRunner,
        feature_env: dict[str, str],
    ) -> None:
        """Disabling main-x should not affect other configured channels."""
        # Pre-configure multiple channels including main-x
        condarc_path = Path(feature_env["CONDARC"])
        condarc_path.write_text(
            f"channels:\n  - {MAIN_X_CHANNEL}\n  - conda-forge\n  - defaults\n"
        )

        initial_channels = get_channels(feature_env)
        assert "conda-forge" in initial_channels
        assert "defaults" in initial_channels

        # Disable main-x
        result = run_ana_feature("feature", "disable", "main-x", "-f")
        assert result.returncode == 0

        # Verify other channels are preserved
        final_channels = get_channels(feature_env)
        assert "conda-forge" in final_channels
        assert "defaults" in final_channels
        assert MAIN_X_CHANNEL not in final_channels


class TestMainXUserInteraction:
    """Tests for user interaction (confirmation prompts)."""

    def test_enable_shows_commands_and_prompts(
        self,
        run_ana_feature: AnaRunner,
    ) -> None:
        """Enable should show conda commands and prompt for confirmation."""
        # Login first
        login_result = run_ana_feature("login")
        assert login_result.returncode == 0

        # Run enable without -f, answer 'n' to abort
        result = run_ana_feature("feature", "enable", "main-x", input="n\n")

        # Should show the command to be executed
        assert "conda config" in result.stderr
        # Should abort when user says no
        assert "Aborted" in result.stderr

    def test_disable_shows_commands_and_prompts(
        self,
        run_ana_feature: AnaRunner,
        feature_env: dict[str, str],
    ) -> None:
        """Disable should show conda commands and prompt for confirmation."""
        # Pre-configure main-x
        condarc_path = Path(feature_env["CONDARC"])
        condarc_path.write_text(f"channels:\n  - {MAIN_X_CHANNEL}\n  - defaults\n")

        # Run disable without -f, answer 'n' to abort
        result = run_ana_feature("feature", "disable", "main-x", input="n\n")

        # Should show the command to be executed
        assert "conda config" in result.stderr
        assert "--remove" in result.stderr
        # Should abort when user says no
        assert "Aborted" in result.stderr

        # Channel should still be present
        final_channels = get_channels(feature_env)
        assert MAIN_X_CHANNEL in final_channels


class TestMainXEndToEnd:
    """End-to-end tests for the main-x feature workflow."""

    def test_enable_then_disable(
        self,
        run_ana_feature: AnaRunner,
        feature_env: dict[str, str],
    ) -> None:
        """Full workflow: login -> enable -> verify -> disable -> verify."""
        # Step 1: Login
        login_result = run_ana_feature("login")
        assert login_result.returncode == 0

        # Step 2: Enable main-x
        enable_result = run_ana_feature("feature", "enable", "main-x", "-f")
        assert enable_result.returncode == 0

        # Step 3: Verify channel was added
        channels_after_enable = get_channels(feature_env)
        assert MAIN_X_CHANNEL in channels_after_enable

        # Step 4: Disable main-x
        disable_result = run_ana_feature("feature", "disable", "main-x", "-f")
        assert disable_result.returncode == 0

        # Step 5: Verify channel was removed
        channels_after_disable = get_channels(feature_env)
        assert MAIN_X_CHANNEL not in channels_after_disable
