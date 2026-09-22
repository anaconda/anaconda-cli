"""Integration tests for `ana mcp setup` / `ana mcp remove`.

These drive the real CLI against an isolated HOME so each supported agent
gets both an MCP server entry and the Anaconda Package Intelligence skill.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from helpers import AnaRunner

SKILL_NAME = "anaconda-intelligence"
SKILL_FILE = "SKILL.md"
SERVER_NAME = "anaconda-mcp"

# User-level skill path relative to HOME for each `ana mcp setup --client`.
CLIENT_SKILLS = {
    "claude-code": Path(".claude") / "skills" / SKILL_NAME / SKILL_FILE,
    "codex": Path(".codex") / "skills" / SKILL_NAME / SKILL_FILE,
    "cursor": Path(".cursor") / "skills" / SKILL_NAME / SKILL_FILE,
    "devin": Path(".codeium") / "windsurf" / "skills" / SKILL_NAME / SKILL_FILE,
    "kilo": Path(".config") / "kilo" / "skills" / SKILL_NAME / SKILL_FILE,
    "opencode": Path(".config") / "opencode" / "skills" / SKILL_NAME / SKILL_FILE,
    "vscode": Path(".copilot") / "skills" / SKILL_NAME / SKILL_FILE,
}

# Key under which the MCP server entry is stored in the client's config file.
CLIENT_CONFIG_KEYS = {
    "claude-code": "mcpServers",
    "codex": "mcp_servers",
    "cursor": "mcpServers",
    "devin": "mcpServers",
    "kilo": "mcp",
    "opencode": "mcp",
    "vscode": "servers",
}


def _assert_skill_md(path: Path) -> None:
    """Confirm the skill is a SKILL.md with the expected frontmatter."""
    assert path.name == SKILL_FILE, f"expected {SKILL_FILE}, got {path.name}"
    assert path.parent.name == SKILL_NAME
    assert path.is_file(), f"skill file was not written: {path}"
    content = path.read_text(encoding="utf-8")
    assert content.startswith(f"---\nname: {SKILL_NAME}\n")
    assert "description: >-" in content
    assert "compatibility: >-" in content
    assert "# Anaconda Package Intelligence" in content


def _assert_mcp_entry(config_path: Path, client: str) -> None:
    """Confirm setup wrote an Anaconda MCP server entry for this client."""
    assert config_path.is_file(), f"MCP config was not written: {config_path}"
    text = config_path.read_text(encoding="utf-8")
    if client == "codex":
        assert f"[{CLIENT_CONFIG_KEYS[client]}.{SERVER_NAME}]" in text
        assert "url" in text
        return
    data = json.loads(text)
    entry = data[CLIENT_CONFIG_KEYS[client]][SERVER_NAME]
    assert "url" in entry or "serverUrl" in entry


def _setup_client(
    run_ana_logged_in: AnaRunner,
    client: str,
) -> dict[str, object]:
    result = run_ana_logged_in(
        "mcp",
        "setup",
        "--client",
        client,
        "--json",
        "--no-backup",
    )
    assert result.returncode == 0, f"setup failed for {client}: {result.stderr}"
    payload = json.loads(result.stdout)
    assert client in payload, f"JSON output missing {client}: {payload}"
    return payload[client]


class TestMcpSetup:
    """End-to-end tests for `ana mcp setup`."""

    @pytest.mark.parametrize("client", list(CLIENT_SKILLS))
    def test_setup_writes_mcp_config_and_skill(
        self,
        client: str,
        run_ana_logged_in: AnaRunner,
        fake_home: Path,
    ) -> None:
        """Each agent receives an MCP config entry and a SKILL.md in the right place."""
        info = _setup_client(run_ana_logged_in, client)

        expected_skill = fake_home / CLIENT_SKILLS[client]
        skill_path = Path(str(info["skill_path"]))
        assert skill_path == expected_skill
        _assert_skill_md(skill_path)

        config_path = Path(str(info["config_path"]))
        _assert_mcp_entry(config_path, client)
        assert info["server_name"] == SERVER_NAME
        assert info["created"] is True

    def test_setup_all_clients_writes_each_skill(
        self,
        run_ana_logged_in: AnaRunner,
        fake_home: Path,
    ) -> None:
        """A single setup invocation with every --client still installs each skill."""
        args: list[str] = ["mcp", "setup", "--json", "--no-backup"]
        for client in CLIENT_SKILLS:
            args.extend(["--client", client])
        result = run_ana_logged_in(*args)
        assert result.returncode == 0, result.stderr
        payload = json.loads(result.stdout)
        assert set(payload) == set(CLIENT_SKILLS)

        for client, relative in CLIENT_SKILLS.items():
            skill_path = fake_home / relative
            assert Path(str(payload[client]["skill_path"])) == skill_path
            _assert_skill_md(skill_path)
            _assert_mcp_entry(Path(str(payload[client]["config_path"])), client)
