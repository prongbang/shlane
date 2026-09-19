# 16 — What is left

Where the project stands at `0.2.2`, and what remains. Written to be picked up on
another machine: each item says what to do, where, and how to know it worked.

Checked against the code rather than the plan — `shlane action list` reports 40
actions, `cargo test` 440, and `master` is green on Linux, macOS and Windows.

## Start here

```sh
git pull
cargo test                                    # 440
cargo clippy --all-targets --all-features -- -D warnings
```

A note that cost a few hours: **run each check as its own command and read its exit
code.** An `&&` chain that fails early reports success for the checks it skipped. And
if a change seems to have no effect, check the binary's timestamp — `cargo` can get a
stale fingerprint, and `cargo clean -p shlane` clears it.

## The four things nobody has verified

These are the real risk. Everything below them is small by comparison.

| | Needs | How to check |
|---|---|---|
| The iOS actions | macOS, Xcode, an Apple Developer account | `build_ios`'s export, `testflight`, `keychain`, `codesign_sync` against a real app. `test_ios` and an unsigned `build_ios` archive already run in CI |
| `appstore` | An App Store Connect key | `--dry-run` first: it reads Apple for real and prints every change without making one |
| `play_store` | A Google Play service account | Only the request shapes are tested |
| The Windows binary | A real Windows machine | CI runs its tests, but nobody has used it |

The unit tests cover what each one *builds* — the command line, the export plist, the
JWT claims, the request bodies. None of them cover the round trip. Treat a first real
run as debugging, not as confirmation.

## Release

`v0.2.2` is the latest, on [GitHub Releases](https://github.com/prongbang/shlane/releases/tag/v0.2.2)
and [crates.io](https://crates.io/crates/shlane), both from the tagged commit: macOS
(arm64, x86-64), Linux (x86-64, arm64, musl) and Windows (x86-64), with `SHA256SUMS`.
The macOS arm64 binary and `install.sh` have been run by hand; the others have only
been built.

To cut the next one:

1. Bump `version` in `Cargo.toml`, and the `prongbang/shlane@vX.Y.Z` tag in `README.md`
   and `docs/fastlane-in-15-minutes.md`.
2. Rename `## [Unreleased]` in `CHANGELOG.md` to the version and date, and open a new
   empty `## [Unreleased]` above it. The release notes are that section.
3. Commit, `git tag -a vX.Y.Z`, push both. The release workflow builds the binaries,
   publishes the GitHub Release and then runs `cargo publish`, which cannot be undone.

The crates.io step needs a `CARGO_REGISTRY_TOKEN` secret in this repository: a
crates.io API token with the `publish-update` scope, limited to the `shlane` crate.
Without it the job warns and succeeds, and `cargo publish` has to be run by hand. The
secret is not set yet: 0.2.2 was published by hand. It
refuses a tag that does not match `Cargo.toml`, and skips a version that is already on
crates.io, so publishing by hand first does no harm.

There is no Homebrew tap; it was dropped after 0.2.0.

## Actions the plan names that do not exist

None left. `notify_discord`, `notify_teams` and `clean_build_artifacts` were the last
three in [`06`](06-actions-core.md). Like `notify_slack`, the two webhooks are tested
for what they send and for masking the URL; nobody has posted to a real Discord or
Teams channel with them yet.

## Documentation

- **`docs/migration.md` is done**, generated from `src/migrate/mapping.rs`. After
  changing a mapping, run `UPDATE_DOCS=1 cargo test` and commit the result.
- **[Move off fastlane in 15 minutes](../fastlane-in-15-minutes.md) exists.** Writing
  it against a real Fastfile found three `migrate` and loader bugs, fixed with it, and
  `increment_build_number` now converts to `agvtool` as fastlane runs it.
- **No documentation site.** The README, `docs/schema-v1.md` and `docs/plan/` cover
  the content; this is packaging, not writing.
- **The GitHub Action is pinned by exact tag** (`prongbang/shlane@v0.2.3` in the README
  and the guide). There is no moving `v0`/`v1` tag, so each release means updating
  both.
- **There is no `ios:` block** for Appfile values, although plan 12 names one. `migrate`
  puts a `TODO-<name>` in each step instead.

## CI

- **The nightly run exists now**: `ci.yml` also runs on a schedule, so both sample
  jobs build nightly. The 1.0 criterion in [`15`](15-roadmap.md) — green for two weeks
  running — starts counting once this is on `master`.
- **`examples/android-sample/` exists**, and has passed locally with Gradle 8.13, AGP
  8.13 and JDK 21 (tests, then an APK and an AAB built and signed). It has not run on a
  GitHub runner yet; the first run on `master` is its real check.

## Known gaps in what exists

- **The first `v0.2.2` was tagged before the version bump**, and the release shipped
  binaries named 0.2.2 that reported 0.2.1; the release and the tag had to be deleted
  by hand and cut again. The release workflow now checks the tag against `Cargo.toml`
  before it builds anything.

- **The release workflow's manual trigger ignores its `tag` input.** A manual run
  builds the branch it was started on and names the files after it. The crates.io step
  only runs for a pushed tag, so this cannot publish anything, but the input does
  nothing.

- **`zip` cannot `exclude` on Windows.** `Compress-Archive` has no exclude, so
  `src/actions/core/files.rs` refuses rather than silently dropping the argument — a
  file kept out of an archive on purpose could be a keystore. Closing it means a Rust
  zip crate or filtering the input first.
- **`firebase_distribution` wraps the `firebase` CLI**, against the "one binary" goal.
  The REST upload returns a long-running operation that has to be polled; see
  [`08`](08-actions-android.md).
- **`codesign_sync`, `provisioning_profile` and `certificate` are read-only** by
  choice. Issuing and revoking stays with `match` and the portal, because getting that
  wrong takes away a team's ability to ship. Revisit only with a way to test it.
- **Process groups and signal forwarding are POSIX-only.** On Windows a timed-out step
  is killed rather than asked to stop first (`src/runtime/shell.rs`).

## Two things worth keeping

Both were learned the expensive way in the Windows work, and both are recorded in
[`15`](15-roadmap.md):

- **A platform's CI job has to be green on its own tests before the platform counts as
  supported.** The Windows target and its job were added together; the job could not
  compile, so no test ran there, and three real bugs hid behind one unused-import
  error. "Compiles and links" is a weaker claim than it sounds.
- **A diagnostic that throws away what a tool said costs more than it saves.**
  `shlane plugin verify` reported only "did not answer describe"; printing the exit
  code and the plugin's own output found the real bug — the shell resolving to the WSL
  launcher — in one round after several of guessing.
