# 03 — The `shlane.yaml` v1 specification

## Principles

- Every field has a sensible default, so the shortest useful file really is short.
- The schema can be checked before anything runs, with `shlane validate`.
- The existing file (`example/shlane.yaml`) keeps working. No breaking change without
  a reason.

## The whole thing

```yaml
version: 1                      # new — so the schema can move later
min_shlane: "0.5.0"             # new — in place of fastlane_version

env:
  APP_ENV: production
env_files:                      # new — see 10
  - .env
  - .env.${SHLANE_PROFILE}

include:                        # new — split across files
  - lanes/ios.yaml
  - lanes/android.yaml

script: |-                      # the shared Rhai script (already there)
  fn greet(name) { print("Hello, " + name); }

before_all:                     # new — global
  - run: git rev-parse --short HEAD
after_all:
  - action: notify_slack
    with: { text: "done" }
error:                          # new — runs when any lane fails
  - action: notify_slack
    with: { text: "failed at ${error.step}" }

lanes:
  beta:
    description: "build and ship to TestFlight"   # new — shown by `shlane list`
    platform: ios                                 # new — grouping
    private: false                                # new
    params:                                       # new — declared parameters
      target:
        type: string                              # string | int | bool
        required: true
        values: [staging, production]
        description: "Where to deploy"
      notes:
        type: string
        default: "no notes"
    steps:
      - name: "check the tree is clean"      # new — something readable in the log
        action: ensure_git_status_clean

      - id: build                            # new — keeps what it produced
        action: build_ios
        with:
          scheme: MyApp
          configuration: Release

      - name: upload
        action: testflight
        with:
          ipa: ${steps.build.ipa}            # new — what an earlier step produced
        if: param("target") == "production"  # new — a condition (a Rhai expression)
        retry: 2                             # new
        timeout: 20m                         # new

      - run: ./scripts/cleanup.sh            # the old shape still works
        workdir: ./ios                       # new
        continue_on_error: true              # new
        env: { FOO: bar }                    # new — environment for this step only

      - script: |                            # a script can be a step now
          print("done " + param("target"));

      - lane: notify                         # new — call another lane
        with: { channel: "#releases" }

  notify:
    private: true
    steps:
      - action: notify_slack
        with: { channel: "${params.channel}" }
```

## Kinds of step

A step carries exactly one of these four:

| Key | Meaning |
|---|---|
| `run:` | a shell command |
| `action:` | a built-in action or a plugin (see [06](06-actions-core.md)) |
| `script:` | inline Rhai |
| `lane:` | another lane in the same file |

Every step also takes `name`, `id`, `if`, `env`, `workdir`, `timeout`, `retry` and
`continue_on_error`.

## Interpolation

Today's `${key}` (`src/main.rs:65-72`) grows namespaces:

| Form | Resolves to |
|---|---|
| `${params.x}` | a lane parameter |
| `${env.X}` | an environment variable |
| `${steps.<id>.<field>}` | what an earlier step produced (`stdout`, `code`, or an action's own outputs) |
| `${shlane.lane}`, `${shlane.version}` | the run itself |

> **Noted while implementing this (M1):** `if:` takes a Rhai expression
> (`param("x") == "y"`) rather than a `${...}` template. Shell quoting rules do not
> apply inside Rhai, so substituting into an expression would mis-quote it with nothing
> to notice.

Two rules that differ from today:

1. **A name that resolves to nothing is an error**, rather than a literal `${x}` being
   handed to the shell (`src/main.rs:69`).
2. **A value substituted into `run:` is escaped.** Today it is concatenated, so
   `target="a; rm -rf /"` actually runs.

Bare `${key}` keeps working, resolving params then env.

## What `shlane validate` has to catch

- YAML that does not parse, with the line number
- `lane:` naming a lane that does not exist, or lanes that call each other in a loop
- `action:` naming something not in the registry
- `with:` missing an argument the action requires, or carrying one it does not take
- a step with none of `run`/`action`/`script`/`lane`, or more than one
- a `${...}` that cannot resolve — a `steps.x` with no such step id, or one that comes
  later in the lane than the reference to it
- a `params.default` that does not match its declared `type`
- a `min_shlane` higher than the version installed

## Finding the config

Today only `shlane.yaml` in the working directory is read (`src/main.rs:75`). Instead:

1. `--file <path>`, if given
2. `$SHLANE_CONFIG`
3. walk up from the working directory to the root, looking for `shlane.yaml`, then
   `shlane.yml`, then `.shlane/shlane.yaml`
4. nothing found — an error that suggests `shlane init`

Every step's working directory starts at the directory holding the config, not the
caller's, so a lane behaves the same wherever it is started.
