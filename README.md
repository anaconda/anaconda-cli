# Anaconda CLI

The command-line interface for the Anaconda platform.

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

### Configuring pixi for Anaconda channels

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

### Deploying ML workflows to Outerbounds (experimental)

An experimental integration with the Outerbounds platform is available for production ML workflows. See the [Outerbounds integration guide](docs/outerbounds.md) for setup and usage.

```bash
ana feature enable outerbounds
ana ob configure --instance your-org.outerbounds.com
ana ob init my-pipeline --name recommendation-engine
ana ob deploy
```

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

### Feature flags

Experimental features are stored in `~/.ana/config.toml`:

```toml
[ana.features]
outerbounds = true
```

## Command reference

### Authentication

```bash
ana login                        # Interactive login (browser-based device flow)
ana login --api-key "your-key"   # Login with API key directly
echo "$KEY" | ana login --api-key  # Login with API key from stdin
ana whoami                       # View account info
ana whoami --json                # Account info as JSON
ana logout                       # Log out
```

### Tool management

```bash
ana tool list                    # List available tools
ana tool install <tool>          # Install a tool
ana tool uninstall <tool>        # Uninstall a tool
```

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

Verify your API key at [anaconda.cloud](https://anaconda.cloud). Keys are environment-specific (production vs. staging) and won't work cross-environment.

### SSL certificate errors

If you're behind a corporate proxy or using custom CA certificates:

```bash
export ANA_SSL_VERIFY=false
ana login
```

Only disable SSL verification in trusted network environments.

### Verbose logging

```bash
ana -vvv login                   # Up to 5 verbosity levels
RUST_LOG=ana=debug ana login      # Fine-grained control via RUST_LOG
```

## Documentation

- [Official Docs](https://www.anaconda.com/docs/cli-reference/ana/getting-started)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Contributing guide](CONTRIBUTING.md)

## License

Anaconda CLI is licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.
