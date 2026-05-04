# Releasing

This document describes the process for creating releases of the `ana` CLI.

## Overview

Releases are automated via GitHub Actions. The workflow is triggered by pushing a version tag (e.g., `v1.2.3`) and handles:

- Building binaries for all platforms (Linux x86_64, Linux aarch64, macOS, Windows)
- Generating checksums
- Uploading binaries to the GitHub release
- Publishing conda packages to Anaconda.org
- Updating the `latest` release pointer

## Creating a Release

### 1. Create a Prerelease on GitHub

1. Go to [Releases](https://github.com/anaconda/ana-cli/releases) and click **Draft a new release**
2. Click **Choose a tag** and type your new version (e.g., `v1.2.3`)
3. Select **Create new tag: v1.2.3 on publish**
4. Write your release notes
5. Check **Set as a pre-release**
6. Click **Publish release**

> [!IMPORTANT]
> You must publish as a **prerelease** (not draft) to create the tag and trigger the workflow. The workflow will automatically convert it to a full release after uploading artifacts.

### 2. Wait for the Workflow

The release workflow will:

1. Run CI builds (~20 minutes)
2. Upload binaries and checksums to your release
3. Convert the prerelease to a full release
4. Update the `latest` release to point to this version
5. Publish conda packages to Anaconda.org with the `main` label

You can monitor progress at [Actions](https://github.com/anaconda/ana-cli/actions/workflows/release.yaml).

### 3. Verify the Release

Once the workflow completes:

- Check that all 8 assets are attached (4 binaries + 4 checksums)
- Verify the release is no longer marked as a prerelease
- Confirm the `latest` release points to the new version

## Version Format

Tags must follow semantic versioning with a `v` prefix:

- `v1.0.0` - stable release
- `v1.0.0-alpha.1` - prerelease/alpha
- `v1.0.0-rc.1` - release candidate

## Automatic Prereleases

Every push to `main` automatically creates a prerelease with a dev version (e.g., `v0.0.4.dev1`). These are published to Anaconda.org with the `dev` label.

## Troubleshooting

### Release stuck as prerelease

If the workflow fails after creating the release but before converting it:

```bash
gh release edit v1.2.3 --prerelease=false
```

### Need to re-run the workflow

If artifacts weren't uploaded correctly:

1. Delete the release assets (not the release itself)
2. Re-run the failed workflow job from the Actions tab
