# Installation

## Quick Install

### bash

```bash
curl -fsSL https://anaconda.com/install.sh | sh
```

### PowerShell

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://anaconda.com/install.ps1 | iex"
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
    | sh -s --

ana --version
```

Or in PowerShell:

```powershell
$env:GITHUB_TOKEN=gh auth token
irm "https://api.github.com/repos/anaconda/ana-cli/contents/scripts/install.ps1" -Headers @{
    Authorization = "token $env:GITHUB_TOKEN"
    Accept = "application/vnd.github.raw"
} | iex

ana --version
```

## Options

The install script supports the following options:

| Option                  | PowerShell            | Environment Variable        | Default        | Description                     |
| ----------------------- | --------------------- | --------------------------- | -------------- | ------------------------------- |
| `-d, --install-dir DIR` | `-InstallDir`         | `ANA_INSTALL_DIR`           | `~/.local/bin` | Installation directory          |
| `-v, --version VERSION` | `-Version`            | `ANA_VERSION`               | `latest`       | Version to install              |
| `--no-verify-checksum`  | `-NoVerifyChecksum`   | `ANA_VERIFY_CHECKSUM=false` | verify         | Disable checksum validation     |
| `--no-path-update`      | `-NoPathUpdate`       | `ANA_NO_PATH_UPDATE`        | update         | Skip shell profile modification |
| `-t, --token TOKEN`     | `-Token`              | `GITHUB_TOKEN`              |                | GitHub token for private repo   |
| `-f, --force`           | `-Force`              | `ANA_FORCE_INSTALL`         | prompt         | Overwrite existing installation |
| `-h, --help`            | Use `Get-Help` cmdlet |                             |                | Show help message               |

## Examples

### bash

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

### PowerShell

```powershell
# Install a specific version
& .\install.ps1 -Version '1.0.0'

# Install to a custom directory
& .\install.ps1 --InstallDir C:\ProgramData\ana

# Install using environment variables
$env:ANA_VERSION='1.0.0'
& .\install.ps1

# Force reinstall without prompting
& .\install.ps1 -Force
```


## Supported Platforms

- macOS (Intel and Apple Silicon)
- Linux (x86_64)
- Windows (x86_64)
