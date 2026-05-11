#!/usr/bin/env bash
set -euo pipefail

# Lock only the tools whose pixi.toml files were passed as arguments
# Usage: ./lock-changed.sh tool-specs/anaconda-cli/pixi.toml tool-specs/pixi/pixi.toml ...

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ $# -eq 0 ]]; then
    echo "No files specified"
    exit 0
fi

# Extract unique tool names from file paths and pass to lock-all.sh
for file in "$@"; do
    basename "$(dirname "$file")"
done | sort -u | xargs "$SCRIPT_DIR/lock-all.sh"
