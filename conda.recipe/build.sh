#!/bin/bash
set -euo pipefail

# Disable telemetry and suppress tracing output so otel ERROR messages
# don't pollute stdout and break version/output assertions.
export ANA_ENABLE_TELEMETRY=false
export RUST_LOG=off

BINFILE=$(dirname "$RECIPE_DIR")/target/release/ana
echo "Binary path: $BINFILE"
if [ ! -x "$BINFILE" ]; then
  echo "FAIL: Release binary not found"
  exit 1
fi

actual=$($BINFILE --version | head -1)
echo "Version: $actual"
if [ "$actual" != "$PKG_VERSION" ]; then
  echo "FAIL: Expected $PKG_VERSION"
  exit 1
fi

mkdir -p "$PREFIX/bin"
cp "$BINFILE" "$PREFIX/bin/"
