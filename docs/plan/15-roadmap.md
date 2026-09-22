# 15 — The roadmap

The order the work actually happens in. Every milestone has to **ship something
usable**, not leave a refactor half-finished.

## M0 — The foundations, needed before anything else ✅ done

| Work | Document |
|---|---|
| write `README.md`, and add the `LICENSE` file (Apache-2.0) | [14](14-release-and-distribution.md) |
| write characterization tests for today's behaviour | [13](13-testing-and-quality.md) |
| split `main.rs` into the modules in [02](02-architecture.md) | [02](02-architecture.md) |
| turn `expect()` into `Result` and `ShlaneError`, and remove `panic = "abort"` | [02](02-architecture.md) |
| set up CI: fmt, clippy (`-D warnings`), a test matrix | [13](13-testing-and-quality.md) |
| fix the command injection in interpolation | [03](03-config-schema.md) |

**Done when:** everything that worked before still works, with tests over it, CI is
green, and every error message can be understood.

## M1 — Config and CLI v1 ✅ done

- schema v1 from [03](03-config-schema.md): `version`, `params` with type/required/default,
  `description`, `private`, `platform`, `if`, `id`, `workdir`, `timeout`, `retry`,
  `continue_on_error`
- a `lane:` step, calling another lane, and the `error` hook
- finding the config by walking up the directory tree
- the `list`, `validate`, `init` and `completions` commands
- the exit codes from [04](04-cli-ux.md), the summary table at the end, and `--dry-run`

**Done when:** a real pipeline written with nothing but `run:` can replace a shell
script.

> Left over from M1: handling Ctrl-C, which needs a signal handler, plus `--json`,
> `--verbose/-q` and colour — moved in with the logging work in M2.

## M2 — The runtime and scripting ✅ done

- the full `LaneContext`, and every `env::set_var` gone
- `.env`, `env_files`, and the environment precedence ([10](10-secrets-and-env.md))
- the `SecretRegistry`, and masking on every path out
- the new Rhai API from [05](05-scripting-rhai.md): `run()` returning a struct,
  `set_output()`, `call_lane()`, the limits
- logging through `tracing`, `--json`, `--verbose/-q`

**Done when:** a test proves no secret escapes into the log in any form, and a value can
be passed from one step to another.

> Noted while implementing this: `call_lane()` and `action()` in Rhai mean re-entering
> the executor from inside a builtin, which means pulling the ownership apart. `action()`
> landed in M3; `call_lane()` later, by building a second runner over the same shared
> state instead of borrowing the one that is executing. Logging uses a `ui`
> module of its own instead of `tracing`: a CLI wants a steady event stream more than it
> wants a subscriber stack.

## M3 — The action framework, and the core actions ✅ done

- `trait Action`, the registry, and `shlane action list/show`
- the P0 actions from [06](06-actions-core.md): `sh`, `git_*`, `bump_version`,
  `read_version`, `notify_slack`, `ensure_env_vars`, `http_request`

**Done when:** a "bump version, commit, tag, push, tell Slack" lane needs no shell at
all.

> The P0 set done, and later the P1/P2 rest: `which_tool`, `git_pull`, `zip`, `unzip`,
> `copy_artifacts`, `download` and `template_render`, plus `action()` in Rhai.
> `call_lane()` came later still, with a nested runner rather than a nested executor.
>
> Changed from the plan: `ureq` instead of `reqwest`, because a blocking CLI should not
> carry an async runtime. The cost: MSRV moved 1.74 → 1.85, and the binary 3.6 → 5.4 MB.

## M4 — Android ✅ done

- `gradle`, `build_android`, `test_android`, `sign_android`
  ([08](08-actions-android.md))
- `play_store`, on Publishing API v3
- `firebase_distribution`
- the JUnit report ([11](11-ci-integration.md))

**Done when:** a real Android project can delete its `Gemfile`.

> Done: `gradle`, `build_android`, `test_android`, `sign_android`, `play_store`,
> `firebase_distribution`, and `--report junit|json|md`.
>
> Different from the plan: `firebase_distribution` wraps the `firebase` CLI (option 1 in
> the document) rather than the REST API, because the upload endpoint returns a
> long-running operation that has to be polled, and none of it could be tested against
> the real thing. REST is still future work.
>
> Not verified: `play_store` is only tested on the shape of the request, in unit tests.
> Really talking to Google needs a service account, so that is e2e work.

## M5 — iOS ✅ done, but not verified against a real Xcode

- `build_ios`, `test_ios` and reading `.xcresult`
- `keychain`, `setup_ci`
- the App Store Connect API (JWT), and `testflight`
- code signing option C — an API key plus `-allowProvisioningUpdates` — from
  [07](07-actions-ios.md)

**Done when:** a real iOS project can reach TestFlight from CI.

