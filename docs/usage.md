# Usage

## Running Tasks

Use `ana run` to execute tasks in your project. The tool is auto-detected from project files (e.g., `pixi.toml`):

```bash
# Run a task (auto-detects tool from project)
ana run test
ana run build --release

# Explicitly specify which tool to use
ana run --tool pixi test
```

## Managing Tools

Install tools that `ana` can use to run your projects:

```bash
# Install pixi
ana tool install pixi
```

Currently supported tools:

| Tool  | Description                      |
|-------|----------------------------------|
| pixi  | Fast conda/PyPI package manager  |
