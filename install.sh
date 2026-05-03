#!/usr/bin/env bash
# anima-tagger installer for macOS and Linux.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/fwaunstp/anima-tagger/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/fwaunstp/anima-tagger/main/install.sh | sh -s -- --version v0.2.0
#   curl -fsSL https://raw.githubusercontent.com/fwaunstp/anima-tagger/main/install.sh | sh -s -- --cli-only
#
# Flags:
#   --version <tag>   release tag to install (default: latest)
#   --prefix <dir>    install root for CLI binary (default: $HOME/.local)
#   --app-dir <dir>   install root for macOS .app (default: $HOME/Applications)
#   --cli-only        skip GUI install
#   --gui-only        skip CLI install
#   --no-verify       skip SHA256 verification

set -euo pipefail

REPO="fwaunstp/anima-tagger"
VERSION="latest"
PREFIX="${HOME}/.local"
APP_DIR="${HOME}/Applications"
INSTALL_CLI=1
INSTALL_GUI=1
VERIFY=1

err() { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '==> %s\n' "$*"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --version)    VERSION="$2"; shift 2 ;;
        --prefix)     PREFIX="$2"; shift 2 ;;
        --app-dir)    APP_DIR="$2"; shift 2 ;;
        --cli-only)   INSTALL_GUI=0; shift ;;
        --gui-only)   INSTALL_CLI=0; shift ;;
        --no-verify)  VERIFY=0; shift ;;
        -h|--help)
            sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) err "unknown flag: $1" ;;
    esac
done

command -v curl >/dev/null 2>&1 || err "curl is required"
command -v tar  >/dev/null 2>&1 || err "tar is required"

OS_RAW="$(uname -s)"
ARCH_RAW="$(uname -m)"
case "$OS_RAW" in
    Darwin) OS=macos ;;
    Linux)  OS=linux ;;
    *) err "unsupported OS: $OS_RAW" ;;
esac
case "$ARCH_RAW" in
    arm64|aarch64) ARCH=arm64 ;;
    x86_64|amd64)  ARCH=x64 ;;
    *) err "unsupported arch: $ARCH_RAW" ;;
esac

if [ "$OS" = "macos" ] && [ "$ARCH" = "x64" ]; then
    err "Intel macOS prebuilt binaries are not published. Build from source with: cargo install --git https://github.com/${REPO} anima-tagger-cli"
fi

TARGET="${OS}-${ARCH}"
info "platform: ${TARGET}"

# Resolve version → tag
if [ "$VERSION" = "latest" ]; then
    info "resolving latest release"
    TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep -m1 '"tag_name":' \
        | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
    [ -n "$TAG" ] || err "could not resolve latest tag"
else
    TAG="$VERSION"
fi
VER="${TAG#v}"
info "version: ${TAG}"

BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

# Optional checksum verification
if [ "$VERIFY" = "1" ] && command -v sha256sum >/dev/null 2>&1; then
    info "downloading SHA256SUMS"
    curl -fsSL -o "$TMPDIR/SHA256SUMS" "${BASE_URL}/SHA256SUMS" || {
        info "SHA256SUMS not found on this release; skipping verification"
        VERIFY=0
    }
fi

verify() {
    [ "$VERIFY" = "1" ] || return 0
    [ -f "$TMPDIR/SHA256SUMS" ] || return 0
    ( cd "$TMPDIR" && grep -F " $1" SHA256SUMS | sha256sum -c - >/dev/null )
}

download() {
    local name="$1"
    info "downloading ${name}"
    curl -fL --retry 3 -o "$TMPDIR/${name}" "${BASE_URL}/${name}"
    verify "$name" || err "checksum verification failed for ${name}"
}

# CLI install
if [ "$INSTALL_CLI" = "1" ]; then
    CLI_NAME="anima-tagger-cli-${VER}-${TARGET}.tar.gz"
    download "$CLI_NAME"
    mkdir -p "${PREFIX}/bin"
    tar xzf "$TMPDIR/${CLI_NAME}" -C "${PREFIX}/bin"
    chmod +x "${PREFIX}/bin/anima-tagger"
    info "installed CLI: ${PREFIX}/bin/anima-tagger"
fi

# GUI install
if [ "$INSTALL_GUI" = "1" ]; then
    case "$OS" in
        macos)
            GUI_NAME="anima-tagger-${VER}-${TARGET}.app.tar.gz"
            download "$GUI_NAME"
            mkdir -p "$APP_DIR"
            # Replace existing copy if present.
            rm -rf "$APP_DIR/anima-tagger.app"
            tar xzf "$TMPDIR/${GUI_NAME}" -C "$APP_DIR"
            xattr -dr com.apple.quarantine "$APP_DIR/anima-tagger.app" 2>/dev/null || true
            info "installed GUI: ${APP_DIR}/anima-tagger.app"
            info "first launch: open the app via Finder (right-click → Open) since the build is not notarized"
            ;;
        linux)
            GUI_NAME="anima-tagger-${VER}-${TARGET}.AppImage"
            download "$GUI_NAME"
            mkdir -p "${PREFIX}/bin"
            cp "$TMPDIR/${GUI_NAME}" "${PREFIX}/bin/anima-tagger-gui"
            chmod +x "${PREFIX}/bin/anima-tagger-gui"
            info "installed GUI: ${PREFIX}/bin/anima-tagger-gui"
            ;;
    esac
fi

case ":${PATH}:" in
    *":${PREFIX}/bin:"*) ;;
    *) printf '\nnote: %s/bin is not on $PATH. Add this to your shell rc:\n  export PATH="%s/bin:$PATH"\n' "$PREFIX" "$PREFIX" ;;
esac

info "done."
