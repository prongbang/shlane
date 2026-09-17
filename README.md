# shlane

A fastlane-like automation tool written in Rust, with [Rhai](https://rhai.rs) scripting.

Define your build, test and release steps as *lanes* in a YAML file, then run them with
a single static binary — no Ruby, no `bundle install`, no gem conflicts, and a start-up
time measured in milliseconds.

```yaml
lanes:
  beta:
    description: "Build and ship to TestFlight"
    platform: ios
    steps:
      - action: git_status_clean
      - id: bumped
        action: bump_version
        with: { part: build }
      - id: build
        action: build_ios
        with: { workspace: MyApp.xcworkspace, scheme: MyApp }
      - action: testflight
        with:
          ipa: ${steps.build.ipa}
          key_id: ${ASC_KEY_ID}
          issuer_id: ${ASC_ISSUER_ID}
          key: ${ASC_KEY_P8}
```

```sh
shlane run beta
```

> **Status: early but real.** Everything documented here works and is covered by tests.
> What has *not* been verified against real hardware is called out where it appears —
> the iOS actions need macOS and Xcode, and `play_store` has never spoken to Google.
> The plan behind all of it is in [`docs/plan/`](docs/plan/README.md).

## Contents

- [Install](#install) · [Quick start](#quick-start) · [Commands](#commands)
- [Configuration](#configuration): [lanes](#a-lane), [steps](#a-step), [parameters](#parameters), [references](#references)
- [Actions](#actions): [core](#core-actions), [Android](#android), [iOS](#ios), [signing](#code-signing)
- [Environment and secrets](#environment-and-secrets) · [Scripting](#scripting)
- [Plugins](#plugins) · [Migrating from fastlane](#migrating-from-fastlane)
- [On CI](#on-ci) · [Reports](#reports) · [Exit codes](#exit-codes) · [Benchmark](#benchmark)

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/prongbang/shlane/master/install.sh | sh
```

Every download is checked against the release's `SHA256SUMS`, and the installer refuses
to proceed if the checksums cannot be fetched. `SHLANE_VERSION` pins a version and
`SHLANE_INSTALL_DIR` chooses where it goes.

From source:

```sh
cargo install --path .
```

Prebuilt binaries cover macOS (Apple Silicon and Intel) and Linux (x86-64, arm64,
musl). There is no Windows build: the runner shells out to `sh`, so it would fail on
the first step.

## Quick start

```sh
shlane init          # writes a starter shlane.yaml, guessing the project type
shlane list          # what lanes exist
shlane validate      # check the config without running anything
shlane run test
```

A minimal config:

```yaml
version: 1

env:
  APP_ENV: production

lanes:
  test:
    steps:
      - run: cargo test

  deploy:
    params:
      target:
        required: true
        values: [staging, production]
    steps:
      - run: ./scripts/deploy.sh ${target}
```

```sh
shlane run deploy target=staging
```

## Commands

| Command | What it does |
|---|---|
| `shlane run <lane> [key=value ...]` | Run a lane |
| `shlane list` | Every lane, with its description and parameters |
| `shlane validate` | Check the config without running anything |
| `shlane init` | Write a starter config, guessing the project type |
| `shlane env` | The environment a lane would run with, secrets masked |
| `shlane action list` / `show <name>` | The built-in actions and their arguments |
| `shlane plugin install/list/lock/verify` | Fetch and inspect plugins |
| `shlane migrate` | Convert a Fastfile into a `shlane.yaml` |
| `shlane completions <shell>` | A shell completion script |

| Flag | What it does |
|---|---|
| `-f, --file <PATH>` | Use this config instead of searching for one |
| `-C, --cwd <DIR>` | Work from this directory |
| `--dry-run` | Print what would run, without running it (`run` only) |
| `--env <PROFILE>` | Select `.env.<profile>` |
| `-v, --verbose` / `-q, --quiet` | More detail / errors and command output only |
| `--json` | One JSON event per line, for other tools to read |
| `--report <fmt>:<path>` | Write results as `junit`, `json` or `md`, repeatable (`run` only) |

`shlane.yaml` (or `shlane.yml`) is looked up from the current directory upwards, so it
can be run from anywhere inside a project; `$SHLANE_CONFIG` overrides the search. Steps
run from the directory holding the config, so a lane behaves the same wherever it is
started.

## Configuration

### Top level

| Key | Meaning |
|---|---|
| `version` | Schema version. `1` today |
| `min_shlane` | Minimum shlane version this config needs |
| `env` | Environment variables for every command in every lane |
| `env_files` | `.env` files to read, lowest priority first |
| `secrets` | Values to hide wherever they appear |
| `plugins` | Plugins to load |
| `script` | Rhai evaluated once before a lane runs — shared functions live here |
| `before_all` / `after_all` | Steps run around the lane |
| `error` | Steps run when a lane fails |
| `lanes` | The lanes themselves |

### A lane

| Key | Meaning |
|---|---|
| `description` | One line, shown by `shlane list` |
| `platform` | Free-form grouping, e.g. `ios` |
| `private` | When true, only reachable from another lane |
| `params` | Declared parameters, checked before the lane runs |
| `env` | Environment variables for this lane only |
| `before` / `steps` / `after` | The lane's steps |
| `script` | Rhai run after the steps |

The phases run in order: `before` → `steps` → `script` → `after`.

### A step

A step is one of `run:` (a shell command), `action:` (a built-in or a plugin),
`script:` (Rhai) or `lane:` (another lane).

```yaml
steps:
  - name: upload                          # shown in logs and the summary
    id: upload                            # publishes this step's outputs
    run: ./scripts/upload.sh ${target}
    if: param("target") == "production"   # a Rhai expression
    workdir: ./ios                        # relative to the config file
    env: { EXTRA: value }
    timeout: 20m                          # 30, 30s, 20m, 1h
    retry: 2                              # extra attempts after the first
    continue_on_error: false

  - action: git_tag
    with: { name: "v1.2.3" }

  - lane: notify                          # call another lane
    with: { channel: "#releases" }
```

A bare string is a `run:` step, so `before: [echo hi]` works.

### Parameters

```yaml
params:
  target:
    type: string            # string | int | bool
    required: true
    values: [staging, production]
    description: "Where to deploy"
  notes:
    default: "no notes"
```

A missing required parameter, a value outside `values`, or one of the wrong type stops
the lane before anything runs, with exit code `4`.

### References

`${name}` looks in parameters, then in `env`. The namespaced forms say exactly where to
look:

| Reference | Resolves to |
|---|---|
| `${params.x}` | A parameter |
| `${env.X}` | An environment variable |
| `${steps.<id>.<key>}` | What an earlier step produced |
| `${shlane.lane}`, `${shlane.version}` | The run itself |

A step with an `id:` publishes what it did — a command publishes `stdout`, `stderr` and
`code`; an action publishes whatever it produced:

```yaml
steps:
  - id: version
    run: cat VERSION
  - run: echo "building ${steps.version.stdout}"
```

A name that resolves to nothing is an error, so a typo fails the lane rather than
passing `${targt}` to your shell.

**Values are shell-quoted**, and the quoting follows the surrounding context: a
reference inside `"..."` or `'...'` is escaped in place rather than wrapped in another
layer. A value containing `;`, spaces or backticks cannot inject a command. Where a
value really is meant to expand into several words, ask for it:

```yaml
  - run: cargo build ${flags:raw}      # flags="--release --locked"
```

Write `$${literal}` for a literal `${literal}`. Plain shell syntax (`$HOME`, `$(date)`)
passes through untouched.

## Actions

An action is a named step that knows its own arguments, so `shlane validate` checks it
before anything runs and `shlane action show <name>` documents it.

Under `--dry-run`, an action's *reads* still run — `git status`, `git describe`, reading
a version file — while its *changes* are only described. A dry run that invents results
reports problems that do not exist and hides the ones that do.

### Core actions

| Action | What it does |
|---|---|
| `sh` | Run a shell command |
| `ensure_env_vars` | Fail early when a variable is missing |
| `git_status_clean` | Fail if the working tree is dirty |
| `git_branch` | The branch and short SHA |
| `git_commit` / `git_tag` / `git_push` | The release trio |
| `last_git_tag` | The most recent tag, or `found: false` |
| `changelog_from_commits` | Commit subjects since a tag |
| `read_version` / `bump_version` | `Cargo.toml`, `package.json`, `pubspec.yaml` or `VERSION` |
| `http_request` | Any HTTP call, with retries |
| `notify_slack` | Post to an incoming webhook |

A release with no shell in sight:

```yaml
lanes:
  release:
    steps:
      - action: git_status_clean
      - id: bumped
        action: bump_version
        with: { part: patch }
      - action: git_commit
        with: { message: "release: v${steps.bumped.version}" }
      - action: git_tag
        with: { name: "v${steps.bumped.version}" }
      - action: git_push
        with: { tags: true }
      - action: notify_slack
        with:
          webhook: ${SLACK_WEBHOOK}          # masked in every log line
          text: "Released v${steps.bumped.version}"
```

### Android

| Action | What it does |
|---|---|
| `gradle` | Run a Gradle task |
| `build_android` | Assemble an APK or AAB, and report where it landed |
| `test_android` | Run the unit tests and collect their reports |
| `sign_android` | Sign with a keystore, from a file or base64 |
| `play_store` | Upload to Google Play |
| `firebase_distribution` | Distribute through Firebase App Distribution |

```yaml
lanes:
  beta:
    platform: android
    steps:
      - action: test_android
      - id: build
        action: build_android
        with:
          format: aab
          flavor: prod
          properties: |
            KEYSTORE_PASSWORD=${KEYSTORE_PASSWORD}
      - action: play_store
        with:
          package_name: com.example.app
          aab: ${steps.build.aab}
          track: internal
          service_account_json: ${PLAY_SERVICE_ACCOUNT}
```

Gradle does not say where it put things, so `build_android` finds the artifact and
publishes its path. A property whose name looks sensitive is passed as
`ORG_GRADLE_PROJECT_<NAME>` instead of `-P<name>=` — a command line is visible in `ps`
and in most CI logs — and its value is masked on the way back.

`play_store` signs its own service-account JWT, so there is no `gcloud` to install. It
opens an edit, uploads, points the track at the new version code and commits, so a
failure part-way leaves the store untouched. **Its request shapes are unit-tested; the
round trip against Google is not.** `firebase_distribution` wraps the `firebase` CLI,
which must be installed.

### iOS

| Action | What it does |
|---|---|
| `build_ios` | Archive and export an `.ipa` |
| `test_ios` | Run tests in a simulator, optionally writing JUnit |
| `keychain` | Create, unlock or delete a keychain |
| `testflight` | Upload a build to TestFlight |
| `asc_request` | Any App Store Connect API call, authenticated |

```yaml
lanes:
  beta:
    platform: ios
    steps:
      - action: keychain
        with:
          name: shlane-ci.keychain-db
          password: ${KEYCHAIN_PASSWORD}
      - id: build
        action: build_ios
        with:
          workspace: MyApp.xcworkspace
          scheme: MyApp
          export_method: app-store
          team_id: ABCDE12345
      - action: testflight
        with:
          ipa: ${steps.build.ipa}
          key_id: ${ASC_KEY_ID}
          issuer_id: ${ASC_ISSUER_ID}
          key: ${ASC_KEY_P8}          # PEM, base64 of it, or a path
```

`build_ios` writes the `ExportOptions.plist` that `-exportArchive` insists on — the
part of `gym` people do not notice they are getting until they try to do without it —
and finds the `.ipa` afterwards. Leave out both `workspace:` and `project:` to let
`xcodebuild` resolve the directory, which is what a Swift package needs.

`test_ios` converts the `.xcresult` into JUnit when given `junit:`. The report is
written even when the suite fails — that is when it matters — and a report that could
not be produced is a warning, not a failed lane. It needs Xcode 16 or newer.

`testflight` uploads with `xcrun altool`, writing the `.p8` into a directory it points
`API_PRIVATE_KEYS_DIR` at, with `0600` permissions, removed however the step ends.
`asc_request` signs an ES256 token and calls any App Store Connect endpoint, so the
parts of the API without a dedicated action are still reachable.

**These need macOS and Xcode.** What to run is decided by functions that are tested
here; the round trip is not. [`examples/ios-sample`](examples/ios-sample) is a small
SwiftUI counter with unit tests that the macOS CI job runs end to end, checking that the
JUnit report describes the real run.

### Code signing

Two ways. Xcode's own `-allowProvisioningUpdates` with an App Store Connect key needs no
certificate store at all. If your team already has a fastlane `match` repository,
`codesign_sync` reads it:

```yaml
      - id: certs
        action: codesign_sync
        with:
          git_url: git@github.com:acme/certificates.git
          type: appstore            # or adhoc, development, enterprise
          app_identifier: com.example.app
          passphrase: ${MATCH_PASSWORD}
          keychain: shlane-ci.keychain-db
          keychain_password: ${KEYCHAIN_PASSWORD}
      - run: echo "signing with ${steps.certs.name} (team ${steps.certs.team_id})"
```

Decryption is OpenSSL-compatible and done in-process: macOS ships LibreSSL under the
name `openssl`, and the differences there are what has broken `match` for people before.
Both the current `-md sha256` and the older `-md md5` form are read, tested against
files real OpenSSL produced.

**It is read-only.** `match` also creates and revokes certificates, and getting that
wrong takes away a team's ability to ship. Keep issuing with `match`; let shlane consume
the repository. `install: false` fetches and decrypts without touching a keychain.

## Environment and secrets

```yaml
env:
  APP_ENV: production
env_files:
  - .env
  - .env.${SHLANE_PROFILE}     # selected by --env, skipped when there is no profile
secrets:
  - ${env.LICENCE_KEY}         # hide a value whose name does not look secret
```

Precedence, lowest first: config `env:`, each file in order, the environment shlane was
started with — so a value injected by CI always wins over one committed to a file — then
the lane's `env:`, then the step's.

**Secrets are masked in everything shlane prints**: the command it echoes, the command's
own stdout and stderr, error messages, the summary and the `--json` stream. A value is
secret when its name ends in `_TOKEN`, `_SECRET`, `_PASSWORD`, `_KEY` or `_CREDENTIALS`,
when it is listed under `secrets:`, when an action's argument is declared sensitive, or
when a script calls `secret(value)`.

Masking means reading the command's output, so a command that colours its output only
for a terminal will see a pipe and turn colour off.

`shlane env` shows what a lane would see, masked — what the config contributes by
default, everything with `--all`.

## Scripting

Lanes can run [Rhai](https://rhai.rs) for what YAML cannot express:

```yaml
script: |-
  fn tag_for(version) { "v" + version }

lanes:
  release:
    script: |
      let version = capture("git describe --tags");
      let result = try_run("./scripts/optional-check.sh");
      if !result.success {
        ui_error("check failed: " + result.stderr);
      }
      set_output("tag", tag_for(version));
```

| Function | Returns | Notes |
|---|---|---|
| `run(cmd)` | `CmdResult` | Stops the lane if the command fails |
| `try_run(cmd)` | `CmdResult` | Returns the failure instead of raising it |
| `capture(cmd)` | string | stdout, trimmed, without echoing — and it runs under `--dry-run` |
| `action(name, args)` | map | Run an action and get its outputs |
| `param(key)` / `param_or(key, default)` / `has_param(key)` | | |
| `env(key)` / `set_env(key, value)` | | `set_env` applies to later steps |
| `set_output(key, value)` / `output(id, key)` | | Pass values between steps |
| `is_ci()` / `ci_provider()` | bool / string | |
| `secret(value)` | | Hide a value computed at runtime |
| `ui_message` / `ui_success` / `ui_error` / `print` | | Masked like everything else |

`CmdResult` has `.stdout`, `.stderr`, `.code` and `.success`.

Use a `lane:` step to call another lane; there is no `call_lane()`.

## Plugins

An action shlane does not have can come from a plugin: a directory with a manifest and
either a program that speaks a small JSON protocol, or a Rhai script.

```yaml
plugins:
  - name: line-notify
    path: ./tools/line-notify                          # vendored
  - name: release-helpers
    source: github:someone/shlane-helpers@v0.1.0       # fetched
lanes:
  notify:
    steps:
      - action: notify_line          # validated like any built-in
        with:
          token: ${LINE_TOKEN}
          message: shipped
```

```yaml
# tools/line-notify/shlane-plugin.yaml
name: line-notify
version: 0.2.0
protocol: 1
executable: notify.sh        # or `script: helpers.rhai`
actions:
  - name: notify_line
    description: Send a LINE message
    args:
      - name: token
        required: true
        sensitive: true      # masked wherever it appears
      - name: message
```

An executable plugin reads one JSON object on stdin and writes one per line back:

```
{"protocol":1,"op":"run","action":"notify_line","args":{…},"context":{…}}

{"type":"log","level":"info","message":"sending"}
{"type":"secret","value":"a-token-it-just-obtained"}
{"type":"result","ok":true,"outputs":{"id":"msg-1"}}
```

A Rhai plugin is one function per action, given the declared arguments plus `dry_run`
and returning a map of outputs. It gets the same builtins a lane's script has, except
`action()` — the registry holds the plugin, so a plugin cannot be handed it back.

```rhai
fn tag_release(args) {
    let version = capture("cat VERSION");
    if args.dry_run { return #{ tag: args.prefix + version, created: "false" }; }
    run("git tag " + args.prefix + version);
    #{ tag: args.prefix + version, created: "true" }
}
```

### Keeping a plugin honest

A plugin runs with the same permissions as shlane, on the machine holding the signing
keys, so:

```sh
shlane plugin install     # fetches what the config declares; never part of a run
shlane plugin lock        # records each SHA-256; commit shlane-plugins.lock
shlane plugin verify      # each plugin against its own manifest
```

A lane whose plugin is missing says so and stops rather than downloading anything. A
source with no `@tag` is flagged, and if the lockfile already has an entry, a plugin
that no longer matches is refused — which is what catches a moved tag. The clone's git
history is discarded, so a plugin cannot be updated in place without going through the
checksum again.

## Migrating from fastlane

```sh
shlane migrate                       # reads fastlane/Fastfile, writes shlane.yaml
shlane validate
```

It converts platforms, lanes, `desc`, `sh` and the actions in the mapping table, and
turns `ENV["X"]` and `options[:x]` into `${X}` and `${x}`. Required arguments fastlane
read from the Appfile are filled with visible `TODO-` placeholders, so the result
validates and every gap is in one list instead of appearing one failed run at a time.

It is best effort by construction — a Fastfile is Ruby, and Ruby can do anything.
Anything it does not understand is carried across as a `# TODO` comment rather than
dropped, and the report says what needs a person. It says so loudly when a conditional
block is flattened: those steps now run unconditionally.

## On CI

```yaml
- uses: prongbang/shlane@v1
  with:
    lane: beta
    params: target=production
    args: --report junit:reports/shlane.xml
```

The action installs shlane — a 2.6 MB download, against the 48 seconds a cold
`bundle install` of fastlane took when [measured](benchmarks/README.md) — and runs the
lane.

shlane recognises GitHub Actions, GitLab, Bitrise, CircleCI, Jenkins, Buildkite, Travis,
TeamCity and Azure Pipelines. On GitHub a failure is also emitted as an `::error`
annotation, so it appears on the pull request rather than only in the log. Lanes and
scripts can branch on it with `if: is_ci()`.

Ctrl-C (or a `SIGTERM` from a CI shutting a job down) stops the running step, runs the
`error` hooks and exits `130`. Every step runs in its own process group, so nothing it
started survives.

## Reports

```sh
shlane run beta --report junit:reports/shlane.xml --report md:$GITHUB_STEP_SUMMARY
```

Every step becomes a test case, so a CI that understands JUnit shows which step failed.
Reports are written whether the lane passed or failed — one that only appears on success
is no use to the job that has to explain the failure. Markdown is appended, so it can
point at GitHub's step summary.

## Benchmark

Both tools were given the same three lanes and timed on the same machine — the harness
and the full numbers are in [`benchmarks/`](benchmarks/README.md). Medians of 10 runs,
fastlane 2.240.1 on Ruby 3.3.6:

| | fastlane | shlane |
|---|---|---|
| Start up and list the lanes | 1.274 s | 0.002 s |
| A lane with one no-op step | 1.278 s | 0.004 s |
| A lane with 20 shell steps | 1.477 s | 0.039 s |
| Peak memory | 78 MB | 12 MB |
| Cold `bundle install` vs unpacking the binary | 48.8 s | 0.06 s |

Nearly all of fastlane's cost is fixed — about 1.28 s of Ruby start-up per invocation,
then ~10 ms per step, against shlane's ~4 ms and ~1.8 ms. It is the fixed second that a
CI job pays every time.

None of this touches Xcode or Gradle. On a real release the build dominates and takes
the same minutes either way; what is measured here is only the overhead each tool adds
on top of it.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | The lane failed — a step returned non-zero, or a script errored |
| `2` | The config could not be parsed or did not validate |
| `3` | No config file, or no such lane (or the lane is private) |
| `4` | A parameter was missing, of the wrong type, or not allowed |
| `5` | A tool shlane needs is not installed |
| `130` | Stopped by Ctrl-C |

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

Minimum supported Rust version: **1.85** (required by `ureq`).

The plan the project is being built to — gap analysis against fastlane, the action
system, migration, milestones — is in [`docs/plan/`](docs/plan/README.md). Changes are
recorded in [CHANGELOG.md](CHANGELOG.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
