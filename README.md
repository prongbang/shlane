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
| `shlane action list` / `shlane action show <name>` | The built-in actions and their arguments |
| `shlane completions <shell>` | Print a shell completion script |

| Flag | What it does |
|---|---|
| `-f, --file <PATH>` | Use this config instead of searching for one |
| `-C, --cwd <DIR>` | Work from this directory |
| `--dry-run` | Print what would run, without running it |
| `--env <PROFILE>` | Select `.env.<profile>` |
| `-v, --verbose` / `-q, --quiet` | More detail / errors and command output only |
| `--json` | One JSON event per line, for other tools to read |
| `--report <fmt>:<path>` | Write results as `junit`, `json` or `md` (repeatable) |

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
| `env_files` | `.env` files to read, lowest priority first |
| `secrets` | Values to hide wherever they appear in the output |
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
| `env` | Environment variables for this lane only |
| `before` / `steps` / `after` | The lane's steps |
| `script` | Rhai source run after the steps |

The phases run in this order: `before` → `steps` → `script` → `after`.
A single ordered `steps:` list that can also mix in actions is planned
(see [`docs/plan/03-config-schema.md`](docs/plan/03-config-schema.md)).

### A step

A step is one of `run:` (a shell command), `action:` (a built-in), `script:` (Rhai) or
`lane:` (another lane).

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

  - action: git_tag         # a built-in
    with:
      name: "v1.2.3"

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

A step with an `id:` publishes what it did, for later steps to read:

```yaml
steps:
  - id: version
    run: cat VERSION
  - run: echo "building ${steps.version.stdout}"   # also .stderr and .code
```

## Actions

An action is a named step that knows its own arguments, so `shlane validate` checks it
before anything runs and `shlane action show` documents it.

```yaml
lanes:
  release:
    steps:
      - action: git_status_clean
      - id: bumped
        action: bump_version
        with:
          part: patch
      - action: git_commit
        with:
          message: "release: v${steps.bumped.version}"
      - action: git_tag
        with:
          name: "v${steps.bumped.version}"
      - action: git_push
        with:
          tags: true
      - action: notify_slack
        with:
          webhook: ${SLACK_WEBHOOK}      # masked in every log line
          text: "Released v${steps.bumped.version}"
```

| Action | What it does |
|---|---|
| `sh` | Run a shell command |
| `ensure_env_vars` | Fail early when a variable is missing |
| `git_status_clean` | Fail if the working tree is dirty |
| `git_branch` | Report the branch and short SHA |
| `git_commit` / `git_tag` / `git_push` | The release trio |
| `last_git_tag` | The most recent tag, or `found: false` |
| `changelog_from_commits` | Commit subjects since a tag |
| `read_version` / `bump_version` | Cargo.toml, package.json, pubspec.yaml or VERSION |
| `http_request` | Any HTTP call, with retries |
| `notify_slack` | Post to an incoming webhook |
| `gradle` | Run a Gradle task |
| `build_android` | Assemble an APK or AAB and report where it landed |
| `test_android` | Run the unit tests and collect their reports |
| `sign_android` | Sign with a keystore, from a file or base64 |
| `play_store` | Upload to Google Play |
| `firebase_distribution` | Distribute through Firebase App Distribution |

Scripts can call the same actions:

```yaml
script: |
  let result = action("bump_version", #{ part: "minor" });
  ui_message("now at " + result.version);
```

Under `--dry-run`, an action's *reads* still run — `git status`, `git describe`, reading
a version file — while its *changes* are only described. A dry run that invents results
reports problems that do not exist and hides the ones that do.

### Android

```yaml
lanes:
  beta:
    platform: android
    steps:
      - action: test_android
      - id: build
        action: build_android
        with:
          format: aab
          flavor: prod
          properties: |
            KEYSTORE_PASSWORD=${KEYSTORE_PASSWORD}
      - action: play_store
        with:
          package_name: com.example.app
          aab: ${steps.build.aab}
          track: internal
          service_account_json: ${PLAY_SERVICE_ACCOUNT}
```

