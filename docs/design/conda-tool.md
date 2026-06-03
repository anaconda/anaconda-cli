# Conda Tool Design

> **Status**: Draft — this document accompanies an experimental PR and is intended for internal review and discussion.

This document describes the design and implementation of the managed conda tool in ana.

## Overview

ana provides conda as a managed tool, offering a lightweight alternative to traditional Miniconda/Anaconda installations. The design philosophy is explicitly intended to match [conda-express (cx)](https://github.com/jezdez/conda-express) by Jannis Leidel as closely as possible, providing a streamlined conda experience with subshell-based activation.

## Purpose of This Document

This document serves three goals:

1. **Document our implementation** — Capture what we built and how it works
2. **Identify gaps and changes** — Note where we diverged from conda-express or encountered issues that required workarounds
3. **Drive upstream collaboration** — Provide a basis for proposing improvements to conda-express and related projects, and explore whether we should extract a shared library crate that both ana and conda-express could use for rattler-based installation

## Installed Packages

The conda tool environment is defined in `tool-specs/conda/pixi.toml` and includes:

| Package | Purpose |
|---------|---------|
| `python >=3.12,<3.14` | Python runtime for conda |
| `conda >=26.3.2` | Core conda package manager |
| `conda-spawn >=0.1.0` | Subshell-based environment activation ([CEP 22](https://github.com/conda/ceps/blob/main/cep-22.md)) |
| `conda-self >=0.2.0` | Self-management commands for the base environment |
| `conda-anaconda-telemetry` | Telemetry integration for Anaconda |
| `conda-anaconda-tos` | Terms of service acceptance tracking |
| `conda-content-trust` | Package signature verification |
| `anaconda-anon-usage` | Anonymous usage tracking |
| `anaconda-auth` | Authentication for Anaconda services |

All packages are sourced from `https://repo.anaconda.com/pkgs/main`.

## Wrapper Architecture

### Standalone Wrapper Binary

The conda wrapper is a standalone binary compiled from `src/wrappers/conda.rs` and embedded into ana at build time. When `ana tool install conda` runs, this binary is written to `~/.ana/bin/conda` (or `conda.exe` on Windows).

This approach was chosen over symlinking `conda` to `ana` because:
- It keeps the wrapper logic self-contained and testable
- It avoids ana needing to detect how it was invoked
- It's the same approach on all platforms (no symlinks vs shims distinction for wrappers)

The wrapper binary is small (~500KB) and has no external dependencies.

```
User runs: conda create -n myenv python
           │
           ▼
    ~/.ana/bin/conda (standalone wrapper binary)
           │
           ▼
    Wrapper processes args:
           │
           ├─► Intercepts: activate, deactivate, init
           │   (prints helpful error with guidance)
           │
           ├─► Transforms: shell → spawn
           │   (conda shell myenv → conda spawn myenv)
           │
           ├─► Filters: create with -y flag
           │   (replaces activation hints in output)
           │
           └─► Passes through: all other commands
               (spawns real conda at ~/.ana/tools/conda/bin/conda)
```

### Build Process

The wrapper is compiled during `cargo build` via `build.rs`:

1. `build.rs` compiles `src/wrappers/conda.rs` with `rustc`
2. The resulting binary is placed in `OUT_DIR`
3. `install.rs` embeds it via `include_bytes!`
4. On `ana tool install conda`, the binary is written to `~/.ana/bin/conda`

For signed releases, set `CONDA_WRAPPER_PATH` to use a pre-built binary instead of compiling.

## Shell Activation with conda-spawn

Traditional conda requires shell initialization (`conda init`) to modify the shell's startup scripts. This enables `conda activate/deactivate` but has drawbacks:
- Modifies user's shell configuration
- Requires shell restart after installation
- Can conflict with other tools

### conda-spawn Approach

conda-spawn ([CEP 22](https://github.com/conda/ceps/blob/main/cep-22.md)) provides an alternative activation model:

```bash
# Traditional conda (NOT available via ana)
conda activate myenv    # Modifies current shell
conda deactivate        # Modifies current shell

# ana's approach (uses conda-spawn)
conda shell myenv       # Spawns new subshell with environment active
exit                    # Returns to original shell
```

The `conda shell` command (aliased from `conda spawn`):
1. Spawns a new shell process (bash, zsh, etc.)
2. Sets environment variables (PATH, CONDA_PREFIX, etc.)
3. Sets shell prompt to indicate active environment
4. User works within this subshell
5. Exiting (Ctrl+D or `exit`) returns to the original shell

### Wrapper Command Handling

| User Command | Wrapper Action |
|--------------|----------------|
| `conda activate myenv` | Prints error with guidance to use `conda shell myenv` |
| `conda deactivate` | Prints error with guidance to use `exit` |
| `conda init` | Prints message that init is not needed |
| `conda shell myenv` | Translates to `conda spawn myenv` and executes |
| `conda create -n myenv -y` | Executes conda, filters output to show spawn instructions |
| `conda install numpy` | Passes through to real conda |
| `conda list` | Passes through to real conda |

### Output Filtering for `conda create`

When users create environments with the `-y` flag, the wrapper filters conda's output to replace the traditional activation instructions:

```
# Original conda output:
# To activate this environment, use
#     $ conda activate myenv
# To deactivate, use
#     $ conda deactivate

# Filtered output from wrapper:
# To activate this environment, use
#     $ conda shell myenv
# To leave the environment, exit the subshell (Ctrl+D or `exit`).
```

## Post-Install Configuration

### .condarc

The wrapper writes a `.condarc` to the tool prefix with:

```yaml
default_channels:
  - https://repo.anaconda.com/pkgs/main
  - https://repo.anaconda.com/pkgs/r

channels:
  - defaults

auto_activate_base: false
notify_outdated_conda: false

self_permanent_packages:
  - anaconda-anon-usage
  - anaconda-auth
  - conda-spawn
```

### Frozen Base Environment

The wrapper writes a `conda-meta/frozen` marker file (per [CEP 22](https://github.com/conda/ceps/blob/main/cep-22.md)):

```json
{
  "message": "This environment is managed by ana.\nTo install packages, use: conda self install <package>\nTo update conda, use: conda self update\nTo override, pass --override-frozen to conda commands."
}
```

This prevents users from accidentally modifying the managed conda installation with `conda install`. They should use `conda self install` for base environment modifications.

## Environment Variables

The wrapper sets these environment variables when delegating to conda:

| Variable | Value | Purpose |
|----------|-------|---------|
| `CONDA_ROOT_PREFIX` | `~/.ana/tools/conda` | Tells conda where its root environment is |
| `PATH` | `~/.ana/bin:$PATH` | Ensures ana's wrapper takes precedence for subcommands |

## Comparison with conda-express

| Feature | ana | conda-express |
|---------|-----|---------------|
| Embedded lockfile | ✓ | ✓ |
| Rattler-based installation | ✓ | ✓ |
| conda-spawn activation | ✓ | ✓ |
| `shell` command alias | ✓ | ✓ |
| Frozen base environment | ✓ | ✓ |
| Intercepts activate/deactivate | ✓ | ✓ |
| Multi-tool manager | ✓ | ✗ |
| Default channel | Anaconda main | conda-forge |

## Findings and Gaps

This section documents differences from conda-express and issues encountered during implementation. These findings may inform upstream contributions or shared infrastructure.

### Differences from conda-express

1. **Multi-tool context** — ana manages multiple tools (pixi, anaconda-cli, conda), so the wrapper architecture needed to be generic. conda-express is purpose-built for conda only.

2. **Channel defaults** — ana defaults to Anaconda's `pkgs/main` channel while conda-express defaults to conda-forge. This is an intentional product decision, not a gap.

3. **Windows shim architecture** — ana already had a Windows shim system for other tools. We extended it with wrapper detection (`_ANA_INTERNAL_WRAPPER_INVOCATION` env var) rather than building a conda-specific solution.

### Implementation Gaps / Workarounds

> **TODO**: Fill in specific issues encountered during development. Examples might include:
> - Issues with `CONDA_ROOT_PREFIX` not being respected in certain scenarios
> - Edge cases in output filtering for `conda create`
> - Interactions with conda plugins
> - Shell detection issues with conda-spawn

### Potential Upstream Contributions

> **TODO**: Based on gaps identified above, propose specific improvements:
> - Improvements to conda-spawn
> - Improvements to conda-self
> - Documentation improvements

### Shared Library Crate Opportunity

Both ana and conda-express implement similar functionality:
- Rattler-based lockfile installation
- Post-install configuration (frozen markers, .condarc)
- Wrapper/shim architecture for command interception

**Question for discussion**: Should we extract a shared `conda-bootstrap` or `rattler-conda-install` crate that provides:
- Lockfile parsing and installation via rattler
- Frozen environment marker writing
- Common post-install hooks

This would:
- Reduce code duplication between ana and conda-express
- Provide a tested, reusable foundation for other projects
- Allow conda-express to potentially drop its own rattler integration code

**Considerations**:
- Maintenance burden of a shared crate
- API stability requirements
- Whether the implementations are similar enough to share

## Experimental Status

The conda tool is marked as experimental. When users run `ana tool install conda`, they see a warning:

```
! Conda as a managed tool is experimental.
  Uses conda-spawn for activation (conda shell <env>) instead of conda activate.
  Please report issues with ana self feedback, not to conda directly.
```

This directs users to report issues against ana rather than upstream conda, since most issues will likely be related to the wrapper behavior or ana-specific configuration.

Additionally, if any conda command exits with a non-zero status, the wrapper prints a reminder:

```
If this error is related to ana's conda integration, please report it with ana self feedback.
```

This is implemented by using `spawn()` + `wait()` instead of `exec()` on Unix, which allows the wrapper to intercept the exit code before the process terminates.

## User Experience

```bash
# Install conda (shows experimental warning)
ana tool install conda

# Create an environment
conda create -n myenv python=3.12 -y

# Activate (spawns subshell)
conda shell myenv
# Prompt changes to: (myenv) $

# Work in the environment
python --version
pip install requests

# Exit the environment
exit
# Back to original shell
```
