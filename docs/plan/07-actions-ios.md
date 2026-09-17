# 07 — The iOS actions

The hardest part of replacing fastlane, because Apple's code signing really is as
complicated as it looks.

## The mapping

| fastlane | shlane | Difficulty | Priority |
|---|---|---|---|
| `gym` / `build_app` | `build_ios` | high | P0 |
| `scan` / `run_tests` | `test_ios` | medium | P0 |
| `pilot` / `upload_to_testflight` | `testflight` | medium | P0 |
| `match` | `codesign_sync` | very high | P1 |
| `sigh` / `get_provisioning_profile` | `provisioning_profile` | high | P1 |
| `cert` / `get_certificates` | `certificate` | high | P1 |
| `deliver` / `upload_to_app_store` | `appstore` | very high | P2 |
| `produce` | — | — | not doing it |
| `snapshot` | — | — | not doing it (call xcodebuild from a `run:` step) |
| `frameit` | — | — | not doing it |
| `pem`, `precheck`, `spaceship` | — | — | not doing it |
| `setup_ci` | `setup_ci` (see [11](11-ci-integration.md)) | medium | P0 |
| `create_keychain`, `unlock_keychain`, `delete_keychain` | `keychain` | medium | P0 |
| `update_project_team`, `update_code_signing_settings` | `xcode_settings` | medium | P2 |
| Appfile | an `ios:` block in `shlane.yaml` | low | P1 |
| Matchfile | a `codesign:` block in `shlane.yaml` | low | P1 |

## `build_ios`, in place of gym

A wrapper around `xcodebuild archive` and `xcodebuild -exportArchive`.

```yaml
- id: build
  action: build_ios
  with:
    workspace: MyApp.xcworkspace     # or project:
    scheme: MyApp
    configuration: Release
    export_method: app-store         # app-store | ad-hoc | development | enterprise
    output_dir: ./build
    destination: "generic/platform=iOS"
    xcargs: "-allowProvisioningUpdates"
    clean: true
    silent: false
```

Outputs: `ipa`, `dsym`, `archive`, `app_path`.

The real work is:

- building `ExportOptions.plist` from the arguments — this is what gym does for people
  without them noticing
- parsing xcodebuild's very long output. There has to be a summarising mode, the way
  xcpretty has one, or the log drowns everything.
- telling a compile error apart from a signing error, because the two are fixed in
  completely different ways
- reading the `xcresult` bundle for the result

## `test_ios`, in place of scan

```yaml
- action: test_ios
  with:
    scheme: MyAppTests
    devices: ["iPhone 15"]
    result_bundle: ./build/test.xcresult
    output: [junit, json]
    code_coverage: true
```

The `.xcresult` has to be read with `xcrun xcresulttool get --format json` and turned
into JUnit XML ([11](11-ci-integration.md)).

## Code signing — the strategic decision

`match` is the number one reason teams are still stuck on fastlane. It keeps encrypted
certificates and profiles in a git repository and syncs them onto every machine.

There are three options:

| Option | For | Against |
|---|---|---|
| **A. Work with an existing match repo** | a team can move over without issuing new certificates | means reverse-engineering match's encryption (OpenSSL AES-256-CBC) and its folder layout exactly |
| **B. Build something new** | a clean design, on something safer like age/sops | the team has to migrate its certificates, which hurts |
| **C. App Store Connect API plus `-allowProvisioningUpdates` only** | by far the simplest; no secrets to keep, Apple does it | does not cover some enterprise and ad-hoc cases, and needs an API key |

**The proposal: C first (M5), then A later (M6+).** C covers most modern CI and gives
value soonest; A is what decides whether a large team can move at all.

> **Both C and A were built (M5/M6):** `build_ios` passes
> `-allowProvisioningUpdates` (C), and `codesign_sync` can read an existing match repo
> (A) **read-only** — it does not issue or revoke certificates, because getting that
> wrong costs a team its ability to ship. The decryption happens in process (the
> `openssl` CLI is not called, because macOS ships LibreSSL, which has broken match
> before). Both `-md sha256` and older repos' `-md md5` are supported.

## The App Store Connect API

Both `testflight` and `appstore` need a JWT signed with ES256 from a `.p8` key.

```yaml
ios:
  api_key:
    key_id: ${env.ASC_KEY_ID}
    issuer_id: ${env.ASC_ISSUER_ID}
    key_content: ${env.ASC_KEY_P8}     # base64 — must always be masked in the log
```

- signed with `jsonwebtoken` plus `p256`/`ring`
- the token lasts 20 minutes, so a long upload has to refresh it automatically
- uploading the binary itself still goes through `xcrun altool` / `iTMSTransporter` in
  the first phase — write an uploader later if it turns out to be necessary

## Status — M5

| Action | Status |
|---|---|
| `build_ios` | ✅ done (archive + export + ExportOptions.plist) |
| `test_ios` | ✅ done, including xcresult → JUnit (needs Xcode 16+) |
| `keychain` | ✅ done |
| `testflight` | ✅ done, through `xcrun altool` |
| `asc_request` | ✅ done — calls any ASC API endpoint, with an ES256 JWT |
| `codesign_sync` (match) | ✅ done, read-only — reads an existing match repo, never issues or revokes a certificate |
| `appstore` (deliver) | ❌ not done (M7) |

## Risks

- **Apple changes the APIs and the behaviour often.** There has to be an integration
  test that really runs on a macOS runner, at least once a week.
- **It is hard to test.** A real Apple Developer account is needed, so the tests come in
  two layers: unit tests over how the arguments are assembled (every PR), and e2e (on a
  schedule).
- **macOS runners are expensive**, which limits how many e2e runs there can be.
