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

# Target triples for per-platform SBOM generation
TARGETS=(x86_64-unknown-linux-gnu aarch64-apple-darwin x86_64-pc-windows-msvc)

# Generate per-target CycloneDX SBOMs (reproducible timestamp)
TARGET_FILES=()
for target in "${TARGETS[@]}"; do
    echo "==> Generating SBOM for $target"
    SOURCE_DATE_EPOCH=0 cargo cyclonedx --format json --target "$target" \
        --override-filename "ana-${target}"
    TARGET_FILES+=("ana-${target}.json")
done

# Run cargo-audit (allow non-zero exit for found vulnerabilities)
cargo audit --json > audit.raw.json 2>/dev/null || true

# Process into SBOM.json and SBOM.md (merge per-target SBOMs + audit)
python3 scripts/sbom-process.py ${FORCE_FLAG:+"$FORCE_FLAG"} \
    --audit audit.raw.json \
    --output-json SBOM.json \
    --output-md SBOM.md \
    "${TARGET_FILES[@]}"
