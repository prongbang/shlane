# `shlane.yaml`, schema version 1

Every key shlane accepts, as it is actually implemented, and what will and will
not change. `shlane validate` checks a config against this, and unknown keys are
rejected rather than ignored — a misspelled key that is silently dropped is a
lane that quietly stops doing something.

Where this document and the code disagree, the code is right and this is a bug.
[`tests/schema_v1.rs`](https://github.com/prongbang/shlane/blob/master/tests/schema_v1.rs) exists to stop them drifting.

## The compatibility promise

While shlane is on 1.x:

- **Nothing here is removed or renamed.** A config that validates today keeps
  validating.
- **A default does not change.** `retry` stays 0, `platform` stays absent,
  `wrapper` stays true.
- **New keys are optional**, and a config that does not use them behaves as it
  did.
- **An action keeps its name and its arguments.** A new argument is optional; an
  existing one keeps its meaning. An action can gain outputs, but it does not
  drop or rename one.
- **Exit codes keep their meanings** (see the README).

What is *not* frozen, because it is not a contract:

- what shlane prints, other than `--json` events and the report formats
- the wording of an error
- which actions exist beyond those listed here — more will be added

Before 1.0, a breaking change is possible, but it goes in the CHANGELOG and
`shlane validate` says how to fix it.

A config can state what it needs:

```yaml
version: 1              # the schema it was written against
min_shlane: "0.1.0"     # the oldest shlane that understands it
```

`version:` other than 1 is refused by this build rather than guessed at.

## Top level

| Key | Type | Default | What it does |
|---|---|---|---|
| `version` | integer | absent | The schema version. Only `1` is accepted. |
| `min_shlane` | string | absent | Fails if the running shlane is older. |
| `env` | map of string to string | `{}` | Environment for every command in every lane. |
| `env_files` | list of string | `[]` | `.env` files to read, lowest priority first. A name that does not resolve is skipped. |
| `secrets` | list of string | `[]` | Values to mask wherever they appear. Usually `${env.X}`. |
| `plugins` | list of plugin | `[]` | See below. |
| `script` | string | absent | Rhai evaluated once before a lane runs; its functions are available to every lane. |
| `before_all` | list of step | `[]` | Runs before the lane's own steps. |
| `after_all` | list of step | `[]` | Runs after them, on success. |
| `error` | list of step | `[]` | Runs when a lane fails. A failure here does not replace the original one. |
| `lanes` | map of name to lane | required | What can be run. |

### A plugin

| Key | Type | What it does |
|---|---|---|
| `name` | string | Required. How the plugin is referred to. |
| `path` | string | A directory holding `shlane-plugin.yaml`. |
| `source` | string | `github:owner/repo@tag`, a git URL, or ssh. Fetched by `shlane plugin install`, never during a run. |

Exactly one of `path` or `source`.

## A lane

| Key | Type | Default | What it does |
|---|---|---|---|
| `description` | string | absent | Shown by `shlane list`. |
| `platform` | string | absent | Free-form grouping, e.g. `ios`. |
| `private` | bool | `false` | A private lane can only be reached from another lane or a script. |
| `params` | map of name to parameter | `{}` | See below. |
| `env` | map of string to string | `{}` | Environment for this lane only. |
| `before` | list of step | `[]` | |
| `steps` | list of step | `[]` | |
| `after` | list of step | `[]` | Runs on success. |
| `script` | string | absent | Rhai run after the lane's steps. |

### A parameter

| Key | Type | Default | What it does |
|---|---|---|---|
| `type` | `string`, `int` or `bool` | `string` | Checked before the lane runs. |
| `required` | bool | `false` | |
| `default` | scalar | absent | Used when the parameter is not passed. |
| `values` | list of string | absent | Restricts it to this set. |
| `description` | string | absent | Shown by `shlane list`. |

## A step

A step carries exactly one of these:

| Key | Type | What it runs |
|---|---|---|
| `run` | string | A shell command, in a POSIX shell. |
| `action` | string | A built-in action or a plugin's, with `with:`. |
| `script` | string | Inline Rhai. |
| `lane` | string | Another lane, with `with:` as its parameters. |

And any of these:

| Key | Type | Default | What it does |
|---|---|---|---|
| `name` | string | the command | What the log and the summary call it. |
| `id` | string | absent | Makes what it produced available as `${steps.<id>.<key>}`. |
| `with` | map | `{}` | Arguments for `action:`, parameters for `lane:`. |
| `if` | string | absent | A Rhai expression; the step runs only when it is true. |
| `env` | map of string to string | `{}` | Environment for this step only. |
| `workdir` | string | the config's directory | Where the step runs. |
| `timeout` | duration | none | `30s`, `5m`, `1h`, or a number of seconds. |
| `retry` | integer | `0` | Extra attempts after the first. |
| `continue_on_error` | bool | `false` | A failure is reported and the lane goes on. |

## Interpolation

`${...}` is substituted in `run:`, in `with:`, and in a step's `env` and
`workdir`.

| Form | Resolves to |
|---|---|
| `${params.x}` | a lane parameter |
| `${env.X}` | an environment variable |
| `${steps.<id>.<key>}` | what an earlier step produced |
| `${shlane.lane}`, `${shlane.version}` | the run itself |
| `${x}` | a parameter, then an environment variable |
| `$${` | a literal `${` |

Two rules that do not change:

1. **A name that resolves to nothing is an error**, not an empty string and not
   a literal `${x}` handed to the shell. The exception is `${steps.<id>.<key>}`
   during a `--dry-run`, where the step that would have produced it was skipped.
2. **A value substituted into a command is escaped**, according to the quoting
   it lands in. `target="a; rm -rf /"` runs one command, not two. That covers
   `run:` and any action argument the action runs as a shell command, such as
   `sh`'s `command`. Every other action argument is substituted literally,
   because an action decides what its own argument means — quoting a file path
   would put the quotes in the path.

`if:` is a Rhai expression, not a template: shell quoting rules do not apply
inside Rhai, so substituting into one would mis-quote it with nothing to notice.

## Environment precedence

Highest wins:

1. the step's `env`
2. `--param`, and anything `set_env()` set in a script
3. the lane's `env`
4. the process environment
5. `.env.<profile>`
6. `.env`
7. the config's `env`

The process environment is never modified: everything is handed to child
processes explicitly.
