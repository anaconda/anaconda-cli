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
import subprocess
import sys

from setuptools_scm import get_version

FALLBACK_VERSION = "0.0.0"


def main() -> int:
    try:
        version = get_version()
    except LookupError:
        version = FALLBACK_VERSION

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
