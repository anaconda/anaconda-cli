"""Live integration tests for `ana intel`.

These run the real binary against the real Anaconda services (anaconda.com).
Subprocess stdout is captured, which is how an agent's shell runs `ana`:
output must be JSON with no flag, errors must be JSON on stdout with exit
code 1, and argument errors come from clap with exit code 2.

The suite is kept small: each test makes at most one network call, and the
login happens once per module. Argument parsing and response assembly are
covered exhaustively by the Rust unit tests; this file checks that the
pieces work end to end against the live services.

- `search-forum` needs no credentials, so its tests always run.
- `package-info` needs a real API key: set `ANA_TEST_API_KEY` (CI injects
  it). Without it those tests are skipped.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
from pathlib import Path

import pytest
from conftest import PRESERVE_VARS
from helpers import IS_WINDOWS

TIMEOUT_SECONDS = 60

requires_api_key = pytest.mark.skipif(
    not os.environ.get("ANA_TEST_API_KEY"),
    reason="ANA_TEST_API_KEY not set - required for live package-info tests",
)


def _run(
    binary: Path, env: dict[str, str], *args: str
) -> subprocess.CompletedProcess[str]:
    """Run ana non-interactively: stdout captured, no stdin, bounded time."""
    return subprocess.run(
        [str(binary), *args],
        capture_output=True,
        stdin=subprocess.DEVNULL,
        text=True,
        encoding="utf-8",
        env=env,
        timeout=TIMEOUT_SECONDS,
    )


def _json(result: subprocess.CompletedProcess[str]) -> dict:
    """Parse stdout as one JSON document (fails on any stray text)."""
    return json.loads(result.stdout)


@pytest.fixture(scope="module")
def binary(ana_binary: Path | None) -> Path:
    if ana_binary is None:
        pytest.skip("ana binary not found. Build with 'pixi run build-release'")
    return ana_binary


@pytest.fixture(scope="module")
def live_env(tmp_path_factory: pytest.TempPathFactory) -> dict[str, str]:
    """Production services, logged out, with an isolated home and keyring."""
    home = tmp_path_factory.mktemp("intel_home")
    env = {
        key: val
        for key, val in os.environ.items()
        if (not key.startswith("ANA_") and key != "GITHUB_TOKEN")
        or key in PRESERVE_VARS
    }
    env["USERPROFILE" if IS_WINDOWS else "HOME"] = str(home)
    env.update(
        {
            "ANA_KEYRING_PATH": str(home / "keyring"),
            "ANA_OPEN_BROWSER": "false",
            "ANA_ENABLE_TELEMETRY": "false",
            "NO_COLOR": "1",
        }
    )
    return env


@pytest.fixture(scope="module")
def logged_in_env(
    binary: Path, tmp_path_factory: pytest.TempPathFactory, live_env: dict[str, str]
) -> dict[str, str]:
    """Production services, logged in once with ANA_TEST_API_KEY."""
    api_key = os.environ.get("ANA_TEST_API_KEY")
    if not api_key:
        pytest.skip("ANA_TEST_API_KEY not set")
    env = {
        **live_env,
        "ANA_KEYRING_PATH": str(tmp_path_factory.mktemp("intel_auth") / "keyring"),
    }
    result = subprocess.run(
        [str(binary), "login", "--api-key"],
        input=f"{api_key}\n",
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
        timeout=TIMEOUT_SECONDS,
    )
    assert result.returncode == 0, f"login failed: {result.stderr}"
    return env


def test_help(binary: Path, live_env: dict[str, str]) -> None:
    result = _run(binary, live_env, "intel", "--help")
    assert result.returncode == 0
    assert "package-info" in result.stdout + result.stderr
    assert "search-forum" in result.stdout + result.stderr
    # Subcommand help works without the required arguments.
    result = _run(binary, live_env, "intel", "package-info", "--help")
    assert result.returncode == 0
    assert "--python-version" in result.stdout + result.stderr


# =============================================================================
# package-info
# =============================================================================


@requires_api_key
class TestPackageInfo:
    def test_latest(self, binary: Path, logged_in_env: dict[str, str]) -> None:
        result = _run(binary, logged_in_env, "intel", "package-info", "numpy")
        assert result.returncode == 0, result.stderr
        body = _json(result)
        assert body["name"] == "numpy"
        assert body["channel"] == "main"
        assert body["version"]
        assert "linux-64" in body["platforms"]
        assert body["python_versions"]
        assert body["license"]
        assert set(body["cve_summary"]) == {"count", "critical"}
        assert body["policy_status"] is None
        assert isinstance(body["notes"], list)

    def test_all_arguments(self, binary: Path, logged_in_env: dict[str, str]) -> None:
        """Every option in one call; each one shows up in the result."""
        result = _run(
            binary,
            logged_in_env,
            "intel",
            "package-info",
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
        body = _json(result)
        assert body["version"] == "1.26.4"
        assert body["channel"] == "main"
        notes = body["notes"]
        assert any(n.startswith("A newer version (") for n in notes)
        assert any(n.startswith("Not available on bogus-99.") for n in notes)
        assert any(n.startswith("Not available for Python 2.7.") for n in notes)

    def test_unknown_package(self, binary: Path, logged_in_env: dict[str, str]) -> None:
        name = "definitely-not-a-real-package-xyz"
        result = _run(binary, logged_in_env, "intel", "package-info", name)
        assert result.returncode == 1
        body = _json(result)
        assert body["error"] == "package_not_found"
        assert body["name"] == name
        # Exactly one machine-readable error: no second report on stderr.
        assert name not in result.stderr

    @pytest.mark.skipif(IS_WINDOWS, reason="pseudo-terminals are Unix-only")
    def test_text_output_in_terminal(
        self, binary: Path, logged_in_env: dict[str, str]
    ) -> None:
        import pty

        master, slave = pty.openpty()
        proc = subprocess.Popen(
            [str(binary), "intel", "package-info", "numpy"],
            stdin=subprocess.DEVNULL,
            stdout=slave,
            stderr=subprocess.PIPE,
            env=logged_in_env,
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
        proc.wait(timeout=TIMEOUT_SECONDS)
        os.close(master)
        text = re.sub(r"\x1b\[[0-9;]*m", "", b"".join(chunks).decode())
        assert proc.returncode == 0, text
        assert text.startswith("numpy "), text
        assert "Platforms:" in text


def test_package_info_logged_out(binary: Path, live_env: dict[str, str]) -> None:
    """No key: fails fast with JSON, never prompts or starts a login flow."""
    result = _run(binary, live_env, "intel", "package-info", "numpy")
    assert result.returncode == 1
    body = _json(result)
    assert body["error"] == "unauthenticated"
    assert "Login now?" not in result.stdout + result.stderr


def test_package_info_invalid_argument(binary: Path, live_env: dict[str, str]) -> None:
    """Rejected by clap before any request (exit 2, nothing on stdout)."""
    result = _run(binary, live_env, "intel", "package-info", "../etc")
    assert result.returncode == 2
    assert result.stdout == ""
    assert "invalid value" in result.stderr.lower()


# =============================================================================
# search-forum
# =============================================================================


def test_search_forum(binary: Path, live_env: dict[str, str]) -> None:
    """Multi-word query with paging options, logged out."""
    result = _run(
        binary,
        live_env,
        "intel",
        "search-forum",
        "conda",
        "environment",
        "--page",
        "2",
        "--page-size",
        "3",
    )
    assert result.returncode == 0, result.stderr
    body = _json(result)
    assert set(body) == {"topics", "total", "pagination", "notes"}
    assert body["total"] > 3
    assert body["pagination"]["page"] == 2
    assert body["pagination"]["page_size"] == 3
    assert 0 < len(body["topics"]) <= 3
    for topic in body["topics"]:
        assert isinstance(topic["id"], int)
        assert topic["title"]
        assert topic["url"] is None or topic["url"].startswith(
            "https://forum.anaconda.com/"
        )


def test_search_forum_result_window(binary: Path, live_env: dict[str, str]) -> None:
    """Rejected before the request: JSON error, exit 1."""
    result = _run(
        binary,
        live_env,
        "intel",
        "search-forum",
        "conda",
        "--page",
        "101",
        "--page-size",
        "100",
    )
    assert result.returncode == 1
    assert _json(result)["error"] == "invalid_request"


def test_search_forum_invalid_argument(binary: Path, live_env: dict[str, str]) -> None:
    """Rejected by clap before any request (exit 2, nothing on stdout)."""
    result = _run(
        binary, live_env, "intel", "search-forum", "conda", "--page-size", "101"
    )
    assert result.returncode == 2
    assert result.stdout == ""
