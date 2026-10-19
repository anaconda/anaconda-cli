"""Live integration tests for `ana package info`.

These run the real binary against anaconda.com. Subprocess stdout is
captured, which is how an agent's shell runs `ana`: output must be JSON with
no flag, errors must be JSON on stdout with exit code 1, and argument errors
come from clap with exit code 2.

Kept small: each test makes at most one network call and the login happens
once per module. Argument parsing and response assembly are covered by the
Rust unit tests.

Most tests need a real API key: set `ANA_TEST_API_KEY` (CI injects it).
Without it they are skipped.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
from pathlib import Path

import pytest
from helpers import IS_WINDOWS
from helpers import LIVE_TIMEOUT_SECONDS
from helpers import run_live

requires_api_key = pytest.mark.skipif(
    not os.environ.get("ANA_TEST_API_KEY"),
    reason="ANA_TEST_API_KEY not set - required for live package info tests",
)


@pytest.fixture(scope="module")
def live_logged_in_env(
    live_binary: Path,
    live_env: dict[str, str],
    tmp_path_factory: pytest.TempPathFactory,
) -> dict[str, str]:
    """Production services, logged in once with ANA_TEST_API_KEY."""
    api_key = os.environ.get("ANA_TEST_API_KEY")
    if not api_key:
        pytest.skip("ANA_TEST_API_KEY not set")
    env = {
        **live_env,
        "ANA_KEYRING_PATH": str(tmp_path_factory.mktemp("live_auth") / "keyring"),
    }
    result = run_live(live_binary, env, "login", "--api-key", input=f"{api_key}\n")
    assert result.returncode == 0, f"login failed: {result.stderr}"
    return env


def test_help(live_binary: Path, live_env: dict[str, str]) -> None:
    """Help works without the required package name."""
    result = run_live(live_binary, live_env, "package", "info", "--help")
    assert result.returncode == 0
    assert "--python-version" in result.stdout + result.stderr


@requires_api_key
class TestPackageInfo:
    def test_latest(
        self, live_binary: Path, live_logged_in_env: dict[str, str]
    ) -> None:
        result = run_live(live_binary, live_logged_in_env, "package", "info", "numpy")
        assert result.returncode == 0, result.stderr
        body = json.loads(result.stdout)
        assert body["name"] == "numpy"
        assert body["channel"] == "main"
        assert body["version"]
        assert "linux-64" in body["platforms"]
        assert body["python_versions"]
        assert body["license"]
        assert set(body["cve_summary"]) == {"count", "critical"}
        assert body["policy_status"] is None
        assert isinstance(body["notes"], list)

    def test_all_arguments(
        self, live_binary: Path, live_logged_in_env: dict[str, str]
    ) -> None:
        """Every option in one call; each one shows up in the result."""
        result = run_live(
            live_binary,
            live_logged_in_env,
            "package",
            "info",
            "numpy",
            "--version",
            "1.26.4",
            "--channel",
            "main",
            "--platform",
            "bogus-99",
            "--python-version",
            "2.7",
            "--json",
        )
        assert result.returncode == 0, result.stderr
        body = json.loads(result.stdout)
        assert body["version"] == "1.26.4"
        assert body["channel"] == "main"
        notes = body["notes"]
        assert any(n.startswith("A newer version (") for n in notes)
        assert any(n.startswith("Not available on bogus-99.") for n in notes)
        assert any(n.startswith("Not available for Python 2.7.") for n in notes)

    def test_unknown_package(
        self, live_binary: Path, live_logged_in_env: dict[str, str]
    ) -> None:
        name = "definitely-not-a-real-package-xyz"
        result = run_live(live_binary, live_logged_in_env, "package", "info", name)
        assert result.returncode == 1
        body = json.loads(result.stdout)
        assert body["error"] == "package_not_found"
        assert body["name"] == name
        # Exactly one machine-readable error: no second report on stderr.
        assert name not in result.stderr

    @pytest.mark.skipif(IS_WINDOWS, reason="pseudo-terminals are Unix-only")
    def test_text_output_in_terminal(
        self, live_binary: Path, live_logged_in_env: dict[str, str]
    ) -> None:
        import pty

        master, slave = pty.openpty()
        proc = subprocess.Popen(
            [str(live_binary), "package", "info", "numpy"],
            stdin=subprocess.DEVNULL,
            stdout=slave,
            stderr=subprocess.PIPE,
            env=live_logged_in_env,
        )
        os.close(slave)
        chunks = []
        while True:
            try:
                data = os.read(master, 4096)
            except OSError:  # EIO once the child closes the terminal
                break
            if not data:
                break
            chunks.append(data)
        proc.wait(timeout=LIVE_TIMEOUT_SECONDS)
        os.close(master)
        text = re.sub(r"\x1b\[[0-9;]*m", "", b"".join(chunks).decode())
        assert proc.returncode == 0, text
        assert text.startswith("numpy "), text
        assert "Platforms:" in text


def test_logged_out(live_binary: Path, live_env: dict[str, str]) -> None:
    """No key: fails fast with JSON, never prompts or starts a login flow."""
    result = run_live(live_binary, live_env, "package", "info", "numpy")
    assert result.returncode == 1
    assert json.loads(result.stdout)["error"] == "unauthenticated"
    assert "Login now?" not in result.stdout + result.stderr


def test_invalid_argument(live_binary: Path, live_env: dict[str, str]) -> None:
    """Rejected by clap before any request (exit 2, nothing on stdout)."""
    result = run_live(live_binary, live_env, "package", "info", "../etc")
    assert result.returncode == 2
    assert result.stdout == ""
    assert "invalid value" in result.stderr.lower()
