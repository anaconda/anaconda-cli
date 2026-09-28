# Agent Guidance

Read [CLAUDE.md](CLAUDE.md) for the project's development, testing, error-handling,
commit, and pull-request conventions.

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
