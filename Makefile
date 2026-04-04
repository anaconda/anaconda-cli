.PHONY: help debug pre-commit test test-integration lockfiles

help:  ## Display help on all Makefile targets
	@@grep -h '^[a-zA-Z]' $(MAKEFILE_LIST) | awk -F ':.*?## ' 'NF==2 {printf "   %-20s%s\n", $$1, $$2}' | sort

debug:  ## Build the debug binary
	pixi run build-debug

pre-commit:  ## Run pre-commit hooks on all files
	pixi run pre-commit

test:  ## Run all the unit tests
	pixi run test

test-integration:  ## Run CLI integration tests
	pixi run test-integration

lockfiles:  ## Regenerate all lockfiles
	./lockfiles/lock-all.sh
