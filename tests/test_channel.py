"""Integration tests for the 'ana channel' command.

ana channel builds an argument vector (see src/packages/commands.rs) and
forwards it to the anaconda-cli package's `anaconda channel` subcommand via
src/tools/run.rs::run_tool_binary, auto-installing anaconda-cli first if
needed (src/packages/run.rs).

The unit tests in src/packages/commands.rs already cover the arg vector for
every subcommand/flag combination, so the cases here cover what those cannot:
the help text clap renders before any dispatch, and the handoff to the
anaconda binary.

TestChannelPassthrough stops at ana's boundary. anaconda-client has its own
test suite, so rather than assert on its behaviour (which also drags in auth,
the network, and version-dependent message wording) these tests substitute a
stub `anaconda` binary in the isolated HOME that records the argv it was
handed and exits with a code of the test's choosing. That verifies everything
ana owns: flag normalization, argument order, and exit-code handling.

Note on streams: output from the anaconda child process goes to ana's stdout,
while ana's own status and error lines go to stderr.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from helpers import IS_WINDOWS
from helpers import AnaRunner
from helpers import assert_output_contains


class TestChannelHelp:
    """Tests for 'ana channel' help output."""

    def test_channel_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("channel", "--help")
        assert result.returncode == 0
        assert_output_contains(
            result.stdout,
            "Manage channels and packages",
            "Usage: ana channel",
            "COMMANDS",
            "create",
            "remove",
            "upload",
        )

    def test_channel_short_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("channel", "-h")
        assert result.returncode == 0
        assert "Manage channels and packages" in result.stdout

    def test_channel_no_args_shows_help(self, run_ana: AnaRunner) -> None:
        """A bare 'ana channel' shows help instead of invoking the proxied
        binary, unlike 'ana org' which has no bare-command help."""
        result = run_ana("channel")
        assert result.returncode == 0
        assert "Usage: ana channel" in result.stdout

    def test_channel_create_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("channel", "create", "--help")
        assert result.returncode == 0
        assert_output_contains(
            result.stdout,
            "Create a new channel",
            "Usage: ana channel create",
            "--private",
            "--public",
            "--namespace",
        )

    def test_channel_remove_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("channel", "remove", "--help")
        assert result.returncode == 0
        assert_output_contains(
            result.stdout,
            "Remove a channel",
            "Usage: ana channel remove",
            "--namespace",
        )

    def test_channel_upload_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("channel", "upload", "--help")
        assert result.returncode == 0
        assert_output_contains(
            result.stdout,
            "Upload a package to a channel",
            "Usage: ana channel upload",
            "-c, --channel",
            "--no-progress",
        )

    def test_root_help_lists_channel_under_packages(self, run_ana: AnaRunner) -> None:
        """The PACKAGES section (src/help/data.rs) advertises channel."""
        result = run_ana("--help")
        assert result.returncode == 0
        assert_output_contains(result.stdout, "PACKAGES", "channel")


class TestChannelArgumentErrors:
    """Tests that bad channel args fail locally, before any passthrough."""

    def test_unknown_channel_subcommand(self, run_ana: AnaRunner) -> None:
        result = run_ana("channel", "bogus")
        assert result.returncode == 2
        assert "Unknown subcommand 'bogus'" in result.stderr

    def test_create_rejects_unknown_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("channel", "create", "--bogus-flag", "org/channel")
        assert result.returncode == 2
        assert "--bogus-flag" in result.stderr

    def test_upload_rejects_unknown_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("channel", "upload", "--bogus-flag", "pkg.conda")
        assert result.returncode == 2
        assert "--bogus-flag" in result.stderr


@pytest.fixture
def stub_anaconda(fake_home: Path) -> Path:
    """Install a stub anaconda binary and return the file it records argv to.

    The stub stands in for anaconda-cli so the handoff can be inspected
    without an install, credentials or a network call. It exits with
    STUB_EXIT_CODE (default 0) so exit-code handling can be driven too.

    A sentinel at ~/.ana/bin/anaconda is also needed, because that is the path
    src/packages/run.rs checks before deciding to auto-install.
    """
    argv_log = fake_home / "anaconda-argv.txt"

    tool_bin = fake_home / ".ana" / "tools" / "anaconda-cli" / "bin"
    tool_bin.mkdir(parents=True)
    stub = tool_bin / "anaconda"
    stub.write_text(
        "#!/bin/sh\n"
        f'printf "%s\\n" "$@" > "{argv_log}"\n'
        'exit "${STUB_EXIT_CODE:-0}"\n'
    )
    stub.chmod(0o755)

    bin_dir = fake_home / ".ana" / "bin"
    bin_dir.mkdir(parents=True)
    (bin_dir / "anaconda").touch()

    return argv_log


@pytest.mark.skipif(IS_WINDOWS, reason="stub anaconda binary is a shell script")
class TestChannelPassthrough:
    """Tests for what ana hands off to the anaconda binary.

    Assertions stop at ana's boundary: anaconda-client covers its own
    behaviour, so only the argv and the exit-code handling are checked.
    """

    def test_create_hands_off_argv(
        self, run_ana: AnaRunner, stub_anaconda: Path
    ) -> None:
        """Flags precede the channel positional (src/packages/commands.rs)."""
        result = run_ana(
            "channel", "create", "--private", "--namespace", "my-ns", "org/channel"
        )
        assert result.returncode == 0, f"create failed: {result.stderr}"
        assert stub_anaconda.read_text().splitlines() == [
            "channel",
            "create",
            "--private",
            "--namespace",
            "my-ns",
            "org/channel",
        ]

    def test_remove_hands_off_argv(
        self, run_ana: AnaRunner, stub_anaconda: Path
    ) -> None:
        result = run_ana("channel", "remove", "--namespace", "my-ns", "org/channel")
        assert result.returncode == 0, f"remove failed: {result.stderr}"
        assert stub_anaconda.read_text().splitlines() == [
            "channel",
            "remove",
            "--namespace",
            "my-ns",
            "org/channel",
        ]

    def test_upload_hands_off_argv(
        self, run_ana: AnaRunner, stub_anaconda: Path
    ) -> None:
        """The -c short flag is normalized to --channel, and every file is
        forwarded after the flags."""
        result = run_ana(
            "channel",
            "upload",
            "-c",
            "org/channel",
            "--no-progress",
            "one.conda",
            "two.conda",
        )
        assert result.returncode == 0, f"upload failed: {result.stderr}"
        assert stub_anaconda.read_text().splitlines() == [
            "channel",
            "upload",
            "--channel",
            "org/channel",
            "--no-progress",
            "one.conda",
            "two.conda",
        ]

    def test_upload_without_channel_hands_off_argv(
        self, run_ana: AnaRunner, stub_anaconda: Path
    ) -> None:
        """Without -c, ana must not invent a --channel flag; anaconda-client
        owns the "no channel specified" error."""
        result = run_ana("channel", "upload", "one.conda")
        assert result.returncode == 0, f"upload failed: {result.stderr}"
        assert stub_anaconda.read_text().splitlines() == [
            "channel",
            "upload",
            "one.conda",
        ]

    def test_child_failure_is_reported(
        self, run_ana: AnaRunner, stub_anaconda: Path
    ) -> None:
        """A non-zero exit from the child is reported with its real code, but
        ana itself always exits 1 (src/tools/run.rs)."""
        result = run_ana(
            "channel", "remove", "org/channel", env={"STUB_EXIT_CODE": "3"}
        )
        assert result.returncode == 1
        assert "anaconda exited with code 3" in result.stderr
