# Migrating from fastlane

Move one lane at a time. A lane that has not moved yet can still call fastlane, so
both run side by side until the last one is done:

```yaml
lanes:
  test:
    steps:
      - action: test_android               # moved
  beta:
    steps:
      - run: bundle exec fastlane beta     # not yet
```

A good order is `test`, then `build`, then version bumps and changelogs, then
distribution, which is the hardest part.

## `shlane migrate`

```sh
shlane migrate --fastfile fastlane/Fastfile --out shlane.yaml
shlane validate
```

It converts the lanes, `desc`, `private_lane`, `platform`, `sh "..."`,
`options[:key]` and every action in the table below. Anything it does not understand
stays in the file as a `TODO` comment, and it prints a summary of what is left.
If an action needs a value that fastlane took from the Appfile or the environment, it
gets a `TODO-<name>` placeholder, so the file still validates and every gap is in
one place.

Arguments missing from the table keep their fastlane names. When shlane uses a
different name, `shlane validate` reports it.

## Not an action

| fastlane | shlane |
|---|---|
| `sh "..."` | a `run:` step |
| `lane_context[SharedValues::X]` | `${steps.<id>.<output>}` |
| `Appfile` | the step's own arguments; `migrate` leaves a `TODO-<name>` for each |
| fastlane's `.env` files | `env_files:` |
| a plugin | a shlane plugin, or `http_request` for a webhook |

`shlane action list` shows every action, and `shlane action show <name>` shows its
arguments.

<!-- BEGIN GENERATED: cargo test updates this, do not edit by hand -->

## Actions

| fastlane | shlane | Arguments |
|---|---|---|
| `add_git_tag` | `git_tag` | `tag` → `name` |
| `build_app` | `build_ios` | `skip_package_ipa` → `skip_export` |
| `build_ios_app` | `build_ios` | `skip_package_ipa` → `skip_export` |
| `changelog_from_git_commits` | `changelog_from_commits` | `between` → `from`, `pretty` → `format` |
| `create_keychain` | `keychain` | adds `action: create` |
| `delete_keychain` | `keychain` | adds `action: delete` |
| `deliver` | `appstore` | `app_identifier` → `bundle_id`, `metadata_path` → `metadata_dir` |
| `ensure_git_status_clean` | `git_status_clean` |  |
| `firebase_app_distribution` | `firebase_distribution` | `app` → `app_id` |
| `get_version_number` | `read_version` |  |
| `git_branch` | `git_branch` |  |
| `git_commit` | `git_commit` | `path` → `paths` |
| `gradle` | `gradle` |  |
| `gym` | `build_ios` | `skip_package_ipa` → `skip_export` |
| `increment_build_number` | `bump_version` | adds `part: build` |
| `increment_version_number` | `bump_version` | `bump_type` → `part` |
| `last_git_tag` | `last_git_tag` |  |
| `pilot` | `testflight` |  |
| `push_git_tags` | `git_push` | adds `tags: true` |
| `push_to_git_remote` | `git_push` | `local_branch` → `branch` |
| `run_tests` | `test_ios` | `devices` → `destination` |
| `scan` | `test_ios` | `devices` → `destination` |
| `setup_ci` | `setup_ci` |  |
| `slack` | `notify_slack` | `message` → `text`, `slack_url` → `webhook` |
| `supply` | `play_store` | `json_key` → `service_account_json`, `json_key_data` → `service_account_json` |
| `unlock_keychain` | `keychain` | `path` → `name`, adds `action: unlock` |
| `upload_to_app_store` | `appstore` | `app_identifier` → `bundle_id`, `metadata_path` → `metadata_dir` |
| `upload_to_play_store` | `play_store` | `json_key` → `service_account_json`, `json_key_data` → `service_account_json` |
| `upload_to_testflight` | `testflight` |  |

## Moved by hand

| fastlane | Why |
|---|---|
| `match`, `sync_code_signing` | codesign_sync reads an existing match repository; it never creates or revokes certificates, so move this by hand |
| `sigh`, `get_provisioning_profile` | provisioning_profile downloads an existing profile; it never creates one, so move this by hand |
| `cert`, `get_certificates` | certificate downloads an existing certificate; it never creates one, so move this by hand |
| `snapshot`, `screengrab`, `frameit`, `precheck`, `produce`, `pem` | no equivalent; keep using a `run:` step for this |

<!-- END GENERATED -->
