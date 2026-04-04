#!/usr/bin/env bash
# Regenerate Cargo.lock (if Cargo.toml changed) and update the SBOM.
# Used as a pre-commit hook and in CI.
# Usage: update_lockfiles.sh [--force]
set -euo pipefail

FORCE_FLAG="${1:-}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# Regenerate Cargo.lock if Cargo.toml is newer or lock is missing
if [ ! -f Cargo.lock ] || [ Cargo.toml -nt Cargo.lock ]; then
    echo "==> Regenerating Cargo.lock"
    cargo generate-lockfile
fi

# Generate raw CycloneDX SBOM
cargo cyclonedx --format json

# Run cargo-audit (allow non-zero exit for found vulnerabilities)
cargo audit --json > audit.raw.json 2>/dev/null || true

# Process into SBOM.json and SBOM.md
python3 scripts/sbom-process.py $FORCE_FLAG \
    ana.cdx.json audit.raw.json SBOM.json SBOM.md
