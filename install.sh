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

# musl or glibc. Alpine says so in a file; everywhere else, ask the loader --
# musl's ldd writes its banner to stderr and exits non-zero, so keep both.
libc() {
    if [ -f /etc/alpine-release ]; then
        echo musl
    elif (ldd --version) 2>&1 | grep -qi musl; then
        echo musl
    else
        echo gnu
    fi
}

target() {
    os="$(uname -s)"
    arch="$(uname -m)"
    # Git Bash, MSYS2 and Cygwin all run on Windows and all name themselves
    # something else: MINGW64_NT-10.0-22631, MSYS_NT-10.0, CYGWIN_NT-10.0.
    case "$os" in
        MINGW*|MSYS*|CYGWIN*) os="Windows" ;;
    esac
    case "$os $arch" in
        "Darwin arm64")   echo "aarch64-apple-darwin" ;;
        "Darwin x86_64")  echo "x86_64-apple-darwin" ;;
        "Windows x86_64") echo "x86_64-pc-windows-msvc" ;;
        "Linux x86_64")
            if [ "$(libc)" = musl ]; then
                echo "x86_64-unknown-linux-musl"
            else
                echo "x86_64-unknown-linux-gnu"
            fi
            ;;
        "Linux aarch64" | "Linux arm64")
            if [ "$(libc)" = musl ]; then
                fail "there is no aarch64 musl build yet; build from source with: cargo install --git https://github.com/$REPO"
            fi
            echo "aarch64-unknown-linux-gnu"
            ;;
        *) fail "no prebuilt binary for $os $arch; build from source with: cargo install --git https://github.com/$REPO" ;;
    esac
}

# Git for Windows' curl checks certificate revocation and gives up with
# CRYPT_E_REVOCATION_OFFLINE when it cannot reach the server that answers for
# it -- which happens, and has nothing to do with the download. That and an
# ordinary dropped connection are worth another go. Retrying in the script
# rather than with --retry-all-errors keeps it working with an older curl.
fetch() {
    attempt=1
    while :; do
        curl -fsSL "$1" -o "$2" && return 0
        code=$?
        # 22 is "the server answered, and said no". A 404 will not become a
        # 200, so there is nothing to wait for.
        if [ "$code" -eq 22 ] || [ "$attempt" -ge 3 ]; then
            return "$code"
        fi
        attempt=$((attempt + 1))
        sleep 2
    done
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

# tests/install-sh.sh sources this file to check target() on its own, without
# downloading anything.
if [ -n "${SHLANE_INSTALL_SH_SOURCED:-}" ]; then
    return 0
fi

TARGET="$(target)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ -n "${SHLANE_VERSION:-}" ]; then
    VERSION="${SHLANE_VERSION#v}"
else
    need sed
    fetch "https://api.github.com/repos/$REPO/releases/latest" "$TMP/latest.json" \
        || fail "could not reach GitHub to work out the latest version; set SHLANE_VERSION"
    VERSION="$(sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' "$TMP/latest.json" | head -n 1)"
    [ -n "$VERSION" ] || fail "could not work out the latest version; set SHLANE_VERSION"
fi

NAME="shlane-${VERSION}-${TARGET}"
BASE="https://github.com/$REPO/releases/download/v${VERSION}"
case "$TARGET" in
    *-windows-*) BIN="shlane.exe" ;;
    *)           BIN="shlane" ;;
esac

echo "Downloading shlane ${VERSION} for ${TARGET}"
fetch "$BASE/${NAME}.tar.gz" "$TMP/${NAME}.tar.gz" \
    || fail "could not download ${NAME}.tar.gz"
fetch "$BASE/SHA256SUMS" "$TMP/SHA256SUMS" \
    || fail "could not download SHA256SUMS; refusing to install unverified"

expected="$(grep " ${NAME}.tar.gz\$" "$TMP/SHA256SUMS" | cut -d' ' -f1)"
[ -n "$expected" ] || fail "${NAME}.tar.gz is not listed in SHA256SUMS"

actual="$(checksum "$TMP/${NAME}.tar.gz")"
[ "$expected" = "$actual" ] || fail "checksum mismatch: expected $expected, got $actual"

tar -C "$TMP" -xzf "$TMP/${NAME}.tar.gz"
mkdir -p "$INSTALL_DIR"
install -m 755 "$TMP/${NAME}/${BIN}" "$INSTALL_DIR/${BIN}" 2>/dev/null \
    || { cp "$TMP/${NAME}/${BIN}" "$INSTALL_DIR/${BIN}" && chmod 755 "$INSTALL_DIR/${BIN}"; }

echo "Installed $INSTALL_DIR/${BIN}"
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "Add it to your PATH:  export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac
"$INSTALL_DIR/${BIN}" --version
