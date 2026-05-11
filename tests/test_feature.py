"""Integration tests for the 'ana feature' command."""

from __future__ import annotations

import json
import os
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


def is_pixi_available() -> bool:
    """Check if pixi is available in PATH."""
    try:
        result = subprocess.run(
            ["pixi", "--version"],
            capture_output=True,
            text=True,
        )
        return result.returncode == 0
    except FileNotFoundError:
        return False


requires_conda = pytest.mark.skipif(
    not is_conda_available(),
    reason="conda not available in PATH",
)

requires_pixi = pytest.mark.skipif(
    not is_pixi_available(),
    reason="pixi not available in PATH",
)


def get_test_api_key() -> str | None:
    """Get API key for integration tests from environment."""
    return os.environ.get("ANA_TEST_API_KEY")


requires_api_key = pytest.mark.skipif(
    get_test_api_key() is None,
    reason="ANA_TEST_API_KEY not set - required for real pixi auth tests",
)


@pytest.fixture
def conda_isolated_env(fake_home: Path, env_isolated: dict[str, str]) -> dict[str, str]:
    """Provide an environment with isolated conda configuration.

    Creates a .condarc file in the fake HOME directory so conda uses isolated
    configuration and doesn't modify the user's actual conda settings.
    """
    condarc_path = fake_home / ".condarc"
    condarc_path.write_text("channels:\n  - defaults\n")

    return {
        **env_isolated,
        "CONDARC": str(condarc_path),
    }


