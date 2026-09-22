# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-09-22

### Changed

- **BREAKING: `sign_android` takes `keystore_file` in place of `keystore` and
  `keystore_base64`.** It accepts a path to the keystore or the keystore itself as
  base64, the way CI carries it in a secret; an existing file wins. A decoded
  keystore is still written next to the build and removed once signing is over.
  Rename the argument: `shlane validate` names `keystore_file` for a config that
  still uses either old one.

## [0.4.0] - 2026-09-22

### Added

- **`asc_api_key`** packs `key_id`, `issuer_id` and `key` (PEM, base64 or a path),
  or `key_path` (always read as a file), into the base64 `{keyId, issuerId, authKey}`
  object that `api_key` reads, so a lane can move to one credential without
  hand-building JSON. The result is masked.
- **`shlane action run <name> key=value...`** runs one built-in action with no
  `shlane.yaml` and prints its outputs; a single output prints as its bare value, so
  `API_KEY=$(shlane action run asc_api_key key_id=... issuer_id=... key=...)` works.
- **A documentation site**, at <https://prongbang.github.io/shlane/>. `docs/site/` is
  an mdBook, published by `.github/workflows/docs.yml` on every push to `master` that
  touches the README, `docs/` or the changelog. Its pages carry no prose of their own:
  each one includes Markdown that already exists in the repository, so the site and
  what a reader sees on GitHub cannot drift apart.
- **`tests/docs_site.rs`**, which is what makes that safe. mdBook renders an include
  whose file moved or whose anchor was renamed as a page holding nothing but its
  title, and still exits 0, so a green build proves little on its own. The tests fail
  on a broken include, a page missing from `SUMMARY.md`, a stale anchor, and a README
  section that was never given a page.

### Fixed

- **The README's relative links now work off GitHub.** Links such as
  `docs/schema-v1.md` and `CHANGELOG.md` resolved against whatever page they were
  rendered on, so every one of them was broken on crates.io. They are absolute now,
  pointing at the documentation site for prose and at GitHub for repository files.
- **`timeout:` did not stop a step on Windows.** It was reported as stopped, with the
  right message, and the lane took as long as the step would have anyway: Windows has
  no process group to signal, and killing the shell leaves what it started holding the
  pipes shlane reads, so shlane went on waiting for the step it had just stopped. The
  whole tree is taken now. On a 2 s timeout over a 10 s step, the lane took 10 s and
  now takes 2 s.
- **`install.sh` no longer asks api.github.com which version is latest.**
  `releases/latest` redirects to the tag, which needs no token and is not rate limited;
  the API is, per address, so an office or a CI runner that shares one could be told
  `403` without having asked for anything. The API is still the fallback.
- **`install.sh` retries a download that failed for a reason that can change.** Git for
  Windows' curl gives up with `CRYPT_E_REVOCATION_OFFLINE` when it cannot reach the
  server that answers for certificate revocation, which has nothing to do with the
  file being fetched. It tries three times, and does not retry an HTTP error: a 404
  will not become a 200.
- **`install.sh` picks the musl build on Alpine** and anywhere else `ldd` reports musl,
  instead of the glibc one, which does not run there. On aarch64, where there is no musl
  release yet, it says so rather than installing a binary that cannot start.
- **`install.sh` recognises Git Bash, MSYS2 and Cygwin** (`MINGW*`, `MSYS*`, `CYGWIN*`)
  and installs `shlane.exe` from the Windows release. It used to refuse with `no
  prebuilt binary for MINGW64_NT-10.0-... x86_64` although that binary exists.
- **The GitHub Action installs anything at all.** `version: latest`, its default,
  became `SHLANE_VERSION=latest` and the installer asked for
  `shlane-latest-<target>.tar.gz`, which is a 404 — on every runner, since the first
  release. The expression meant to blank it out,
  `inputs.version == 'latest' && '' || inputs.version`, returns `latest`, because an
  empty string is false to GitHub. CI now runs the action on Ubuntu x86-64, Ubuntu
  arm64 and Windows, with the default and with a pinned version.
