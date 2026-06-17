# Anaconda MCP Registry Publishing

## Context

The official MCP Registry can list local MCP servers distributed as MCPB
packages. Anaconda MCP is a conda package today, so the usual PyPI or `uvx`
bootstrap path does not work. The practical local-install path is an MCPB
package that bundles the `ana` binary and runs:

```bash
ana mcp serve
```

That makes `anaconda-cli` part of the release chain even when the public server
users discover is Anaconda MCP.

## Ownership Model

`anaconda-mcp` owns the server implementation, public documentation, and tool behavior.

`anaconda-cli` owns the `ana` binary and the lockfile that determines which
Anaconda MCP runtime `ana mcp serve` installs. The relevant runtime pin is in
`tool-specs/anaconda-cli/pixi.toml`:

```toml
anaconda-mcp = "==<runtime-version>"
```

An MCPB registry artifact built from `ana` assets must publish the locked
Anaconda MCP runtime version, not the `ana` CLI version. For example, if
`ana v0.2.1` still locks `anaconda-mcp ==1.1.1`, the MCP Registry server
version is `1.1.1`.

## Release Sequence

Use this sequence when publishing Anaconda MCP through the official MCP Registry:

1. Release the Anaconda MCP conda package.
2. Update `tool-specs/anaconda-cli/pixi.toml` and lockfiles so
   `ana mcp serve` installs that Anaconda MCP version.
3. Release `ana` so signed platform binaries exist for the new lockfile.
4. Build the MCPB package from the released `ana` binaries.
5. Publish or update the MCP Registry entry only if the locked Anaconda MCP
   runtime version has changed.

Do not publish an MCP Registry update for an `ana`-only release when the locked
`anaconda-mcp` version is unchanged. That would either republish the same server
version or make the registry version describe the bootstrapper instead of the
MCP server.

## Registry Metadata

The server name should remain:

```json
"name": "io.github.anaconda/anaconda-mcp"
```

GitHub OIDC authentication can publish this namespace from an Anaconda-owned
repository. The package URL may point at a GitHub release asset, but the
registry `version` should continue to identify the Anaconda MCP runtime.

The package metadata should include:

- `registryType: "mcpb"`
- `fileSha256` for the generated `.mcpb`
- stdio transport
- documentation and support URLs for `anaconda-mcp`
- Anaconda privacy policy URL, because the server authenticates with Anaconda services

## Runtime UX

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

## Migration Plan

The first registry implementation may live in `anaconda-mcp` so the server
package can establish the registry entry quickly. Moving publishing here should
happen only after the team agrees on:

- where the `.mcpb` release asset should be hosted
- how to skip registry publishing for `ana` releases that do not change the
  locked Anaconda MCP runtime
- whether the MCPB install UI should require an API key or keep it optional for
  already-authenticated users

Once publishing moves here, remove the stable-tag MCP Registry publish step from
the `anaconda-mcp` release workflow so only one repository owns the registry
update.
