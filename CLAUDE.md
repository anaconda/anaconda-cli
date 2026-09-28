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

PR titles must use a lowercase type and an uppercase first letter in the subject,
for example: `docs: Add Mermaid architecture documentation`.
Check [.github/workflows/lint-pr-title.yaml](.github/workflows/lint-pr-title.yaml)
for the current validation rules before creating or renaming a PR. This is a
PR-title requirement, not a reason to rewrite existing commit messages.

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

### Vulnerability resolutions

Apply this repository convention to **all vulnerability resolutions**, regardless
of severity, including dependency updates, component removal, configuration
changes, and fixes in first-party code. It supports release traceability and is
broader than the internal CRA release-note requirement for exploitable
Critical/High vulnerabilities. It is not a claim that CRA mandates a particular
commit-title format.

- Make commit and PR subjects identify the affected component and the resolution.
  Include a verified advisory identifier in the subject when practical. Use the
  normal allowed conventional-commit types; do not invent a `security` type.
- In the PR body, list all verified advisory identifiers (such as CVE, GHSA or
  RUSTSEC), link the advisory or approved tracking record, and explain the change.
  Record the fixed dependency version or planned product release when known and
  applicable; do not invent a version for removals or configuration fixes.
- Include tests, scan results or other evidence supporting the resolution, and
  state any remaining limitations. A dependency bump alone is not proof that every
  affected build variant or vulnerability is resolved.
- If no public advisory identifier exists, say so and use an approved tracking
  reference. Do not fabricate identifiers, severity, exploitability or clearance.
- Preserve the security-update information in the release notes for the release
  that actually ships the resolution: product/component name, release version,
  date, resolution and approved advisory references. Verify the final notes rather
  than assuming a commit or PR body will appear in generated notes. Do not silently
  exclude a vulnerability resolution with `ignore-for-release`.
- Follow Security's coordinated-disclosure instructions. Do not publish embargoed
  identifiers or vulnerability details in public commits, PRs or release notes
  without approval. Keep the full traceability record in the approved restricted
  location until disclosure is authorized, and use approved public wording.
- Record false-positive or non-exploitable findings as triage decisions with their
  rationale, not as vulnerabilities fixed by a change that did not resolve them.

Example subject pattern (replace placeholders with verified, approved details):

```text
fix: Update <dependency> to <fixed-version> to address <advisory-id>
```

Internal references: [Release Notes aka Change Log](https://anaconda.atlassian.net/wiki/spaces/TEC/pages/6517817373)
and [Software Version Scheme](https://anaconda.atlassian.net/wiki/spaces/TEC/pages/6525943809).

## Architecture Documentation

Keep [docs/architecture.md](docs/architecture.md) current in the same pull request
as changes that affect the architecture.

- Update the relevant Mermaid diagrams and prose when changing runtime components,
  authentication, MCP integrations, local storage, telemetry, or external services.
- Update distribution tables and flows when standalone and conda capabilities,
  installation, or update behavior change.
- Update build, signing, publication, and hosting diagrams when their workflows or
  trust boundaries change. Keep source links and reviewed-revision notes accurate.
- Check that diagrams match the implementation and that links resolve. Verify
  Mermaid rendering when possible; report when rendering was not checked.
- Distinguish implemented behavior from planned changes and externally unverified
  controls. Documentation updates do not imply release or security approval.

Changes with no architectural impact do not require an architecture-document edit.

## Release Process

When creating a new release:

1. **GitHub Release** - Publishing a GitHub release triggers the [release workflow](.github/workflows/release.yaml); pushing a tag alone does not. Review the workflow and obtain release approval before publishing.
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
