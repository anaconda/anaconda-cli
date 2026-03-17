# ana

The next-generation Anaconda CLI for managing data science and ML development toolchains with secure-by-default principles.

```
curl -fsSL https://anaconda.sh/install.sh | bash
```

No prior Python or conda installation required.

## What it does

ana installs, configures, and manages the tools you use for data science and ML work — conda, pixi, uv, pip, Jupyter, and Anaconda Desktop — from a single interface with opinionated, secure defaults.

```bash
ana install conda        # installs conda; disables base auto-activation
ana install jupyter      # installs jupyter in an isolated environment
ana install uv           # installs uv
ana update all           # updates all managed tools
ana configure all        # applies consistent configuration across tools
ana tools                # shows what's installed
```

When you install conda via ana, base environment auto-activation is disabled. When you install Jupyter, it runs in a dedicated, isolated environment. These defaults are applied automatically so you don't need to find them and set yourself.

`ana login` connects your local environment to the Anaconda platform for authenticated access to package repositories, channel & environment management, and the Anaconda Platform's security and governance capabilities, such as the Anaconda dependency firewall that filters packages based on vulnerability data.

## Command reference

| Command | Description |
| --- | --- |
| `ana install [tool]` | Install a managed tool (conda, pixi, uv, pip, jupyter, desktop) |
| `ana update [tool\|all]` | Update one or all installed tools |
| `ana configure [tool\|all]` | Configure one or all installed tools |
| `ana uninstall [tool]` | Remove an installed tool |
| `ana tools` | List installed tools |
| `ana jupyter` | Launch Jupyter (prompts for install if not present) |
| `ana login` / `ana logout` | Authenticate with or disconnect from the Anaconda platform |
| `ana whoami` | Display the current authenticated user |
| `ana workspace [subcommand]` | Create and manage workspaces |

## License

Apache 2.0. See [LICENSE](LICENSE) for details.

Contributions require a [Developer Certificate of Origin](https://developercertificate.org/) (DCO) sign-off. The "ana" name and Anaconda marks are subject to [trademark policy](TRADEMARKS.md).
