#!/bin/sh
# Installer script for ana.
#
# Usage:
#
#   TODO: The URLs below will be updated once the final URLs are determined.
#
#   Direct download:
#   > curl -fsSL https://anaconda.com/ana/install.sh | sh
#   or:
#   > wget -qO- https://anaconda.com/ana/install.sh | sh
#
#   Direct script invocation:
#   > ./install.sh [OPTIONS]
#
# Run with --help for full usage information.

# shellcheck disable=SC3043  # 'local' is widely supported even if not strictly POSIX
set -eu

# The __wrap__ function ensures that we download the entire function definition before
# execution. This is important when piping a downloaded script into sh, to avoid partial
# script execution if download of the script fails. We download the entire function
# definition and then initially call it.
__wrap__() {

REPO="anaconda/ana-cli"
BINARY_NAME="ana"

# Defaults (can be overridden via environment variables or CLI options)
DEFAULT_INSTALL_DIR="$HOME/.local/bin"
DEFAULT_VERSION="latest"
DEFAULT_VERIFY_CHECKSUM="true"

usage() {
    # Replace $HOME with ~ for display
    local _display_dir
    _display_dir="$(echo "$DEFAULT_INSTALL_DIR" | sed "s|^$HOME|~|")"

    cat <<EOF
Usage: install.sh [OPTIONS]

Install the ana CLI tool.

Options:
  -d, --install-dir DIR    Install directory (default: ${_display_dir})
  -v, --version VERSION    Version to install (default: ${DEFAULT_VERSION})
      --no-verify-checksum Disable checksum validation after download (default: false)
      --no-path-update     Skip shell profile modification
  -t, --token TOKEN        GitHub token for private repo access
  -f, --force              Overwrite existing installation without prompting
  -h, --help               Show this help message

Environment variables:
  ANA_INSTALL_DIR          Same as --install-dir
  ANA_VERSION              Same as --version
  ANA_VERIFY_CHECKSUM      Set to "false" to skip checksum verification
  ANA_NO_PATH_UPDATE       Set to non-empty to skip PATH update
  ANA_FORCE_INSTALL        Set to non-empty to overwrite without prompting
  GITHUB_TOKEN             Same as --token

Examples:
  # Direct download via pipe:
  > curl -fsSL https://anaconda.com/ana/install.sh | sh

  # Script invocation with options:
  > ./install.sh --version 1.0.0 --install-dir /usr/local/bin

  # Script invocation with environment variables:
  > ANA_VERSION=1.0.0 ./install.sh
EOF
}

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            -h|--help)
                usage
                exit 0
                ;;
            -d|--install-dir)
                [ $# -ge 2 ] || err "Missing argument for $1"
                ANA_INSTALL_DIR="$2"
                shift 2
                ;;
            -v|--version)
                [ $# -ge 2 ] || err "Missing argument for $1"
                ANA_VERSION="$2"
                shift 2
                ;;
            -t|--token)
                [ $# -ge 2 ] || err "Missing argument for $1"
                GITHUB_TOKEN="$2"
                shift 2
                ;;
            --no-verify-checksum)
                ANA_VERIFY_CHECKSUM="false"
                shift
                ;;
            --no-path-update)
                ANA_NO_PATH_UPDATE="1"
                shift
                ;;
            -f|--force)
                ANA_FORCE_INSTALL="1"
                shift
                ;;
            -*)
                err "Unknown option: %s\nRun 'install.sh --help' for usage." "$1"
                ;;
            *)
                err "Unexpected argument: %s\nRun 'install.sh --help' for usage." "$1"
                ;;
        esac
    done
}

