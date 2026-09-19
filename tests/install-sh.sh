#!/bin/sh
# What install.sh picks for a machine, without downloading anything.
#
#   tests/install-sh.sh
#
# install.sh reads `uname` and `ldd`, so each case runs it with a fake pair of
# those on PATH and checks the target it chose.
set -eu

here="$(cd "$(dirname "$0")/.." && pwd)"
failures=0

# uname -s, uname -m, what `ldd --version` says (- for "no ldd"), expected target.
case_is() {
    os="$1"
    machine="$2"
    ldd_says="$3"
    want="$4"

    fake="$(mktemp -d)"
    printf '#!/bin/sh\ncase "$1" in -s) echo "%s" ;; -m) echo "%s" ;; esac\n' \
        "$os" "$machine" > "$fake/uname"
    if [ "$ldd_says" != "-" ]; then
        # musl's ldd writes to stderr and exits non-zero; glibc's does neither,
        # and either way install.sh only reads the text.
        printf '#!/bin/sh\necho "%s" >&2\nexit 1\n' "$ldd_says" > "$fake/ldd"
        chmod +x "$fake/ldd"
    fi
    chmod +x "$fake/uname"

    got="$(PATH="$fake:$PATH" SHLANE_INSTALL_SH_SOURCED=1 sh -c \
        ". '$here/install.sh' >/dev/null 2>&1; target" 2>&1 || true)"
    rm -rf "$fake"

    if [ "$got" = "$want" ]; then
        echo "ok   $os $machine -> $got"
    else
        echo "FAIL $os $machine"
        echo "     wanted: $want"
        echo "     got:    $got"
        failures=$((failures + 1))
    fi
}

no_binary="install.sh: no prebuilt binary for"

case_is Darwin arm64 - aarch64-apple-darwin
case_is Darwin x86_64 - x86_64-apple-darwin

case_is Linux x86_64 "ldd (Ubuntu GLIBC 2.39-0ubuntu8.3) 2.39" x86_64-unknown-linux-gnu
case_is Linux aarch64 "ldd (Ubuntu GLIBC 2.39-0ubuntu8.3) 2.39" aarch64-unknown-linux-gnu
case_is Linux arm64 "ldd (GNU libc) 2.36" aarch64-unknown-linux-gnu

# Alpine and friends: the gnu build does not run there.
case_is Linux x86_64 "musl libc (x86_64) Version 1.2.5" x86_64-unknown-linux-musl
case_is Linux aarch64 "musl libc (aarch64) Version 1.2.5" \
    "install.sh: there is no aarch64 musl build yet; build from source with: cargo install --git https://github.com/prongbang/shlane"

# Git Bash, MSYS2 and Cygwin are all Windows wearing a different name.
case_is MINGW64_NT-10.0-22631 x86_64 - x86_64-pc-windows-msvc
case_is MSYS_NT-10.0-22631 x86_64 - x86_64-pc-windows-msvc
case_is CYGWIN_NT-10.0-22631 x86_64 - x86_64-pc-windows-msvc

# And the ones there is no build for say so.
case_is MINGW32_NT-6.2 i686 - "$no_binary Windows i686; build from source with: cargo install --git https://github.com/prongbang/shlane"
case_is FreeBSD amd64 - "$no_binary FreeBSD amd64; build from source with: cargo install --git https://github.com/prongbang/shlane"
case_is Linux riscv64 "ldd (GNU libc) 2.39" \
    "$no_binary Linux riscv64; build from source with: cargo install --git https://github.com/prongbang/shlane"

if [ "$failures" -ne 0 ]; then
    echo
    echo "$failures case(s) failed"
    exit 1
fi
