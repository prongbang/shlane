# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

- No Windows binary: the runner still shells out to `sh`, so it would not work there.
  Saying so is better than shipping one that fails on the first step.

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
  than "function not found".
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
  fastlane's `match` is **not** implemented; for a team that depends on one, that is
  the remaining blocker.
- `test_ios` does not convert `.xcresult` to JUnit yet. Doing that means parsing
  `xcresulttool`'s output, whose shape cannot be checked without Xcode, and guessing at
  it would ship something that looks finished and is not.
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

### Foundations from [`docs/plan/15-roadmap.md`](docs/plan/15-roadmap.md) — correctness,
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
