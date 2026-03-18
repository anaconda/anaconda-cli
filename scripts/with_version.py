#!/usr/bin/env python3
"""Run a command with PKG_VERSION set from git tags.

Usage:
    python scripts/with_version.py                     # Print version only
    python scripts/with_version.py <command> [args...] # Run command with PKG_VERSION set

Examples:
    python scripts/with_version.py
    python scripts/with_version.py cargo build
    python scripts/with_version.py cargo build --release
    python scripts/with_version.py rattler-build build --recipe conda.recipe
"""

import os
import re
import subprocess
import sys

FALLBACK_VERSION = "0.0.0"


def get_version() -> str:
    """Get package version from git describe."""
    try:
        result = subprocess.run(
            ["git", "describe", "--tags", "--always", "--dirty"],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            return FALLBACK_VERSION
        version = result.stdout.strip()
    except FileNotFoundError:
        return FALLBACK_VERSION

    if not version:
        return FALLBACK_VERSION

    # Strip leading 'v' if present
    if version.startswith("v"):
        version = version[1:]

    return format_version(version)


def format_version(version: str) -> str:
    """Format git describe output to PEP 440-like version.

    Examples:
        "0.2.0" -> "0.2.0"
        "0.2.0-5-gabc1234" -> "0.2.0.dev5+gabc1234"
        "0.2.0-dirty" -> "0.2.0+dirty"
        "0.2.0-5-gabc1234-dirty" -> "0.2.0.dev5+gabc1234.dirty"
        "abc1234" -> "0.0.0.dev0+gabc1234"
        "abc1234-dirty" -> "0.0.0.dev0+gabc1234.dirty"
    """
    # Has commits after tag: X.Y.Z-N-gHASH[-dirty]
    match = re.match(r"^(\d+\.\d+\.\d+)-(\d+)-(g[a-f0-9]+)(-dirty)?$", version)
    if match:
        base, commits, hash_, dirty = match.groups()
        dirty_suffix = ".dirty" if dirty else ""
        return f"{base}.dev{commits}+{hash_}{dirty_suffix}"

    # Exactly at tag but dirty: X.Y.Z-dirty
    match = re.match(r"^(\d+\.\d+\.\d+)-dirty$", version)
    if match:
        return f"{match.group(1)}+dirty"

    # Exactly at tag, clean: X.Y.Z
    if re.match(r"^\d+\.\d+\.\d+$", version):
        return version

    # No tag, just commit hash: HASH[-dirty]
    match = re.match(r"^([a-f0-9]+)(-dirty)?$", version)
    if match:
        hash_, dirty = match.groups()
        dirty_suffix = ".dirty" if dirty else ""
        return f"0.0.0.dev0+g{hash_}{dirty_suffix}"

    # Fallback
    return FALLBACK_VERSION


def main() -> int:
    version = get_version()

    # No command - just print version
    if len(sys.argv) < 2:
        print(version)
        return 0

    # Run command with PKG_VERSION set
    print(f"PKG_VERSION={version}")

    env = os.environ.copy()
    env["PKG_VERSION"] = version

    result = subprocess.run(sys.argv[1:], env=env, check=False)
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
