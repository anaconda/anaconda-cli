# Conda Channel URL Patterns

This document describes the URL patterns conda uses when fetching package metadata from various channel types.

See also [CEP 26 - Identifying Packages and Channels in the conda Ecosystem](https://github.com/conda/ceps/blob/main/cep-0026.md) for the formal specification of channel URLs, names, labels, and subdirs.

## How Conda Resolves Channel Names to URLs

Conda uses several configuration settings to resolve channel names to full URLs:

### channel_alias

The base URL prepended to channel names that aren't full URLs.

```
channel_alias: https://conda.anaconda.org
```

When you specify `-c conda-forge`, conda resolves it to `https://conda.anaconda.org/conda-forge`.

### default_channels

The list of channel URLs that comprise the `defaults` multichannel.

```
default_channels:
  - https://repo.anaconda.com/pkgs/main
  - https://repo.anaconda.com/pkgs/r
```

When you specify `-c defaults` (or use the default config), conda expands it to these channels.

### custom_channels

A map that overrides `channel_alias` for specific channel names. The channel name is the key, and the value is the base URL (channel name is appended).

```
custom_channels:
  pkgs/pro: https://repo.anaconda.com
```

This makes `-c pkgs/pro` resolve to `https://repo.anaconda.com/pkgs/pro` instead of `https://conda.anaconda.org/pkgs/pro`.

### custom_multichannels

Define custom multichannels (like `defaults`) that expand to multiple channels.

```
custom_multichannels:
  my-stack:
    - conda-forge
    - https://my-server.com/channel
```

Using `-c my-stack` would query both channels.

### Resolution Order

1. If the channel is a full URL (starts with `http://`, `https://`, or `file://`), use it directly
2. If the channel matches a key in `custom_multichannels`, expand to those channels
3. If the channel is `defaults`, expand using `default_channels`
4. If the channel matches a key in `custom_channels`, use that base URL
5. Otherwise, prepend `channel_alias` to the channel name

## Test Environment

| Property | Value |
|----------|-------|
| conda version | 26.5.0 |
| Python version | 3.13.13 |
| Platform | osx-arm64 |
| Root prefix | `/Users/mattkram/miniconda3` |

**Configured channels:**
```
- defaults
- https://repo.anaconda.cloud/repo/main-x
```

## Quick summary of results

| Command | URLs Accessed |
|---------|---------------|
| `conda install -c conda-forge numpy` | `https://conda.anaconda.org/t/{token}/conda-forge/repodata_shards.msgpack.zst/osx-arm64` <br> `https://conda.anaconda.org/t/{token}/conda-forge/repodata_shards.msgpack.zst/noarch` |
| `conda install -c defaults numpy` | `https://repo.anaconda.com/pkgs/main/osx-arm64/repodata.json.zst` <br> `https://repo.anaconda.com/pkgs/main/noarch/repodata.json.zst` <br> `https://repo.anaconda.com/pkgs/r/osx-arm64/repodata.json.zst` <br> `https://repo.anaconda.com/pkgs/r/noarch/repodata.json.zst` |
| `conda install -c main numpy` | `https://conda.anaconda.org/main/terms.json` <br> `https://conda.anaconda.org/t/{token}/main/osx-arm64/repodata.json.zst` <br> `https://conda.anaconda.org/t/{token}/main/noarch/repodata.json.zst` |
| `conda install -c anaconda-cloud numpy` | `https://conda.anaconda.org/t/{token}/anaconda-cloud/osx-arm64/repodata.json.zst` <br> `https://conda.anaconda.org/t/{token}/anaconda-cloud/noarch/repodata.json.zst` |
| `conda install -c mattkram numpy` | `https://conda.anaconda.org/mattkram/terms.json` <br> `https://conda.anaconda.org/t/{token}/mattkram/osx-arm64/repodata.json.zst` <br> `https://conda.anaconda.org/t/{token}/mattkram/noarch/repodata.json.zst` |
| `conda install -c anaconda-cloud/label/dev numpy` | `https://conda.anaconda.org/anaconda-cloud/label/dev/terms.json` <br> `https://conda.anaconda.org/t/{token}/anaconda-cloud/label/dev/repodata_shards.msgpack.zst/osx-arm64` <br> `https://conda.anaconda.org/t/{token}/anaconda-cloud/label/dev/repodata_shards.msgpack.zst/noarch` |
| `conda install numpy` (with repo.anaconda.cloud channel) | `https://repo.anaconda.cloud/repo/main-x/terms.json` <br> `https://repo.anaconda.com/pkgs/main/osx-arm64/repodata.json.zst` <br> `https://repo.anaconda.com/pkgs/r/osx-arm64/repodata.json.zst` <br> `https://repo.anaconda.cloud/repo/main-x/osx-arm64/repodata.json.zst` |

## Hosts

Conda uses three distinct hosts depending on the channel:

| Host | Purpose | Authentication |
|------|---------|----------------|
| `conda.anaconda.org` | Named channels (conda-forge, user channels, etc.) | Token in URL path (`/t/{token}/...`) |
| `repo.anaconda.com` | `defaults` channel | Public, no auth required |
| `repo.anaconda.cloud` | Explicit URL channels | Token via `anaconda login` |

## Channel URL Patterns

### conda-forge

```
GET /t/{token}/conda-forge/repodata_shards.msgpack.zst/{platform}
```

Uses sharded repodata format for faster incremental updates.

### defaults

```
GET /pkgs/main/{platform}/repodata.json.zst
GET /pkgs/main/noarch/repodata.json.zst
GET /pkgs/r/{platform}/repodata.json.zst
GET /pkgs/r/noarch/repodata.json.zst
```

The `defaults` channel expands to `pkgs/main` + `pkgs/r` on `repo.anaconda.com`. No authentication required.

### main (on anaconda.org)

```
GET /{channel}/terms.json                                        # Terms check (unauthenticated)
GET /t/{token}/main/repodata_shards.msgpack.zst/{platform}       # Try shards first
GET /t/{token}/main/{platform}/repodata.json.zst                 # Fall back to full repodata
```

### User/Organization Channels (e.g., mattkram)

```
GET /{channel}/terms.json                                        # Terms check (unauthenticated)
GET /t/{token}/{channel}/repodata_shards.msgpack.zst/{platform}  # Try shards first
GET /t/{token}/{channel}/{platform}/repodata.json.zst            # Fall back to full repodata
```

### Labeled Channels (e.g., anaconda-cloud/label/dev)

```
GET /{channel}/label/{label}/terms.json
GET /t/{token}/{channel}/label/{label}/repodata_shards.msgpack.zst/{platform}
```

Labels are encoded as path segments.

### Private Channels on repo.anaconda.cloud

```
GET /repo/{channel}/terms.json                                   # Terms check
GET /repo/{channel}/{platform}/repodata.json.zst                 # Try compressed
GET /repo/{channel}/{platform}/repodata.json                     # Fall back to uncompressed
```

## Repodata Formats

Conda tries formats in order of preference:

1. `repodata_shards.msgpack.zst/{platform}` — Sharded, compressed (conda-forge, some others)
2. `{platform}/repodata.json.zst` — Full repodata, zstd compressed
3. `{platform}/repodata.json` — Full repodata, uncompressed (fallback)

## Platforms

Every channel fetch requests both:
- Architecture-specific: e.g., `osx-arm64`, `linux-64`, `win-64`
- Architecture-independent: `noarch`

## Telemetry Headers

Requests to `repo.anaconda.cloud` include telemetry headers:

| Header | Content |
|--------|---------|
| `anaconda-telemetry-channels` | All channels being queried |
| `anaconda-telemetry-install` | Package being requested |
| `anaconda-telemetry-packages` | Currently installed packages |
| `anaconda-telemetry-sys-info` | Conda version info |
| `anaconda-telemetry-virtual-packages` | Virtual packages |

## Examples

### Installing from conda-forge

```
conda install -c conda-forge numpy
```

Fetches:
```
https://conda.anaconda.org/t/{token}/conda-forge/repodata_shards.msgpack.zst/osx-arm64
https://conda.anaconda.org/t/{token}/conda-forge/repodata_shards.msgpack.zst/noarch
```

### Installing from defaults

```
conda install numpy
```

With default config, fetches:
```
https://repo.anaconda.com/pkgs/main/osx-arm64/repodata.json.zst
https://repo.anaconda.com/pkgs/main/noarch/repodata.json.zst
https://repo.anaconda.com/pkgs/r/osx-arm64/repodata.json.zst
https://repo.anaconda.com/pkgs/r/noarch/repodata.json.zst
```
