"""Integration tests for the 'ana platform' command.

ana platform is a thin, verbatim passthrough to the outerbounds CLI (see
src/outerbounds/run.rs and src/cli.rs's Platform command). It calls
ensure_tool first, so missing outerbounds installs are created
automatically. Since the command is gated behind login, invocations run
with a completed login against the mock auth server (run_ana_logged_in).

ana platform is Unix-only (src/cli.rs: #[cfg(all(unix, tool_install))]).
"""

from __future__ import annotations

from pathlib import Path

import pytest
from helpers import IS_WINDOWS
from helpers import AnaRunner

pytestmark = pytest.mark.skipif(
    IS_WINDOWS, reason="ana platform is Unix-only (src/cli.rs)"
)


@pytest.fixture
def stub_outerbounds(run_ana_logged_in: AnaRunner, fake_home: Path) -> Path:
    """Install a stub outerbounds binary and return the file it logs argv to.

    The stub stands in for the real outerbounds CLI so the handoff — and the
    --help forwarding fixed by CLI-790 — can be inspected without a real
    Outerbounds instance. It prints a known usage string for `check --help`
    (mirroring the real `outerbounds check --help` output) and exits 0,
    without the stub attempting the real check logic.

    outerbounds is first installed for real so that ensure_tool finds a
    valid .lockfile-hash and skips reinstalling (src/tools/install.rs). The
    real outerbounds binary is then replaced with the stub.
    """
    result = run_ana_logged_in("tool", "install", "outerbounds")
    assert result.returncode == 0, f"outerbounds install failed: {result.stderr}"

    argv_log = fake_home / "outerbounds-argv.txt"

    stub = fake_home / ".ana" / "tools" / "outerbounds" / "bin" / "outerbounds"
    stub.write_text(
        "#!/bin/sh\n"
        f'printf "%s\\n" "$@" > "{argv_log}"\n'
        'if [ "$1" = "check" ] && { [ "$2" = "--help" ] || [ "$2" = "-h" ]; }; then\n'
        '  echo "Usage: outerbounds check [OPTIONS]"\n'
        '  echo "Check packages and configuration for common errors"\n'
        "  exit 0\n"
        "fi\n"
        'exit "${STUB_EXIT_CODE:-0}"\n'
    )
    stub.chmod(0o755)

    return argv_log


class TestPlatformCheckHelp:
    """Regression tests for CLI-790/CLI-799: `check` is a leaf command (it
    takes its own flags, unlike subcommand groups such as `kubernetes` or
    `integrations`), so --help must still reach the wrapped tool instead of
    being swallowed before the proxy call."""

    def test_check_help_is_forwarded_and_shows_usage(
        self, run_ana_logged_in: AnaRunner, stub_outerbounds: Path
    ) -> None:
        result = run_ana_logged_in("platform", "check", "--help")
        assert result.returncode == 0, f"check --help failed: {result.stderr}"
        assert "Usage: outerbounds check" in result.stdout
        assert stub_outerbounds.read_text().splitlines() == ["check", "--help"]

    def test_check_short_help_is_forwarded_and_shows_usage(
        self, run_ana_logged_in: AnaRunner, stub_outerbounds: Path
    ) -> None:
        result = run_ana_logged_in("platform", "check", "-h")
        assert result.returncode == 0, f"check -h failed: {result.stderr}"
        assert "Usage: outerbounds check" in result.stdout
        assert stub_outerbounds.read_text().splitlines() == ["check", "-h"]

    def test_check_without_help_does_not_show_usage(
        self, run_ana_logged_in: AnaRunner, stub_outerbounds: Path
    ) -> None:
        """Control: without --help, the stub exercises the real check path
        (here just the argv-logging branch) rather than the help text."""
        result = run_ana_logged_in("platform", "check")
        assert result.returncode == 0, f"check failed: {result.stderr}"
        assert "Usage: outerbounds check" not in result.stdout
        assert stub_outerbounds.read_text().splitlines() == ["check"]
