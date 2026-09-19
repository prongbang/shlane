# 16 — What is left

Where the project stands at `0.2.0`, and what remains. Written to be picked up on
another machine: each item says what to do, where, and how to know it worked.

Checked against the code rather than the plan — `shlane action list` reports 40
actions, `cargo test` 435, and `master` is green on Linux, macOS and Windows.

## Start here

```sh
git pull
cargo test                                    # 435
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

None left. `notify_discord`, `notify_teams` and `clean_build_artifacts` were the last
three in [`06`](06-actions-core.md). Like `notify_slack`, the two webhooks are tested
for what they send and for masking the URL; nobody has posted to a real Discord or
Teams channel with them yet.

## Documentation

- **`docs/migration.md` is done**, generated from `src/migrate/mapping.rs`. After
  changing a mapping, run `UPDATE_DOCS=1 cargo test` and commit the result.
- **No documentation site.** The README, `docs/schema-v1.md` and `docs/plan/` cover
  the content; this is packaging, not writing.
- **Plan 12's "15 minutes" guide and "what shlane still cannot do" page** do not exist
  as separate pages. `docs/migration.md` covers part of both.
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
