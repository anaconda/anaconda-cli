#!/bin/bash
set -ex

mkdir -p "$PREFIX"/bin
mkdir -p "$PREFIX"/share/kilo

# Install binary
install -m 755 kilo "$PREFIX"/bin/kilo

# Install tree-sitter WASM files
if [ -d "tree-sitter" ]; then
    cp -r tree-sitter "$PREFIX"/share/kilo/
fi

# Install console assets
if [ -d "console" ]; then
    cp -r console "$PREFIX"/share/kilo/
fi