- **The GitHub Action works on `windows-latest`.** It refused Git Bash's `uname` like
  the installer did, and it also handed bash the runner's native `D:\a\_temp\...`
  paths. It now converts them
  with `cygpath`, and runs the `install.sh` that ships with the action rather than
  whatever is on `master`, so the script and the action are always the same version.

## [0.3.0] - 2026-09-22

### Added

- **One Apple credential secret for every App Store Connect action.** `testflight`,
  `asc_request`, `appstore`, `provisioning_profile`, and `certificate` now accept
  `api_key` as JSON or base64 JSON containing `keyId`, `issuerId`, and `authKey`.
  The previous `key_id`, `issuer_id`, and `key` triplet remains supported.
- **Base64 Google Play service accounts.** `play_store.service_account_json` now
  accepts raw JSON, an existing JSON file, or base64 JSON without writing the decoded
  credential to disk.

## [0.2.3] - 2026-09-19

### Changed

- **The release workflow checks the tag against `Cargo.toml` before it builds
  anything.** The first `v0.2.2` was tagged ahead of the version bump and published
  binaries named 0.2.2 that reported 0.2.1. There is no change to shlane itself.

## [0.2.2] - 2026-09-19

### Changed

- **The release workflow publishes to crates.io** after the GitHub Release, once a
  `CARGO_REGISTRY_TOKEN` secret is set. It checks the tag against `Cargo.toml` first,
  and skips a version that is already published.
- **`shlane migrate` turns `increment_build_number` into `agvtool`**, which is what
  fastlane runs, instead of `bump_version`. `bump_version` edits `Cargo.toml`,
  `package.json`, `pubspec.yaml` or `VERSION`, and a native Xcode project has none of
  them, so the converted lane failed. `build_number:` becomes `agvtool new-version -all
  <n>`, `xcodeproj:` runs it in that project's directory, and a Ruby expression is left
  for a person.

## [0.2.1] - 2026-09-19

### Added

- **`build_ios` takes `skip_export`**, to stop after the archive. Without an export
  there is no `.ipa`, so nothing needs signing. `shlane migrate` maps gym's
  `skip_package_ipa` to it.
