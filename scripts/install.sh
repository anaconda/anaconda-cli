#!/bin/sh
# Installer script for ana.
#
# Usage:
#
#   Direct download:
#   > curl -fsSL https://anaconda.sh/install.sh | sh
#   or:
#   > wget -qO- https://anaconda.sh/install.sh | sh
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

BINARY_NAME="ana"

# Base URL for downloads - override with ANA_BASE_URL env var
DEFAULT_BASE_URL="https://anaconda.sh"

# Defaults (can be overridden via environment variables or CLI options)
DEFAULT_INSTALL_DIR="$HOME/.local/bin"
DEFAULT_VERSION="latest"
DEFAULT_VERIFY_CHECKSUM="true"
DEFAULT_CHANNEL="stable"

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
  -c, --channel CHANNEL    Release channel: stable or dev (default: ${DEFAULT_CHANNEL})
      --no-verify-checksum Disable checksum validation after download (default: false)
      --no-path-update     Skip shell profile modification
      --no-bootstrap       Skip running 'ana bootstrap' after installation
  -f, --force              Overwrite existing installation without prompting
  -h, --help               Show this help message

Environment variables:
  ANA_INSTALL_DIR          Same as --install-dir
  ANA_VERSION              Same as --version
  ANA_CHANNEL              Same as --channel (stable, dev)
  ANA_BASE_URL             Override download base URL (for testing)
  ANA_VERIFY_CHECKSUM      Set to "false" to skip checksum verification
  ANA_NO_PATH_UPDATE       Set to non-empty to skip PATH update
  ANA_BOOTSTRAP            Set to "false" to skip bootstrap
  ANA_FORCE_INSTALL        Set to non-empty to overwrite without prompting

Examples:
  # Direct download via pipe:
  > curl -fsSL ${DEFAULT_BASE_URL}/install.sh | sh

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
            -c|--channel)
                [ $# -ge 2 ] || err "Missing argument for $1"
                ANA_CHANNEL="$2"
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
            --no-bootstrap)
                ANA_BOOTSTRAP="false"
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

    # Detect platform
    local _os _arch _target
    _os="$(detect_os)"
    _arch="$(detect_arch)"
    _target="$(map_target "$_os" "$_arch")"

    # Configuration
    local _version="${ANA_VERSION:-$DEFAULT_VERSION}"
    local _install_dir="${ANA_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
    local _exe_suffix=""
    if [ "$_os" = "windows" ]; then
        _exe_suffix=".exe"
    fi
    local _asset_name="ana-${_target}${_exe_suffix}"

    # Configuration
    local _base_url="${ANA_BASE_URL:-$DEFAULT_BASE_URL}"
    local _channel="${ANA_CHANNEL:-$DEFAULT_CHANNEL}"

    # Resolve download URLs
    # URL structure: {base_url}/releases/{channel}/{version}/{asset}
    local _url="${_base_url}/releases/${_channel}/${_version}/${_asset_name}"
    local _checksum_url="${_url}.sha256"

    info "Installing ana for %s %s" "$_os" "$_arch"

    # Check for existing installation before downloading
    local _dest="${_install_dir}/${BINARY_NAME}${_exe_suffix}"
    check_existing_install "$_dest"

    info "Downloading %s" "$_url"

    local _tmp
    _tmp="$(mktemp "${TMPDIR:-/tmp}/.ana_install.XXXXXXXX")"
    trap 'rm -f "$_tmp"' EXIT

    download "$_url" "$_tmp"

    if [ ! -s "$_tmp" ]; then
        err "Downloaded file is empty. Check the URL or try again."
    fi

    verify_checksum "$_checksum_url" "$_tmp"

    install_binary "$_tmp" "$_install_dir" "$_exe_suffix"

    if [ -z "${ANA_NO_PATH_UPDATE:-}" ]; then
        update_shell_profile "$_install_dir"
    fi

    run_bootstrap "$_install_dir" "$_exe_suffix"

    printf "🎉 Done! Run '\033[1;36mana --help\033[0m' to get started.\n"
}