@pytest.fixture
def pixi_isolated_env(fake_home: Path, env_isolated: dict[str, str]) -> dict[str, str]:
    """Provide an environment with isolated pixi configuration.

    Uses PIXI_HOME to isolate pixi's global config and auth from user's settings.
    """
    pixi_home = fake_home / ".pixi"
    pixi_home.mkdir(parents=True, exist_ok=True)

    return {
        **env_isolated,
        "PIXI_HOME": str(pixi_home),
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
def pixi_feature_env(
    pixi_isolated_env: dict[str, str],
    keyring_path: Path,
) -> dict[str, str]:
    """Environment for pixi feature tests: isolated pixi + real auth.

    Unlike conda tests which use a mock auth server, pixi tests need real
    credentials because pixi auth login actually validates against repo.anaconda.cloud.
    """
    return {
        **pixi_isolated_env,
        "ANA_KEYRING_PATH": str(keyring_path),
        "ANA_OPEN_BROWSER": "false",
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


@pytest.fixture
def run_ana_pixi_feature(
    ana_binary: Path | None,
    pixi_feature_env: dict[str, str],
) -> AnaRunner:
    """Provide a function to run ana with isolated pixi + mock auth."""
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
        merged_env = {**pixi_feature_env, **(env or {})}
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


def get_pixi_channels(env: dict[str, str]) -> list[str]:
    """Get the list of configured default channels from pixi."""
    result = subprocess.run(
        ["pixi", "config", "list", "--global", "--json"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
    )
    if result.returncode != 0:
        return []

    try:
        config = json.loads(result.stdout)
        return config.get("default-channels", [])
    except json.JSONDecodeError:
        return []


@pytest.fixture
def run_pixi(pixi_isolated_env: dict[str, str]) -> AnaRunner:
    """Provide a function to run pixi commands with isolated configuration."""

    def _run(*args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["pixi", *args],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=pixi_isolated_env,
        )

    return _run


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


@requires_conda
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
        tmp_path: Path,
    ) -> None:
        """Enabling main-x without login should trigger login flow."""
        if ana_binary is None:
            pytest.skip("ana binary not found")

        # Run without auth - use empty keyring, fake domain that won't respond
        empty_keyring = tmp_path / "empty_keyring"
        empty_keyring.write_text("{}")

        result = subprocess.run(
            [str(ana_binary), "feature", "enable", "main-x", "-f"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env={
                **conda_isolated_env,
                "ANA_KEYRING_PATH": str(empty_keyring),
                "ANA_OPEN_BROWSER": "false",
                # Use invalid domain to make login fail quickly
                "ANA_DOMAIN": "invalid.test",
            },
            timeout=10,  # Fail fast if it hangs
        )

        # Should fail trying to login
        assert result.returncode != 0
        assert (
            "login" in result.stderr.lower() or "not logged in" in result.stderr.lower()
        )


@requires_conda
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


@requires_conda
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


@requires_conda
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


# =============================================================================
# Pixi Main-X Tests
# =============================================================================


@requires_pixi
@requires_api_key
class TestMainXPixiEnable:
    """Tests for 'ana feature enable main-x --pixi'.

    These tests require ANA_TEST_API_KEY to be set because pixi auth login
    validates credentials against the real repo.anaconda.cloud.
    """

    def test_enable_main_x_pixi_adds_channel(
        self,
        run_ana_pixi_feature: AnaRunner,
        pixi_feature_env: dict[str, str],
    ) -> None:
        """Enabling main-x with --pixi should add the main-x channel to pixi config."""
        # Verify main-x is not in channels initially
        initial_channels = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL not in initial_channels

        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_pixi_feature("login", api_key, "-f")
        assert login_result.returncode == 0, f"Login failed: {login_result.stderr}"

        # Enable main-x with --pixi and force flag
        result = run_ana_pixi_feature("feature", "enable", "main-x", "--pixi", "-f")
        assert result.returncode == 0, f"Enable failed: {result.stderr}"

        # Verify main-x channel was added
        final_channels = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL in final_channels

    def test_enable_main_x_pixi_idempotent(
        self,
        run_ana_pixi_feature: AnaRunner,
        pixi_feature_env: dict[str, str],
    ) -> None:
        """Enabling main-x with --pixi when already enabled should succeed."""
        # Pre-configure main-x channel via pixi config
        subprocess.run(
            ["pixi", "config", "prepend", "--global", "default-channels", MAIN_X_CHANNEL],
            env=pixi_feature_env,
            check=True,
        )

        # Verify main-x is in channels
        initial_channels = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL in initial_channels

        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_pixi_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        # Try to enable main-x again
        result = run_ana_pixi_feature("feature", "enable", "main-x", "--pixi", "-f")
        assert result.returncode == 0
        assert "already enabled" in result.stderr.lower()

    def test_enable_main_x_pixi_requires_login(
        self,
        ana_binary: Path | None,
        pixi_isolated_env: dict[str, str],
        tmp_path: Path,
    ) -> None:
        """Enabling main-x --pixi without login should trigger login flow."""
        if ana_binary is None:
            pytest.skip("ana binary not found")

        # Run without auth - use empty keyring, fake domain that won't respond
        empty_keyring = tmp_path / "empty_keyring"
        empty_keyring.write_text("{}")

        result = subprocess.run(
            [str(ana_binary), "feature", "enable", "main-x", "--pixi", "-f"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env={
                **pixi_isolated_env,
                "ANA_KEYRING_PATH": str(empty_keyring),
                "ANA_OPEN_BROWSER": "false",
                "ANA_DOMAIN": "invalid.test",
            },
            timeout=10,
        )

        # Should fail trying to login
        assert result.returncode != 0
        assert (
            "login" in result.stderr.lower() or "not logged in" in result.stderr.lower()
        )


@requires_pixi
class TestMainXPixiDisable:
    """Tests for 'ana feature disable main-x --pixi'.

    Disable tests don't require API key since they don't need to authenticate.
    """

    def test_disable_main_x_pixi_removes_channel(
        self,
        run_ana_pixi_feature: AnaRunner,
        pixi_feature_env: dict[str, str],
    ) -> None:
        """Disabling main-x --pixi should remove the main-x channel from pixi config."""
        # Pre-configure main-x channel
        subprocess.run(
            ["pixi", "config", "prepend", "--global", "default-channels", MAIN_X_CHANNEL],
            env=pixi_feature_env,
            check=True,
        )

        # Verify main-x is in channels
        initial_channels = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL in initial_channels

        # Disable main-x with --pixi and force flag
        result = run_ana_pixi_feature("feature", "disable", "main-x", "--pixi", "-f")
        assert result.returncode == 0, f"Disable failed: {result.stderr}"

        # Verify main-x channel was removed
        final_channels = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL not in final_channels

    def test_disable_main_x_pixi_not_enabled(
        self,
        run_ana_pixi_feature: AnaRunner,
        pixi_feature_env: dict[str, str],
    ) -> None:
        """Disabling main-x --pixi when not enabled should succeed with appropriate message."""
        # Verify main-x is not in channels
        initial_channels = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL not in initial_channels

        result = run_ana_pixi_feature("feature", "disable", "main-x", "--pixi", "-f")
        assert result.returncode == 0
        assert "not enabled" in result.stderr.lower()


@requires_pixi
@requires_api_key
class TestMainXPixiUserInteraction:
    """Tests for pixi user interaction (confirmation prompts)."""

    def test_enable_pixi_shows_commands_and_prompts(
        self,
        run_ana_pixi_feature: AnaRunner,
    ) -> None:
        """Enable --pixi should show pixi commands and prompt for confirmation."""
        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_pixi_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        # Run enable without -f, answer 'n' to abort
        result = run_ana_pixi_feature("feature", "enable", "main-x", "--pixi", input="n\n")

        # Should show the command to be executed
        assert "pixi" in result.stderr.lower()
        assert "config" in result.stderr.lower()
        # Should abort when user says no
        assert "Aborted" in result.stderr

    def test_disable_pixi_shows_commands_and_prompts(
        self,
        run_ana_pixi_feature: AnaRunner,
        pixi_feature_env: dict[str, str],
    ) -> None:
        """Disable --pixi should show pixi commands and prompt for confirmation."""
        # Pre-configure main-x
        subprocess.run(
            ["pixi", "config", "prepend", "--global", "default-channels", MAIN_X_CHANNEL],
            env=pixi_feature_env,
            check=True,
        )

        # Run disable without -f, answer 'n' to abort
        result = run_ana_pixi_feature("feature", "disable", "main-x", "--pixi", input="n\n")

        # Should show the commands to be executed
        assert "pixi" in result.stderr.lower()
        # Should abort when user says no
        assert "Aborted" in result.stderr

        # Channel should still be present
        final_channels = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL in final_channels


@requires_pixi
@requires_api_key
class TestMainXPixiEndToEnd:
    """End-to-end tests for the main-x pixi feature workflow."""

    def test_enable_then_disable_pixi(
        self,
        run_ana_pixi_feature: AnaRunner,
        pixi_feature_env: dict[str, str],
    ) -> None:
        """Full workflow: login -> enable --pixi -> verify -> disable --pixi -> verify."""
        # Step 1: Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_pixi_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        # Step 2: Enable main-x
        enable_result = run_ana_pixi_feature("feature", "enable", "main-x", "--pixi", "-f")
        assert enable_result.returncode == 0

        # Step 3: Verify channel was added
        channels_after_enable = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL in channels_after_enable

        # Step 4: Disable main-x
        disable_result = run_ana_pixi_feature("feature", "disable", "main-x", "--pixi", "-f")
        assert disable_result.returncode == 0

        # Step 5: Verify channel was removed
        channels_after_disable = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL not in channels_after_disable

    def test_can_install_package_from_main_x(
        self,
        run_ana_pixi_feature: AnaRunner,
        pixi_feature_env: dict[str, str],
    ) -> None:
        """Verify that after enabling main-x, we can actually install packages from it."""
        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_pixi_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        # Enable main-x
        enable_result = run_ana_pixi_feature("feature", "enable", "main-x", "--pixi", "-f")
        assert enable_result.returncode == 0

        # Try to search for a package from main-x channel
        # Using search instead of install to avoid side effects
        search_result = subprocess.run(
            ["pixi", "search", "abn", "-c", MAIN_X_CHANNEL],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=pixi_feature_env,
        )

        # Search should succeed (not 403 Forbidden) when authenticated
        assert search_result.returncode == 0, f"Search failed: {search_result.stderr}"
        assert "abn" in search_result.stdout.lower()

        # Cleanup: disable main-x
        run_ana_pixi_feature("feature", "disable", "main-x", "--pixi", "-f")

    def test_cannot_access_main_x_without_auth(
        self,
        pixi_isolated_env: dict[str, str],
    ) -> None:
        """Verify that accessing main-x without authentication fails with 403."""
        # Try to search main-x channel without any authentication
        # This should fail with 403 Forbidden
        search_result = subprocess.run(
            ["pixi", "search", "abn", "-c", MAIN_X_CHANNEL],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=pixi_isolated_env,
        )

        # Should fail with 403 Forbidden
        assert search_result.returncode != 0, "Search should fail without auth"
        assert "403" in search_result.stderr or "Forbidden" in search_result.stderr
