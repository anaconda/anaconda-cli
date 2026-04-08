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

# Generate per-target CycloneDX SBOMs
TARGET_FILES=()
for target in "${TARGETS[@]}"; do
    echo "==> Generating SBOM for $target"
    cargo cyclonedx --format json --target "$target" \
        --override-filename "ana-${target}"
    TARGET_FILES+=("ana-${target}.json")
done

# Run cargo-audit: exit code 1 means "vulnerabilities found" (expected),
# but any other non-zero exit indicates an actual failure (missing binary,
# corrupted advisory DB, etc.) that should not be silently swallowed.
cargo audit --json > audit.raw.json 2>audit.stderr.log || {
    rc=$?
    if [ $rc -ne 1 ]; then
        cat audit.stderr.log >&2
        exit $rc
    fi
}

# Process into SBOM.json and SBOM.md (merge per-target SBOMs + audit)
python3 scripts/sbom-process.py ${FORCE_FLAG:+"$FORCE_FLAG"} \
    --audit audit.raw.json \
    --output-json SBOM.json \
    --output-md SBOM.md \
    "${TARGET_FILES[@]}"

# Clean up intermediate files
rm -f audit.raw.json audit.stderr.log ana.cdx.json "${TARGET_FILES[@]}"
