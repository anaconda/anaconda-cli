# QA Testing Notes for ana-cli v0.2.3

GitHub Release: https://github.com/anaconda/anaconda-cli/releases/tag/v0.2.3

## Bug Fixes

### [PR #266](https://github.com/anaconda/ana-cli/pull/266) - fix: Auto-update managed tools when lockfile changes

**Testing notes:**
- After running `ana self update`, verify that installed managed tools are automatically updated if their lockfiles changed
- Test `ana tool update` command manually updates all installed tools
- Test `ANA_AUTO_UPDATE_TOOLS=false` disables auto-update behavior
- Verify pixi tool does NOT auto-update by default (only anaconda-cli and outerbounds do)

### [PR #298](https://github.com/anaconda/ana-cli/pull/298) - fix: Clarify shell restart needed after installation

**Testing notes:**
- Run the installer and verify the post-install message mentions restarting your shell when the shell profile was modified

### [PR #304](https://github.com/anaconda/ana-cli/pull/304) - fix: Sync mcp subcommand wrappers with upstream

**Testing notes:**
- Run `ana mcp --help` and verify subcommands are: serve, clients, setup, remove, terms
- Run `ana mcp discover` and verify it shows "Unknown subcommand" error (removed upstream)
- Run `ana mcp terms --help` and verify it works (new command)

### [PR #307](https://github.com/anaconda/ana-cli/pull/307) - fix: Add typed arguments to MCP subcommand help

**Testing notes:**
- Run `ana mcp setup --help` and verify it shows typed options (--client, --name, --scope, etc.) instead of just `[ARGS]...`
- Verify `ana mcp terms status` and `ana mcp terms accept` work as subcommands

### [PR #314](https://github.com/anaconda/ana-cli/pull/314) - fix: Update main-x tests to use install instead of search

**Testing notes:**
- CI/test infrastructure change only - no user-facing testing needed

## Maintenance (no user-facing testing needed)

### [PR #313](https://github.com/anaconda/ana-cli/pull/313) - chore: Use anaconda-otel-rs from crates.io

- Internal dependency change - no user-facing testing needed

### [PR #324](https://github.com/anaconda/ana-cli/pull/324) - refac: Use owo-colors for terminal color output

**Testing notes:**
- Run `ana --help` and verify colors display correctly
- Run `ana auth status` and verify colored output appears
- Test with `NO_COLOR=1 ana --help` to verify colors are disabled
- Test in a terminal with a custom color scheme to verify user's terminal colors are respected

## Linked Jira Issues

The following issues will have their Fix Version updated:

- [CLI-623](https://anaconda.atlassian.net/browse/CLI-623)
- [CLI-715](https://anaconda.atlassian.net/browse/CLI-715)
- [CLI-724](https://anaconda.atlassian.net/browse/CLI-724)
