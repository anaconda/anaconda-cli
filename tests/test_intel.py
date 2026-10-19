"""Live integration tests for `ana intel`.

These run the real binary against the real Anaconda services (anaconda.com),
covering both commands and every argument. Subprocess stdout is captured,
which is how an agent's shell runs `ana`: output must be JSON with no flag,
errors must be JSON on stdout with exit code 1, and argument errors come from
clap with exit code 2.

- `search-forum` needs no credentials, so its tests always run.
- `package-info` needs a real API key: set `ANA_TEST_API_KEY` (CI injects
  it). Without it those tests are skipped.

Assertions check structure and behaviour, not exact data, so they hold as
packages and forum threads change.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
from pathlib import Path

import pytest
from helpers import IS_WINDOWS

TIMEOUT_SECONDS = 60
FORUM_ORIGIN = "https://forum.anaconda.com/"

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


def _run_in_pty(binary: Path, env: dict[str, str], *args: str) -> tuple[int, str]:
    """Run ana with stdout attached to a pseudo-terminal (a person's view)."""
    import pty

    master, slave = pty.openpty()
    proc = subprocess.Popen(
        [str(binary), *args],
        stdin=subprocess.DEVNULL,
        stdout=slave,
        stderr=subprocess.PIPE,
        env=env,
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
    text = b"".join(chunks).decode("utf-8", errors="replace")
    return proc.returncode, re.sub(r"\x1b\[[0-9;]*m", "", text)


@pytest.fixture
def binary(ana_binary: Path | None) -> Path:
    if ana_binary is None:
        pytest.skip("ana binary not found. Build with 'pixi run build-release'")
    return ana_binary


@pytest.fixture
def live_env(env_isolated: dict[str, str], tmp_path: Path) -> dict[str, str]:
    """Production services, logged out, with an isolated keyring."""
    return {
        **env_isolated,
        "ANA_KEYRING_PATH": str(tmp_path / "keyring"),
        "ANA_OPEN_BROWSER": "false",
        "ANA_ENABLE_TELEMETRY": "false",
        "NO_COLOR": "1",
    }


@pytest.fixture
def logged_in_env(binary: Path, live_env: dict[str, str]) -> dict[str, str]:
    """Production services, logged in with ANA_TEST_API_KEY."""
    api_key = os.environ.get("ANA_TEST_API_KEY")
    if not api_key:
        pytest.skip("ANA_TEST_API_KEY not set")
    result = subprocess.run(
        [str(binary), "login", "--api-key"],
        input=f"{api_key}\n",
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=live_env,
        timeout=TIMEOUT_SECONDS,
    )
    assert result.returncode == 0, f"login failed: {result.stderr}"
    return live_env


def _assert_package_info_shape(body: dict) -> None:
    """Every field the contract promises, with the right types."""
    assert isinstance(body["name"], str)
    assert body["version"] is None or isinstance(body["version"], str)
    assert isinstance(body["channel"], str)
    assert isinstance(body["platforms"], list)
    for key in ("python_versions", "notable_constraints"):
        assert body[key] is None or all(isinstance(v, str) for v in body[key])
    for key in ("license", "summary", "doc_url", "policy_status", "policy_detail"):
        assert body[key] is None or isinstance(body[key], str)
    assert body["build_variants"] is None or all(
        set(v) == {"label", "selector"} for v in body["build_variants"]
    )
    assert set(body["download_count"]) == {"total", "channel"}
    assert set(body["cve_summary"]) == {"count", "critical"}
    assert isinstance(body["notes"], list)


# =============================================================================
# Help
# =============================================================================


def test_intel_help_lists_commands(binary: Path, live_env: dict[str, str]) -> None:
    result = _run(binary, live_env, "intel", "--help")
    assert result.returncode == 0
    output = result.stdout + result.stderr
    assert "package-info" in output
    assert "search-forum" in output


@pytest.mark.parametrize("command", ["package-info", "search-forum"])
def test_subcommand_help(binary: Path, live_env: dict[str, str], command: str) -> None:
    result = _run(binary, live_env, "intel", command, "--help")
    assert result.returncode == 0
    assert "--json" in result.stdout + result.stderr


# =============================================================================
# package-info
# =============================================================================


@requires_api_key
class TestPackageInfo:
    def test_defaults(self, binary: Path, logged_in_env: dict[str, str]) -> None:
        result = _run(binary, logged_in_env, "intel", "package-info", "numpy")
        assert result.returncode == 0, result.stderr
        body = _json(result)
        _assert_package_info_shape(body)
        assert body["name"] == "numpy"
        assert body["channel"] == "main"
        assert body["version"]
        assert "linux-64" in body["platforms"]
        assert body["python_versions"]
        assert body["license"]
        assert body["download_count"]["channel"] == "main"
        assert body["policy_status"] is None

    def test_json_flag(self, binary: Path, logged_in_env: dict[str, str]) -> None:
        result = _run(binary, logged_in_env, "intel", "package-info", "numpy", "--json")
        assert result.returncode == 0, result.stderr
        assert _json(result)["name"] == "numpy"

    def test_older_version(self, binary: Path, logged_in_env: dict[str, str]) -> None:
        result = _run(
            binary,
            logged_in_env,
            "intel",
            "package-info",
            "numpy",
            "--version",
            "1.26.4",
        )
        assert result.returncode == 0, result.stderr
        body = _json(result)
        assert body["version"] == "1.26.4"
        assert any(n.startswith("A newer version (") for n in body["notes"])

    def test_missing_version(self, binary: Path, logged_in_env: dict[str, str]) -> None:
        result = _run(
            binary,
            logged_in_env,
            "intel",
            "package-info",
            "numpy",
            "--version",
            "0.0.0.1",
        )
        assert result.returncode == 1
        body = _json(result)
        assert body["error"] == "version_not_found"
        assert body["name"] == "numpy"
        assert "Available versions:" in body["message"]

    def test_explicit_channel(
        self, binary: Path, logged_in_env: dict[str, str]
    ) -> None:
        result = _run(
            binary, logged_in_env, "intel", "package-info", "numpy", "--channel", "main"
        )
        assert result.returncode == 0, result.stderr
        assert _json(result)["channel"] == "main"

    def test_channel_without_package(
        self, binary: Path, logged_in_env: dict[str, str]
    ) -> None:
        """A named channel is never swapped for another; alternatives are hinted."""
        result = _run(
            binary,
            logged_in_env,
            "intel",
            "package-info",
            "numpy",
            "--channel",
            "no-such-channel-xyz",
        )
        assert result.returncode == 1
        body = _json(result)
        assert body["error"] == "package_not_found"
        assert "Available in: main" in body["message"]

    def test_available_platform(
        self, binary: Path, logged_in_env: dict[str, str]
    ) -> None:
        result = _run(
            binary,
            logged_in_env,
            "intel",
            "package-info",
            "numpy",
            "--platform",
            "linux-64",
        )
        assert result.returncode == 0, result.stderr
        assert not any(n.startswith("Not available on") for n in _json(result)["notes"])

    def test_unavailable_platform(
        self, binary: Path, logged_in_env: dict[str, str]
    ) -> None:
        result = _run(
            binary,
            logged_in_env,
            "intel",
            "package-info",
            "numpy",
            "--platform",
            "bogus-99",
        )
        assert result.returncode == 0, result.stderr
        assert any(
            n.startswith("Not available on bogus-99.") for n in _json(result)["notes"]
        )

    def test_supported_python(
        self, binary: Path, logged_in_env: dict[str, str]
    ) -> None:
        baseline = _json(_run(binary, logged_in_env, "intel", "package-info", "numpy"))
        supported = baseline["python_versions"][-1]
        result = _run(
            binary,
            logged_in_env,
            "intel",
            "package-info",
            "numpy",
            "--python-version",
            supported,
        )
        assert result.returncode == 0, result.stderr
        assert not any(
            n.startswith("Not available for Python") for n in _json(result)["notes"]
        )

    def test_unsupported_python(
        self, binary: Path, logged_in_env: dict[str, str]
    ) -> None:
        result = _run(
            binary,
            logged_in_env,
            "intel",
            "package-info",
            "numpy",
            "--python-version",
            "2.7",
        )
        assert result.returncode == 0, result.stderr
        assert any(
            n.startswith("Not available for Python 2.7.")
            for n in _json(result)["notes"]
        )

    def test_all_arguments(self, binary: Path, logged_in_env: dict[str, str]) -> None:
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
            "linux-64",
            "--python-version",
            "3.12",
            "--json",
        )
        assert result.returncode == 0, result.stderr
        body = _json(result)
        _assert_package_info_shape(body)
        assert body["version"] == "1.26.4"
        assert body["channel"] == "main"

    def test_build_variants(self, binary: Path, logged_in_env: dict[str, str]) -> None:
        result = _run(binary, logged_in_env, "intel", "package-info", "pytorch")
        assert result.returncode == 0, result.stderr
        body = _json(result)
        assert body["build_variants"]
        for variant in body["build_variants"]:
            assert variant["selector"].startswith("pytorch=*=")

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
        returncode, text = _run_in_pty(
            binary, logged_in_env, "intel", "package-info", "numpy"
        )
        assert returncode == 0, text
        assert text.startswith("numpy "), text
        assert "Platforms:" in text
        assert not text.lstrip().startswith("{")


def test_package_info_logged_out(binary: Path, live_env: dict[str, str]) -> None:
    """No key: fails fast with JSON, never prompts or starts a login flow."""
    result = _run(binary, live_env, "intel", "package-info", "numpy")
    assert result.returncode == 1
    body = _json(result)
    assert body["error"] == "unauthenticated"
    assert "ana login" in body["message"]
    assert "Login now?" not in result.stdout + result.stderr


@pytest.mark.parametrize(
    "args",
    [
        ["../etc"],
        ["a/b"],
        ["   "],
        ["x" * 257],
        ["numpy", "--channel", "../x"],
        ["numpy", "--version", " "],
        ["numpy", "--platform", ""],
        ["numpy", "--python-version", " "],
    ],
    ids=[
        "traversal",
        "slash",
        "blank-name",
        "long-name",
        "bad-channel",
        "blank-version",
        "empty-platform",
        "blank-python",
    ],
)
def test_package_info_invalid_arguments(
    binary: Path, live_env: dict[str, str], args: list[str]
) -> None:
    """Rejected by clap before any request (exit 2, message on stderr)."""
    result = _run(binary, live_env, "intel", "package-info", *args)
    assert result.returncode == 2
    assert result.stdout == ""
    assert "invalid value" in result.stderr.lower()


# =============================================================================
# search-forum
# =============================================================================


class TestSearchForum:
    def test_basic_search(self, binary: Path, live_env: dict[str, str]) -> None:
        result = _run(binary, live_env, "intel", "search-forum", "conda environment")
        assert result.returncode == 0, result.stderr
        body = _json(result)
        assert set(body) == {"topics", "total", "pagination", "notes"}
        assert body["total"] > 0
        assert body["topics"]
        assert body["pagination"]["page"] == 1
        assert body["pagination"]["page_size"] == 10
        assert len(body["topics"]) <= 10
        for topic in body["topics"]:
            assert isinstance(topic["id"], int)
            assert isinstance(topic["title"], str)
            assert topic["url"] is None or topic["url"].startswith(FORUM_ORIGIN)
            for blurb in topic["blurbs"]:
                assert set(blurb) == {"username", "blurb", "post_number"}

    def test_query_words_are_joined(
        self, binary: Path, live_env: dict[str, str]
    ) -> None:
        quoted = _json(
            _run(
                binary, live_env, "intel", "search-forum", "solving environment failed"
            )
        )
        words = _json(
            _run(
                binary,
                live_env,
                "intel",
                "search-forum",
                "solving",
                "environment",
                "failed",
            )
        )
        assert [t["id"] for t in words["topics"]] == [t["id"] for t in quoted["topics"]]

    def test_paging(self, binary: Path, live_env: dict[str, str]) -> None:
        first = _json(
            _run(binary, live_env, "intel", "search-forum", "conda", "--page-size", "3")
        )
        second = _json(
            _run(
                binary,
                live_env,
                "intel",
                "search-forum",
                "conda",
                "--page",
                "2",
                "--page-size",
                "3",
            )
        )
        assert second["pagination"]["page"] == 2
        assert second["pagination"]["page_size"] == 3
        assert 0 < len(second["topics"]) <= 3
        first_ids = {t["id"] for t in first["topics"]}
        assert not first_ids & {t["id"] for t in second["topics"]}

    def test_page_size_bounds(self, binary: Path, live_env: dict[str, str]) -> None:
        for size in ("1", "100"):
            result = _run(
                binary, live_env, "intel", "search-forum", "conda", "--page-size", size
            )
            assert result.returncode == 0, result.stderr
            assert len(_json(result)["topics"]) <= int(size)

    def test_result_window_limit(self, binary: Path, live_env: dict[str, str]) -> None:
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
        body = _json(result)
        assert body["error"] == "invalid_request"
        assert "10000" in body["message"]

    def test_unlikely_query(self, binary: Path, live_env: dict[str, str]) -> None:
        result = _run(
            binary, live_env, "intel", "search-forum", "zxqvjkwpfm1234567890qwertyuiop"
        )
        assert result.returncode == 0, result.stderr
        assert isinstance(_json(result)["topics"], list)

    def test_json_flag(self, binary: Path, live_env: dict[str, str]) -> None:
        result = _run(binary, live_env, "intel", "search-forum", "conda", "--json")
        assert result.returncode == 0, result.stderr
        assert "topics" in _json(result)

    @pytest.mark.skipif(IS_WINDOWS, reason="pseudo-terminals are Unix-only")
    def test_text_output_in_terminal(
        self, binary: Path, live_env: dict[str, str]
    ) -> None:
        returncode, text = _run_in_pty(
            binary, live_env, "intel", "search-forum", "conda", "--page-size", "2"
        )
        assert returncode == 0, text
        assert re.search(r"\d+ result\(s\), page 1 of \d+", text), text
        assert "\n1. " in text


@pytest.mark.parametrize(
    "args",
    [
        ["conda", "--page", "0"],
        ["conda", "--page", "-1"],
        ["conda", "--page", "abc"],
        ["conda", "--page-size", "0"],
        ["conda", "--page-size", "101"],
        [],
    ],
    ids=["page-0", "page-negative", "page-text", "size-0", "size-101", "no-query"],
)
def test_search_forum_invalid_arguments(
    binary: Path, live_env: dict[str, str], args: list[str]
) -> None:
    """Rejected by clap before any request (exit 2, message on stderr)."""
    result = _run(binary, live_env, "intel", "search-forum", *args)
    assert result.returncode == 2
    assert result.stdout == ""