- **[Move off fastlane in 15 minutes](https://prongbang.github.io/shlane/fastlane-in-15-minutes.html)**, a walk
  through one ordinary Fastfile from `shlane migrate` to CI, with the output shlane
  really printed.
- **`examples/ios-sample` has an app target now** (`App/project.yml`, generated with
  XcodeGen), and the macOS CI job archives it with `build_ios`. Until now nothing had
  run `build_ios` against a real Xcode.

### Fixed

- **Two lanes with the same name silently became one.** A config is read into a map,
  which keeps the last of two equal keys; the first lane disappeared from `list` and
  `run` without a word. Duplicate keys anywhere in `shlane.yaml` are an error now.
- **`shlane migrate` produced those duplicates** from any Fastfile with, say, both
  `ios test` and `android test`. A lane name used on more than one platform now takes
  the platform (`ios_test`, `android_test`), and the summary says how to call it.
- **The README pointed at `prongbang/shlane@v1`**, a tag that does not exist, so a
  workflow copied from it failed. It names the release tag now.
- **scan's `devices` came out as `"[\"iPhone 16\"]"`**, which is not a destination
  `xcodebuild` accepts. It becomes `platform=iOS Simulator,name=iPhone 16`.

### Removed

- **The Homebrew tap.** `packaging/homebrew/` and the release job that pushed the
  formula are gone. `install.sh` and the release tarballs cover macOS and Linux, and
  a second repository with its own token to keep alive was not worth it.

## [0.2.0] - 2026-09-19

Everything below, from the foundations through M7. 0.1.0 was the 199-line prototype
this replaced; it was never published to crates.io, so this is the first release
anyone can install.

### Added before tagging

- **`notify_discord` and `notify_teams`**, the two notifications the plan named that did
  not exist. Both mask the webhook URL like `notify_slack` does. Teams gets an Adaptive
  Card, which both Workflows webhooks and the older connectors accept.
- **`clean_build_artifacts`** deletes files or whole directories (`.xcarchive`,
  `.dSYM`) matched by the same patterns as `copy_artifacts`. It refuses absolute paths
  and `..`, never enters `.git`, and removes a symlink rather than what it points at,
  because `**` can reach more than the person who wrote it meant.
- **`docs/migration.md`**, the full fastlane-to-shlane action table. It is generated
  from `src/migrate/mapping.rs`, and a test fails when the two differ;
  `UPDATE_DOCS=1 cargo test` rewrites it.
- **`examples/android-sample`**, a small Java app with unit tests. A new CI job runs its
  tests, builds an APK and an AAB, signs both with `sign_android` and checks the
  signatures.
- **A nightly CI run** (03:17 UTC), so both sample projects build against current
  toolchains even on days nobody pushes.

### Changed before tagging

- **`shlane migrate` points `match`, `sigh` and `cert` at `codesign_sync`,
  `provisioning_profile` and `certificate`** instead of saying there is no equivalent.
  They are still left for a person to move, because the fastlane actions can create
  certificates and profiles and the shlane ones only read existing ones.

### Fixed before tagging

- **`sign_android` left a `*.aligned.apk` behind** next to every APK it signed. The
  zipaligned copy is an intermediate; it is removed now whether signing succeeds or
  not, so the next `**/*.apk` pattern does not pick it up.
- **Windows actually works now, rather than only compiling.** Adding the target and its
  CI job in the same change meant the tests never ran there; once they did, three real
  gaps showed up.
  - **A plugin whose entry is a script could not start.** `Command::new(entry)` relies on
    a shebang, which Windows does not have, so a `.sh` plugin failed with "%1 is not a
    valid Win32 application". Anything that is not a native executable now goes through
    the same POSIX shell steps already use.
  - **`zip` and `unzip` have no binary on Windows.** They fall back to PowerShell's
    `Compress-Archive` and `Expand-Archive`, which store and restore the directory the
    same way `zip -r` does. `exclude` is refused rather than silently dropped in that
    path: a file the lane asked to keep out could be a keystore, and an archive gets
    uploaded.
  - **Every step could have run through the WSL launcher.** `bash.exe` was looked up on
    `PATH` but handed to Windows as a bare name, and Windows resolves that against the
    system directory first — where `bash.exe` is the Windows Subsystem for Linux
    launcher. With no WSL distribution installed it exits 1 saying so, in UTF-16, which
    is what `shlane plugin verify` was actually running. The shell is resolved to a full
    path now, and that launcher is skipped.
  - A test hardcoded `/bin/sh` as the shell to switch to.
- **The Windows build failed on an unused import.** `src/runtime/signals.rs` had only a
  `#[cfg(unix)]` test, so its module was empty there and `use super::*` became unused,
  which `-D warnings` makes fatal. It has a test that runs everywhere now. The original
  cross-check missed it because it ran without `RUSTFLAGS`.

### Added since the milestones above were written

- **`setup_ci`** prepares a CI machine for signing and registers the cleanup that
  removes the keychain when the run ends, whether it passed or failed. A build machine
  that keeps the keychain the last job created is one that stops being able to sign.
  Off CI it says so and does nothing, rather than taking over a developer's default
  keychain.
- **`appstore`**, in place of `deliver`: App Store metadata over the App Store Connect
  API, attaching a build, and submitting for review. It reads a fastlane-shaped
  metadata directory. A locale the version does not have is reported rather than
  created, and `name.txt` and `subtitle.txt` are called out as app-level metadata
  shlane does not set instead of being read and dropped. Not verified against Apple.
- **`provisioning_profile` and `certificate`** download what Apple already holds, and
  like `codesign_sync` neither issues nor revokes anything. `certificate` says out loud
  that Apple returns only the public certificate, so nobody discovers that at the
  signing step.
- **`xcode_settings`** changes signing settings in a `.xcodeproj`, in every build
  configuration, and warns about a setting the project does not declare rather than
  reporting a success that changed nothing.
- **`which_tool`, `git_pull`, `zip`, `unzip`, `copy_artifacts`, `download` and
  `template_render`** -- the P1/P2 actions from the plan. 25 actions to 36.
- **`call_lane()` in scripts**, and **`action()` inside a Rhai plugin**. Both were
  blocked on the same thing: a builtin runs in an engine the runner owns. `call_lane()`
  builds a second runner over the same shared state; the registry is now reached
  through a weak handle, so it and the plugin it holds no longer keep each other alive.
- **`shlane plugin add` and `remove`.** `add` writes the entry into `shlane.yaml` as
  text, so comments and formatting survive, and takes the plugin's name from its
  manifest. `remove` refuses while a lane still calls one of the plugin's actions, and
  leaves a `path:` plugin's directory alone.
- **`shlane cache-paths`** reports what a config is going to download, worked out from
  the actions and commands it contains.
- **A Windows build.** Steps run in a POSIX shell there -- the one Git for Windows
  ships -- because the escaping applied to every substituted value is POSIX and
  `cmd.exe` would re-interpret it. `SHLANE_SHELL` points shlane at another shell.
- **`docs/schema-v1.md`**, the schema reference and the compatibility promise, with
  `tests/schema_v1.rs` to stop the document and the code drifting apart.
- **`benchmarks/`**, comparing shlane with fastlane on the same lanes.
- **A Homebrew formula** generated from a release's checksums, pushed to the tap by the
  release workflow.

### Fixed since the milestones above were written

- **A value substituted into an action argument that is run as a shell command was not
  escaped.** `action: sh` with `command: echo ${params.x}` ran what a parameter of
  `x; touch PWNED` asked for, while the same thing written as `run:` was safe. The
  guarantee should not depend on which spelling was used.
- **An error raised inside a script was stringified at every level.** A lane calling
  itself printed sixteen nested "step script failed: lane: ..." wrappers with the real
  reason at the end; it now reports the reason.

### Changed

- **MSRV is 1.89**, up from 1.85: `aes` 0.9.3 requires it. `cargo install` resolves
  dependencies fresh rather than from the committed lockfile, so the declared version
  has to be what a fresh resolve needs.

### Notes

- The iOS actions, `play_store` and `appstore` have not been run against the real
  services: those need an Apple developer account and a Google service account. What
  each of them builds -- the commands, the plists, the JWT claims, the request bodies
  -- is tested here.
- The Windows binary is built and type-checked, and the tests run on `windows-latest`,
  but nobody has used it on a real Windows machine.


### Milestone M7 — CI integration and distribution

#### Added

- **`shlane env`** shows the environment a lane would run with, masked. It lists what
  the config contributes and says how many variables are inherited; `--all` dumps
  everything. Printing the whole process environment by default would have made this
  the easiest way to leak a token.
- **CI detection** for GitHub Actions, GitLab, Bitrise, CircleCI, Jenkins, Buildkite,
  Travis, TeamCity and Azure Pipelines, available to conditions and scripts as
  `is_ci()` and `ci_provider()`. `CI=false` is respected: people set it deliberately.
- **GitHub annotations.** A failure is also emitted as `::error title=shlane::`, so it
  appears on the pull request rather than only in the log.
- **`install.sh`**, which verifies every download against the release's `SHA256SUMS`
  before installing, and refuses to install if the checksums cannot be fetched.
- **`action.yml`**, a composite GitHub Action: `uses: prongbang/shlane@v1` with a
  `lane` to run.
- **A release workflow** building macOS (arm64, x86-64) and Linux (x86-64, arm64,
  musl), with checksums and release notes taken from the changelog.

#### Notes

- No Windows binary at this point: the runner still shelled out to `sh` unconditionally.
  Added later in this release, running in the POSIX shell Git for Windows ships.

### Milestone M5 — iOS, continued

#### Added

- **`test_ios` writes a JUnit report** from the `.xcresult` when given `junit:`. The
  report is written even when the suite fails, which is when it matters; a report that
  could not be produced is a warning rather than a failed lane. Needs Xcode 16 or newer.
- **`examples/ios-sample`**: a small SwiftUI counter with unit tests, as a Swift
  package rather than a generated `.xcodeproj` — a few readable lines instead of a
  pbxproj nobody can review. The macOS CI job runs it through shlane, checks the JUnit
  report describes the real run, and keeps the raw `xcresulttool` output as an
  artifact, because Apple's schema moves and a change should be readable rather than
  guessed at.

#### Fixed

- **`build_ios` and `test_ios` demanded a workspace or a project.** A Swift package has
  neither; `xcodebuild` resolves it from the working directory, and now so do they.
- **A dry run failed on a reference to a step output that had not been produced.**
  `${steps.build.ipa}` now stands in for itself during a dry run, so a lane can be
  checked without running it. A misspelled parameter is still an error, dry run or not.

### Milestone M6 — plugins and migration

#### Added

- **Plugins.** A directory with a `shlane-plugin.yaml` manifest and an executable that
  speaks a small JSON protocol. Its actions are validated, listed and documented
  exactly like the built-ins, arguments the manifest marks sensitive are masked, and a
  plugin can declare a secret it obtained at runtime.
- **`shlane plugin list|lock|verify`.** `lock` records each executable's SHA-256 in
  `shlane-plugins.lock` and a plugin that no longer matches is refused: a plugin runs
  with the same permissions as shlane, on the machine holding the signing keys.
  `verify` asks each plugin to describe itself and reports where its manifest has
  drifted — a `sensitive` flag that only the manifest carries is the one that leaks.
- **`shlane migrate`** converts a Fastfile: platforms, lanes, `desc`, `sh` and the
  actions in the mapping table, with `ENV["X"]` and `options[:x]` becoming `${X}` and
  `${x}`. Required arguments fastlane took from the Appfile are filled with visible
  `TODO-` placeholders, so the result validates and every gap is in one list instead of
  appearing one failed run at a time. Anything it does not understand is carried across
  as a `# TODO` comment rather than dropped, and it says so loudly when a conditional
  block is flattened — those steps now run unconditionally.

#### Added later in M6

- **`codesign_sync`** reads an existing fastlane `match` repository: clone, decrypt,
  import the certificate into a keychain, install the profile. This is the blocker that
  kept a team with a `match` repository from moving.
  - Decryption is OpenSSL-compatible and done in-process rather than by shelling out.
    macOS ships LibreSSL under the name `openssl`, and the differences there are
    exactly what has broken `match` for people before. Both the current `-md sha256`
    and the older `-md md5` form are read, and the implementation is tested against
    files real OpenSSL produced.
  - **Read-only.** `match` also creates and revokes certificates; getting that wrong
    takes away a team's ability to ship. Issuing stays with `match`.
  - `install: false` fetches and decrypts without touching a keychain, which is also
    how it is tested on Linux.

#### Added later in M6

- **Plugins can be written in Rhai.** A manifest with `script:` instead of
  `executable:` points at a `.rhai` file with one function per action. The function
  receives the declared arguments plus `dry_run` and returns a map of outputs. It gets
  the same builtins a lane's script has, except `action()` — the registry holds the
  plugin, so it cannot be handed the registry back; calling it says exactly that rather
  than "function not found". (Later in this release the registry became a weak handle,
  and `action()` works there too.)
- `shlane plugin verify` compiles a Rhai plugin and checks it defines a function for
  every action its manifest declares.

#### Fixed

- **`capture()` returned nothing under `--dry-run`**, so a script computing a tag from
  `capture("cat VERSION")` produced `v` instead of `v2.1.0`. Reads now run for real and
  only changes are skipped, which is what actions already did.

#### Changed

- Actions are looked up through a registry built per run, since which actions exist now
  depends on the config. `ArgSpec` holds owned strings so a plugin can describe itself.

#### Added later in M6

- **`shlane plugin install`** fetches the plugins a config declares with a `source:` —
  `github:owner/repo@tag`, a git URL, or an SSH remote. Fetching never happens as part
  of a run: a lane whose plugin is missing says so and stops, because installing one
  means putting someone else's code on the machine that holds the signing keys.
  - A source with no `@tag` is flagged: an unpinned tag can be moved afterwards.
  - If the lockfile already has an entry, a plugin that no longer matches is refused
    rather than installed — which is what catches a moved tag.
  - The fetch goes to a scratch directory first, so a half-finished clone is never left
    where a plugin is expected, and the clone's `.git` is discarded so a plugin cannot
    be updated in place without going through the checksum again.
  - What was fetched has to be the plugin the config named, speaking a protocol this
    build understands, with the executable its manifest points at.

### Milestone M5 — iOS

#### Added

- **`build_ios`** archives and exports, writing the `ExportOptions.plist` that
  `-exportArchive` requires and then finding the `.ipa`.
- **`test_ios`** runs a scheme's tests in a simulator and reports the `.xcresult`.
- **`keychain`** creates, unlocks or deletes a keychain, with the password passed
  through the environment rather than the command line.
- **`testflight`** uploads with `xcrun altool`, writing the `.p8` to a directory it
  points `API_PRIVATE_KEYS_DIR` at, with `0600` permissions, and deleting it afterwards.
- **`asc_request`** calls any App Store Connect endpoint with a signed ES256 token, so
  the parts of the API without a dedicated action are still reachable. Signing uses
  `ring`, already in the tree.

#### Notes

- Signing goes through Xcode's `-allowProvisioningUpdates` with an App Store Connect
  key — option C in `docs/plan/07-actions-ios.md`. A synced certificate store like
  fastlane's `match` is **not** implemented at this point; `codesign_sync` arrived in
  M6, read-only.
- `test_ios` does not convert `.xcresult` to JUnit at this point. Doing that means
  parsing `xcresulttool`'s output, whose shape cannot be checked without Xcode, and
  guessing at it would ship something that looks finished and is not. Added later in
  this release, against a fixture here and a real `.xcresult` on the macOS runner.
- Everything here needs macOS. The command construction, the plist and the token claims
  are unit-tested; the round trip is not.

### Milestone M4 — Android

#### Added

- **`gradle`, `build_android`, `test_android`, `sign_android`.** `build_android`
  assembles an APK or bundle and then finds it, because Gradle does not say where it
  put things; `test_android` collects the JUnit files it produced; `sign_android`
  handles a keystore given as a path or as base64, writing the decoded file with
  `0600` permissions and deleting it afterwards even when the step fails.
- **`play_store`** uploads to Google Play, signing its own service-account JWT with
  `ring` (already in the tree via rustls) — no `gcloud`, no extra dependency tree. The
  edit is committed only after the upload and track update both succeed, so a failure
  part-way leaves the store untouched.
- **`firebase_distribution`** wraps the `firebase` CLI. The REST upload returns a
  long-running operation that has to be polled, and an action nobody can test against
  the real service is worth less than one that delegates to the tool Google maintains.
- **`--report <format>:<path>`**, repeatable, writing `junit`, `json` or `md`. Reports
  are written whether the lane passed or failed, and Markdown is appended so it can be
  pointed at `$GITHUB_STEP_SUMMARY`.

#### Fixed

- **A gradle property routed through the environment for being sensitive was not
  masked.** shlane kept the value off the command line and then printed it when gradle
  echoed it back.

#### Notes

- `play_store`'s request shapes are unit-tested; the round trip against Google is not,
  and needs a real service account.

### Milestone M3 — the action system

#### Added

- **Actions.** A step can be `action: <name>` with `with:` arguments. Each action
  declares its arguments, so `shlane validate` catches a missing or misspelled one
  before anything runs, and `shlane action list` / `shlane action show <name>`
  document them.
- **Thirteen actions to start with**: `sh`, `ensure_env_vars`, `git_status_clean`,
  `git_branch`, `git_commit`, `git_tag`, `git_push`, `last_git_tag`,
  `changelog_from_commits`, `read_version`, `bump_version`, `http_request` and
  `notify_slack`. `bump_version` understands `Cargo.toml`, `package.json`,
  `pubspec.yaml` (including Flutter's `+build` number) and a plain `VERSION` file.
- **An action's outputs** land under the step's `id`, so `${steps.bumped.version}`
  works the same way a command's `stdout` does.
- **`action(name, #{...})` in scripts**, returning the outputs as a map.
- **Arguments marked sensitive are masked automatically** — a Slack webhook URL never
  reaches the output, even in an error.
- HTTP actions retry transport failures and 5xx with a backoff, because CI networks
  fail often enough that one attempt is not enough.

#### Changed

- **`--dry-run` now runs an action's reads for real** — `git status`, `git describe`,
  reading a version file — and only describes its changes. Returning invented results
  for everything made `git_commit` report "nothing to commit" during a dry run of a
  release that had plenty to commit: a dry run that fabricates results reports problems
  that do not exist and hides the ones that do.
- **MSRV is now 1.85**, raised from 1.74 by `ureq`.
- `ureq` rather than the `reqwest` named in the plan: a blocking CLI does not need to
  carry an async runtime. Binary size 3.6 MB → 5.4 MB.

#### Fixed

- **`read_version` gave up on `package.json`.** A `?` inside the line loop returned
  from the whole function on the first line that did not match, so only a file whose
  very first line held the version was ever read.

### Milestone M2 — environment, secrets and the script API

#### Added

- **`env_files:`** reads `.env` files, with `--env <profile>` selecting
  `.env.<profile>`. Precedence, lowest first: config `env:`, each file in order, the
  environment shlane was started with, the lane's `env:`, the step's `env:`. A file
  whose path contains an unknown variable is skipped rather than failing.
- **Secret masking.** Values whose names end in `_TOKEN`, `_SECRET`, `_PASSWORD`,
  `_KEY` or `_CREDENTIALS`, values listed under `secrets:`, and anything a script
  passes to `secret()` are replaced with `***` in the echoed command, the command's
  own stdout and stderr, error messages, the summary and the JSON stream.
- **Step outputs.** A step with an `id:` publishes `stdout`, `stderr` and `code`,
  readable as `${steps.<id>.<key>}` or `output(id, key)`.
- **A real script API**: `run()` (which now stops the lane on failure), `try_run()`,
  `capture()`, `param_or()`, `has_param()`, `set_env()`, `set_output()`, `output()`,
  `secret()` and `ui_message()` / `ui_success()` / `ui_error()`. `run()` returns a
  `CmdResult` with `.stdout`, `.stderr`, `.code` and `.success`.
- **`--json`**, **`-v/--verbose`** and **`-q/--quiet`**.
- **Ctrl-C** stops the running step, runs the `error` hooks and exits `130`.

#### Fixed

- **A lane's `env:` was parsed and then ignored.**
- **`print()` output was not masked**, so a secret a script printed reached the
  terminal in full.
- **An interrupted or timed-out step could outlive shlane.** Killing `sh` left what it
  had started running and holding the pipes shlane was reading, so shlane waited for
  the process it thought it had stopped — a `sleep 30` step took the full 30 seconds
  to "stop". Every step now runs in its own process group, and shlane forwards Ctrl-C
  to it.
- **The summary printed in `--json` and `--quiet` modes**, mixing human output into
  the event stream.

#### Changed

- `run()` in a script now raises on a non-zero exit instead of returning the code;
  use `try_run()` for the old behaviour. It returns `CmdResult`, not an integer.
- Command output is piped so it can be masked. It is still streamed line by line, but
  a command that colours its output only for a terminal will now see a pipe.
- The plan called for `tracing`; the events are emitted directly instead. A CLI needs
  a documented, stable event stream more than a subscriber stack.

### Milestone M1 — config schema v1 and the commands around it

#### Added

- **Schema v1**: `version`, `min_shlane`, lane `description` / `platform` / `private`,
  declared `params` with `type`, `required`, `default` and `values`, and global
  `before_all` / `after_all` / `error` hooks.
- **Richer steps**: `name`, `id`, `if`, `env`, `workdir`, `timeout`, `retry` and
  `continue_on_error`. A step is one of `run:`, `script:` or `lane:` — `action:`
  parses and reports that it arrives in M3.
- **`lane:` steps** call another lane, passing parameters with `with:`. Private lanes
  can only be reached this way. Loops between lanes are caught by validation.
- **Conditions** are Rhai expressions (`if: param("target") == "production"`) rather
  than `${...}` templates: shell quoting rules do not apply inside Rhai, so a template
  there could be mis-quoted without anyone noticing.
- **`shlane list`**, **`shlane validate`**, **`shlane init`** (which guesses Rust, Node,
  Flutter, Android or iOS) and **`shlane completions <shell>`**.
- **`--file`**, **`--cwd`** and **`--dry-run`**.
- **Config discovery**: `shlane.yaml` / `shlane.yml` is looked up from the current
  directory upwards, or taken from `$SHLANE_CONFIG`. Steps always run from the config's
  own directory, so a lane behaves the same wherever it is started.
- **A summary table** at the end of every run: each step, its result and its duration.
- **Namespaced references**: `${params.x}`, `${env.X}`, `${shlane.lane}`.
- Exit code `4` for a parameter that is missing, of the wrong type, or not allowed.

#### Fixed

- **A timed-out step no longer leaves its work running.** Killing the shell left
  everything it had started — a `sleep`, a build — alive and holding the terminal.
  A step with a `timeout:` now runs in its own process group, which is stopped with
  `SIGTERM` and then `SIGKILL`. (Ctrl-C does not reach such a step yet; that needs a
  signal handler, see `docs/plan/04-cli-ux.md`.)
- **`shlane ... | head` no longer panics.** Rust ignores `SIGPIPE`, which turned a
  closed pipe into a backtrace.
- **`--dry-run` is read as a flag after parameters.** `trailing_var_arg` swallowed it,
  so `shlane run beta target=x --dry-run` silently ran for real.
- **Global hooks see the lane they surround**, so `${shlane.lane}` and the lane's
  parameters resolve inside `before_all`.

### Foundations from [`docs/plan/15-roadmap.md`](https://github.com/prongbang/shlane/blob/master/docs/plan/15-roadmap.md) — correctness,
structure and safety, so the action system can be built on something solid.

#### Fixed

- **Parameter values can no longer inject shell commands.** `${name}` is now
  shell-quoted, so `shlane run deploy "target=x; rm -rf /"` passes one literal
  argument instead of running the injected command. Use `${name:raw}` where a value
  is genuinely meant to expand into several shell words.
- **A failing Rhai script now fails the lane.** It was printed and ignored, which let
  a lane report success after its script had already errored.
- **Scripts that end in an expression now work.** `run("...")` as a script's last
  statement failed with "Output type incorrect", even though the command had run.
- **Shared `script:` functions are callable from lanes.** Rhai keeps function
  definitions in the compiled AST rather than the scope, so every call to a shared
  function failed with "Function not found" -- including `greet()` in
  `example/shlane.yaml`. They are now lifted into a module registered on the engine,
  with the shared script's top-level statements still running exactly once.
- **`print()` works.** Registering a function named `print` broke Rhai's
  value-to-string conversion, so *every* lane script failed — including the one in
  `example/shlane.yaml`. Printing now goes through the engine's print handler.
- **A missing lane exits non-zero.** It printed a message and exited `0`, so CI
  stayed green when a lane name was misspelled.
- **A misspelled config key is rejected** instead of being silently ignored.
- **Unresolvable `${...}` references are an error** instead of being passed through to
  the shell as literal text.
- **A failed step no longer runs the lane's `after` hooks.**
- **Quoting respects the surrounding context.** A `${...}` inside `"..."` or `'...'` is
  escaped in place instead of being wrapped in a second layer of quotes, which would
  otherwise put literal quote characters into the command's output.

#### Changed

- Exit codes are now meaningful: `1` lane failed, `2` bad config, `3` not found,
  `5` missing tool. See the README.
- Environment variables from `env:` are passed to child processes explicitly instead
  of via `std::env::set_var`, which mutated the whole process and is `unsafe` from
  Rust 2024 on.
- Errors name the lane, the phase, the step number and the command that failed, and
  config errors carry the line and column.
- Lane listings are sorted.
- `src/main.rs` is split into `cli`, `config`, `runtime`, `script` and `error` modules.
- `panic = "abort"` removed from the release profile — it turned every error into an
  unreadable crash.
- Rhai scripts run under operation, string and array limits so a runaway script cannot
  hang a CI job.

#### Added

- `README.md`, `LICENSE` (Apache-2.0) and this changelog. `Cargo.toml` referenced a
  README that did not exist, which would have failed `cargo publish`.
- Tests: unit tests for interpolation, config parsing and parameter handling, plus
  end-to-end tests that run the binary against real config files.
- CI running fmt, clippy (`-D warnings`) and the test suite on Linux and macOS.
- `docs/plan/`: the plan for replacing fastlane.

## [0.1.0]

- Initial release: lanes with `before`/`steps`/`script`/`after`, `${...}` parameters,
  and Rhai scripting with `param`, `env`, `run` and `print`.
