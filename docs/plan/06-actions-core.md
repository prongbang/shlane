# 06 — The action system, and the actions that are not tied to a platform

fastlane has around 400 actions, and that is the main reason people are still on it.
This part is the heart of the work.

## The trait

```rust
pub trait Action: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> ArgSchema;                 // used by validate and `shlane action show`
    fn is_supported(&self, platform: Platform) -> bool;
    fn run(&self, ctx: &mut LaneContext, args: &Args) -> Result<ActionOutput>;
    fn dry_run(&self, ctx: &LaneContext, args: &Args) -> Result<String> { ... }
}

pub struct ActionOutput(pub HashMap<String, Value>);   // lands in ctx.outputs[step_id]
```

## The registry

- Static at compile time for the built-ins.
- Extended at runtime by plugins (see [09](09-plugins.md)).
- `shlane action list` and `shlane action show <name>` read from it.
- Names are `snake_case`, as fastlane's are, so the muscle memory carries over.

## What every action has to do

1. **Support `--dry-run`** — print the command it would run, without running it.
2. **Declare the binary it needs.** No `gradle` or `xcodebuild` means exit code 5 and a
   message about installing it, not a bare "command not found".
3. **Mask its secrets.** An argument the schema marks `sensitive: true` is registered
   with the `SecretRegistry` automatically.
4. **Be idempotent where it can** — running it twice should not break.
5. **Return something useful** — `build_ios` returns `ipa`, `dsym`, `build_number`.
6. **Have at least one test** (see [13](13-testing-and-quality.md)).

## The core actions — M3

### Shell and process

| Action | In place of | Notes |
|---|---|---|
| `sh` | `sh` | like `run:`, but callable from a script |
| `ensure_env_vars` | `ensure_env_vars` | fail early when a secret is missing |
| `which_tool` | — | check a binary exists, and its minimum version |

### Git

| Action | In place of |
|---|---|
| `git_status_clean` | `ensure_git_status_clean` |
| `git_branch` | `git_branch` |
| `git_commit` | `git_commit` |
| `git_tag` | `add_git_tag` |
| `git_push` | `push_to_git_remote`, `push_git_tags` |
| `git_pull` | `git_pull` |
| `changelog_from_commits` | `changelog_from_git_commits` |
| `last_git_tag` | `last_git_tag` |

### Versions

| Action | In place of |
|---|---|
| `bump_version` | `increment_version_number` (iOS) and `increment_version_code` (Android), as one action that works out the format |
| `read_version` | `get_version_number`, `get_build_number` |

### Notifications

| Action | In place of |
|---|---|
| `notify_slack` | `slack` |
| `notify_discord` | a plugin — built in instead, since it is one webhook POST |
| `notify_teams` | a plugin — built in instead, same reason |
| `http_request` | — (the escape hatch for any other webhook) |

### Files and artifacts

| Action | In place of |
|---|---|
| `zip` / `unzip` | `zip` |
| `copy_artifacts` | `copy_artifacts` |
| `clean_build_artifacts` | `clean_build_artifacts` |
| `download` | `download` |
| `template_render` | `erb`, with a simple template engine rather than ERB |

## Status — M3, done

`sh`, `ensure_env_vars`, `git_status_clean`, `git_branch`, `git_commit`, `git_tag`,
`git_push`, `last_git_tag`, `changelog_from_commits`, `read_version`, `bump_version`,
`http_request`, `notify_slack`.

Noted while implementing this: under `--dry-run`, an action's *reads* run for real —
`git status`, `git describe`, reading a version file — and only its *changes* are
skipped. A dry run that invents results reports problems that do not exist and hides
the ones that do.

## Priority

Ordered by "without this, the `Gemfile` cannot be deleted":

1. **P0** — `sh`, `git_*`, `bump_version`, `notify_slack`, `ensure_env_vars`
2. **P1** — `changelog_from_commits`, `zip`, `copy_artifacts`, `http_request`
3. **P2** — the rest

Anything outside P0–P2 stays a `run:` step until somebody asks for an action.
