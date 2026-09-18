# 16 — What is left

Where the project stands at `0.2.0`, and what remains. Written to be picked up on
another machine: each item says what to do, where, and how to know it worked.

Checked against the code rather than the plan — `shlane action list` reports 37
actions, `cargo test` 425, and `master` is green on Linux, macOS and Windows.

## Start here

```sh
git pull
cargo test                                    # 425
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
| The iOS actions | macOS, Xcode, an Apple Developer account | `build_ios`, `test_ios`, `testflight`, `keychain`, `codesign_sync` against a real app |
| `appstore` | An App Store Connect key | `--dry-run` first: it reads Apple for real and prints every change without making one |
| `play_store` | A Google Play service account | Only the request shapes are tested |
| The Windows binary | A real Windows machine | CI runs its tests, but nobody has used it |

The unit tests cover what each one *builds* — the command line, the export plist, the
JWT claims, the request bodies. None of them cover the round trip. Treat a first real
run as debugging, not as confirmation.

## Release — the shortest path to something installable

Nothing is published and there are no tags. `cargo publish --dry-run` passes and CI
keeps it passing.

1. **Tag `v0.2.0` and push it.** The release workflow builds macOS (arm64, x86-64),
   Linux (x86-64, arm64, musl) and Windows (x86-64), with `SHA256SUMS`.
   ```sh
   git tag -a v0.2.0 -m "shlane 0.2.0"
   git push origin v0.2.0
   ```
   Watch the run before announcing anything: nothing but CI has exercised that
   workflow end to end.

2. **`cargo publish`.** Public and irreversible; a version number cannot be reused.
   Run `cargo publish --dry-run` once more on the tagged commit first.

3. **The Homebrew tap.** `packaging/homebrew/README.md` has the steps. Create
   `prongbang/homebrew-tap` (Homebrew requires that exact name), add a token with
   `Contents: read and write` as the `HOMEBREW_TAP_TOKEN` secret here, and the release
   job pushes the formula. Without the secret the job says so and succeeds, so a
   release does not go red over it.

## Actions the plan names that do not exist

Small, self-contained, good first work. `src/actions/core/` and the `Action` trait in
`src/actions/mod.rs`; each needs a schema, a test over what it builds, and a line in
the README table.

| Action | From | Notes |
|---|---|---|
| `notify_discord` | [`06`](06-actions-core.md) | A webhook POST. `notify_slack` in `src/actions/core/http.rs` is the shape to copy |
| `notify_teams` | [`06`](06-actions-core.md) | Same, different payload |
| `clean_build_artifacts` | [`06`](06-actions-core.md) | Delete what a build left behind |

`docs/plan/06-actions-core.md` lists these under P1/P2. Everything else in that
document is built.

## Documentation

- **`docs/migration.md` does not exist.** [`12`](12-migration-from-fastlane.md)
  promises "the full table, updated every time an action is added". The data is
  already in `src/migrate/mapping.rs` — consider generating the table from it so the
  two cannot drift, the way `tests/schema_v1.rs` keeps the schema honest.
- **No documentation site.** The README, `docs/schema-v1.md` and `docs/plan/` cover
  the content; this is packaging, not writing.

## CI

- **No nightly e2e.** [`15`](15-roadmap.md) asks for one on both platforms, green for
  two weeks, before calling it 1.0. Today every job runs per push, and nothing runs on
  a schedule.
- **`examples/android-sample/` does not exist.** `examples/ios-sample` is the model:
  a small real project the macOS job runs end to end. An Android equivalent would give
  `gradle`, `build_android` and `sign_android` the same treatment on a cheap runner.

## Known gaps in what exists

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
