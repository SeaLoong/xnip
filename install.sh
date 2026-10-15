#!/bin/sh
# Install a published release. Overrides: XNIP_VERSION, XNIP_INSTALL_DIR, XNIP_REPO.
set -eu
XNIP_REPO="${XNIP_REPO:-SeaLoong/xnip}"
XNIP_VERSION="${XNIP_VERSION:-latest}"
XNIP_INSTALL_DIR="${XNIP_INSTALL_DIR:-$HOME/.local/bin}"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$OS-$ARCH" in
  linux-x86_64) TARGET=x86_64-unknown-linux-musl ;;
  linux-aarch64|linux-arm64) TARGET=aarch64-unknown-linux-musl ;;
  darwin-x86_64) TARGET=x86_64-apple-darwin ;;
  darwin-arm64) TARGET=aarch64-apple-darwin ;;
  *) echo "xnip install: unsupported platform: $OS-$ARCH" >&2; exit 1 ;;
esac
if [ "$XNIP_VERSION" = latest ]; then
  RELEASE_URL="$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$XNIP_REPO/releases/latest")"
  XNIP_VERSION="${RELEASE_URL##*/}"
fi
case "$XNIP_VERSION" in
  v[0-9]*) ;;
  [0-9]*) XNIP_VERSION="v$XNIP_VERSION" ;;
  *) echo "xnip install: cannot resolve release tag: $XNIP_VERSION" >&2; exit 1 ;;
esac
case "$XNIP_VERSION" in *[!a-zA-Z0-9._-]*) echo "Invalid release tag" >&2; exit 1 ;; esac
ASSET="xnip-$XNIP_VERSION-$TARGET.tar.gz"
URL="https://github.com/$XNIP_REPO/releases/download/$XNIP_VERSION/$ASSET"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' 0
trap 'exit 1' HUP INT TERM
echo "Downloading $URL"
curl -fsSL "$URL" -o "$TMP_DIR/$ASSET"
curl -fsSL "$URL.sha256" -o "$TMP_DIR/$ASSET.sha256"
EXPECTED="$(awk 'NR == 1 { print $1 }' "$TMP_DIR/$ASSET.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$TMP_DIR/$ASSET" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "$TMP_DIR/$ASSET" | awk '{ print $1 }')"
else
  echo "xnip install: sha256sum or shasum is required" >&2; exit 1
fi
if [ "$ACTUAL" != "$EXPECTED" ]; then
  echo "xnip install: SHA256 mismatch" >&2; exit 1
fi
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR" xnip
mkdir -p "$XNIP_INSTALL_DIR"
# Stage on the destination filesystem for an atomic replacement.
STAGED="$(mktemp "$XNIP_INSTALL_DIR/.xnip.XXXXXX")"
trap 'rm -rf "$TMP_DIR"; rm -f "$STAGED"' 0
cp "$TMP_DIR/xnip" "$STAGED"
chmod 755 "$STAGED"
mv -f "$STAGED" "$XNIP_INSTALL_DIR/xnip"
echo "Installed to $XNIP_INSTALL_DIR/xnip"
echo "Add $XNIP_INSTALL_DIR to PATH if needed."
"$XNIP_INSTALL_DIR/xnip" --version
