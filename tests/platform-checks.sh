#!/bin/sh
# Section D of docs/plan/17: what a released binary does with a parameter, a
# secret, a timeout, an interrupt and an archive, on the machine running this.
#
#   tests/platform-checks.sh [path/to/shlane]
#
# Defaults to `shlane` on PATH. Checks the machine cannot do are skipped and
# said out loud; anything else that does not match is a failure.
set -eu

shlane="${1:-shlane}"
command -v "$shlane" >/dev/null 2>&1 || [ -x "$shlane" ] || {
    echo "no shlane at '$shlane'" >&2
    exit 2
}

fixture="$(cd "$(dirname "$0")/platform" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cp "$fixture/shlane.yaml" "$work/shlane.yaml"
cd "$work"

failures=0
out=""
status=0

case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) windows=yes ;;
    *) windows=no ;;
esac

# Run a lane, keeping its output and exit code whatever they are.
lane() {
    out="$("$shlane" run "$@" 2>&1)" && status=0 || status=$?
}

pass() { echo "ok   $1"; }
skip() { echo "skip $1 -- $2"; }
fail() {
    echo "FAIL $1 -- $2"
    echo "--- output"
    echo "$out"
    echo "---"
    failures=$((failures + 1))
}

holds() { echo "$out" | grep -qF "$1"; }

# D1. A parameter must reach the shell as text, never as something it can run.
lane quoting
if [ "$status" -ne 0 ]; then
    fail D1 "the lane failed"
elif holds 'it'"'"'s $(whoami) & "quoted"'; then
    pass "D1 a parameter is quoted, not run"
else
    fail D1 "the parameter was not printed literally -- THIS IS A SECURITY BUG"
fi

# D2. A value from env: is a secret, and must not reach the log.
lane masking
if [ "$status" -ne 0 ]; then
    fail D2 "the lane failed"
elif holds 'token is ***' && ! holds 's3cr3t-value-123'; then
    pass "D2 a secret is masked"
else
    fail D2 "the secret was printed, or was not masked"
fi

# D3. A step past its timeout is stopped, and said to have been.
started="$(date +%s)"
lane timeout
took=$(($(date +%s) - started))
if [ "$status" -eq 0 ]; then
    fail D3 "the lane passed, so the timeout did nothing"
elif ! holds "was still running after 2s and was stopped"; then
    fail D3 "it stopped for some other reason"
elif [ "$took" -gt 8 ]; then
    fail D3 "it took ${took}s, so it waited for the step rather than the timeout"
else
    pass "D3 a step past its timeout is stopped (${took}s)"
fi

# D4. And nothing of it is left behind.
if ps -e -o args= >/dev/null 2>&1; then
    if ps -e -o args= | grep -F 'sleep 10' | grep -qv grep; then
        out="$(ps -e -o args= | grep -F 'sleep 10' | grep -v grep)"
        fail D4 "a 'sleep 10' outlived the run"
    else
        pass "D4 the stopped step left nothing running"
    fi
else
    skip D4 "no ps that lists every process"
fi

# D5/D6/D7. zip and unzip, with and without an exclusion.
rm -rf out out.zip extracted
if command -v zip >/dev/null 2>&1; then
    lane archive
    if [ "$status" -ne 0 ]; then
        fail D5 "the lane failed"
    elif [ -f extracted/out/a.txt ] && [ ! -f extracted/out/b.key ]; then
        pass "D5 the archive holds a.txt and not b.key"
    else
        fail D5 "exclude did not leave b.key out"
    fi
elif [ "$windows" != yes ]; then
    skip D5 "no zip binary, and only Windows has a fallback"
else
    # Windows without a zip binary: the PowerShell fallback cannot exclude
    # anything, and quietly including b.key would be the bug.
    lane archive
    if [ "$status" -eq 0 ]; then
        fail D6 "it archived b.key rather than refusing to"
    elif holds "cannot exclude"; then
        pass "D6 it refuses to archive rather than ignore exclude"
    else
        fail D6 "it failed for some other reason"
    fi

    rm -rf out plain.zip extracted_plain
    lane archive_plain
    if [ "$status" -ne 0 ]; then
        fail D7 "the PowerShell fallback could not make an archive"
    elif [ -f extracted_plain/out/a.txt ] && [ -f extracted_plain/out/b.key ]; then
        pass "D7 without exclude it archives through PowerShell"
    else
        fail D7 "the archive is missing a file"
    fi
fi

# D8. Ctrl-C. The signal goes to shlane alone, so nothing but shlane's own
# handling can clean the step up.
if [ "$(uname -s)" = Linux ] || [ "$(uname -s)" = Darwin ]; then
    "$shlane" run timeout > d8.log 2>&1 &
    interrupted=$!
    sleep 1
    kill -INT "$interrupted" 2>/dev/null || true
    wait "$interrupted" && status=0 || status=$?
    out="$(cat d8.log)"
    if [ "$status" -ne 130 ]; then
        fail D8 "it exited $status rather than 130"
    elif ! holds "interrupted while running"; then
        fail D8 "it did not say it was interrupted"
    elif ps -e -o args= | grep -F 'sleep 10' | grep -qv grep; then
        fail D8 "a 'sleep 10' outlived the interrupt"
    else
        pass "D8 an interrupt stops the step too"
    fi
else
    skip D8 "sending a signal needs a POSIX machine"
fi

echo
if [ "$failures" -ne 0 ]; then
    echo "$failures check(s) failed"
    exit 1
fi
echo "every check passed"
