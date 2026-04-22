# Ana CLI

A command-line tool for Anaconda platform management.

## Development

### Build and test
```bash
cargo build
cargo test
```

### Pre-commit
The repo uses pre-commit hooks. Run `pre-commit install` after cloning.

## Release Process

When creating a new release:

1. **GitHub Release** - Created automatically via GitHub Actions when a tag is pushed
2. **Jira Release** - Run the script to sync with Jira:
   ```bash
   source .env  # Contains ATLASSIAN_USER_EMAIL and ATLASSIAN_API_TOKEN
   pixi run python scripts/create_jira_release.py v0.0.9
   ```

### What the Jira release script does
- Creates a Jira release named `ana-cli vX.Y.Z` in the CLI project
- Links to the GitHub release in the description
- Finds Jira issue references (e.g., `CLI-123`) in PR descriptions and sets their Fix Version
- Creates a QA Story with testing notes, including:
  - Links to each PR (excluding Renovate/bot PRs)
  - Grouped by type (Features, Bug Fixes, Maintenance)

### Manual Jira release (if needed)
If you need to create a Jira release manually or via Claude:
- **Project**: CLI (ID: 11160)
- **Naming convention**: `ana-cli vX.Y.Z`
- **Description**: Include link to GitHub release
- **QA Story**: Create a Story issue type with testing notes, linked to the release via Fix Version
- **PR links**: When mentioning PRs in testing notes, always include clickable links to GitHub

### Environment setup for Jira API
Create a `.env` file (gitignored) with:
```
ATLASSIAN_USER_EMAIL=your-email@anaconda.com
ATLASSIAN_API_TOKEN=your-token-from-atlassian
```
Get your API token at: https://id.atlassian.com/manage-profile/security/api-tokens
