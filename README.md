# shlane

A fastlane-like automation tool written in Rust, with [Rhai](https://rhai.rs) scripting support.

Define your build, test and release steps as *lanes* in a YAML file, then run them
with a single static binary — no Ruby, no `bundle install`, no gem conflicts.

> **Status: early.** Lanes, typed parameters, conditions, retries, timeouts, nested
> lanes and Rhai scripting work today. The road to actually replacing fastlane —
> actions, code signing, store uploads, plugins — is written up in
> [`docs/plan/`](docs/plan/README.md).

## Install

```sh
cargo install --path .
```

Prebuilt binaries are planned; see [`docs/plan/14-release-and-distribution.md`](docs/plan/14-release-and-distribution.md).

## Quick start

Create a `shlane.yaml` in your project root:

```yaml
env:
  APP_ENV: production

lanes:
  test:
    steps:
      - run: cargo test

  deploy:
    before:
      - echo "Deploying to ${target}"
    steps:
      - run: ./scripts/deploy.sh ${target}
    after:
      - echo "Done"
```

Then run a lane:

```sh
shlane run test
shlane run deploy target=staging
```

## Commands

| Command | What it does |
|---|---|
| `shlane run <lane> [key=value ...]` | Run a lane |
| `shlane list` | Show every lane, its description and its parameters |
| `shlane validate` | Check the config without running anything |
| `shlane init` | Write a starter config, guessing the project type |
| `shlane completions <shell>` | Print a shell completion script |

| Flag | What it does |
|---|---|
| `-f, --file <PATH>` | Use this config instead of searching for one |
| `-C, --cwd <DIR>` | Work from this directory |
| `--dry-run` | Print what would run, without running it |

`shlane` looks for `shlane.yaml` (or `shlane.yml`) in the current directory and then
each parent, so it can be run from anywhere inside a project. `$SHLANE_CONFIG`
overrides the search. Steps run from the directory holding the config, so a lane
behaves the same wherever it is started.

## Configuration

### Top level

| Key | Meaning |
|---|---|
| `version` | Schema version. `1` today |
| `min_shlane` | Minimum shlane version this config needs |
| `env` | Environment variables handed to every command in every lane |
| `script` | Rhai source evaluated once before a lane runs — a place for shared functions |
| `before_all` / `after_all` | Steps run around the lane |
| `error` | Steps run when a lane fails |
| `lanes` | The lanes themselves |

### A lane

| Key | Meaning |
|---|---|
| `description` | One line, shown by `shlane list` |
| `platform` | Free-form grouping, e.g. `ios` |
| `private` | When true, the lane can only be reached from another lane |
| `params` | Declared parameters, checked before the lane runs |
| `before` / `steps` / `after` | The lane's steps |
| `script` | Rhai source run after the steps |

The phases run in this order: `before` → `steps` → `script` → `after`.
A single ordered `steps:` list that can also mix in actions is planned
(see [`docs/plan/03-config-schema.md`](docs/plan/03-config-schema.md)).

### A step

A step is one of `run:` (a shell command), `script:` (Rhai) or `lane:` (another lane).
`action:` parses but is not implemented yet — it is M3 on the roadmap.

```yaml
steps:
  - name: upload            # shown in logs and the summary
    run: ./scripts/upload.sh ${target}
    if: param("target") == "production"   # a Rhai expression
    workdir: ./ios          # relative to the config file
    env:
      EXTRA: value
    timeout: 20m            # 30, 30s, 20m, 1h
    retry: 2                # extra attempts after the first
    continue_on_error: false

  - lane: notify            # call another lane
    with:
      channel: "#releases"
```

A bare string is still a `run:` step, so `before: [echo hi]` works as before.

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

### Parameters

Anything after the lane name in the form `key=value` becomes a parameter:

```sh
shlane run deploy target=staging notes="first build"
```

Reference it as `${target}` in any command, or as `param("target")` in a script.
A name that resolves to neither a parameter nor an `env` entry is an error, so a typo
fails the lane instead of silently passing `${targt}` to your shell.

A bare `${x}` looks in parameters, then in `env`. The namespaced forms say exactly
where to look: `${params.x}`, `${env.X}`, and `${shlane.lane}` / `${shlane.version}`.

**Values are shell-quoted.** `${target}` always expands to exactly one shell word, so a
value containing `;`, spaces or backticks cannot inject commands. Quoting follows the
surrounding context, so a reference already inside `"..."` or `'...'` is escaped in place
rather than wrapped in another layer of quotes. When you *want* a value to expand into
several words, ask for it explicitly with `:raw`:

```yaml
steps:
  - run: cargo build ${flags:raw}      # flags="--release --locked"
```

Write `$${literal}` for a literal `${literal}`. Plain shell syntax (`$HOME`, `$(date)`)
is passed through untouched.

### Scripting

Lanes can run [Rhai](https://rhai.rs) for anything YAML cannot express:

```yaml
script: |-
  fn greet(name) {
      print("Hello, " + name + "!");
  }

lanes:
  deploy:
    script: |
      let target = param("target");
      if target == env("APP_ENV") {
        run("./scripts/deploy-production.sh");
      } else {
        run("./scripts/deploy-staging.sh");
      }
```

| Function | Returns | Notes |
|---|---|---|
| `param(key)` | string | `""` when the parameter was not passed |
| `env(key)` | string | The lane's environment, falling back to the process environment |
| `run(cmd)` | int | Exit code. Does **not** abort the lane on a non-zero code |
| `print(msg)` | — | Writes to stdout |

A richer API — `run()` returning stdout, passing values between steps, calling other
lanes — is planned in [`docs/plan/05-scripting-rhai.md`](docs/plan/05-scripting-rhai.md).

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | The lane failed — a step returned non-zero, or a script errored |
| `2` | The config could not be parsed or did not validate, or a `${...}` reference could not be resolved |
| `3` | No config file, or no such lane (or the lane is private) |
| `4` | A parameter was missing, of the wrong type, or not an allowed value |
| `5` | A tool shlane needs is not installed |

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

## Roadmap

`docs/plan/` holds the full plan for replacing fastlane — gap analysis, target
architecture, the action system, migration path and milestones M0–M7.
Start at [`docs/plan/README.md`](docs/plan/README.md). *(The plan documents are written in Thai.)*

## License

Apache-2.0. See [LICENSE](LICENSE).
