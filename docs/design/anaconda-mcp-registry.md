# Anaconda MCP Registry Bootstrap Proposal

## Why Anaconda MCP Is Asking

This is a proposal for how `anaconda-cli` can support Anaconda MCP registry
distribution. It does not assume this repository owns that workflow today.

Anaconda MCP needs a registry install path that works from common MCP clients.
That is harder than it is for most Python MCP servers:

- Anaconda MCP is a conda package today, not a PyPI package.
- The usual `uvx <package>` bootstrap path does not work for conda-only
  dependencies.
- Asking a registry or MCP client to run arbitrary `conda create` /
  `conda run` commands requires hardcoding local conda paths and environment
  names.

`ana` is the cleanest bootstrapper we have because it is a standalone binary
that already knows how to install and run the managed Anaconda MCP runtime:

```bash
ana mcp serve
```

The Anaconda MCP PR can bundle released `ana` binaries in an MCPB package, but
that makes Anaconda MCP depend on a small release contract from this repository.

## What Anaconda MCP Needs From `ana`

The minimum useful contract is:

- keep `ana mcp serve` as the stable command for starting Anaconda MCP
- publish fresh signed release assets with stable names for macOS, Linux, and
  Windows
- keep the Anaconda MCP runtime pin explicit in
  `tool-specs/anaconda-cli/pixi.toml`
- release a new `ana` build when Anaconda MCP needs registry distribution for a
  newer runtime version
- make it easy to tell which Anaconda MCP runtime a given `ana` release installs

```toml
anaconda-mcp = "==<runtime-version>"
```

The recent `v0.2.1` release is the kind of thing the registry path depends on:
it rebuilt `ana` assets after a release-cache issue in `v0.2.0`. From Anaconda
MCP's perspective, that means the MCPB pin should move to `v0.2.1` even though
the locked Anaconda MCP runtime remains `1.1.1`.

## Version Invariant

The MCP Registry server version should describe the Anaconda MCP runtime, not
the `ana` bootstrapper version.

For example:

- `ana v0.2.1` locks `anaconda-mcp ==1.1.1`
- the MCP Registry server version is therefore `1.1.1`
- the MCPB artifact can still record that it was built from `ana v0.2.1`

This distinction matters because `ana` can have patch releases that do not
change the Anaconda MCP runtime. Those releases should not automatically publish
a new MCP Registry server version.

## Proposed Release Sequence

One workable sequence is:

1. Release the Anaconda MCP conda package.
2. Update `tool-specs/anaconda-cli/pixi.toml` and lockfiles so
   `ana mcp serve` installs that Anaconda MCP version.
3. Release `ana` so signed platform binaries exist for the new lockfile.
4. Build the MCPB package for Anaconda MCP from the released `ana` binaries.
5. Publish or update the MCP Registry entry only if the locked Anaconda MCP
   runtime version has changed.

The first implementation can continue to live in `anaconda-mcp` while the team
decides where this responsibility belongs. Moving the MCPB build or registry
publish here is useful only if this repository wants to own the release
coordination around the `ana` runtime lock.

## Registry Shape

The public registry entry should still look like Anaconda MCP, not Anaconda CLI:

- server name: `io.github.anaconda/anaconda-mcp`
- documentation and support URLs: Anaconda MCP
- package type: `registryType: "mcpb"`
- transport: stdio
- package contents: a small launcher plus released `ana` binaries

The open question is only where the `.mcpb` release asset and publish workflow
should live once the CLI team is comfortable with the dependency chain.

## Auth And Startup UX

The MCPB startup path is non-interactive. It should not rely on prompting for
login while the MCP client is trying to start the server.

Supported auth paths are:

- install-time MCPB user configuration for `ANACONDA_AUTH_API_KEY`
- existing credentials from `ana login` or `anaconda login`

Accepted Anaconda MCP Beta Terms can be supplied by MCPB user configuration, or
accepted outside the server with:

```bash
anaconda mcp terms accept
```

## Open Questions For CLI Maintainers

- Is `ana mcp serve` the right long-term command for registry-installed MCPB
  packages to call?
- Should `ana` expose a machine-readable way to report the locked Anaconda MCP
  runtime version?
- Should the MCPB artifact be hosted on `anaconda-mcp` releases, on
  `anaconda-cli` releases, or copied between them during release automation?
- Should the registry publish workflow live in `anaconda-mcp` and consume
  released `ana` assets, or eventually move here because this repository owns
  the runtime lock?
- What should the non-interactive auth story be for users who install from an
  MCP client UI rather than from a terminal?

## Suggested Next Step

For now, Anaconda MCP can keep the initial registry workflow in its own repo and
pin a known-good `ana` release. This document is here so the CLI maintainers can
review the proposed contract before we move any workflow ownership into
`anaconda-cli`.
