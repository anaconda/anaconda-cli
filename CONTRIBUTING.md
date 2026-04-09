# Contributing to ana

Thank you for your interest in contributing to ana. This document covers the basics of how to submit changes.

## Getting Started

ana is written in Rust. The project is self-hosting: `ana` manages its own development environment and build tasks.

### Prerequisites

This project depends on `anaconda/anaconda-otel-rs`, a private repository in the Anaconda GitHub org. You must have SSH access to the org configured before building.

### First-Time Setup

```bash
./scripts/bootstrap.sh
```

The bootstrap script is idempotent — you can re-run it at any time to verify or repair your setup. On a fresh clone, it performs the following steps:

1. **Rust toolchain** — Checks for a compatible Rust installation (>= 1.85.0). If Rust is installed at `~/.cargo/bin` but not on your PATH, the script detects it automatically. If Rust is missing or too old, it offers to install one via [rustup](https://rustup.rs/).
2. **Build ana** — Builds `ana` from source using `cargo build --release` and installs the binary to `~/.ana/bin/`.
3. **Development environment** — Runs `ana prepare` to create the project environment (Python, pre-commit, pytest, rattler-build, etc.) from the lockfile into `.pixi/envs/default/`.
4. **Pre-commit hook** — Installs a git pre-commit hook that runs linting and formatting checks via `ana run pre-commit`.
5. **pixi check** — Checks for a [pixi](https://pixi.sh) installation. While not required for building or running tasks, pixi is currently needed for local development workflows such as adding or removing packages from the manifest.
6. **PATH guidance** — If any tools (Rust, ana, pixi) are installed but not on your PATH, the script tells you exactly what to add to your shell profile.

> **Note:** A local Rust toolchain is required today because bootstrap builds `ana` from source. Future versions of the script will download official release binaries instead, and the Rust toolchain included in the project's conda environment (installed by `ana prepare`) will be sufficient for development.

You can also run arbitrary commands in the project environment:

```bash
ana run -- python -c "import sys; print(sys.executable)"
```

### Development Tasks

| Task                  | Description                                      |
| :-------------------- | :----------------------------------------------- |
| `build-debug`         | Build the standalone Rust binary in debug mode   |
| `build-release`       | Build the standalone Rust binary in release mode |
| `pre-commit`          | Run pre-commit hooks on all files                |
| `test`                | Run the unit tests                               |
| `test-install-script` | Run integration tests for the install script     |

Run tasks with:

```bash
ana run test
ana run build-release
```

If the environment is not yet installed, `ana run` will install it automatically on first use. If the manifest has been edited since the last lock, `ana run` will also re-lock and re-install before executing the task.

## Reporting Issues

Open a GitHub Issue for bug reports and feature requests. Include enough detail to reproduce the problem: OS, platform, ana version (`ana --version`), and the command that triggered the issue.

## Submitting Changes

1. Open a GitHub Issue before starting work. Describe the bug or proposed change and wait for confirmation that the contribution is welcome.
2. Fork the repository and create a branch from `main`.
3. Reference the issue in your PR description (e.g., `Closes #123`).
4. Keep pull requests focused. One logical change per PR.
5. Include tests for new functionality or bug fixes.
6. Ensure `ana run test` and `ana run pre-commit` pass before opening the PR.
7. Write a clear PR description that explains *what* changed and *why*.

## PR Title Format

PR titles must follow the following format. This is enforced by CI and will block merge if incorrect.

```
type: Subject starting with uppercase
type(scope): Subject starting with uppercase
```

Allowed types: `feat`, `fix`, `chore`, `refac`, `docs`, `test`, `build`, `ci`

Scopes are optional. The `deps` scope is available for dependency updates.

Examples:

```
feat: Add support for pixi installation
fix(deps): Update tokio to 1.38
docs: Clarify channel configuration behavior
chore: Remove unused build artifacts
```

The subject must start with an uppercase letter. The CI check runs on PR open, edit, and reopen, so you can fix the title without pushing new commits.

## Developer Certificate of Origin

All contributions require a DCO sign-off. This certifies that you wrote or have the right to submit the code under the project's open source license. Add the sign-off to every commit:

```bash
git commit -s -m "Your commit message"
```

This appends a `Signed-off-by` line with your name and email. Use your real name, not a pseudonym. If you forget, you can amend:

```bash
git commit --amend -s
```

The full text of the DCO is at [developercertificate.org](https://developercertificate.org/).

## Trademark Policy

"ana" and Anaconda marks are restricted to official distributions. See the project LICENSE file for details.

## Code of Conduct

This project follows the [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before participating.

## Questions

If something in this guide is unclear, open an issue. We'll improve the docs.
