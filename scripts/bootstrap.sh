#!/usr/bin/env bash
# Bootstrap the ana-cli development environment.
#
# This script is idempotent: run it again to verify or repair your setup.
# On a fresh clone it builds ana from source and installs the full project
# environment. On subsequent runs it confirms everything is in place.
#
# NOTE: Rust is required here only because we build ana from source during
# bootstrap. Once pre-built release binaries are available, this script will
# download them instead, and the project environment (which includes a
# conda-forge Rust toolchain) will provide everything needed for development.
#
# Usage: ./scripts/bootstrap.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

ANA_HOME="$HOME/.ana"
ANA_BIN="$ANA_HOME/bin"
ANA="$ANA_BIN/ana"
ENV_PREFIX="$REPO_ROOT/.pixi/envs/default"

NEEDS_PATH_UPDATE=()

# --- Check prerequisites ---
MIN_RUST="1.85.0"

# If cargo isn't on PATH, check the standard rustup location before prompting
# to download. The user may have Rust installed but not yet in their PATH.
CARGO_BIN="$HOME/.cargo/bin"
if ! command -v cargo &>/dev/null && [ -f "$CARGO_BIN/cargo" ]; then
    echo "Rust already installed, but not on PATH. Using $CARGO_BIN for this session."
    export PATH="$CARGO_BIN:$PATH"
    NEEDS_PATH_UPDATE+=("\$HOME/.cargo/bin")
fi

rust_version_ok() {
    local ver
    ver=$(rustc --version 2>/dev/null | awk '{print $2}')
    [ -n "$ver" ] || return 1
    printf '%s\n%s\n' "$MIN_RUST" "$ver" | sort -V -C
}

if ! command -v cargo &>/dev/null || ! rust_version_ok; then
    if command -v cargo &>/dev/null; then
        echo "Rust $(rustc --version | awk '{print $2}') is installed, but this project requires >= $MIN_RUST."
    else
        echo "Rust is not installed."
    fi
    echo ""
    echo "The following command will install the latest Rust toolchain via rustup:"
    echo ""
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo ""
    read -rp "Run this now? [y/N] " answer
    if [[ "$answer" =~ ^[Yy]$ ]]; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
        echo ""
        if ! rust_version_ok; then
            echo "Error: Rust installation succeeded but version is still below $MIN_RUST." >&2
            exit 1
        fi
    else
        echo ""
        echo "Please install Rust $MIN_RUST or greater and ensure cargo is on your PATH,"
        echo "then re-run this script."
        exit 1
    fi
fi

# --- Ensure Cargo uses system git for SSH support ---
CARGO_CONFIG="$HOME/.cargo/config.toml"
if ! grep -q 'git-fetch-with-cli' "$CARGO_CONFIG" 2>/dev/null; then
    mkdir -p "$HOME/.cargo"
    echo -e '\n[net]\ngit-fetch-with-cli = true' >> "$CARGO_CONFIG"
    echo "Configured cargo to use system git (git-fetch-with-cli)."
else
    echo "Cargo git-fetch-with-cli already configured."
fi

# --- Build and install ana ---
if [ -x "$ANA" ]; then
    echo "Bootstrap binary already exists: $ANA"
else
    echo "Building ana (release)..."
    git describe --tags --long | sed -E 's@^v@@;s@(.*)-(.*)-(.*)@\1+\2.\3@' > VERSION
    cargo build --release 2>&1
    mkdir -p "$ANA_BIN"
    cp "$REPO_ROOT/target/release/ana" "$ANA"
    echo "Installed ana to $ANA"
fi

# If ana isn't on PATH, check the install location
if ! command -v ana &>/dev/null && [ -x "$ANA" ]; then
    echo "ana already installed, but not on PATH. Using $ANA_BIN for this session."
    export PATH="$ANA_BIN:$PATH"
    NEEDS_PATH_UPDATE+=("\$HOME/.ana/bin")
fi

# --- Check for pixi (needed for local development) ---
PIXI_BIN="$HOME/.pixi/bin"
if command -v pixi &>/dev/null; then
    echo "pixi found on PATH."
elif [ -f "$PIXI_BIN/pixi" ]; then
    echo "pixi already installed, but not on PATH. Using $PIXI_BIN for this session."
    export PATH="$PIXI_BIN:$PATH"
    NEEDS_PATH_UPDATE+=("\$HOME/.pixi/bin")
else
    echo ""
    echo "NOTE: pixi is not installed. While not required for running tasks,"
    echo "it is needed for local development (e.g., adding/removing packages)."
    echo "Install it from: https://pixi.sh"
    echo ""
fi

# --- Prepare project environment ---
if [ -d "$ENV_PREFIX/conda-meta" ]; then
    echo "Project environment already prepared: $ENV_PREFIX"
else
    echo "Preparing project environment..."
    "$ANA" prepare
fi

# --- Install pre-commit hook ---
# TODO: The hook uses an absolute path to the ana binary ($ANA), which ties it
# to the user who ran bootstrap. Revisit once tagged release binaries are
# available — at that point we can use a PATH-based lookup instead.
HOOK="$REPO_ROOT/.git/hooks/pre-commit"
if [ -f "$HOOK" ] && grep -q 'ana run pre-commit' "$HOOK" 2>/dev/null; then
    echo "Pre-commit hook already installed."
else
    echo "Installing pre-commit hook..."
    cat > "$HOOK" <<HOOK_EOF
#!/usr/bin/env bash
exec $ANA run pre-commit
HOOK_EOF
    chmod +x "$HOOK"
fi

echo ""
echo "=========================================="
echo " Bootstrap complete!"
echo "=========================================="
echo ""

if [ ${#NEEDS_PATH_UPDATE[@]} -gt 0 ]; then
    PATH_ADDITION=$(IFS=:; echo "${NEEDS_PATH_UPDATE[*]}")
    echo "To make the following tools available everywhere, add this to your shell profile:"
    echo ""
    echo "  export PATH=\"$PATH_ADDITION:\$PATH\""
    echo ""
fi

echo "Run project tasks with:"
echo ""
echo "  ana run test              # Run unit tests"
echo "  ana run build-release     # Build release binary"
echo "  ana run build-debug       # Build debug binary"
echo "  ana run pre-commit        # Run pre-commit hooks"
echo "  ana run test-integration  # Run integration tests"
echo ""
