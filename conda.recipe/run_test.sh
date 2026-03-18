#!/bin/bash
set -euxo pipefail

ana

expected="Hello, world!"
actual="$(ana)"
if [ "$actual" != "$expected" ]; then
  echo "FAIL: Output mismatch"
  echo "  Expected: $expected"
  echo "  Actual:   $actual"
  exit 1
fi
