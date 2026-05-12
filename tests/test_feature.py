"""Integration tests for the 'ana feature' command."""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

import pytest
from helpers import AnaRunner
from mock_auth_server import MockAuthServer

MAIN_CHANNEL = "https://repo.anaconda.cloud/repo/main"
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


def is_pip_available() -> bool:
    """Check if pip or pip3 is available in PATH."""
    for cmd in ["pip", "pip3"]:
        try:
            result = subprocess.run(
                [cmd, "--version"],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                return True
        except FileNotFoundError:
            continue
    return False


def is_uv_available() -> bool:
    """Check if uv is available in PATH."""
    try:
        result = subprocess.run(
            ["uv", "--version"],
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

requires_pip = pytest.mark.skipif(
    not is_pip_available(),
    reason="pip not available in PATH",
)

requires_uv = pytest.mark.skipif(
    not is_uv_available(),
    reason="uv not available in PATH",
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
def pip_isolated_env(fake_home: Path, env_isolated: dict[str, str]) -> dict[str, str]:
    """Provide an environment with isolated pip configuration.

    pip config locations:
    - Linux/macOS: $XDG_CONFIG_HOME/pip/pip.conf (or ~/.config/pip/pip.conf)
    - Windows: %APPDATA%\\pip\\pip.ini
    """
    from helpers import IS_WINDOWS

    if IS_WINDOWS:
        # On Windows, pip uses %APPDATA%\pip\pip.ini
        appdata = fake_home / "AppData" / "Roaming"
        pip_config_dir = appdata / "pip"
        pip_config_dir.mkdir(parents=True, exist_ok=True)
        return {
            **env_isolated,
            "APPDATA": str(appdata),
        }
    else:
        # On Linux/macOS, pip uses XDG_CONFIG_HOME/pip/pip.conf
        # We must set XDG_CONFIG_HOME for pip to use the isolated config
        xdg_config = fake_home / ".config"
        pip_config_dir = xdg_config / "pip"
        pip_config_dir.mkdir(parents=True, exist_ok=True)
        return {
            **env_isolated,
            "XDG_CONFIG_HOME": str(xdg_config),
        }


@pytest.fixture
def uv_isolated_env(fake_home: Path, env_isolated: dict[str, str]) -> dict[str, str]:
    """Provide an environment with isolated uv configuration.

    uv config locations (via dirs::config_dir):
    - macOS: ~/Library/Application Support/uv/uv.toml
    - Linux: ~/.config/uv/uv.toml
    - Windows: %APPDATA%\\uv\\uv.toml
    """
    from helpers import IS_MACOS
    from helpers import IS_WINDOWS

    if IS_WINDOWS:
        appdata = fake_home / "AppData" / "Roaming"
        uv_config_dir = appdata / "uv"
        uv_config_dir.mkdir(parents=True, exist_ok=True)
        return {
            **env_isolated,
            "APPDATA": str(appdata),
            "UV_KEYRING_PROVIDER": "disabled",
        }
    elif IS_MACOS:
        uv_config_dir = fake_home / "Library" / "Application Support" / "uv"
    else:
        uv_config_dir = fake_home / ".config" / "uv"
    uv_config_dir.mkdir(parents=True, exist_ok=True)

    return {
        **env_isolated,
        "UV_KEYRING_PROVIDER": "disabled",
    }


@pytest.fixture
def pip_feature_env(
    pip_isolated_env: dict[str, str],
    keyring_path: Path,
) -> dict[str, str]:
    """Environment for pip feature tests: isolated pip + real auth."""
    return {
        **pip_isolated_env,
        "ANA_KEYRING_PATH": str(keyring_path),
        "ANA_OPEN_BROWSER": "false",
    }


@pytest.fixture
def uv_feature_env(
    uv_isolated_env: dict[str, str],
    keyring_path: Path,
) -> dict[str, str]:
    """Environment for uv feature tests: isolated uv + real auth."""
    return {
        **uv_isolated_env,
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


@pytest.fixture
def run_ana_pip_feature(
    ana_binary: Path | None,
    pip_feature_env: dict[str, str],
) -> AnaRunner:
    """Provide a function to run ana with isolated pip + real auth."""
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
        merged_env = {**pip_feature_env, **(env or {})}
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
def run_ana_uv_feature(
    ana_binary: Path | None,
    uv_feature_env: dict[str, str],
) -> AnaRunner:
    """Provide a function to run ana with isolated uv + real auth."""
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
        merged_env = {**uv_feature_env, **(env or {})}
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


def get_pip_index_url(env: dict[str, str]) -> str | None:
    """Get the configured global index-url from pip config file."""
    from helpers import IS_WINDOWS

    if IS_WINDOWS:
        appdata = env.get("APPDATA")
        if not appdata:
            return None
        config_path = Path(appdata) / "pip" / "pip.ini"
    else:
        # On Linux/macOS, pip uses XDG_CONFIG_HOME/pip/pip.conf
        xdg_config = env.get("XDG_CONFIG_HOME")
        if xdg_config:
            config_path = Path(xdg_config) / "pip" / "pip.conf"
        else:
            home = env.get("HOME")
            if not home:
                return None
            config_path = Path(home) / ".config" / "pip" / "pip.conf"

    if not config_path.exists():
        return None

    import configparser

    config = configparser.ConfigParser()
    config.read(config_path)

    try:
        return config.get("global", "index-url")
    except (configparser.NoSectionError, configparser.NoOptionError):
        return None


def get_uv_default_index(env: dict[str, str]) -> str | None:
    """Get the configured default index from uv config."""
    from helpers import IS_MACOS
    from helpers import IS_WINDOWS

    if IS_WINDOWS:
        appdata = env.get("APPDATA")
        if not appdata:
            return None
        config_path = Path(appdata) / "uv" / "uv.toml"
    elif IS_MACOS:
        home = env.get("HOME")
        if not home:
            return None
        config_path = Path(home) / "Library" / "Application Support" / "uv" / "uv.toml"
    else:
        home = env.get("HOME")
        if not home:
            return None
        config_path = Path(home) / ".config" / "uv" / "uv.toml"

    if not config_path.exists():
        return None

    import tomllib

    with open(config_path, "rb") as f:
        config = tomllib.load(f)

    # Look for anaconda-wheels index
    for index in config.get("index", []):
        if index.get("name") == "anaconda-wheels":
            return index.get("url")
    return None


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
        # Pre-configure both main and main-x channels via pixi config
        # (both are required for "already enabled" since the feature adds both)
        subprocess.run(
            [
                "pixi",
                "config",
                "prepend",
                "--global",
                "default-channels",
                MAIN_X_CHANNEL,
            ],
            env=pixi_feature_env,
            check=True,
        )
        subprocess.run(
            ["pixi", "config", "prepend", "--global", "default-channels", MAIN_CHANNEL],
            env=pixi_feature_env,
            check=True,
        )

        # Verify both channels are configured
        initial_channels = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL in initial_channels
        assert MAIN_CHANNEL in initial_channels

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
            [
                "pixi",
                "config",
                "prepend",
                "--global",
                "default-channels",
                MAIN_X_CHANNEL,
            ],
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
        result = run_ana_pixi_feature(
            "feature", "enable", "main-x", "--pixi", input="n\n"
        )

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
            [
                "pixi",
                "config",
                "prepend",
                "--global",
                "default-channels",
                MAIN_X_CHANNEL,
            ],
            env=pixi_feature_env,
            check=True,
        )

        # Run disable without -f, answer 'n' to abort
        result = run_ana_pixi_feature(
            "feature", "disable", "main-x", "--pixi", input="n\n"
        )

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
        enable_result = run_ana_pixi_feature(
            "feature", "enable", "main-x", "--pixi", "-f"
        )
        assert enable_result.returncode == 0

        # Step 3: Verify channel was added
        channels_after_enable = get_pixi_channels(pixi_feature_env)
        assert MAIN_X_CHANNEL in channels_after_enable

        # Step 4: Disable main-x
        disable_result = run_ana_pixi_feature(
            "feature", "disable", "main-x", "--pixi", "-f"
        )
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
        enable_result = run_ana_pixi_feature(
            "feature", "enable", "main-x", "--pixi", "-f"
        )
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


# =============================================================================
# Wheels Feature Tests (pip)
# =============================================================================

WHEELS_INDEX_URL = "https://repo.anaconda.cloud/repo/anaconda-wheels/simple"


@requires_pip
@requires_api_key
class TestWheelsPipEnable:
    """Tests for 'ana feature enable wheels --pip'."""

    @pytest.fixture(autouse=True)
    def _skip_if_no_wheels(self, wheels_feature_available: bool) -> None:
        if not wheels_feature_available:
            pytest.skip(
                "wheels feature requires binary compiled with 'unstable' feature"
            )

    def test_enable_wheels_pip_configures_index(
        self,
        run_ana_pip_feature: AnaRunner,
        pip_feature_env: dict[str, str],
    ) -> None:
        """Enabling wheels with --pip should configure pip's global index-url."""
        # Verify no index-url is configured initially
        initial_index = get_pip_index_url(pip_feature_env)
        assert initial_index is None or WHEELS_INDEX_URL not in initial_index

        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_pip_feature("login", api_key, "-f")
        assert login_result.returncode == 0, f"Login failed: {login_result.stderr}"

        # Enable wheels with --pip and force flag
        result = run_ana_pip_feature("feature", "enable", "wheels", "--pip", "-f")
        assert result.returncode == 0, f"Enable failed: {result.stderr}"

        # Verify pip index-url was configured (contains the base URL with auth)
        final_index = get_pip_index_url(pip_feature_env)
        assert final_index is not None, "pip index-url should be configured"
        assert "repo.anaconda.cloud" in final_index

    def test_enable_wheels_pip_requires_login(
        self,
        ana_binary: Path | None,
        pip_isolated_env: dict[str, str],
        tmp_path: Path,
    ) -> None:
        """Enabling wheels --pip without login should fail."""
        if ana_binary is None:
            pytest.skip("ana binary not found")

        # Run without auth
        empty_keyring = tmp_path / "empty_keyring"
        empty_keyring.write_text("{}")

        result = subprocess.run(
            [str(ana_binary), "feature", "enable", "wheels", "--pip", "-f"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env={
                **pip_isolated_env,
                "ANA_KEYRING_PATH": str(empty_keyring),
                "ANA_OPEN_BROWSER": "false",
                "ANA_DOMAIN": "invalid.test",
            },
            timeout=10,
        )

        assert result.returncode != 0
        assert (
            "login" in result.stderr.lower() or "not logged in" in result.stderr.lower()
        )


@requires_pip
class TestWheelsPipDisable:
    """Tests for 'ana feature disable wheels --pip'."""

    @pytest.fixture(autouse=True)
    def _skip_if_no_wheels(self, wheels_feature_available: bool) -> None:
        if not wheels_feature_available:
            pytest.skip(
                "wheels feature requires binary compiled with 'unstable' feature"
            )

    def test_disable_wheels_pip_removes_config(
        self,
        run_ana_pip_feature: AnaRunner,
        pip_feature_env: dict[str, str],
    ) -> None:
        """Disabling wheels --pip should remove pip's global index-url config."""
        # Pre-configure pip with an index URL
        for cmd in ["pip", "pip3"]:
            try:
                subprocess.run(
                    [
                        cmd,
                        "config",
                        "set",
                        "global.index-url",
                        "https://example.com/simple/",
                    ],
                    env=pip_feature_env,
                    check=True,
                    capture_output=True,
                )
                break
            except (subprocess.CalledProcessError, FileNotFoundError):
                continue

        # Disable wheels with --pip
        result = run_ana_pip_feature("feature", "disable", "wheels", "--pip", "-f")
        assert result.returncode == 0, f"Disable failed: {result.stderr}"

        # Verify pip index-url was removed
        final_index = get_pip_index_url(pip_feature_env)
        assert final_index is None or final_index == ""


@requires_pip
@requires_api_key
class TestWheelsPipEndToEnd:
    """End-to-end tests for the wheels pip feature workflow."""

    @pytest.fixture(autouse=True)
    def _skip_if_no_wheels(self, wheels_feature_available: bool) -> None:
        if not wheels_feature_available:
            pytest.skip(
                "wheels feature requires binary compiled with 'unstable' feature"
            )

    def test_enable_then_disable_pip(
        self,
        run_ana_pip_feature: AnaRunner,
        pip_feature_env: dict[str, str],
    ) -> None:
        """Full workflow: login -> enable --pip -> verify -> disable --pip -> verify."""
        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_pip_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        # Enable wheels
        enable_result = run_ana_pip_feature(
            "feature", "enable", "wheels", "--pip", "-f"
        )
        assert enable_result.returncode == 0

        # Verify config was added
        index_after_enable = get_pip_index_url(pip_feature_env)
        assert index_after_enable is not None
        assert "repo.anaconda.cloud" in index_after_enable

        # Disable wheels
        disable_result = run_ana_pip_feature(
            "feature", "disable", "wheels", "--pip", "-f"
        )
        assert disable_result.returncode == 0

        # Verify config was removed
        index_after_disable = get_pip_index_url(pip_feature_env)
        assert index_after_disable is None or index_after_disable == ""

    def test_can_install_abn_from_wheels(
        self,
        run_ana_pip_feature: AnaRunner,
        pip_feature_env: dict[str, str],
    ) -> None:
        """Verify that after enabling wheels, pip can install abn package."""
        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_pip_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        # Enable wheels
        enable_result = run_ana_pip_feature(
            "feature", "enable", "wheels", "--pip", "-f"
        )
        assert enable_result.returncode == 0

        # Try to install abn with --dry-run to verify access without side effects
        install_result: subprocess.CompletedProcess[str] | None = None
        for cmd in ["pip", "pip3"]:
            try:
                install_result = subprocess.run(
                    [cmd, "install", "--dry-run", "abn"],
                    capture_output=True,
                    text=True,
                    encoding="utf-8",
                    env=pip_feature_env,
                )
                break
            except FileNotFoundError:
                continue

        if install_result is None:
            pytest.fail("pip not found")
            return  # unreachable, but helps type checker

        # Should succeed (not 401/403)
        assert install_result.returncode == 0, (
            f"Install failed: {install_result.stderr}"
        )
        assert (
            "abn" in install_result.stdout.lower()
            or "abn" in install_result.stderr.lower()
        )

        # Cleanup
        run_ana_pip_feature("feature", "disable", "wheels", "--pip", "-f")

    def test_cannot_install_abn_without_auth(
        self,
        pip_isolated_env: dict[str, str],
    ) -> None:
        """Verify that installing abn without authentication fails."""
        result: subprocess.CompletedProcess[str] | None = None
        for cmd in ["pip", "pip3"]:
            try:
                result = subprocess.run(
                    [
                        cmd,
                        "install",
                        "--dry-run",
                        "abn",
                        "--index-url",
                        WHEELS_INDEX_URL,
                    ],
                    capture_output=True,
                    text=True,
                    encoding="utf-8",
                    env=pip_isolated_env,
                )
                break
            except FileNotFoundError:
                continue

        if result is None:
            pytest.skip("pip not found")
            return  # unreachable, but helps type checker

        # Should fail - either auth error or empty package list (pip returns empty when auth fails)
        assert result.returncode != 0, "Install should fail without auth"
        error_indicators = [
            "401",
            "403",
            "Unauthorized",
            "Forbidden",
            "not found",
            "Could not find a version",  # pip returns this when index returns empty due to auth failure
            "No matching distribution",
        ]
        assert any(indicator in result.stderr for indicator in error_indicators), (
            f"Unexpected error: {result.stderr}"
        )


# =============================================================================
# Wheels Feature Tests (uv)
# =============================================================================


@requires_uv
@requires_api_key
class TestWheelsUvEnable:
    """Tests for 'ana feature enable wheels --uv'."""

    @pytest.fixture(autouse=True)
    def _skip_if_no_wheels(self, wheels_feature_available: bool) -> None:
        if not wheels_feature_available:
            pytest.skip(
                "wheels feature requires binary compiled with 'unstable' feature"
            )

    def test_enable_wheels_uv_configures_index(
        self,
        run_ana_uv_feature: AnaRunner,
        uv_feature_env: dict[str, str],
    ) -> None:
        """Enabling wheels with --uv should configure uv's default index."""
        # Verify no anaconda-wheels index initially
        initial_index = get_uv_default_index(uv_feature_env)
        assert initial_index is None

        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_uv_feature("login", api_key, "-f")
        assert login_result.returncode == 0, f"Login failed: {login_result.stderr}"

        # Enable wheels with --uv and force flag
        result = run_ana_uv_feature("feature", "enable", "wheels", "--uv", "-f")
        assert result.returncode == 0, f"Enable failed: {result.stderr}"

        # Verify uv config was created with anaconda-wheels index
        final_index = get_uv_default_index(uv_feature_env)
        assert final_index is not None, "uv index should be configured"
        assert "repo.anaconda.cloud" in final_index

    def test_enable_wheels_uv_requires_login(
        self,
        ana_binary: Path | None,
        uv_isolated_env: dict[str, str],
        tmp_path: Path,
    ) -> None:
        """Enabling wheels --uv without login should fail."""
        if ana_binary is None:
            pytest.skip("ana binary not found")

        # Run without auth
        empty_keyring = tmp_path / "empty_keyring"
        empty_keyring.write_text("{}")

        result = subprocess.run(
            [str(ana_binary), "feature", "enable", "wheels", "--uv", "-f"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env={
                **uv_isolated_env,
                "ANA_KEYRING_PATH": str(empty_keyring),
                "ANA_OPEN_BROWSER": "false",
                "ANA_DOMAIN": "invalid.test",
            },
            timeout=10,
        )

        assert result.returncode != 0
        assert (
            "login" in result.stderr.lower() or "not logged in" in result.stderr.lower()
        )


@requires_uv
@requires_api_key
class TestWheelsUvDisable:
    """Tests for 'ana feature disable wheels --uv'."""

    @pytest.fixture(autouse=True)
    def _skip_if_no_wheels(self, wheels_feature_available: bool) -> None:
        if not wheels_feature_available:
            pytest.skip(
                "wheels feature requires binary compiled with 'unstable' feature"
            )

    def test_disable_wheels_uv_removes_config(
        self,
        run_ana_uv_feature: AnaRunner,
        uv_feature_env: dict[str, str],
    ) -> None:
        """Disabling wheels --uv should remove the anaconda-wheels index from uv config."""
        # First enable wheels properly (this sets up both config and auth)
        api_key = get_test_api_key()
        login_result = run_ana_uv_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        enable_result = run_ana_uv_feature("feature", "enable", "wheels", "--uv", "-f")
        assert enable_result.returncode == 0

        # Verify it's configured
        initial_index = get_uv_default_index(uv_feature_env)
        assert initial_index is not None

        # Disable wheels with --uv
        result = run_ana_uv_feature("feature", "disable", "wheels", "--uv", "-f")
        assert result.returncode == 0, f"Disable failed: {result.stderr}"

        # Verify uv config was removed
        final_index = get_uv_default_index(uv_feature_env)
        assert final_index is None


@requires_uv
@requires_api_key
class TestWheelsUvEndToEnd:
    """End-to-end tests for the wheels uv feature workflow."""

    @pytest.fixture(autouse=True)
    def _skip_if_no_wheels(self, wheels_feature_available: bool) -> None:
        if not wheels_feature_available:
            pytest.skip(
                "wheels feature requires binary compiled with 'unstable' feature"
            )

    def test_enable_then_disable_uv(
        self,
        run_ana_uv_feature: AnaRunner,
        uv_feature_env: dict[str, str],
    ) -> None:
        """Full workflow: login -> enable --uv -> verify -> disable --uv -> verify."""
        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_uv_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        # Enable wheels
        enable_result = run_ana_uv_feature("feature", "enable", "wheels", "--uv", "-f")
        assert enable_result.returncode == 0

        # Verify config was added
        index_after_enable = get_uv_default_index(uv_feature_env)
        assert index_after_enable is not None
        assert "repo.anaconda.cloud" in index_after_enable

        # Disable wheels
        disable_result = run_ana_uv_feature(
            "feature", "disable", "wheels", "--uv", "-f"
        )
        assert disable_result.returncode == 0

        # Verify config was removed
        index_after_disable = get_uv_default_index(uv_feature_env)
        assert index_after_disable is None

    def test_can_install_abn_from_wheels(
        self,
        run_ana_uv_feature: AnaRunner,
        uv_feature_env: dict[str, str],
    ) -> None:
        """Verify that after enabling wheels, uv can install abn package."""
        # Login with API key
        api_key = get_test_api_key()
        login_result = run_ana_uv_feature("login", api_key, "-f")
        assert login_result.returncode == 0

        # Enable wheels
        enable_result = run_ana_uv_feature("feature", "enable", "wheels", "--uv", "-f")
        assert enable_result.returncode == 0

        # Try to install abn with --dry-run to verify access without side effects
        install_result = subprocess.run(
            ["uv", "pip", "install", "--dry-run", "abn"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=uv_feature_env,
        )

        # Should succeed (not 401/403)
        assert install_result.returncode == 0, (
            f"Install failed: {install_result.stderr}"
        )
        # uv shows package info in stdout or stderr during dry-run
        combined_output = install_result.stdout + install_result.stderr
        assert "abn" in combined_output.lower(), f"abn not in output: {combined_output}"

        # Cleanup
        run_ana_uv_feature("feature", "disable", "wheels", "--uv", "-f")

    def test_cannot_install_abn_without_auth(
        self,
        uv_isolated_env: dict[str, str],
    ) -> None:
        """Verify that installing abn without authentication fails."""
        result = subprocess.run(
            [
                "uv",
                "pip",
                "install",
                "--dry-run",
                "abn",
                "--index-url",
                WHEELS_INDEX_URL,
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=uv_isolated_env,
        )

        # Should fail due to authentication (401/403) or package not found after auth fails
        assert result.returncode != 0, "Install should fail without auth"
        # Accept either auth errors or "not found" (which happens when index can't be accessed)
        error_indicators = [
            "401",
            "403",
            "Unauthorized",
            "Forbidden",
            "not found",
            "No solution",
        ]
        assert any(indicator in result.stderr for indicator in error_indicators), (
            f"Unexpected error: {result.stderr}"
        )
