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

Expected: `Downloading shlane <version> for x86_64-unknown-linux-musl`, then `shlane
<version>`. Until this checklist was first written `install.sh` always picked the
**gnu** build, which does not run on musl; it now reads `/etc/alpine-release` and what
`ldd --version` says. `tests/install-sh.sh` covers the choice, but nothing has run the
installed binary on a real Alpine yet.

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

Expected: `Downloading shlane <version> for x86_64-pc-windows-msvc`, `Installed
~/.local/bin/shlane.exe`, then `shlane <version>`. This used to say `no prebuilt binary
for MINGW64_NT-... x86_64`; `install.sh` now treats `MINGW*`, `MSYS*` and `CYGWIN*` as
Windows. Record the exact `uname -s` if it still refuses.

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

Expected: passes on all three runners. CI now runs this matrix itself, in the `action`
job, against the checked-out action; a scratch repository checks the released tag, which
is the part CI cannot.

Running it was worth more than the checklist expected: `windows-latest` failed for the
same reason as A4, but *every* runner failed before that, and had since the action was
written. `version: latest`, the default, reached `install.sh` as `SHLANE_VERSION=latest`
and it asked for `shlane-latest-<target>.tar.gz`. The blanking expression,
`inputs.version == 'latest' && '' || inputs.version`, evaluates to `latest`: `''` is
false to GitHub, so the `||` arm wins.

## Done when

- Every check passes on L1 and W1.
- L2 and L3 pass A, B, C and D.
- Anything that fails is opened as an issue with the record above, the command, and its
  full output. D1 failing is a security bug and goes first.

## What the run turned into

The three known gaps had one fix between them, and it is in: `install.sh` recognises
`MINGW*`/`MSYS*`/`CYGWIN*` and unpacks `shlane.exe` from the Windows tarball, and picks
the musl build when `/etc/alpine-release` exists or `ldd --version` mentions musl.
`tests/install-sh.sh` holds the table of what it should pick for each machine, and CI's
`installer` and `action` jobs run the script and the action for real on Ubuntu x86-64,
Ubuntu arm64, macOS and Windows.

## The run

```
Machine:   L1
OS:        Ubuntu 24.04.4 LTS (container, x86-64, glibc 2.39)
shlane:    shlane 0.2.3 (the released x86_64-unknown-linux-gnu tarball)
Shell:     n/a
Results:   A1 pass, A3 pass*, B1-B4 pass, C pass, D1-D5 pass, D8 pass,
           G pass. D6, D7 and E are Windows-only. F and H not run: see below.
```

- **A3 on L1, not L3** (`*`): the musl tarball's checksum matched, `file` says
  `static-pie linked`, `ldd` says `statically linked`, and it ran on this glibc machine
  — B1–B4, C and D1–D5 all pass with the musl binary too. It has still never run on
  Alpine, which is what L3 is for.
- **D1** printed `it's $(whoami) & "quoted"` literally, and the trace shows shlane
  escaped it (`printf '%s\n' "it's \$(whoami) & \"quoted\""`). **D2** printed `token
  is ***` and the value appears nowhere. **D3** failed after 2.0 s with the exact
  message. **D4**: `sh -c sleep 10` and `sleep 10` were both running during the step and
  neither was left afterwards. **D8**: `SIGINT` to shlane alone (not to the process
  group, so nothing but shlane's own handling could clean up) left no `sleep` behind and
  exited 130 with `error: interrupted while running 'sleep 10' in lane 'timeout'`.
- **G** printed `4 lane(s), 9 action(s) converted, 0 line(s) left for you` and listed
  `android_test`, `beta`, `deploy`, `ios_test`, matching
  [`../fastlane-in-15-minutes.md`](../fastlane-in-15-minutes.md) line for line.
- **F was not run**: the machine has Gradle 8.14.3 and JDK 21 but no Android SDK, and
  its network policy blocks `dl.google.com`, so neither the SDK nor the Android Gradle
  Plugin can be fetched. CI's `android-sample` job covers this on Linux; W1 is still
  open.
- **H was not run** as a scratch repository. The two CI jobs above cover the same
  ground for the action as it stands on a branch.
- **L2, L3, W1, W2, W3 are still open.** The install side of L3 and W1 is now covered by
  `tests/install-sh.sh` and the `installer` job, but nothing has executed the musl binary
  on Alpine or any Windows binary on Windows outside `cargo test`.
