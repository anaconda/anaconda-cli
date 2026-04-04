.PHONY: help version build debug release test test-release test-integration pre-commit conda cargo-lock lockfiles lockfiles-force

help:  ## Display help on all Makefile targets
	@@grep -h '^[a-zA-Z]' $(MAKEFILE_LIST) | awk -F ':.*?## ' 'NF==2 {printf "   %-20s%s\n", $$1, $$2}' | sort

version:  ## Derive version from git tags
	pixi run get-version

build: release  ## Build the release binary (alias for release)

debug:  ## Build the debug binary
	pixi run build-debug

release:  ## Build the release binary
	pixi run build-release

test:  ## Run the unit tests
	pixi run test

test-release:  ## Run the unit tests in release mode
	pixi run test-release

test-integration:  ## Run CLI integration tests
	pixi run test-integration

pre-commit:  ## Run pre-commit hooks on all files
	pixi run pre-commit

conda:  ## Build the conda package
	pixi run build-conda

cargo-lock:  ## Regenerate Cargo lockfile
	pixi run cargo-lock

lockfiles:  ## Regenerate Cargo.lock (if needed) and update SBOM
	pixi run lockfiles

lockfiles-force:  ## Regenerate Cargo.lock and SBOM unconditionally
	pixi run lockfiles-force
