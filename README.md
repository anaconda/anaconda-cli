# ana-cli
A Single Entry Point for Data Science, ML, & AI Powered Applications

## Development

### Prerequisites

Install [pixi](https://pixi.sh):

```bash
curl -fsSL https://pixi.sh/install.sh | bash
```

### Available Tasks

| Task            | Description                                      |
|-----------------|--------------------------------------------------|
| `build-conda`   | Build the conda package                          |
| `build-debug`   | Build the standalone Rust binary in debug mode   |
| `build-release` | Build the standalone Rust binary in release mode |
| `pre-commit`    | Run pre-commit hooks on all files                |
| `test`          | Run the unit tests                               |

Run a task with:

```bash
pixi run <task>
```

For example:

```bash
pixi run build-debug
```
