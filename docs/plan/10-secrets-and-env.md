# 10 — The environment, and secrets

## What is wrong today

```rust
// src/main.rs:79-83
if let Some(envs) = config.env {
    for (key, value) in envs {
        env::set_var(key, value);      // changes the whole process
    }
}
```

1. `env::set_var` is `unsafe` in Rust 2024, because it is not thread-safe.
2. Values leak between lanes — a lane called later sees the previous lane's environment.
3. `example/shlane.yaml` holds `API_KEY: "abc123"` directly, in a file that is
   committed, which teaches the wrong habit.
4. Nothing is masked in the log at all.

## Environment precedence, highest wins

```
1. the step's env       (step.env)
2. --param, and anything set_env() set in a script
3. the lane's env       (lane.env)
4. the process env      (what CI injected)
5. .env.<profile>       (from --env or $SHLANE_PROFILE)
6. .env
7. the config's env     (config.env)
```

All of it lives in `LaneContext.env` and reaches the child process through
`Command::envs()` — **the main process's own environment is never touched**.

## .env files

```yaml
env_files:
  - .env                    # not committed
  - .env.${SHLANE_PROFILE}  # not committed
  - .env.defaults           # can be committed — nothing secret in it
```

- a file that does not exist is skipped silently when its name came from a `${...}` that
  did not resolve
- `shlane init` adds `.env*` to `.gitignore`, except `.env.defaults`

## Masking secrets in the log

The `SecretRegistry` holds the values to hide. A value is registered when:

- it is an action argument whose schema says `sensitive: true`
- it is an environment variable whose name matches `*_TOKEN`, `*_SECRET`, `*_PASSWORD`,
  `*_KEY` or `*_CREDENTIALS`
- it is declared in the config:
  ```yaml
  secrets:
    - ${env.MY_CUSTOM_VALUE}
  ```
- a plugin sent `{"type":"secret","value":"..."}` (see [09](09-plugins.md))

Masking has to happen on **every path out**: a child process's stdout and stderr, error
messages, the summary table, `--json` output, and the commands printed by `--dry-run`.

One thing to watch: the raw value, its base64 form and its url-encoded form all have to
be masked, because the tool at the other end usually transforms it before logging it.

## Getting a secret into a subprocess

- **Never on the command line** — it shows up in `ps aux`, and in the log of any CI that
  echoes the command.
- Use the child's environment, or write it to a temporary file with permission `0600`
  and delete it through an RAII guard, so it goes even on a panic or a Ctrl-C.

## A credential store, later

On a developer's machine, keep it in the macOS Keychain or libsecret through the
`keyring` crate, so no `.env` has to sit on disk. This is post-1.0 work.

## The checklist to pass

- [ ] no `env::set_var` left in the code
- [ ] `shlane env` shows every value, with secrets as `***`
- [ ] a test proving a secret reaches neither stdout, stderr, an error message, nor the
      JSON output
- [ ] `example/shlane.yaml` reads from the environment instead of holding
      `API_KEY: "abc123"`
