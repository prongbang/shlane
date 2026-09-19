# 17 — Verifying the Linux and Windows binaries

CI runs the test suite on Ubuntu and Windows, but the binaries a release ships have
only been run by hand on macOS arm64. This is the checklist for running them on real
Linux and Windows machines. Each check says what to run and what should come out; the
expected output was taken from a real run on macOS.

Work through it on a released version (`v0.2.3` or later), not a local build: the
point is to check what users download.

## Machines

| # | Machine | Binary | Why |
|---|---|---|---|
| L1 | Ubuntu 24.04, x86-64 | `x86_64-unknown-linux-gnu` | The common case, and what most CI runs |
| L2 | Linux arm64 (Graviton, Ampere, a Raspberry Pi 4/5 on a 64-bit OS, or GitHub's `ubuntu-24.04-arm`) | `aarch64-unknown-linux-gnu` | Cross-compiled; never executed anywhere |
| L3 | Alpine 3.x, x86-64 (a container is enough) | `x86_64-unknown-linux-musl` | Static build; never executed anywhere |
| W1 | Windows 11, x86-64, with Git for Windows | `x86_64-pc-windows-msvc` | The supported Windows setup |
| W2 | W1, but with Git for Windows removed from `PATH` | same | The error when there is no shell |
| W3 | W1 with WSL enabled but no distribution installed | same | The `bash.exe` trap described in the README |

W2 and W3 can be the same machine as W1 with a change to `PATH` or a Windows feature.

## Record

Copy this per machine into the issue that reports the run:

```
Machine:   L1 / L2 / L3 / W1 / W2 / W3
OS:        (e.g. Ubuntu 24.04.1, Windows 11 23H2)
shlane:    (output of shlane --version)
Shell:     (W only: output of `where bash` in cmd)
Results:   A1 pass, A2 pass, ...  — paste the output of anything that failed
```

## A. Install

**A1 — `install.sh` (L1, L2).**

```sh
curl -fsSL https://raw.githubusercontent.com/prongbang/shlane/master/install.sh | sh
```

Expected: `Downloading shlane <version> for x86_64-unknown-linux-gnu` (or
`aarch64-unknown-linux-gnu` on L2), `Installed ~/.local/bin/shlane`, then
`shlane <version>`.

**A2 — `install.sh` on Alpine (L3).** Run the same line in `alpine:3` (with
`apk add curl`).

Expected today: it picks the **gnu** build, which does not run on musl — `shlane
--version` fails with `not found` or a loader error. This is a known gap: `install.sh`
never chooses the musl build. Record what happens; the fix belongs in `install.sh`.

**A3 — The musl tarball by hand (L3).**

```sh
v=0.2.3
curl -fsSLO https://github.com/prongbang/shlane/releases/download/v$v/shlane-$v-x86_64-unknown-linux-musl.tar.gz
curl -fsSLO https://github.com/prongbang/shlane/releases/download/v$v/SHA256SUMS
grep musl SHA256SUMS | sha256sum -c -
tar xzf shlane-$v-x86_64-unknown-linux-musl.tar.gz
./shlane-$v-x86_64-unknown-linux-musl/shlane --version
```

Expected: `...: OK`, then `shlane 0.2.3`. `ldd` on the binary should say it is not a
dynamic executable.

**A4 — `install.sh` on Windows (W1, in Git Bash).**

Expected today: `no prebuilt binary for MINGW64_NT-... x86_64`. `install.sh` does not
recognise Git Bash's `uname`, so it refuses although a Windows binary exists. Known
gap; record the exact `uname -s` output, which is what the fix needs.

**A5 — The Windows tarball by hand (W1, in PowerShell).**

```powershell
$v = "0.2.3"
$base = "https://github.com/prongbang/shlane/releases/download/v$v"
Invoke-WebRequest "$base/shlane-$v-x86_64-pc-windows-msvc.tar.gz" -OutFile shlane.tar.gz
Invoke-WebRequest "$base/SHA256SUMS" -OutFile SHA256SUMS
(Get-FileHash shlane.tar.gz -Algorithm SHA256).Hash.ToLower()
Select-String windows SHA256SUMS
tar -xzf shlane.tar.gz
.\shlane-$v-x86_64-pc-windows-msvc\shlane.exe --version
```

Expected: the two hashes match, then `shlane 0.2.3`. Put the folder on `PATH` for the
rest of the checks.

## B. Smoke (every machine)

| # | Run | Expected |
|---|---|---|
| B1 | `shlane --version` | `shlane <version>` |
| B2 | `shlane action list` | 40 actions, ending with `notify_teams` |
| B3 | `shlane action show build_ios` | the arguments, including `skip_export` |
| B4 | `shlane completions bash` | a completion script, exit code 0 |

## C. The example project (L1, L2, L3, W1)

From a clone of the repository:

```sh
cd example
shlane validate
shlane run deploy target=production
shlane run deploy target=staging
```

Expected: `shlane.yaml is valid (5 lane(s))`, and both runs end with
`Lane 'deploy' completed successfully!`. This is what CI's "example config runs" job
does, but with the released binary.

## D. Shell behaviour (every machine)

Save this as `shlane.yaml` in an empty directory:

```yaml
version: 1
env:
  DEMO_TOKEN: s3cr3t-value-123
lanes:
  quoting:
    params:
      name:
        type: string
        default: "it's $(whoami) & \"quoted\""
    steps:
      - run: printf '%s\n' "${name}"
  masking:
    steps:
      - run: echo "token is $DEMO_TOKEN"
  timeout:
    steps:
      - run: sleep 10
        timeout: 2s
  archive:
    steps:
      - run: mkdir -p out && echo a > out/a.txt && echo b > out/b.key
      - action: zip
        with:
          path: out
          output: out.zip
          exclude: "*.key"
      - action: unzip
        with:
          archive: out.zip
          into: extracted
      - run: ls -R extracted
```

| # | Run | Expected |
|---|---|---|
| D1 | `shlane run quoting` | prints `it's $(whoami) & "quoted"` literally. If it prints your user name, a parameter reached the shell unescaped: **stop and report it as a security bug** |
| D2 | `shlane run masking` | `token is ***`. The value itself must not appear anywhere in the output |
| D3 | `shlane run timeout` | fails after about 2 s: `step 'sleep 10' was still running after 2s and was stopped` |
| D4 | during D3, from another terminal: `ps aux \| grep 'sleep 10'` (L) | nothing left once shlane exits. On Windows the step is killed rather than signalled; check Task Manager for a leftover `sleep.exe` |
| D5 | `shlane run archive` (L, and W1 with `zip` installed) | `extracted/out` holds `a.txt` and not `b.key` |
| D6 | `shlane run archive` (W1 **without** a `zip` binary) | fails with `'zip' is not installed, and the PowerShell fallback cannot exclude anything; install zip, or drop the exclude argument`. Refusing is correct: silently including `b.key` would be the bug |
| D7 | W1 without `zip`, the `exclude:` line removed | passes, through PowerShell's `Compress-Archive` |
| D8 | press Ctrl-C during `shlane run timeout` (L) | shlane stops, and so does `sleep` |

## E. Windows only

| # | Machine | Run | Expected |
|---|---|---|---|
| E1 | W1 | `shlane run masking` in **cmd.exe** and in **PowerShell** | same as D2: steps go through Git's bash whichever shell started shlane |
| E2 | W2 | `shlane run masking` | fails with `shlane runs steps in a POSIX shell, and there is none on PATH. Install Git for Windows, which ships one, or point SHLANE_SHELL at the shell you want.` |
| E3 | W2 | `set SHLANE_SHELL=C:\Program Files\Git\bin\bash.exe` then E2 again | passes: the override is honoured |
| E4 | W3 | `shlane run masking` | passes. It must **not** say "has no installed distributions": that means it picked `C:\Windows\System32\bash.exe`, the WSL launcher |
| E5 | W1 | `shlane plugin verify` in a project with a `.sh` plugin (the README's `line-notify` example) | the plugin answers `describe`: a script plugin runs through the shell, since Windows has no shebangs |

## F. Android (L1 and W1, with the Android SDK and Gradle 8.13+)

```sh
cd examples/android-sample
shlane run test
shlane run release
```

Expected: `report_count=2`, then `signed build/counter-signed.apk and
build/counter-signed.aab`, and no `*.aligned.apk` left anywhere under `app/build`.
`apksigner verify --print-certs build/counter-signed.apk` shows `CN=shlane sample`.
On W1 this also checks that `ANDROID_HOME` paths with backslashes reach `zipalign`
and `apksigner` intact.

## G. Migration (one Linux and one Windows machine)

Put the Fastfile from [`../fastlane-in-15-minutes.md`](../fastlane-in-15-minutes.md)
in `fastlane/Fastfile` and run `shlane migrate`, then `shlane list`.

Expected: `4 lane(s), 9 action(s) converted`, and the lanes `android_test`, `beta`,
`deploy`, `ios_test`. On Windows, also check `shlane.yaml` reads back cleanly after an
editor saves it with CRLF line endings: `shlane validate` should give the same result.

## H. The GitHub Action

In a scratch repository, a workflow with one job per runner:

```yaml
jobs:
  try:
    strategy:
      matrix:
        os: [ubuntu-latest, ubuntu-24.04-arm, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: prongbang/shlane@v0.2.3
      - run: shlane --version
        shell: bash
```

Expected: passes on both Ubuntu runners. On `windows-latest` it fails today, for the
same reason as A4 — the action runs `install.sh`. Record the log line; it is the same
fix.

## Done when

- Every check passes on L1 and W1, except the known gaps (A2, A4, and H on Windows),
  which should fail exactly as described.
- L2 and L3 pass A, B, C and D.
- Anything else that fails is opened as an issue with the record above, the command,
  and its full output. D1 failing is a security bug and goes first.

## What the run is likely to turn into

The known gaps have one fix between them: `install.sh` has to recognise
`MINGW*`/`MSYS*`/`CYGWIN*` and pick the Windows tarball (unpacking `shlane.exe`), and
pick the musl build when `ldd --version` mentions musl or `/etc/alpine-release`
exists. That also makes the GitHub Action work on Windows runners.