main() {
    parse_args "$@"

    ensure_cmd uname
    ensure_cmd chmod
    ensure_cmd mkdir

    local _os _arch _target _version _install_dir _url _checksum_url _tmp _auth_header

    _os="$(detect_os)"
    _arch="$(detect_arch)"
    _target="$(map_target "$_os" "$_arch")"
    _version="${ANA_VERSION:-$DEFAULT_VERSION}"
    _install_dir="${ANA_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"

    local _asset_name="ana-${_target}"

    # ANA_BASE_URL can override the download URL (useful for testing)
    if [ -n "${ANA_BASE_URL:-}" ]; then
        _url="${ANA_BASE_URL}/${_asset_name}"
        _checksum_url="${_url}.sha256"
        _auth_header=""
    else
        _auth_header="$(get_auth_header)"
        # Private repo: use GitHub API to resolve asset URL
        if [ -n "$_auth_header" ]; then
            _url="$(resolve_github_asset_url "$_version" "$_asset_name" "$_auth_header")"
            _checksum_url="$(resolve_github_asset_url "$_version" "${_asset_name}.sha256" "$_auth_header" 2>/dev/null)" || _checksum_url=""
        # Public repo: use direct download URL
        elif [ "$_version" = "latest" ]; then
            _url="https://github.com/${REPO}/releases/latest/download/${_asset_name}"
            _checksum_url="${_url}.sha256"
        else
            _url="https://github.com/${REPO}/releases/download/v${_version#v}/${_asset_name}"
            _checksum_url="${_url}.sha256"
        fi
    fi

    _tmp="$(mktemp "${TMPDIR:-/tmp}/.ana_install.XXXXXXXX")"
    trap 'rm -f "$_tmp"' EXIT

    info "Installing ana for %s %s" "$_os" "$_arch"
    info "Downloading %s" "$_url"

    download "$_url" "$_tmp" "$_auth_header"

    if [ ! -s "$_tmp" ]; then
        err "Downloaded file is empty. Check the URL or try again."
    fi

    if [ "${ANA_VERIFY_CHECKSUM:-$DEFAULT_VERIFY_CHECKSUM}" = "true" ]; then
        info "Verifying checksum"
        verify_checksum "$_checksum_url" "$_tmp" "$_auth_header"
    else
        warn "Checksum verification disabled"
    fi

    chmod +x "$_tmp"
    mkdir -p "$_install_dir"

    local _dest="${_install_dir}/${BINARY_NAME}"
    if [ -f "$_dest" ] && [ -z "${ANA_FORCE_INSTALL:-}" ]; then
        if [ -t 0 ]; then
            printf "  %s already exists. Overwrite? [y/N] " "$_dest"
            read -r _reply
            case "$_reply" in
                [Yy]|[Yy][Ee][Ss]) ;;
                *) err "Installation cancelled." ;;
            esac
        else
            err "%s already exists. Use --force to overwrite." "$_dest"
        fi
    fi

    mv -f "$_tmp" "$_dest"
    trap - EXIT

    info "Installed ana to %s/%s" "$_install_dir" "$BINARY_NAME"

    if [ -z "${ANA_NO_PATH_UPDATE:-}" ]; then
        update_shell_profile "$_install_dir"
    fi

    printf "🎉 Done! Run '\033[1;36mana --help\033[0m' to get started."
}

detect_os() {
    local _os
    _os="$(uname -s)"
    case "$_os" in
        Linux)  echo "linux" ;;
        Darwin) echo "macos" ;;
        *)      err "Unsupported operating system: %s" "$_os" ;;
    esac
}

detect_arch() {
    local _arch
    _arch="$(uname -m)"
    case "$_arch" in
        x86_64|amd64)   echo "x86_64" ;;
        aarch64|arm64)  echo "aarch64" ;;
        *)              err "Unsupported architecture: %s" "$_arch" ;;
    esac
}

map_target() {
    local _os="$1" _arch="$2"
    case "${_os}-${_arch}" in
        linux-x86_64)   echo "linux-x86_64" ;;
        linux-aarch64)  echo "linux-aarch64" ;;
        macos-x86_64)   echo "darwin-x86_64" ;;
        macos-aarch64)  echo "darwin-arm64" ;;
        *)              err "No prebuilt binary for %s %s" "$_os" "$_arch" ;;
    esac
}

get_auth_header() {
    local _token

    # Use GITHUB_TOKEN if provided, otherwise try gh auth token
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        _token="$GITHUB_TOKEN"
    elif check_cmd gh; then
        _token="$(gh auth token 2>/dev/null)" || true
    fi

    if [ -n "${_token:-}" ]; then
        printf 'Authorization: token %s' "$_token"
    else
        err "Must provide GitHub token to access private repo."
    fi
}

# --- Private repo support (remove this section when repo is public) ---
# GitHub's /releases/download/ URLs don't work for private repos even with auth.
# We must use the API to get the asset URL instead.

resolve_github_asset_url() {
    local _version="$1" _asset_name="$2" _auth_header="$3" _api_url _response _asset_url

    if [ "$_version" = "latest" ]; then
        _api_url="https://api.github.com/repos/${REPO}/releases/tags/latest"
    else
        _api_url="https://api.github.com/repos/${REPO}/releases/tags/v${_version#v}"
    fi

    _response="$(curl -fsSL -H "$_auth_header" "$_api_url")" || {
        err "Failed to fetch release info from GitHub API for version: (%s)" "$_version"
    }

    # Parse asset URL from JSON without jq
    # Looks for: "name": "ana-darwin-arm64" ... "url": "https://api.github.com/.../assets/12345"
    _asset_url="$(printf '%s' "$_response" | grep -B5 "\"name\": \"${_asset_name}\"" | grep '"url":' | head -1 | sed 's/.*"url": "\([^"]*\)".*/\1/')"

    if [ -z "$_asset_url" ]; then
        err "Asset '%s' not found in release %s" "$_asset_name" "$_version"
    fi

    printf '%s' "$_asset_url"
}

