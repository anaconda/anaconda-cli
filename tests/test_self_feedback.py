"""Integration tests for the 'ana self feedback' command."""

from __future__ import annotations

from helpers import AnaRunner


class TestSelfFeedback:
    """Tests for 'ana self feedback' command."""

    def test_feedback_prints_issues_url(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "feedback", env={"ANA_OPEN_BROWSER": "0"})
        assert result.returncode == 0
        assert "https://github.com/anaconda/ana-cli/issues/new/choose" in result.stderr

    def test_feedback_help(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "feedback", "--help")
        assert result.returncode == 0
        assert "Usage: ana self feedback" in result.stdout
        assert "Open GitHub issues page" in result.stdout

    def test_feedback_rejects_unknown_flag(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "feedback", "--bogus-flag")
        assert result.returncode == 2
        assert "Unexpected argument" in result.stderr

    def test_feedback_rejects_unexpected_argument(self, run_ana: AnaRunner) -> None:
        result = run_ana("self", "feedback", "extra-arg")
        assert result.returncode == 2
        assert "Unexpected argument" in result.stderr
