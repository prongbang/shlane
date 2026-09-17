# 01 — Gap analysis: fastlane vs shlane

fastlane's features against where `shlane` stood when this was written
(`src/main.rs`, 199 lines).

## Feature by feature

| fastlane | shlane then | The gap | Covered by |
|---|---|---|---|
| Fastfile + lanes | ✅ `lanes:` in YAML | — | [03](03-config-schema.md) |
| `before_all` / `after_all` | ⚠️ `before`/`after`, but per lane only | nothing global | [03](03-config-schema.md) |
| `error` block | ❌ | no hook when something fails | [03](03-config-schema.md) |
| Calling one lane from another | ❌ | the steps have to be repeated | [03](03-config-schema.md) |
| `platform :ios do ... end` | ❌ | no way to group lanes | [03](03-config-schema.md) |
| Private lanes | ❌ | every lane is callable from outside | [03](03-config-schema.md) |
| `options[:key]` with defaults | ⚠️ `param()`, but no default, required or type | a wrong parameter runs anyway, returning `""` | [03](03-config-schema.md) |
| `fastlane lanes` / `list` | ❌ | no way to see what lanes exist, except by misspelling one | [04](04-cli-ux.md) |
| `fastlane init` | ❌ | — | [04](04-cli-ux.md) |
| ~400 actions | ❌ none | the largest gap by far | [06](06-actions-core.md), [07](07-actions-ios.md), [08](08-actions-android.md) |
| `lane_context[SharedValues::...]` | ❌ | no way to pass a value between steps | [02](02-architecture.md) |
| A command's output | ❌ `run()` returns only an exit code (`src/main.rs:180`) | the output cannot be used | [05](05-scripting-rhai.md) |
| `.env` / `--env` | ⚠️ `env:` in YAML, but no `.env` file | secrets would have to live in a committed file | [10](10-secrets-and-env.md) |
| Hiding secrets in the log | ❌ | secrets reach the CI log | [10](10-secrets-and-env.md) |
| Plugins (`fastlane add_plugin`) | ❌ | no way to extend it | [09](09-plugins.md) |
| `is_ci`, `setup_ci` | ❌ | the keychain is the user's problem on CI | [11](11-ci-integration.md) |
| Reports (JUnit/JSON) | ❌ | CI cannot read the result | [11](11-ci-integration.md) |
| Appfile / Matchfile | ❌ | — | [07](07-actions-ios.md) |
| `fastlane_version` | ❌ | no `version:` in the config | [03](03-config-schema.md) |
| A summary of what each action took | ❌ | no way to see which step is slow | [04](04-cli-ux.md) |

## Problems in the existing code, to be fixed first

| Problem | Where | What it means |
|---|---|---|
| `expect()` on every file read and parse | `src/main.rs:75-76`, `159` | the error tells the user nothing, and `panic = "abort"` removes the backtrace too |
| Only looks for `shlane.yaml` in the working directory | `src/main.rs:75` | cannot be run from a subdirectory of the repository |
| `env::set_var` mutates the whole process | `src/main.rs:81` | `unsafe` from Rust 2024 on, and values leak between lanes |
| `run()` in Rhai does not stop on failure | `src/main.rs:180-193` | the script carries on after a command has already failed |
| `${k}` is substituted only in `before`/`steps`/`after` | `src/main.rs:65-72` | an unknown name reaches the shell as a literal `${k}` |
| Parameter values are concatenated into the command unescaped | `src/main.rs:114` | a value containing `;` or a backtick is command injection |
| The order is fixed: before → steps → script → after | `src/main.rs:102-136` | the user cannot control it — see `example/shlane.yaml`, where `deploy`'s `steps` reference `${target}` before `script` sets it |
| No tests | the whole project | a refactor cannot be shown to be safe |

## In summary

The gap has three layers, and they have to be filled in order:

1. **Foundations** (M0–M2): error handling, module structure, config schema, CLI,
   context. Skip these and every action has to be rewritten later.
2. **Actions** (M3–M5): the bulk of the work, but it can be done one at a time, with
   something useful released each time.
3. **Ecosystem** (M6–M7): plugins, the migration tool, distribution.