# --- End private repo support ---

download() {
    local _url="$1" _dest="$2" _auth_header="${3:-}"

    if [ ! -t 1 ]; then
        CURL_OPTS="--silent"
        WGET_OPTS="--no-verbose"
    else
        CURL_OPTS="--progress-bar"
        WGET_OPTS="--show-progress"
    fi

    if check_cmd curl; then
        local _http_code
        _http_code="$(curl -fSL $CURL_OPTS \
            ${_auth_header:+-H "$_auth_header"} \
            ${_auth_header:+-H "Accept: application/octet-stream"} \
            "$_url" --output "$_dest" --write-out "%{http_code}")" || {
            err "Download failed. Is curl working? URL: %s" "$_url"
        }
        if [ "$_http_code" -lt 200 ] || [ "$_http_code" -gt 299 ]; then
            err "Download failed with HTTP %s: %s" "$_http_code" "$_url"
        fi
    elif check_cmd wget; then
        wget $WGET_OPTS \
            ${_auth_header:+--header="$_auth_header"} \
            ${_auth_header:+--header="Accept: application/octet-stream"} \
            --output-document="$_dest" "$_url" || {
            err "Download failed. Is wget working? URL: %s" "$_url"
        }
    else
        err "Need curl or wget to download files"
    fi
}

verify_checksum() {
    local _checksum_url="$1" _file="$2" _auth_header="${3:-}" _expected _actual _tmp_sha

    if [ -z "$_checksum_url" ]; then
        warn "Checksum file not available, skipping verification"
        return 0
    fi

    _tmp_sha="$(mktemp "${TMPDIR:-/tmp}/.ana_sha.XXXXXXXX")"
    if ! download "$_checksum_url" "$_tmp_sha" "$_auth_header" 2>/dev/null; then
        warn "Checksum file not available, skipping verification"
        rm -f "$_tmp_sha"
        return 0
    fi

    _expected="$(awk '{print $1}' "$_tmp_sha")"
    rm -f "$_tmp_sha"

    if check_cmd sha256sum; then
        _actual="$(sha256sum "$_file" | awk '{print $1}')"
    elif check_cmd shasum; then
        _actual="$(shasum -a 256 "$_file" | awk '{print $1}')"
    else
        warn "No sha256sum or shasum found, skipping verification"
        return 0
    fi

    if [ "$_expected" != "$_actual" ]; then
        err "Checksum mismatch!\n  expected: %s\n  actual:   %s" "$_expected" "$_actual"
    fi

    info "Checksum OK"
}

update_shell_profile() {
    local _dir="$1" _line

    if echo "$PATH" | tr ':' '\n' | grep -qx "$_dir" 2>/dev/null; then
        return 0
    fi

    _line="export PATH=\"${_dir}:\$PATH\""

    case "$(basename "${SHELL:-}")" in
        bash)
            append_line_if_missing "$HOME/.bashrc" "$_line"
            ;;
        zsh)
            append_line_if_missing "$HOME/.zshrc" "$_line"
            ;;
        fish)
            _line="set -gx PATH \"${_dir}\" \$PATH"
            append_line_if_missing "$HOME/.config/fish/config.fish" "$_line"
            ;;
        *)
            warn "%s is not in your PATH." "$_dir"
            warn "Add it with:  %s" "$_line"
            return 0
            ;;
    esac
    printf "\n"
}

append_line_if_missing() {
    local _file="$1" _line="$2"

    if [ -f "$_file" ] && grep -Fxq "$_line" "$_file" 2>/dev/null; then
        return 0
    fi

    [ -f "$_file" ] || touch "$_file"

    printf '\n%s\n' "$_line" >> "$_file"
    info "Updated %s — restart your shell or run:  source %s" "$_file" "$_file"
}

check_cmd() {
    command -v "$1" >/dev/null 2>&1
}

ensure_cmd() {
    if ! check_cmd "$1"; then
        err "Required command not found: %s" "$1"
    fi
}

info() {
    local _fmt="$1"; shift
    # shellcheck disable=SC2059
    printf "\033[1;32m>\033[0m $_fmt\n" "$@"
}

warn() {
    local _fmt="$1"; shift
    # shellcheck disable=SC2059
    printf "\033[1;33m!\033[0m $_fmt\n" "$@" >&2
}

err() {
    local _fmt="$1"; shift
    # shellcheck disable=SC2059
    printf "\033[1;31mx\033[0m $_fmt\n" "$@" >&2
    exit 1
}

main "$@"
} && __wrap__ "$@"
