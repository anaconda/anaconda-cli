# Anaconda CLI

The command-line interface for the Anaconda platform.

## Table of contents

- [When to use this](#when-to-use-this)
- [Project status](#project-status)
- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Quick start](#quick-start)
- [Common workflows](#common-workflows)
- [Configuration](#configuration)
- [Command reference](#command-reference)
- [Troubleshooting](#troubleshooting)
- [Documentation](#documentation)
- [License](#license)

## When to use this

The Anaconda CLI is the entry point for the Anaconda ecosystem. It handles authentication to Anaconda services, configures tools like conda and pixi, and manages access to package channels, so you don't have to manually edit config files or manage credentials across tools separately.

If you're building with Anaconda's data science, ML, or AI tooling, `ana` gets you from zero to a working environment in minutes.

## Project status

Anaconda CLI is under active development and follows semantic versioning. Commands not marked as experimental are considered stable. Features behind `ana feature enable` may change between releases.

## Prerequisites

- macOS (Apple Silicon), Linux (x86_64 or aarch64), or Windows (x86_64)
- [conda](https://www.anaconda.com/download) installed if you plan to create conda environments
- Internet connection required on first run on macOS (Gatekeeper notarization check)

## Installation

### macOS and Linux

```bash
curl -fsSL https://anaconda.sh/install.sh | sh
```

### Windows (PowerShell)

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://anaconda.sh/install.ps1 | iex"
```

### Installation options

| Bash | PowerShell | Environment variable | Default | Description |
|------|------------|---------------------|---------|-------------|
| `-d, --install-dir` | `-InstallDir` | `ANA_INSTALL_DIR` | `~/.local/bin` | Installation directory |
| `-v, --version` | `-Version` | `ANA_VERSION` | `latest` | Version to install |
| `--no-verify-checksum` | `-NoVerifyChecksum` | `ANA_VERIFY_CHECKSUM` | `true` | Set to `false` to skip checksum validation |
| `--no-path-update` | `-NoPathUpdate` | `ANA_NO_PATH_UPDATE` | | Set to skip shell profile modification |
| `-f, --force` | `-Force` | `ANA_FORCE_INSTALL` | | Set to overwrite without prompting |

## Quick start

```bash
# 1. Verify installation
ana --version

# 2. Log in to your Anaconda account (opens browser if available)
ana login

# 3. Enable early-access packages from the main-x channel
ana feature enable main-x

# 4. Create your first environment
conda create -n myproject python=3.12 numpy pandas jupyter
conda activate myproject
```

## Common workflows

### Configuring conda and pixi for Anaconda channels

The CLI can configure both conda and pixi to use Anaconda channels. Install pixi separately if you prefer it over conda, then point it at main-x:

```bash
ana feature enable main-x
```

### CI/CD authentication

For non-interactive environments, authenticate with an API key via stdin:

```bash
echo "$ANACONDA_API_KEY" | ana login --api-key
```

### Integrating AI assistants with your environment

`ana` provides MCP (Model Context Protocol) support so AI assistants like Claude can discover and work with packages in your conda environments:

```bash
ana mcp setup
ana mcp discover
```

See `ana mcp --help` for the full list of subcommands.

### Deploying ML workflows to Outerbounds

`ana platform` wraps the [Outerbounds](https://outerbounds.com) CLI. All arguments are passed straight through to `outerbounds`.

```bash
ana platform configure <token>
ana platform app list
```

See `ana platform --help` for the full list of subcommands.

## Configuration

View current settings with `ana config`.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ANA_DOMAIN` | `anaconda.com` | Authentication domain |
| `ANA_SSL_VERIFY` | `true` | Verify SSL certificates |
| `ANA_OPEN_BROWSER` | `true` | Auto-open browser during login |
| `ANA_ENABLE_TELEMETRY` | `true` | Send anonymous usage telemetry (see [telemetry policy](docs/telemetry.md)) |
| `ANA_PRERELEASES` | `false` | Include pre-releases in update checks |
| `ANA_KEYRING_PATH` | `~/.anaconda/keyring` | API key storage location |

Boolean values accept `true`/`false`, `1`/`0`, or empty string (treated as false).

## Command reference

### Authentication

```bash
ana login                          # Interactive login (browser-based device flow)
ana login --api-key "your-key"     # Login with API key directly
echo "$KEY" | ana login --api-key  # Login with API key from stdin
ana whoami                         # View account info
ana whoami --json                  # Account info as JSON
ana logout                         # Log out
```

### Tool management

```bash
ana tool list                    # List available tools
ana tool install <tool>          # Install a tool
ana tool update [<tool>]         # Update installed tools, or just one
ana tool uninstall <tool>        # Uninstall a tool
```

Most tools are pinned by lockfiles that ship with `ana`, so they update when `ana` does. `kilo-cli` is resolved from its channel at install time instead, so `ana tool update` picks up new kilo-cli releases without updating `ana`.

Installing `kilo-cli` makes `kilo` the entrypoint for both CLIs: kilo's own commands work as usual, and ana's commands are available under `kilo ana`:

```bash
kilo run "fix the failing test"  # kilo command
kilo ana tool update             # ana command, same as `ana tool update`
```

When a newer kilo-cli is published, `kilo` asks whether to update before running a command. It checks at most once a day and only in an interactive terminal; set `ANA_UPDATE_CHECK=0` to turn it off. To install a specific (e.g. older) kilo-cli release, use `ana tool install kilo-cli --version <version>`.

### MCP marketplace

`ana marketplace` installs an MCP server from a conda package and adds it to kilo in one step. The package is solved with rattler into its own environment under `~/.ana/marketplace/<name>`, and its server executable is registered as a local MCP server in kilo's config:

```bash
kilo ana marketplace list                       # catalog and what's installed
kilo ana marketplace install mcp-server-fetch   # install and register with kilo
kilo ana marketplace remove mcp-server-fetch    # unregister and uninstall
```

`marketplace list` shows a curated catalog of conda-forge MCP servers: `conda-meta-mcp`, `excel-mcp-server`, `markitdown-mcp-server`, `mcp-atlassian`, `mcp-grafana`, and `mcp-server-fetch`. Catalog packages install with no extra options. Other conda packages work too: they are solved against `main` and `conda-forge` by default; use `--channel` to choose others and `--command` when a package installs several executables. Use `--env KEY=VALUE` to pass settings such as credentials to the server.

### Package features

```bash
ana feature enable main-x        # Enable early-access packages (conda and pixi)
ana feature enable main-x --pixi # Enable for pixi only
ana feature disable main-x       # Disable a feature
```

### Self-management

```bash
ana self update                  # Update to latest version
ana self update v1.0.0           # Update to specific version
ana self update --check          # Check for updates without installing
ana self update --list           # List available versions
```

### MCP (Model Context Protocol)

```bash
ana mcp setup                    # Configure AI clients for Anaconda MCP
ana mcp discover                 # Discover MCP servers from environments
ana mcp clients                  # List supported AI clients
ana mcp serve                    # Start MCP servers
ana mcp remove                   # Remove MCP configuration
```

### API access

```bash
ana api fetch /api/account                                        # GET request
ana api fetch /api/endpoint --method POST --json '{"key": "val"}' # POST with JSON
ana api fetch /api/search -q "name=numpy,version=1.24"            # Query parameters
```

### Anaconda CLI (classic)

If you need the classic `anaconda` command (for uploading packages to anaconda.org, etc.):

```bash
ana bootstrap
```

This installs the `anaconda` command, which you can then use directly:

```bash
anaconda upload my-package.tar.bz2
anaconda search numpy
```

## Troubleshooting

### "Command not found" after installation

The installer adds `~/.local/bin` to your PATH via your shell profile. Restart your shell, or run:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Login fails with "invalid API key"

Verify your API key at [anaconda.com](https://anaconda.com/app/profile/api-keys).
Keys are environment-specific (anaconda.com vs. self-hosted) and won't work cross-environment.

### SSL certificate errors

On Linux, point `SSL_CERT_FILE` to a CA bundle containing your trusted
certificates, including the CA used by your corporate proxy:

```bash
export SSL_CERT_FILE=/path/to/ca-bundle.pem
ana login
```

Linux clients reject the compromised eMudhra intermediate keys even when the
selected CA bundle still trusts their issuers.

### Verbose logging

```bash
ana -vvv login                   # Up to 5 verbosity levels
RUST_LOG=ana=debug ana login      # Fine-grained control via RUST_LOG
```

## Documentation

- [Architecture and diagrams](docs/architecture.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Contributing guide](CONTRIBUTING.md)

## License

Anaconda CLI is licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.
