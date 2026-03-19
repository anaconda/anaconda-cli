#!/bin/bash
set -euxo pipefail

ana

actual=$(ana --version)
if [ "$actual" != "$PKG_VERSION" ]; then
  echo "FAIL: Version mismatch"
  echo "  Expected: $PKG_VERSION"
  echo "  Actual:   $actual"
  exit 1
fi
