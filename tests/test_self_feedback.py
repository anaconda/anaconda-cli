"""Integration tests for the 'ana self feedback' command."""

from __future__ import annotations

import pytest
from helpers import IS_WINDOWS
from helpers import AnaRunner


class TestSelfFeedback:
    """Tests for 'ana self feedback' command."""

    @pytest.mark.skipif(
        IS_WINDOWS,
        reason=(
            "ana self feedback always calls webbrowser::open (not gated behind "
            "ANA_OPEN_BROWSER like other commands), which launches a real Edge "
            "process on Windows runners. That process inherits this test's "
            "stdout/stderr pipe handles, so subprocess.run() blocks forever "
            "waiting for the pipes to close. Re-enable once feedback::open_feedback "
            "respects ANA_OPEN_BROWSER."
        ),
    )
    def test_feedback_prints_issues_url(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "feedback")
        assert result.returncode == 0
        assert "https://github.com/anaconda/ana-cli/issues/new/choose" in result.stderr
