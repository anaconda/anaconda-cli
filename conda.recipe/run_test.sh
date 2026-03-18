#!/bin/bash
set -euxo pipefail

ana

actual="$(ana)"
if [[ "$actual" != Hello,\ world!\ \(v*\) ]]; then
  echo "FAIL: Output mismatch"
  echo "  Expected: Hello, world! (v*)"
  echo "  Actual:   $actual"
  exit 1
fi