detect_os() {
    local _os
    _os="$(uname -s)"
    case "$_os" in
        Linux)                    echo "linux" ;;
        Darwin)                   echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*)     echo "windows" ;;
        *)                        err "Unsupported operating system: %s" "$_os" ;;
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
        linux-x86_64)    echo "linux-x86_64" ;;
        linux-aarch64)   echo "linux-aarch64" ;;
        macos-x86_64)    echo "darwin-x86_64" ;;
        macos-aarch64)   echo "darwin-arm64" ;;
        windows-x86_64)  echo "windows-x86_64" ;;
        *)               err "No prebuilt binary for %s %s" "$_os" "$_arch" ;;
    esac
}

download() {
    local _url="$1" _dest="$2"

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
            "$_url" --output "$_dest" --write-out "%{http_code}")" || {
            err "Download failed. Is curl working? URL: %s" "$_url"
        }
        if [ "$_http_code" -lt 200 ] || [ "$_http_code" -gt 299 ]; then
            err "Download failed with HTTP %s: %s" "$_http_code" "$_url"
        fi
    elif check_cmd wget; then
        wget $WGET_OPTS --output-document="$_dest" "$_url" || {
            err "Download failed. Is wget working? URL: %s" "$_url"
        }
    else
        err "Need curl or wget to download files"
    fi
}

verify_checksum() {
    local _checksum_url="$1" _file="$2"
    local _verify="${ANA_VERIFY_CHECKSUM:-$DEFAULT_VERIFY_CHECKSUM}"

    case "$_verify" in
        false|0)
            warn "Checksum verification disabled"
            return 0
            ;;
        true|1) ;;
        *)
            err "Invalid ANA_VERIFY_CHECKSUM value '%s'. Must be 'true', 'false', '1', or '0'." "$_verify"
            ;;
    esac

    info "Verifying checksum"

    if [ -z "$_checksum_url" ]; then
        warn "Checksum file not available, skipping verification"
        return 0
    fi

    local _expected _actual _tmp_sha
    _tmp_sha="$(mktemp "${TMPDIR:-/tmp}/.ana_sha.XXXXXXXX")"
    if ! download "$_checksum_url" "$_tmp_sha" 2>/dev/null; then
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

check_existing_install() {
    local _dest="$1"

    if [ -f "$_dest" ] && [ -z "${ANA_FORCE_INSTALL:-}" ]; then
        if [ -t 0 ]; then
            printf "  %s already exists. Overwrite? [y/N] " "$_dest"
            read -r _reply
            case "$_reply" in
                [Yy]|[Yy][Ee][Ss]) ;;
                *) err "Installation cancelled." ;;
            esac
        else
            err "%s already exists. Use --force or ANA_FORCE_INSTALL=1 to overwrite." "$_dest"
        fi
    fi
}

install_binary() {
    local _src="$1" _install_dir="$2" _exe_suffix="${3:-}"
    local _dest="${_install_dir}/${BINARY_NAME}${_exe_suffix}"

    chmod +x "$_src"
    mkdir -p "$_install_dir"
    mv -f "$_src" "$_dest"
    trap - EXIT

    info "Installed ana to %s" "$_dest"
}

run_bootstrap() {
    local _install_dir="$1" _exe_suffix="${2:-}"
    local _ana_bin="${_install_dir}/${BINARY_NAME}${_exe_suffix}"
    local _bootstrap="${ANA_BOOTSTRAP:-true}"

    case "$_bootstrap" in
        false|0)
            info "Skipping bootstrap (disabled)"
            return 0
            ;;
        true|1) ;;
        *)
            err "Invalid ANA_BOOTSTRAP value '%s'. Must be 'true', 'false', '1', or '0'." "$_bootstrap"
            ;;
    esac

    info "Running ana bootstrap..."
    if "$_ana_bin" bootstrap; then
        info "Bootstrap completed successfully"
    else
        warn "Bootstrap failed. You can run 'ana bootstrap' manually later."
    fi
}

update_shell_profile() {
    local _dir="$1"
    local _ana_bin_dir="$HOME/.ana/bin"

    # Add both the install directory and ~/.ana/bin (for tool symlinks)
    add_to_path "$_dir"
    add_to_path "$_ana_bin_dir"
}

add_to_path() {
    local _dir="$1" _line

    # Already in $PATH
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
