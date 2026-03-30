# Configuration

Ana loads configuration from environment variables at runtime. You can view the current configuration with:

```bash
ana config
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ANA_AUTH_DOMAIN` | `anaconda.com` | Authentication domain |
| `ANA_AUTH_CLIENT_ID` | *(Anaconda's ID)* | OAuth client ID |
| `ANA_SSL_VERIFY` | `true` | Verify SSL certificates |
| `ANA_OPEN_BROWSER` | `true` | Auto-open browser during login |

## Precedence

Configuration values are resolved in the following order (highest priority first):

1. **Environment variables** - Always take precedence when set
2. **Default values** - Built-in defaults defined in [`config.rs`](../src/config.rs)

Future versions will add support for a configuration file (`~/.ana/config.toml`), which will fall between environment variables and defaults.

## Boolean Parsing

Boolean environment variables are parsed as follows:

| Value | Result |
|-------|--------|
| `"false"` (case-insensitive) | `false` |
| `"0"` | `false` |
| `""` (empty) | `false` |
| Any other value | `true` |

Examples:
```bash
# Disable SSL verification
export ANA_SSL_VERIFY=false

# Disable browser auto-open
export ANA_OPEN_BROWSER=0

# Enable (any non-false value works)
export ANA_SSL_VERIFY=true
export ANA_SSL_VERIFY=1
export ANA_SSL_VERIFY=yes
```
