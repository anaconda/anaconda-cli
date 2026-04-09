#!/usr/bin/env python3
"""Write conda.recipe/variants.yaml with the version from the built binary.

Usage: python scripts/write_conda_variant.py [path/to/ana]

If no path is given, uses target/release/ana (or .exe on Windows).
"""

import platform
import subprocess
import sys
from pathlib import Path

RECIPE_DIR = Path(__file__).resolve().parent.parent / "conda.recipe"


def main() -> int:
    if len(sys.argv) > 1:
        binary = sys.argv[1]
    else:
        ext = ".exe" if platform.system() == "Windows" else ""
        binary = f"target/release/ana{ext}"

    try:
        result = subprocess.run(
            [binary, "--version"], capture_output=True, text=True, check=True
        )
    except (FileNotFoundError, subprocess.CalledProcessError) as exc:
        print(f"ERROR: Could not get version from {binary}: {exc}", file=sys.stderr)
        return 1

    version = result.stdout.strip()
    variant_path = RECIPE_DIR / "variants.yaml"
    variant_path.write_text(f'pkg_version:\n  - "{version}"\n')
    print(f"PKG_VERSION={version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
