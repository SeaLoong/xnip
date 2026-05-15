#!/usr/bin/env bash
# xnip — installer (macOS / Linux)
#
# Usage:
#   curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh
#
# Env:
#   XNIP_VERSION    — release tag (default: latest)
#   XNIP_INSTALL_DIR — install dir (default: $HOME/.local/bin)
#   XNIP_REPO       — `owner/repo` (default: SeaLoong/xnip)

set -euo pipefail

XNIP_REPO="${XNIP_REPO:-SeaLoong/xnip}"
XNIP_VERSION="${XNIP_VERSION:-latest}"
XNIP_INSTALL_DIR="${XNIP_INSTALL_DIR:-$HOME/.local/bin}"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS-$ARCH" in
  linux-x86_64)   TARGET="x86_64-unknown-linux-musl" ;;
  linux-aarch64)  TARGET="aarch64-unknown-linux-musl" ;;
  linux-arm64)    TARGET="aarch64-unknown-linux-musl" ;;
  darwin-x86_64)  TARGET="x86_64-apple-darwin" ;;
  darwin-arm64)   TARGET="aarch64-apple-darwin" ;;
  *)
    echo "xnip install: unsupported platform: $OS-$ARCH" >&2
    exit 1
    ;;
esac

if [ "$XNIP_VERSION" = "latest" ]; then
  URL="https://github.com/${XNIP_REPO}/releases/latest/download"
else
  URL="https://github.com/${XNIP_REPO}/releases/download/${XNIP_VERSION}"
fi

ASSET="xnip-${XNIP_VERSION#v}-${TARGET}.tar.gz"
# When using "latest", the artifact filename still bakes in the version we don't know;
# release.yml writes them as `xnip-${tag}-${target}`. We resolve via redirect.
if [ "$XNIP_VERSION" = "latest" ]; then
  echo "xnip install: 'latest' channel needs explicit version; pass XNIP_VERSION=vX.Y.Z" >&2
  exit 1
fi

mkdir -p "$XNIP_INSTALL_DIR"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading $URL/$ASSET"
curl -fsSL "$URL/$ASSET" -o "$TMP_DIR/$ASSET"

echo "Extracting"
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"

echo "Installing to $XNIP_INSTALL_DIR/xnip"
mv "$TMP_DIR/xnip" "$XNIP_INSTALL_DIR/xnip"
chmod +x "$XNIP_INSTALL_DIR/xnip"

echo "Done."
echo
echo "If $XNIP_INSTALL_DIR is not on your PATH, add:"
echo "  export PATH=\"$XNIP_INSTALL_DIR:\$PATH\""
echo
"$XNIP_INSTALL_DIR/xnip" --version || true
