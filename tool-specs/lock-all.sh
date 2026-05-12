#!/usr/bin/env bash
set -euo pipefail

# Lock tool-spec lockfiles
# Usage: ./lock-all.sh [tool-or-path...]
#
# Arguments can be:
#   - Tool names: anaconda-cli, pixi, outerbounds
#   - File paths: tool-specs/anaconda-cli/pixi.toml (for pre-commit integration)
#
# If no arguments specified, locks all subdirectories with pixi.toml

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

# Convert argument to tool name (handles both "tool" and "path/to/tool/pixi.toml")
to_tool_name() {
    local arg="$1"
    if [[ "$arg" == */* ]]; then
        # It's a path - extract tool name from parent directory
        basename "$(dirname "$arg")"
    else
        echo "$arg"
    fi
}

if [[ $# -gt 0 ]]; then
    # Lock specific tools (deduplicated)
    for arg in "$@"; do
        to_tool_name "$arg"
    done | sort -u | while read -r tool; do
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
