"""Live integration tests for `ana forum search`.

These run the real binary against the live forum search service, which
needs no credentials. Subprocess stdout is captured (as in an agent's
shell): output must be JSON with no flag, and errors must be JSON on stdout
with exit code 1; argument errors come from clap with exit code 2.

Kept small: at most one network call per test. Paging math, URL checks and
blurb clean-up are covered by the Rust unit tests.
"""

from __future__ import annotations

import json
from pathlib import Path

from helpers import run_live


def test_help(live_binary: Path, live_env: dict[str, str]) -> None:
    """Help works without the required query."""
    result = run_live(live_binary, live_env, "forum", "search", "--help")
    assert result.returncode == 0
    assert "--page-size" in result.stdout + result.stderr


def test_search(live_binary: Path, live_env: dict[str, str]) -> None:
    """Multi-word query with paging options, logged out."""
    result = run_live(
        live_binary,
        live_env,
        "forum",
        "search",
        "conda",
        "environment",
        "--page",
        "2",
        "--page-size",
        "3",
    )
    assert result.returncode == 0, result.stderr
    body = json.loads(result.stdout)
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


def test_result_window(live_binary: Path, live_env: dict[str, str]) -> None:
    """Rejected before the request: JSON error, exit 1."""
    result = run_live(
        live_binary,
        live_env,
        "forum",
        "search",
        "conda",
        "--page",
        "101",
        "--page-size",
        "100",
    )
    assert result.returncode == 1
    assert json.loads(result.stdout)["error"] == "invalid_request"


def test_invalid_argument(live_binary: Path, live_env: dict[str, str]) -> None:
    """Rejected by clap before any request (exit 2, nothing on stdout)."""
    result = run_live(
        live_binary, live_env, "forum", "search", "conda", "--page-size", "101"
    )
    assert result.returncode == 2
    assert result.stdout == ""
