# 02 — Architecture

## The module layout to aim for

`src/main.rs` (199 lines) breaks up into:

```
src/
  main.rs              # a thin entry point: parse the CLI, dispatch, map an error to an exit code
  cli/
    mod.rs             # clap definitions
    commands/          # run.rs, list.rs, init.rs, validate.rs, action.rs, plugin.rs, migrate.rs
  config/
    mod.rs
    model.rs           # Config, Lane, Step, Param (serde)
    loader.rs          # find and read the file; include and merge
    validate.rs        # the schema, lanes that do not exist, lanes that call each other in a loop
  runtime/
    mod.rs
    executor.rs        # the order lanes and steps run in, hooks, error handling
    context.rs         # LaneContext: params, env, outputs, dry_run, workdir
    shell.rs           # spawn, stream, capture stdout/stderr, timeouts
    interpolate.rs     # ${var} and escaping
  script/
    mod.rs
    engine.rs          # the Rhai engine: limits, modules
    builtins.rs        # run/param/env/ui/...
  actions/
    mod.rs
    registry.rs        # trait Action, and the registry
    core/              # sh, git, http, notify, version, file
    ios/
    android/
  plugin/
    mod.rs             # loading external plugins
  report/
    mod.rs             # the summary, JUnit XML, JSON
  error.rs             # ShlaneError
```

The rule: `main.rs` stays under about 50 lines and holds no business logic.

## Error handling

No more `expect()` — there are three today, at `src/main.rs:75`, `76` and `159`.

```rust
// error.rs
#[derive(Debug, thiserror::Error)]
pub enum ShlaneError {
    #[error("no config file found: {0}")]
    ConfigNotFound(PathBuf),
    #[error("invalid config at {path}:{line}: {msg}")]
    ConfigInvalid { path: PathBuf, line: usize, msg: String },
    #[error("lane '{name}' not found (available: {available})")]
    LaneNotFound { name: String, available: String },
    #[error("step '{step}' failed with exit code {code}")]
    StepFailed { step: String, code: i32 },
    #[error("script error: {0}")]
    Script(String),
    #[error("action '{action}' failed: {msg}")]
    Action { action: String, msg: String },
}
```

- `anyhow::Result` at the top, `thiserror` underneath.
- **Remove `panic = "abort"` from `Cargo.toml`.** It turns every error into a crash
  with nothing to read.
- Every error should be able to say which lane, which step, and which line of the YAML.

## LaneContext, in place of global state

The problem today: `env::set_var` (`src/main.rs:81`) mutates the whole process. It is
`unsafe` from Rust 2024 on, and the values leak from one lane into the next.

```rust
pub struct LaneContext {
    pub lane: String,
    pub params: HashMap<String, Value>,   // from the CLI, plus the config's defaults
    pub env: HashMap<String, String>,     // handed to Command::envs(); the process is untouched
    pub outputs: HashMap<String, Value>,  // what steps produced, in place of fastlane's lane_context
    pub workdir: PathBuf,
    pub dry_run: bool,
    pub secrets: SecretRegistry,          // values to mask in the output (see 10)
    pub started_at: Instant,
}
```

- A step with an `id:` stores what it did under `outputs[id]`, reachable as
  `${steps.build.stdout}` or `output("build")` from Rhai.
- `env` is a map handed to child processes. Nothing global is mutated.

## The order a lane runs in

Today it is fixed: before → steps → script → after (`src/main.rs:102-136`), which is
why the `deploy` lane in `example/shlane.yaml` runs in the wrong order.

The target: **`steps` is the one sequence.** A script is a kind of step, not a separate
block.

```
global before_all
  └─ lane before
       └─ steps[0..n]   (run | action | script | lane)
            └─ on failure → the lane's error hook → global after_all → exit
       └─ lane after
global after_all
```

The lane-level `before`, `after` and `script` stay for compatibility, but the
documentation should point people at `steps` alone.

## What the shell layer needs

`shell.rs` has to do what `run_shell_command` does not (`src/main.rs:152-165`):

| Capability | Why |
|---|---|
| Capture stdout and stderr while still streaming them | the output is needed afterwards, but people want to watch it live |
| A timeout per step | so a job cannot hang on CI |
| A `workdir` per step | monorepos |
| A choice of shell (`sh`/`bash`/`pwsh`) | Windows |
| Never call `exit(1)` from inside | return a `Result` and let the executor decide — there are error hooks to run |
| Mask secrets before printing | see [10](10-secrets-and-env.md) |

## Dependencies this will add

| Crate | For |
|---|---|
| `anyhow`, `thiserror` | errors |
| `serde_yaml` (already there) or a move to `serde_yaml_ng` | `serde_yaml` is deprecated — decide in M0 |
| `tracing` + `tracing-subscriber` | levelled logging with context |
| `console` / `owo-colors` | colour and symbols in the terminal |
| `which` | finding binaries (xcodebuild, gradle) |
| `reqwest` (rustls) | the actions that make HTTP calls |
| `tempfile`, `assert_cmd`, `insta` | dev-dependencies for the tests |

Avoid pulling in an async runtime unless something needs one: almost all of this is
waiting on a subprocess.

> **What was actually built:** close to this, with three departures, each recorded in
> the document it departs from — `ureq` instead of `reqwest` (a blocking CLI does not
> need an async runtime), a small `ui` module instead of `tracing`, and no `anyhow`
> (the error enum alone turned out to be enough).
