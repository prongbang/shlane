# 04 — The CLI and its output

Today there is one command: `shlane run <name> [params...]` (`src/main.rs:17-27`).

## The commands to have

| Command | What it does | Milestone |
|---|---|---|
| `shlane run <lane> [k=v ...]` | run a lane (already there) | — |
| `shlane list` / `shlane lanes` | every lane, with its description and parameters (in place of `fastlane lanes`) | M1 |
| `shlane validate` | check the config without running it | M1 |
| `shlane init` | write a starter `shlane.yaml`, working out whether this is an iOS, Android or Flutter project | M1 |
| `shlane action list` | the actions available | M3 |
| `shlane action show <name>` | one action's arguments | M3 |
| `shlane env` | the resolved environment, with secrets masked | M2 |
| `shlane plugin add/list/remove` | manage plugins | M6 |
| `shlane migrate` | convert a Fastfile into a `shlane.yaml` | M6 |
| `shlane completions <shell>` | shell completion | M1 |

## Global flags

| Flag | Meaning |
|---|---|
| `-f, --file <path>` | use this config file |
| `-C, --cwd <dir>` | start from this directory |
| `--dry-run` | show what would happen without doing it |
| `-v, --verbose` / `-vv` | more detail |
| `-q, --quiet` | errors only |
| `--json` | one JSON event per line, for other tools |
| `--no-color` | no colour (detected from `NO_COLOR` and whether this is a tty) |
| `--env <profile>` | select `.env.<profile>` (see [10](10-secrets-and-env.md)) |
| `--param k=v` | clearer than trailing arguments |

## Exit codes

Today a failing command calls `exit(1)` immediately (`src/main.rs:163`), and a lane
that does not exist still exits `0` (`src/main.rs:139-149`). Both need fixing.

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | the lane failed — some step did |
| 2 | the config is wrong, or did not validate |
| 3 | no such lane, or no config file |
| 4 | a parameter is not acceptable |
| 5 | a tool that was needed is not installed (xcodebuild, gradle, ...) |
| 130 | interrupted (Ctrl-C) |

## What the output looks like

```
shlane 0.5.0 · lane: beta · platform: ios

  ✔ check the tree is clean                            0.2s
  ✔ build_ios (scheme=MyApp)                          2m 14s
  ⠋ testflight (ipa=build/MyApp.ipa)
```

And a summary at the end, as fastlane has:

```
Summary
  #  step                     result    time
  1  ensure_git_status_clean  ✔          0.2s
  2  build_ios                ✔       2m 14s
  3  testflight               ✘        45.1s

  failed at step 3 (testflight): invalid API key
  total 2m 59s
```

Requirements:

- Output streams as it happens, never buffered to the end. A CI job timing out because
  nothing printed is a real failure mode.
- The spinner turns itself off when this is not a tty.
- The final error names **the lane, the step's number and name, the line in the YAML,
  and the command that actually ran**.
- `--json` emits `lane_started`, `step_started`, `step_output`, `step_finished` and
  `lane_finished`.

## Interrupts

Ctrl-C should stop the running child, run the `error` hooks, and exit 130. Nothing
handles it today, which is how an `xcodebuild` ends up orphaned.
