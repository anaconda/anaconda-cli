"""Integration tests for the 'ana self feedback' command."""

from __future__ import annotations

from helpers import AnaRunner


class TestSelfFeedback:
    """Tests for 'ana self feedback' command."""

    def test_feedback_prints_issues_url(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "feedback")
        assert result.returncode == 0
        assert "https://github.com/anaconda/ana-cli/issues/new/choose" in result.stderr

    def test_feedback_exits_zero_when_browser_open_fails(
        self, run_ana: AnaRunner
    ) -> None:
        # No display/browser is available in the isolated test environment, so
        # webbrowser::open is expected to fail; the command should still not
        # treat that as a fatal error.
        result = run_ana("self", "feedback", env={"DISPLAY": "", "BROWSER": ""})
        assert result.returncode == 0
