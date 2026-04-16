# Conda Tool

ana provides conda as a managed tool, offering a lightweight alternative to traditional Miniconda/Anaconda installations. This document explains how ana's conda integration works and how it compares to other lightweight conda distributions.

## Quick Start

```bash
# Install conda via ana
ana tool install conda

# Check conda version
conda --version

# Create an environment
conda create -n myenv python=3.12 -y

# Activate using conda-spawn (subshell-based activation)
conda shell myenv

# Exit the environment (Ctrl+D or type 'exit')
exit
```

## How It Works

### Installation

When you run `ana tool install conda`, ana:

1. Downloads and installs conda packages from a pre-solved lockfile embedded in the ana binary
2. Creates a conda environment at `~/.ana/tools/conda/`
3. Creates a wrapper symlink at `~/.ana/bin/conda` that points to ana itself
4. Configures default channels to use `repo.anaconda.com/pkgs/main`
5. Freezes the base environment to prevent accidental modifications

### The Wrapper

Unlike pixi (where the symlink points directly to the tool binary), the `conda` command is actually a symlink to ana. When you run `conda`, ana detects it's being invoked as "conda" and acts as a smart wrapper:

- **Intercepts** `activate`, `deactivate`, and `init` with helpful messages pointing to `conda shell`
- **Aliases** `conda shell` to `conda spawn` for familiar syntax
- **Filters** `conda create` output to show conda-spawn activation instructions
- **Passes through** all other commands to the real conda binary

### Environment Activation

ana uses [conda-spawn](https://github.com/conda-incubator/conda-spawn) for environment activation instead of traditional shell-based activation. This means:

- **No shell initialization required** - no `conda init`, no modifying `.bashrc`
- **Subshell-based activation** - `conda shell myenv` spawns a new shell with the environment active
- **Clean deactivation** - just exit the subshell (Ctrl+D or `exit`)

```bash
# Instead of:
conda activate myenv  # ❌ Not available

# Use:
conda shell myenv     # ✓ Spawns a subshell with myenv active
```

### Frozen Base Environment

The base conda environment is frozen using [CEP 22](https://github.com/conda/ceps/blob/main/cep-22.md) markers. This prevents accidental package installation into the managed environment:

```bash
conda install numpy  # ❌ Blocked - base is frozen
conda create -n work numpy  # ✓ Create a named environment instead
```

## Comparison with conda-express

ana's conda integration shares design philosophy with [conda-express (cx)](https://github.com/jezdez/conda-express), a lightweight conda bootstrapper by Jannis Leidel. Both projects aim to provide a minimal, fast conda experience without traditional Miniconda overhead.

### Similarities

| Feature | ana | conda-express |
|---------|-----|---------------|
| Embedded lockfile | ✓ | ✓ |
| Rattler-based installation | ✓ | ✓ |
| conda-spawn activation | ✓ | ✓ |
| `shell` command alias | ✓ | ✓ |
| Frozen base environment | ✓ | ✓ |
| No `conda init` required | ✓ | ✓ |
| Intercepts activate/deactivate | ✓ | ✓ |

### Differences

| Aspect | ana | conda-express |
|--------|-----|---------------|
| **Scope** | Multi-tool manager (conda, pixi, etc.) | Conda-only bootstrapper |
| **Binary** | Single `ana` binary manages all tools | Single `cx` binary for conda |
| **Installation** | `ana tool install conda` | Self-bootstrapping on first run |
| **Location** | `~/.ana/tools/conda/` | `~/.cx/` |
| **Solver** | Uses rattler for lockfile installation | Uses rattler, excludes libmamba |
| **Channels** | Defaults to `repo.anaconda.com/pkgs/main` | Defaults to conda-forge |
| **Updates** | `ana tool install conda` (re-lock) | Rebuild cx binary |

### When to Use Which

**Use ana if you:**
- Want a unified tool manager for multiple tools (conda, pixi, anaconda-cli)
- Prefer Anaconda's main channel as the default
- Are already using ana for other purposes

**Use conda-express if you:**
- Only need conda
- Prefer conda-forge as the default channel
- Want minimal dependencies (excludes libmamba entirely)

## Included Packages

The ana-managed conda installation includes:

- Python 3.12
- conda 26.x
- conda-libmamba-solver
- conda-spawn (for `conda shell` / `conda spawn`)
- conda-self (for environment management)
- anaconda-auth (for Anaconda.org authentication)
- anaconda-anon-usage (for anonymous usage telemetry)

## Configuration

The conda configuration is stored at `~/.ana/tools/conda/.condarc` and includes:

```yaml
# Default channels
default_channels:
  - https://repo.anaconda.com/pkgs/main
  - https://repo.anaconda.com/pkgs/r

channels:
  - defaults

# No auto-activation of base
auto_activate_base: false

# ana handles updates
notify_outdated_conda: false
```

## Troubleshooting

### "conda activate is not available"

This is expected. Use `conda shell myenv` instead:

```bash
conda shell myenv  # Spawns a subshell
# ... do work ...
exit  # Leave the environment
```

### "conda init is not needed"

ana's conda doesn't require shell initialization. Just ensure `~/.ana/bin` is in your PATH.

### "Environment is frozen"

The base environment is intentionally frozen. Create a named environment for your work:

```bash
conda create -n myenv python numpy pandas
conda shell myenv
```

## See Also

- [conda-spawn documentation](https://github.com/conda-incubator/conda-spawn)
- [conda-express](https://github.com/jezdez/conda-express)
- [CEP 22: Frozen environments](https://github.com/conda/ceps/blob/main/cep-22.md)
