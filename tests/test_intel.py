"""Integration tests for `ana intel`.

These run the real binary with stdout captured, which is how an agent's shell
runs it: output must be JSON (no `--json` needed), errors must be JSON on
stdout with a non-zero exit, and nothing may prompt or hang.
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

import pytest
from helpers import AnaRunner


def _json(result: subprocess.CompletedProcess[str]) -> dict:
    """Parse stdout as a single JSON document (fails on any stray text)."""
    return json.loads(result.stdout)


def test_intel_help(run_ana: AnaRunner) -> None:
    result = run_ana("intel", "--help")
    assert result.returncode == 0
    output = result.stdout + result.stderr
    assert "package-info" in output
    assert "search-forum" in output


def test_package_info_prints_json_when_piped(
    run_ana_logged_in: AnaRunner,
) -> None:
    result = run_ana_logged_in(
        "intel", "package-info", "numpy", "--python-version", "3.12"
    )
    assert result.returncode == 0, result.stderr
    body = _json(result)
    assert body["name"] == "numpy"
    assert body["version"] == "2.4.1"
    assert body["channel"] == "main"
    assert body["platforms"] == ["linux-64", "osx-arm64"]
    assert body["python_versions"] == ["3.12"]
    assert body["cve_summary"] == {"count": 0, "critical": 0}
    assert body["policy_status"] is None
    assert body["notes"] == []


def test_package_info_unknown_package_is_json_error(
    run_ana_logged_in: AnaRunner,
) -> None:
    result = run_ana_logged_in("intel", "package-info", "not-a-package")
    assert result.returncode == 1
    body = _json(result)
    assert body["error"] == "package_not_found"
    assert body["name"] == "not-a-package"
    # Exactly one machine-readable error: no miette report on stderr.
    assert "not-a-package" not in result.stderr


def test_package_info_invalid_name_is_json_error(
    run_ana_logged_in: AnaRunner,
) -> None:
    result = run_ana_logged_in("intel", "package-info", "../etc")
    assert result.returncode == 1
    assert _json(result)["error"] == "invalid_request"


def test_package_info_logged_out_fails_fast_with_json(
    ana_binary: Path | None, auth_env: dict[str, str]
) -> None:
    """Logged out, non-interactive: no login prompt, no device flow, no hang."""
    if ana_binary is None:
        pytest.skip("ana binary not found")
    result = subprocess.run(
        [str(ana_binary), "intel", "package-info", "numpy"],
        capture_output=True,
        stdin=subprocess.DEVNULL,
        text=True,
        encoding="utf-8",
        env=auth_env,
        timeout=30,
    )
    assert result.returncode == 1
    body = _json(result)
    assert body["error"] == "unauthenticated"
    assert "ana login" in body["message"]
    assert "Login now?" not in result.stdout + result.stderr


def test_search_forum_works_logged_out(
    run_ana: AnaRunner, auth_env: dict[str, str]
) -> None:
    result = run_ana(
        "intel",
        "search-forum",
        "solving",
        "environment",
        "--page-size",
        "5",
        env=auth_env,
    )
    assert result.returncode == 0, result.stderr
    body = _json(result)
    assert body["total"] == 1
    assert body["topics"][0]["url"] == (
        "https://forum.anaconda.com/t/solving-environment-failed/7"
    )
    assert body["topics"][0]["created_at"] == "2024-01-02T03:04:05Z"
    assert body["pagination"] == {"page": 1, "page_size": 5, "total_pages": 1}


def test_search_forum_invalid_page_size_is_json_error(
    run_ana: AnaRunner, auth_env: dict[str, str]
) -> None:
    result = run_ana("intel", "search-forum", "x", "--page-size", "500", env=auth_env)
    assert result.returncode == 1
    assert _json(result)["error"] == "invalid_request"


def test_json_flag_is_accepted_when_piped(
    run_ana_logged_in: AnaRunner,
) -> None:
    result = run_ana_logged_in("intel", "package-info", "numpy", "--json")
    assert result.returncode == 0, result.stderr
    assert _json(result)["name"] == "numpy"
