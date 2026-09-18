#!/bin/sh
# Install shlane.
#
#   curl -fsSL https://raw.githubusercontent.com/prongbang/shlane/master/install.sh | sh
#
# Set SHLANE_VERSION to pick a version, SHLANE_INSTALL_DIR to choose where it
# goes. Every download is checked against the release's SHA256SUMS before it is
# installed.
set -eu

REPO="prongbang/shlane"
INSTALL_DIR="${SHLANE_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
    echo "install.sh: $1" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

need curl
need tar

target() {
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os $arch" in
        "Darwin arm64")  echo "aarch64-apple-darwin" ;;
        "Darwin x86_64") echo "x86_64-apple-darwin" ;;
        "Linux x86_64")  echo "x86_64-unknown-linux-gnu" ;;
        "Linux aarch64") echo "aarch64-unknown-linux-gnu" ;;
        "Linux arm64")   echo "aarch64-unknown-linux-gnu" ;;
        *) fail "no prebuilt binary for $os $arch; build from source with: cargo install --git https://github.com/$REPO" ;;
    esac
}

checksum() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        fail "neither shasum nor sha256sum is available; cannot verify the download"
    fi
}

TARGET="$(target)"

if [ -n "${SHLANE_VERSION:-}" ]; then
    VERSION="${SHLANE_VERSION#v}"
else
    need sed
    VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)"
    [ -n "$VERSION" ] || fail "could not work out the latest version; set SHLANE_VERSION"
fi

NAME="shlane-${VERSION}-${TARGET}"
BASE="https://github.com/$REPO/releases/download/v${VERSION}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading shlane ${VERSION} for ${TARGET}"
curl -fsSL "$BASE/${NAME}.tar.gz" -o "$TMP/${NAME}.tar.gz" \
    || fail "could not download ${NAME}.tar.gz"
curl -fsSL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS" \
    || fail "could not download SHA256SUMS; refusing to install unverified"

expected="$(grep " ${NAME}.tar.gz\$" "$TMP/SHA256SUMS" | cut -d' ' -f1)"
[ -n "$expected" ] || fail "${NAME}.tar.gz is not listed in SHA256SUMS"

actual="$(checksum "$TMP/${NAME}.tar.gz")"
[ "$expected" = "$actual" ] || fail "checksum mismatch: expected $expected, got $actual"

tar -C "$TMP" -xzf "$TMP/${NAME}.tar.gz"
mkdir -p "$INSTALL_DIR"
install -m 755 "$TMP/${NAME}/shlane" "$INSTALL_DIR/shlane" 2>/dev/null \
    || { cp "$TMP/${NAME}/shlane" "$INSTALL_DIR/shlane" && chmod 755 "$INSTALL_DIR/shlane"; }

echo "Installed $INSTALL_DIR/shlane"
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "Add it to your PATH:  export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac
"$INSTALL_DIR/shlane" --version
