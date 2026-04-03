#!/usr/bin/env bash
set -euo pipefail

# Lock all tools in the lockfiles directory
# Usage: ./lock-all.sh [tool...]
# If no tools specified, locks all subdirectories with pixi.toml

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

lock_tool() {
    local tool="$1"
    local tool_dir="$SCRIPT_DIR/$tool"

    if [[ ! -f "$tool_dir/pixi.toml" ]]; then
        echo "Skipping $tool: no pixi.toml found"
        return 0
    fi

    echo "==> Locking $tool"
    (cd "$tool_dir" && pixi lock)
}

if [[ $# -gt 0 ]]; then
    # Lock specific tools
    for tool in "$@"; do
        lock_tool "$tool"
    done
else
    # Lock all tools
    for tool_dir in "$SCRIPT_DIR"/*/; do
        tool="$(basename "$tool_dir")"
        lock_tool "$tool"
    done
fi

echo "Done."
