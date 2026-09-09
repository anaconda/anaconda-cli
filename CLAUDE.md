# Ana CLI

A command-line tool for Anaconda platform management.

## Development

### Build and test
```bash
cargo build
cargo test
```

Always run `cargo clippy` and `cargo test` and ensure both pass before pushing any changes.

### Pre-commit
The repo uses pre-commit hooks. Run `pre-commit install` after cloning.

### Error handling
Use `miette::Result<T>` for all fallible functions. For domain-specific errors that need reuse across modules, add them to `src/errors.rs` using both `thiserror::Error` and `miette::Diagnostic`:

```rust
#[derive(Error, Debug, Diagnostic)]
pub enum MyError {
    #[error("Description: {0}")]
    #[diagnostic(code(ana::module::error_name))]
    VariantName(String),
}
```

For one-off errors, use `miette!("message")`. Avoid `Box<dyn Error>`.

### CommandContext
All commands receive a `CommandContext` (`ctx`). Always access `config` and `client` through `ctx`:
- Use `ctx.config` - never call `Config::load()`
- Use `ctx.client()` - never construct a new `Client`
- For specialized HTTP clients, use `ctx.github_client()`, `ctx.download_client()`, or `ctx.unauthenticated_client()`

### Commit and PR titles
Use conventional commit format: `<type>: <description>`

Available types:
- `feat` - New features
- `fix` - Bug fixes
- `chore` - Maintenance tasks
- `refac` - Code refactoring
- `docs` - Documentation changes
- `test` - Test additions/changes
- `build` - Build system changes
- `ci` - CI/CD changes

### PR descriptions
Follow this format for PR descriptions:

```markdown
## Summary
<Brief description of changes - can be bullet points or paragraphs>

## Test plan
- [ ] <Checklist of manual testing steps>
- [ ] <Include specific commands to run>

Jira: [CLI-XXX](https://anaconda.atlassian.net/browse/CLI-XXX)
```

Notes:
- The `## Summary` section is required
- Include `## Test plan` with checkboxes for manual testing when applicable
- If there's a linked Jira ticket, add it at the bottom with the format `Jira: [CLI-XXX](url)`
- Omit the Jira line if there's no associated ticket

## Release Process

When creating a new release:

1. **GitHub Release** - Created automatically via GitHub Actions when a tag is pushed
2. **Jira Release** - Two-step process with human review:
   ```bash
   source .env  # Contains ATLASSIAN_USER_EMAIL and ATLASSIAN_API_TOKEN

   # Step 1: Generate QA notes for review
   pixi run python scripts/create_jira_release.py v0.0.9 --generate-notes > qa_notes.md

   # Step 2: Edit qa_notes.md - synthesize testing guidance from PR descriptions
   # Remove the quoted PR descriptions after writing testing notes

   # Step 3: Create Jira release with reviewed notes
   pixi run python scripts/create_jira_release.py v0.0.9 --notes-file qa_notes.md
   ```

### What the Jira release script does
- **Step 1 (--generate-notes)**: Fetches GitHub release, extracts PRs, outputs markdown with PR descriptions for review
- **Step 2 (human review)**: You synthesize testing guidance from PR descriptions, removing the quoted raw descriptions
- **Step 3 (--notes-file)**: Creates Jira release, updates Fix Versions on linked issues, creates QA Story with your reviewed notes

### Writing good QA notes
When reviewing the generated markdown:
- Focus on user-facing behavior changes
- Synthesize the PR description into clear testing steps
- Skip internal/CI changes that don't need QA testing
- Include specific commands to run and expected outcomes
- Remove the quoted `> PR description` blocks after synthesizing

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
# test trailing whitespace