> Done: `build_ios`, including generating ExportOptions.plist, `test_ios`, `keychain`,
> `setup_ci`, `testflight` through altool, and `asc_request` (ES256 JWT with ring).
>
> - ~~`codesign_sync` / match~~ ✅ done in M6, read-only — it can read an existing match
>   repo
> - ~~`.xcresult` → JUnit~~ ✅ done (`test_ios` plus `junit:`). The parser is written to
>   survive a schema change; it is tested against a fixture here, and the real thing is
>   confirmed by a job on a macOS runner (`examples/ios-sample`), which keeps
>   `xcresulttool`'s raw output as an artifact to read when Apple changes the schema.
>
> **Not verified:** every one of these actions needs macOS and Xcode. The unit tests
> cover how the commands, the plist and the JWT claims are assembled, but nothing has run
> against the real thing.

## M6 — The ecosystem ⚠️ partly done

- external-executable and Rhai-module plugins ([09](09-plugins.md))
- `shlane migrate` and `docs/migration.md` ([12](12-migration-from-fastlane.md))
- a `codesign_sync` that can read an existing match repo (option A)
- the GitHub Action wrapper (the Homebrew tap was dropped after 0.2.0)
  ([14](14-release-and-distribution.md))

> Done: external-executable plugins (`path:` only), the SHA-256 lockfile,
> `plugin list/lock/verify`, and `shlane migrate`.
>
> **Not done:**
> - ~~fetching a plugin from a git host~~ ✅ done (`shlane plugin install`, checked
>   against the lockfile; never installed automatically during a run)
> - ~~Rhai module plugins (option B)~~ ✅ done
> - ~~a `codesign_sync` that reads an existing match repo~~ ✅ done, read-only
> - ~~the GitHub Action wrapper~~ ✅ done (`action.yml` and `install.sh`)
> - ~~the Homebrew tap~~ dropped after 0.2.0; `install.sh` covers macOS and Linux

## M7 — 1.0 ⚠️ partly done

> Done: CI detection and GitHub annotations, `shlane env`, `install.sh` with a checksum
> check, `action.yml`, and the release workflow (macOS arm64/x86-64, Linux
> x86-64/arm64/musl).
>
> `appstore` is done: metadata from a fastlane-shaped directory, attaching a build, and
> submitting for review, over the App Store Connect API. Not verified against Apple.
>
> Schema v1 is frozen: `docs/schema-v1.md` documents every key with the compatibility
> promise, and `tests/schema_v1.rs` fails if a key is accepted without being written
> down.
>
> `cargo publish --dry-run` passes, with `exclude` keeping the plan, the benchmarks and
> the sample projects out of what a `cargo install` downloads, and CI runs the dry run so
> it cannot regress. `v0.3.0` is the next release; `v0.2.3` remains the latest published
> tag on GitHub Releases and crates.io until the release workflow completes.
>
> **Not done:** the documentation site. The Homebrew tap was dropped.
>
> The Windows binary is built and released now. Steps still run in a POSIX shell there —
> the one Git for Windows ships — because the escaping applied to every substituted value
> is POSIX, and `cmd.exe` would re-interpret it. A script plugin goes through that shell
> too, and `zip`/`unzip` fall back to PowerShell.
>
> Adding the target and its CI job together was a mistake worth recording: the job could
> not compile, so the tests never ran there, and three real gaps sat hidden behind one
> unused-import error. The lesson is that a new platform's job has to be green on its own
> tests before the platform counts as supported. Nobody has used it on a real Windows
> machine yet, but its tests now run.

- the full documentation, on the web
- the nightly e2e on both platforms green for two weeks running

## If there is only time for some of it

If only three things get done: **M0 → M1 → M4.** That gives a tool that really can
replace a shell script on Android, without touching any of Apple's complexity.

## The main risks

| Risk | What it costs | What to do about it |
|---|---|---|
| **Scope creep, chasing all 400 actions** | it never finishes | hold to the P0/P1/P2 split in [06](06-actions-core.md), and let `run:` and plugins carry the rest |
| **`match` is the wall a large team cannot get over** | they cannot really move | do option C first for the quick value, then A in M6 |
| **Apple and Google change their APIs** | an action breaks silently | nightly e2e, and one HTTP layer so there is one place to fix |
| **A single maintainer** | bus factor of 1 | make plugins good from the start, so the community can fill the gaps |
| **"fastlane works fine as it is"** | nobody moves | focus on what actually hurts: setup time on CI, and error messages that can be read |
| **Testing iOS needs an Apple account** | it cannot be tested | pull `build_command()` out and test it pure ([13](13-testing-and-quality.md)) |

## What success is measured by

| Measure | Target |
|---|---|
| setup time on CI, against `bundle install` | under 5 seconds, from 30–120 |
| how long `shlane run` takes to start | under 100 ms |
| binary size | under 15 MB |
| moving a 100-line Fastfile | under an hour |
| real projects that deleted their `Gemfile` | at least one per platform before 1.0 |
