# Usage

## Managing Tools

Install and manage tools with `ana`:

```bash
# List available tools and their installation status
ana tool list

# Install a tool
ana tool install pixi

# Uninstall a tool
ana tool uninstall pixi
```

Currently supported tools:

| Tool         | Description                      |
|--------------|----------------------------------|
| anaconda-cli | Anaconda.org CLI                 |
| pixi         | Fast conda/PyPI package manager  |


> [!IMPORTANT]
> Using `ana` for the first time on macOS requires an internet connection
> so that Gatekeeper can look up the notarization record with Apple.
