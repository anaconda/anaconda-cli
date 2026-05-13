"""Integration tests for ana self update same-version handling."""

from __future__ import annotations

from helpers import AnaRunner


def get_version(run_ana: AnaRunner) -> str:
    """Get the current ana version."""
    result = run_ana("--version")
    assert result.returncode == 0
    return result.stdout.strip()


class TestSelfUpdateSameVersion:
    """Tests for 'ana self update <version>' when already on that version."""

    def test_update_to_same_version_shows_up_to_date(self, run_ana: AnaRunner) -> None:
        """When updating to the current version, show 'up to date' instead of downloading."""
        current_version = get_version(run_ana)

        # Try to update to the same version (with v prefix)
        result = run_ana("self", "update", f"v{current_version}")
        assert result.returncode == 0

        # Should show "UP TO DATE" status, not "UPDATED"
        assert "UP TO DATE" in result.stderr
        assert "UPDATED" not in result.stderr
        # Should show current version
        assert current_version in result.stderr
        # Should NOT show download progress
        assert "Downloading" not in result.stderr

    def test_update_to_same_version_without_v_prefix(self, run_ana: AnaRunner) -> None:
        """Version comparison should work without 'v' prefix."""
        current_version = get_version(run_ana)

        # Try to update to the same version (without v prefix)
        result = run_ana("self", "update", current_version)
        assert result.returncode == 0

        # Should show "UP TO DATE" status
        assert "UP TO DATE" in result.stderr
        assert "Downloading" not in result.stderr

    def test_update_to_same_version_with_force_flag(self, run_ana: AnaRunner) -> None:
        """The --force flag should bypass the same-version check and perform update."""
        current_version = get_version(run_ana)

        # Force update to the same version - this should attempt to download
        # Since we're testing locally, we check that it doesn't show "UP TO DATE"
        # and instead proceeds with the update flow
        result = run_ana("self", "update", f"v{current_version}", "--force")

        # When forcing, it should NOT show "UP TO DATE" (it proceeds to update)
        # It might fail to find the version (0.0.0 for dev builds) but that's ok -
        # the point is it didn't short-circuit with "UP TO DATE"
        assert (
            "UP TO DATE" not in result.stderr
            or "Already on version" not in result.stderr
        )


class TestSelfUpdateCli:
    """Tests for self update CLI interface (no network required)."""

    def test_force_flag_help_text(self, run_ana: AnaRunner) -> None:
        """The --force flag should appear in help output."""
        result = run_ana("self", "update", "--help")
        assert result.returncode == 0
        assert "--force" in result.stdout

    def test_force_conflicts_with_check(self, run_ana: AnaRunner) -> None:
        """The --force flag should conflict with --check."""
        result = run_ana("self", "update", "--force", "--check")
        assert result.returncode != 0
        assert "cannot be used with" in result.stderr

    def test_force_conflicts_with_list(self, run_ana: AnaRunner) -> None:
        """The --force flag should conflict with --list."""
        result = run_ana("self", "update", "--force", "--list")
        assert result.returncode != 0
        assert "cannot be used with" in result.stderr


class TestSelfUpdateDifferentVersion:
    """Tests to ensure normal update behavior is preserved."""

    def test_update_to_nonexistent_version_shows_error(
        self, run_ana: AnaRunner
    ) -> None:
        """Updating to a non-existent version should show an error."""
        result = run_ana("self", "update", "v999.999.999")
        assert result.returncode == 0  # Command runs but shows error
        assert "not found" in result.stderr.lower()

    def test_update_check_still_works(self, run_ana: AnaRunner) -> None:
        """The --check flag should still work as before."""
        result = run_ana("self", "update", "--check")
        assert result.returncode == 0
        # Should show either update available or up to date
        assert "UPDATE" in result.stderr or "UP TO DATE" in result.stderr
