# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
