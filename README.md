# shlane

A fastlane-like automation tool written in Rust, with [Rhai](https://rhai.rs) scripting support.

Define your build, test and release steps as *lanes* in a YAML file, then run them
with a single static binary — no Ruby, no `bundle install`, no gem conflicts.

> **Status: early.** The basics (lanes, shell steps, parameters, Rhai scripting) work
> today. The road to actually replacing fastlane — actions, code signing, store
> uploads, plugins — is written up in [`docs/plan/`](docs/plan/README.md).

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

## Configuration

| Key | Meaning |
|---|---|
| `env` | Environment variables handed to every command in every lane |
| `script` | Rhai source evaluated once before a lane runs — a place for shared functions |
| `lanes.<name>.before` | Shell commands run before the lane's steps |
| `lanes.<name>.steps` | The lane's shell commands, each as `- run: <command>` |
| `lanes.<name>.script` | Rhai source run after the steps |
| `lanes.<name>.after` | Shell commands run once the lane has succeeded |

The phases always run in this order: `before` → `steps` → `script` → `after`.
A single ordered `steps:` list that can mix commands, scripts and actions is planned
(see [`docs/plan/03-config-schema.md`](docs/plan/03-config-schema.md)).

`shlane` reads `shlane.yaml` from the directory you run it in.

### Parameters

Anything after the lane name in the form `key=value` becomes a parameter:

```sh
shlane run deploy target=staging notes="first build"
```

Reference it as `${target}` in any command, or as `param("target")` in a script.
A name that resolves to neither a parameter nor an `env` entry is an error, so a typo
fails the lane instead of silently passing `${targt}` to your shell.

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
| `2` | The config could not be parsed, or a `${...}` reference could not be resolved |
| `3` | No config file, or no such lane |
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
