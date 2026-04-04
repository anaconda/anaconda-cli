#!/usr/bin/env bash
# Derive the package version from git tags and write it to VERSION
# and conda.recipe/variants.yaml.
#
# Version format:
#   Exact tag match (v0.1.0-0-ghash):        0.1.0
#   Commits past a tag (v0.1.0-5-gabcdef1):  0.1.0+5.gabcdef1
#   No matching tag:                          0.0.0dev0
#
# Usage: ./scripts/get_version.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION_FILE="$REPO_ROOT/VERSION"
VARIANT_FILE="$REPO_ROOT/conda.recipe/variants.yaml"

VERSION=$(git describe --tags --long --match 'v[0-9]*.[0-9]*.[0-9]*' --exclude 'v*.dev*' 2>/dev/null \
    | sed -E 's@^v@@; s@(.+)-0-g.+@\1@; s@(.+)-([0-9]+)-(.+)@\1+\2.\3@') \
    || VERSION="0.0.0dev0"

echo "Version: $VERSION"

# Only overwrite if changed, to preserve mtime for build caches
write_if_changed() {
    local file="$1" content="$2"
    if [ ! -f "$file" ] || [ "$(cat "$file")" != "$content" ]; then
        printf '%s\n' "$content" > "$file"
        echo "  $file updated."
    else
        echo "  $file unchanged."
    fi
}

write_if_changed "$VERSION_FILE" "$VERSION"
write_if_changed "$VARIANT_FILE" "pkg_version:
  - $VERSION"
