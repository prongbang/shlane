# 12 — Migrating from fastlane

Nobody moves if it means rewriting everything, so the migration path matters as much as
the features do.

## The strategy: one lane at a time, not all at once

During the move, both have to be able to live together:

```yaml
lanes:
  test:
    steps:
      - action: test_android        # moved over

  beta:
    steps:
      - run: bundle exec fastlane beta   # not moved yet — call the old fastlane
```

The order worth suggesting: `test` → `build` → `bump version`/`changelog` →
`distribute`, which is the hardest and goes last.

## `shlane migrate`

```
shlane migrate --fastfile fastlane/Fastfile --out shlane.yaml
```

It is a **best-effort** tool, not a complete translator — a Fastfile is Ruby, and it can
run anything.

What it can do:

- turn `lane :name do |options| ... end` into a lane
- turn `platform :ios do ... end` into `platform: ios`
- turn `private_lane` into `private: true`
- turn `desc "..."` into `description`
- convert the actions in the table below, with their arguments
- turn `sh "..."` into `run:`
- turn `options[:key]` into `${params.key}`

What it cannot do, and has to leave a `# TODO: move this by hand` for:

- Ruby conditionals and loops, and calls to methods the user wrote
- anything complicated built on `lane_context[SharedValues::X]`
- plugins with no equivalent
- Ruby in the `Fastfile` that sits outside a lane

The output has to come with a **summary**: how many lanes and actions were converted, and
what is left to do by hand.

## The action mapping, abridged

| fastlane | shlane | Document |
|---|---|---|
| `sh` | `run:` | [03](03-config-schema.md) |
| `gym` / `build_app` | `build_ios` | [07](07-actions-ios.md) |
| `scan` / `run_tests` | `test_ios` | [07](07-actions-ios.md) |
| `pilot` / `upload_to_testflight` | `testflight` | [07](07-actions-ios.md) |
| `match` | `codesign_sync` | [07](07-actions-ios.md) |
| `gradle` | `gradle` / `build_android` | [08](08-actions-android.md) |
| `supply` | `play_store` | [08](08-actions-android.md) |
| `firebase_app_distribution` | `firebase_distribution` | [08](08-actions-android.md) |
| `increment_build_number` | `bump_version` | [06](06-actions-core.md) |
| `git_commit` / `add_git_tag` / `push_to_git_remote` | `git_commit` / `git_tag` / `git_push` | [06](06-actions-core.md) |
| `changelog_from_git_commits` | `changelog_from_commits` | [06](06-actions-core.md) |
| `slack` | `notify_slack` | [06](06-actions-core.md) |
| `ensure_git_status_clean` | `git_status_clean` | [06](06-actions-core.md) |
| `setup_ci` | `setup_ci` | [11](11-ci-integration.md) |
| `Appfile` | an `ios:` block | [07](07-actions-ios.md) |
| `Matchfile` | a `codesign:` block | [07](07-actions-ios.md) |
| fastlane's `.env` | `env_files:` | [10](10-secrets-and-env.md) |

The full table belongs in `docs/migration.md`, updated every time an action is added.

## The documents that have to come with it

1. **"Move off fastlane in 15 minutes"** — short, aimed at an ordinary project.
2. **The complete action comparison table**, so somebody can look up whether the action
   they use has a replacement.
3. **"What shlane still cannot do"** — being honest up front beats someone moving over
   and getting stuck.

## The measure

The migration path works when somebody who has never used shlane can move a project with
a ~100-line Fastfile in under an hour.
