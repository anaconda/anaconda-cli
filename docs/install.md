# Installation

## Quick Install

```bash
curl -fsSL https://anaconda.com/install.sh | sh
```

> [!NOTE]
> The public URL is not yet available. See [Private Repo Access](#private-repo-access) for current installation method.

## Private Repo Access

While the repository is private, you need a GitHub token to download releases. If you have the [`gh` CLI](https://cli.github.com/) installed:

```bash
export GITHUB_TOKEN=$(gh auth token)

curl -fsSL \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.raw" \
    "https://api.github.com/repos/anaconda/ana-cli/contents/scripts/install.sh" \
    | sh -s -- --token $GITHUB_TOKEN

ana --version
```

## Options

The install script supports the following options:

| Option                   | Environment Variable        | Default        | Description                      |
|--------------------------|-----------------------------|----------------|----------------------------------|
| `-d, --install-dir DIR`  | `ANA_INSTALL_DIR`           | `~/.local/bin` | Installation directory           |
| `-v, --version VERSION`  | `ANA_VERSION`               | `latest`       | Version to install               |
| `--no-verify-checksum`   | `ANA_VERIFY_CHECKSUM=false` | verify         | Disable checksum validation      |
| `--no-path-update`       | `ANA_NO_PATH_UPDATE`        | update         | Skip shell profile modification  |
| `-t, --token TOKEN`      | `GITHUB_TOKEN`              |                | GitHub token for private repo    |
| `-f, --force`            | `ANA_FORCE_INSTALL`         | prompt         | Overwrite existing installation  |
| `-h, --help`             |                             |                | Show help message                |

## Examples

```bash
# Install a specific version
./install.sh --version 1.0.0

# Install to a custom directory
./install.sh --install-dir /usr/local/bin

# Install using environment variables
ANA_VERSION=1.0.0 ./install.sh

# Force reinstall without prompting
./install.sh --force
```

## Supported Platforms

- macOS (Intel and Apple Silicon)
- Linux (x86_64)

Windows support via PowerShell is planned for a future release.
