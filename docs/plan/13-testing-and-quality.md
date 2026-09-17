# 13 — Testing, and the quality of the code

The project has **no tests at all** right now, and that has to be fixed before the
refactor in M0, not after it.

## The layers

| Layer | What it covers | When it runs | With |
|---|---|---|---|
| **Unit** | parsing the config, interpolation, how an action assembles its arguments, masking secrets | every commit | `cargo test` |
| **Integration (CLI)** | running the real binary against an example `shlane.yaml` | every commit | `assert_cmd` and `tempfile` |
| **Snapshot** | what `list` and `validate` print, the summary table, error messages | every commit | `insta` |
| **Contract** | the actions that make HTTP calls (Play Store, App Store Connect) | every commit | `wiremock`, or a mock server |
| **E2E** | a real build of an example project | nightly, and before a release | a macOS/Linux runner |

## What has to have a test in M0, before the old code is touched

Write characterization tests for today's behaviour first, so the refactor shows what it
broke:

- [ ] a lane with `before`, `steps`, `script` and `after` runs them in the expected order
- [ ] a lane that does not exist prints the lanes that do (`src/main.rs:139-149`)
- [ ] `${key}` is substituted in `run:` (`src/main.rs:65-72`)
- [ ] a command that exits non-zero stops the program (`src/main.rs:161-164`)
- [ ] malformed YAML does not panic with no message

## Fixtures

```
tests/
  fixtures/
    minimal.yaml           # one lane, one step
    full.yaml              # every field in the spec (see 03)
    invalid_syntax.yaml
    invalid_lane_ref.yaml
    cyclic_lanes.yaml
    secrets.yaml           # checks that a secret does not escape
  cli/
    run.rs  list.rs  validate.rs  init.rs
  snapshots/
```

## Testing an action without Xcode or Gradle

Always split an action in two:

```rust
// the testable half (pure): args → the command it would run
fn build_command(args: &BuildIosArgs) -> Vec<String>

// the untestable half: call build_command(), then spawn
fn run(...)
```

Test `build_command()` thoroughly. That is where nearly all the bugs are — the wrong
argument, the wrong quoting, the wrong order — not in the spawning.

For e2e, keep `examples/ios-sample/` and `examples/android-sample/` as empty projects
that really build.

## Code quality

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo deny check          # licences and advisories
cargo test --all-features
```

- **MSRV**: pick one, put it in `Cargo.toml` as `rust-version`, and test it in CI.
- **No `unwrap()` or `expect()` in production code** — enforced with the clippy lints
  `unwrap_used` and `expect_used`, tests excepted.
- **No `panic = "abort"`** in the release profile (see [02](02-architecture.md)).

## shlane's own CI workflow

```yaml
jobs:
  check:       # fmt, clippy, deny — ubuntu
  test:        # matrix: ubuntu, macos, windows × stable, MSRV
  e2e-android: # ubuntu plus the Android SDK — nightly
  e2e-ios:     # macos-14 — nightly
```

## Coverage goals

- config, interpolation and secret masking: **> 90%**
- runtime/executor: **> 80%**
- actions: every one needs a `build_command()` test, at least one happy path and one
  error case
- no target for the project as a whole, because chasing one means writing tests for glue
  code that prove nothing