A gradle property whose name looks sensitive is passed through the environment as
`ORG_GRADLE_PROJECT_<NAME>` instead of `-P<name>=`, because a command line is visible
in `ps` and in most CI logs — and its value is masked on the way back.

`play_store` signs its own requests with the service account (no `gcloud` needed). Its
request shapes are unit-tested, but the round trip against Google is not: that needs a
real account, and is e2e work
([`docs/plan/13-testing-and-quality.md`](docs/plan/13-testing-and-quality.md)).
`firebase_distribution` wraps the `firebase` CLI, which must be installed.

iOS actions (`build_ios`, `test_ios`, `testflight`) are M5; see
[`docs/plan/07-actions-ios.md`](docs/plan/07-actions-ios.md).

## Environment and secrets

```yaml
env:
  APP_ENV: production
env_files:
  - .env
  - .env.${SHLANE_PROFILE}     # selected by --env, skipped if there is no profile
secrets:
  - ${env.LICENCE_KEY}         # hide a value whose name does not look secret
```

Precedence, lowest first: `env:` in the config, then each file in `env_files:` in
order, then the environment shlane was started with — so a value injected by CI always
wins over one committed to a file — then the lane's `env:`, then the step's.

**Secrets are masked in everything shlane prints**: the command it echoes, the
command's own stdout and stderr, error messages, the summary and the `--json` stream.
A value is treated as secret when its name ends in `_TOKEN`, `_SECRET`, `_PASSWORD`,
`_KEY` or `_CREDENTIALS`, when it is listed under `secrets:`, or when a script calls
`secret(value)`.

Masking requires reading the command's output, so a command that colours its output
based on whether it is talking to a terminal will see a pipe and turn colour off.

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
| `run(cmd)` | `CmdResult` | Stops the lane if the command fails |
| `try_run(cmd)` | `CmdResult` | Returns the failure instead of raising it |
| `capture(cmd)` | string | The command's stdout, trimmed, without echoing it |
| `param(key)` / `param_or(key, default)` / `has_param(key)` | string / string / bool | |
| `env(key)` / `set_env(key, value)` | string / — | `set_env` applies to later steps in the lane |
| `set_output(key, value)` / `output(id, key)` | — / string | Pass values between steps |
| `action(name, args)` | map | Run a built-in action and get its outputs |
| `secret(value)` | — | Hide a value computed at runtime |
| `ui_message(text)` / `ui_success(text)` / `ui_error(text)` | — | |
| `print(msg)` | — | Writes to stdout, masked like everything else |

`CmdResult` has `.stdout`, `.stderr`, `.code` and `.success`.

```yaml
script: |
  let version = capture("git describe --tags");
  let result = try_run("./scripts/optional-check.sh");
  if !result.success {
    ui_error("check failed: " + result.stderr);
  }
  set_output("version", version);
```

`action(name, #{ ... })` runs a built-in and returns its outputs as a map.
`call_lane()` is not available: calling a lane from a script means re-entering the
executor, so use a `lane:` step instead.

## Reports

```sh
shlane run beta --report junit:reports/shlane.xml --report md:$GITHUB_STEP_SUMMARY
```

Every step becomes a test case, so a CI that understands JUnit shows which step failed.
The report is written whether the lane passed or failed — one that only appears on
success is no use to the job that has to explain the failure. A Markdown report is
appended, so it can be pointed at GitHub's step summary.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | The lane failed — a step returned non-zero, or a script errored |
| `2` | The config could not be parsed or did not validate, or a `${...}` reference could not be resolved |
| `3` | No config file, or no such lane (or the lane is private) |
| `4` | A parameter was missing, of the wrong type, or not an allowed value |
| `5` | A tool shlane needs is not installed |
| `130` | Stopped by Ctrl-C |

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

Minimum supported Rust version: **1.85** (required by `ureq`, used by the HTTP actions).

## Roadmap

`docs/plan/` holds the full plan for replacing fastlane — gap analysis, target
architecture, the action system, migration path and milestones M0–M7.
Start at [`docs/plan/README.md`](docs/plan/README.md). *(The plan documents are written in Thai.)*

## License

Apache-2.0. See [LICENSE](LICENSE).
