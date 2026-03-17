#!/bin/bash
set -euxo pipefail

cargo build --release
mkdir -p "$PREFIX/bin"
cp target/release/ana "$PREFIX/bin/"
