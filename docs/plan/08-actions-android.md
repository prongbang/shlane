# 08 — The Android actions

Much easier than iOS, and worth doing **before** iOS, to prove the action architecture
on work that can be kept under control.

## The mapping

| fastlane | shlane | Difficulty | Priority |
|---|---|---|---|
| `gradle` | `gradle` | low | P0 |
| `gradle(task: "assembleRelease")` | `build_android` | low | P0 |
| `gradle(task: "bundleRelease")` | `build_android` (`format: aab`) | low | P0 |
| `gradle(task: "test")` | `test_android` | low | P0 |
| `supply` / `upload_to_play_store` | `play_store` | medium | P1 |
| `firebase_app_distribution` (a plugin) | `firebase_distribution` | medium | P1 |
| `sign_apk` / `zipalign` (a plugin) | `sign_android` | medium | P1 |
| `get_version_code` (a plugin) | `read_version` (see [06](06-actions-core.md)) | low | P0 |
| `screengrab` | — | — | not doing it |

## `gradle` — the one everything else sits on

```yaml
- action: gradle
  with:
    task: assembleRelease
    project_dir: ./android
    properties:
      android.injected.version.code: ${steps.ver.code}
    flags: ["--no-daemon", "--stacktrace"]
    wrapper: true        # use ./gradlew when there is one (the default)
```

- `gradlew` has to be found by walking up from `project_dir`, and checked for the
  execute bit
- gradle's output has to be read to find what it built (the `apk`/`aab` path) and
  returned as an output — today people hardcode that path themselves
- a secret value should go through `ORG_GRADLE_PROJECT_*` rather than a long string of
  `-P` arguments, so it never appears in the process list

## `build_android`

```yaml
- id: build
  action: build_android
  with:
    format: aab            # apk | aab
    flavor: prod
    build_type: release
    project_dir: ./android
```

Outputs: `aab` / `apk`, `mapping_txt`, `version_code`, `version_name`.

## `sign_android`

```yaml
- action: sign_android
  with:
    input: ${steps.build.apk}
    keystore: ${env.ANDROID_KEYSTORE_PATH}      # or keystore_base64
    keystore_password: ${env.KEYSTORE_PASSWORD} # sensitive
    key_alias: upload
    key_password: ${env.KEY_PASSWORD}           # sensitive
```

- uses `apksigner` and `zipalign` from the Android SDK build-tools, found through
  `$ANDROID_HOME`
- takes a base64 keystore from the environment, for CI, and writes it to a temporary
  file that is always deleted, including when something fails
- every password goes into the `SecretRegistry` (see [10](10-secrets-and-env.md))

## `play_store`, in place of supply

Uses the Google Play Developer Publishing API v3, which is plain REST:

```yaml
- action: play_store
  with:
    package_name: com.example.app
    aab: ${steps.build.aab}
    track: internal          # internal | alpha | beta | production
    release_status: draft
    rollout: 0.1
    service_account_json: ${env.PLAY_SERVICE_ACCOUNT}   # sensitive
    mapping: ${steps.build.mapping_txt}
    release_notes:
      en-US: ${params.notes}
```

The API goes `edits.insert` → `edits.bundles.upload` → `edits.tracks.update` →
`edits.commit`. What has to be handled: the OAuth2 service account (JWT → access
token), a resumable upload for large files, and retries on 5xx and on the rate limit.

## `firebase_distribution`

- Option 1: wrap the `firebase` CLI. Quick — a day's work — but it adds a dependency the
  user has to install, which is against the "one binary" goal.
- Option 2: call the Firebase App Distribution REST API directly, with the same service
  account as Play.

**The proposal: option 2**, with `use_cli: true` as a fallback.

## Why Android comes first

1. It can be tested on a Linux runner, which is cheap and fast.
2. No Apple account is needed to develop it.
3. The Play API is ordinary REST, so it proves out the HTTP, auth and retry layer that
   iOS will reuse.
4. Android's code signing is "a keystore file and a password", which is far easier to
   understand than match.
