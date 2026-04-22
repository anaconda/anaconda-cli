"""Test helpers and utilities."""

from __future__ import annotations

import subprocess
import sys
from collections.abc import Callable
from pathlib import Path

IS_WINDOWS = sys.platform == "win32"


def _find_repo_root() -> Path:
    """Find the repository root by looking for .git directory."""
    path = Path(__file__).resolve()
    for parent in path.parents:
        if (parent / ".git").exists():
            return parent
    raise RuntimeError("Could not find repository root")


REPO_ROOT = _find_repo_root()

AnaRunner = Callable[..., subprocess.CompletedProcess[str]]


def assert_output_contains(text: str, *patterns: str) -> None:
    """Assert that patterns appear in text in order.

    Provides clear error messages showing:
    - Which pattern failed to match
    - What patterns matched before it
    - The relevant portion of text around the search position

    Args:
        text: The text to search (e.g., result.stderr)
        *patterns: Strings that should appear in order

    Example:
        assert_output_contains(
            result.stderr,
            "ACCOUNT",
            "username",
            "testuser",
            "SUBSCRIPTIONS",
        )
    """
    pos = 0
    matched = []

    for pattern in patterns:
        idx = text.find(pattern, pos)
        if idx >= 0:
            matched.append(pattern)
            pos = idx + len(pattern)
            continue

        # Pattern not found - build error message with context
        lines = text.splitlines()
        # Find which line we're at
        chars_seen = 0
        current_line = 0
        for i, line in enumerate(lines):
            if chars_seen + len(line) >= pos:
                current_line = i
                break
            chars_seen += len(line) + 1  # +1 for newline

        # Show context: a few lines before and after current position
        start_line = max(0, current_line - 2)
        end_line = min(len(lines), current_line + 5)
        context_lines = lines[start_line:end_line]
        context = "\n".join(
            f"  {'>' if i == current_line - start_line else ' '} {line}"
            for i, line in enumerate(context_lines)
        )

        matched_str = "\n  - ".join([""] + matched) if matched else " (none)"
        msg = (
            f"Pattern not found: {pattern!r}\n\n"
            f"Already matched:{matched_str}\n\n"
            f"Searching from line {current_line + 1}:\n{context}\n\n"
            f"Full output:\n{text}"
        )
        raise AssertionError(msg)
