"""Integration tests for `ana mcp serve`, the local stdio MCP server.

Drive the real binary over stdin/stdout like an AI client would. Every stdout
line must be a JSON-RPC message: anything else would corrupt the protocol.
"""

from __future__ import annotations

import json
import queue
import subprocess
import threading
from pathlib import Path
from typing import Any

import pytest
from helpers import AnaRunner

TIMEOUT_SECONDS = 30


class McpSession:
    """Minimal MCP client speaking newline-delimited JSON-RPC over stdio."""

    def __init__(self, binary: Path, env: dict[str, str]) -> None:
        self.proc = subprocess.Popen(
            [str(binary), "mcp", "serve"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            env=env,
        )
        self._lines: queue.Queue[str | None] = queue.Queue()
        self._next_id = 1
        threading.Thread(target=self._read_stdout, daemon=True).start()

    def _read_stdout(self) -> None:
        assert self.proc.stdout is not None
        for line in self.proc.stdout:
            self._lines.put(line)
        self._lines.put(None)

    def _send(self, message: dict[str, Any]) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()

    def _receive(self) -> dict[str, Any]:
        line = self._lines.get(timeout=TIMEOUT_SECONDS)
        assert line is not None, "server closed stdout unexpectedly"
        message = json.loads(line)  # fails if stdout carries non-protocol text
        assert message.get("jsonrpc") == "2.0", message
        return message

    def request(self, method: str, params: dict[str, Any] | None = None) -> Any:
        request_id = self._next_id
        self._next_id += 1
        message: dict[str, Any] = {"jsonrpc": "2.0", "id": request_id, "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)
        response = self._receive()
        assert response.get("id") == request_id, response
        assert "error" not in response, response
        return response["result"]

    def initialize(self) -> dict[str, Any]:
        result = self.request(
            "initialize",
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "pytest", "version": "0"},
            },
        )
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        return result

    def call_tool(self, name: str, arguments: dict[str, Any]) -> tuple[bool, Any]:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        body = json.loads(result["content"][0]["text"])
        return bool(result.get("isError")), body

    def close(self) -> tuple[int, str]:
        assert self.proc.stdin is not None
        self.proc.stdin.close()
        returncode = self.proc.wait(timeout=TIMEOUT_SECONDS)
        assert self.proc.stderr is not None
        stderr = self.proc.stderr.read()
        # Nothing besides the protocol may have reached stdout.
        while (line := self._lines.get(timeout=TIMEOUT_SECONDS)) is not None:
            json.loads(line)
        return returncode, stderr


@pytest.fixture
def mcp_session(ana_binary: Path | None):
    """Start `ana mcp serve` sessions with a given environment."""
    if ana_binary is None:
        pytest.skip("ana binary not found. Build with 'pixi run build-release'")
    sessions: list[McpSession] = []

    def _start(env: dict[str, str]) -> McpSession:
        session = McpSession(ana_binary, env)
        sessions.append(session)
        return session

    yield _start
    for session in sessions:
        if session.proc.poll() is None:
            session.proc.kill()


def test_mcp_serve_lists_tools(mcp_session, logged_in_env: dict[str, str]) -> None:
    session = mcp_session(logged_in_env)
    init = session.initialize()
    assert init["serverInfo"]["name"] == "anaconda-mcp"
    assert "package_info" in init["instructions"]

    tools = session.request("tools/list")["tools"]
    assert sorted(tool["name"] for tool in tools) == ["package_info", "search_forum"]
    for tool in tools:
        assert tool["annotations"]["readOnlyHint"] is True

    returncode, _ = session.close()
    assert returncode == 0


def test_mcp_serve_package_info(mcp_session, logged_in_env: dict[str, str]) -> None:
    session = mcp_session(logged_in_env)
    session.initialize()

    is_error, body = session.call_tool(
        "package_info", {"name": "numpy", "python_version": "3.12"}
    )
    assert not is_error, body
    assert body["name"] == "numpy"
    assert body["version"] == "2.4.1"
    assert body["channel"] == "main"
    assert body["platforms"] == ["linux-64", "osx-arm64"]
    assert body["python_versions"] == ["3.12"]
    assert body["cve_summary"] == {"count": 0, "critical": 0}
    assert body["policy_status"] is None

    is_error, body = session.call_tool("package_info", {"name": "not-a-package"})
    assert is_error
    assert body["error"] == "package_not_found"

    returncode, _ = session.close()
    assert returncode == 0


def test_mcp_serve_without_login_reports_unauthenticated(
    mcp_session, auth_env: dict[str, str]
) -> None:
    """Not logged in: the server still starts, never prompts, and says why."""
    session = mcp_session(auth_env)
    session.initialize()

    is_error, body = session.call_tool("package_info", {"name": "numpy"})
    assert is_error
    assert body["error"] == "unauthenticated"
    assert "ana login" in body["message"]

    # Forum search needs no credentials.
    is_error, body = session.call_tool("search_forum", {"query": "solving environment"})
    assert not is_error, body
    assert body["topics"][0]["url"] == (
        "https://forum.anaconda.com/t/solving-environment-failed/7"
    )

    returncode, _ = session.close()
    assert returncode == 0


def test_mcp_serve_help(run_ana: AnaRunner) -> None:
    result = run_ana("mcp", "serve", "--help")
    assert result.returncode == 0
    assert "stdio" in result.stdout + result.stderr
