# 18 — Store credential objects

## Goal

Let the store-publishing actions accept the credential format CI secret stores
can supply safely: plain JSON or base64 of JSON. App Store Connect actions get
one Apple credential object instead of three separately managed values; Google
Play keeps its existing service-account JSON argument and gains base64 input.

## Public configuration

The new optional `api_key` action argument has this exact JSON object shape:

```json
{"keyId":"ABC123","issuerId":"69a6de7e-0000-0000-0000-000000000000","authKey":"-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----"}
```

`api_key` may instead contain standard base64 whose decoded UTF-8 text is that
same JSON object. It is intended for one environment variable:

```yaml
with:
  api_key: ${ASC_API_KEY}
```

`authKey` keeps the pre-existing key behavior: it can be PEM text, standard
base64 of PEM text, or a path relative to `shlane.yaml`.

The existing `key_id`, `issuer_id`, and `key` arguments remain supported for
backward compatibility. A step must provide exactly one credential shape:

- `api_key`; or
- all of `key_id`, `issuer_id`, and `key`.

Mixing the two shapes, or giving only part of the legacy triplet, is a
configuration error. `shlane validate` must report that error before a lane
runs.

## Google Play configuration

`play_store.service_account_json` continues to accept its existing raw Google
service-account JSON or a path to a JSON file. It additionally accepts standard
base64 whose decoded UTF-8 text is that same service-account JSON:

```yaml
with:
  service_account_json: ${PLAY_SERVICE_ACCOUNT_BASE64}
```

Resolution is unambiguous and backward compatible: raw JSON is used directly;
otherwise an existing relative file is read; otherwise shlane decodes the value
as base64 and parses the resulting JSON. A malformed value produces a generic
credential-format error without echoing any part of the supplied secret.

## Scope

The shared input applies to every built-in action that authenticates against
App Store Connect:

- `testflight`
- `asc_request`
- `appstore`
- `provisioning_profile`
- `certificate`

The Google change is limited to `play_store.service_account_json`. Firebase
Distribution remains path-only because it passes its value to the Firebase CLI
rather than parsing service-account credentials itself.

## Design

`src/actions/asc.rs` owns an Apple credential-schema helper, a cross-field
validation helper, and a single loader. Each affected action appends the shared
schema and delegates its custom validation and loading to those helpers. This
keeps the JSON vocabulary, base64 decoding, and compatibility rule identical
across all Apple actions.

The action trait receives a default `validate_args` hook. Generic validation
continues to reject unknown arguments and ordinary required fields; actions
with credential alternatives use the hook for the one-of-two credential rule.
The default preserves source compatibility for third-party action
implementations.

The credential-object deserializer accepts only the three camel-case fields
`keyId`, `issuerId`, and `authKey`, and rejects unknown fields and blank
values. Its errors never repeat the supplied credential. `api_key` is marked
sensitive so the already-expanded object is registered for output masking.
The existing temporary `.p8` file lifecycle for `testflight` remains
unchanged.

`src/actions/google.rs` owns Google service-account resolution. It preserves
the existing raw-JSON and file-path behavior, then falls back to base64 decoding
and JSON deserialization. The Play action continues to receive a parsed
`ServiceAccount`; no decrypted JSON is written to a temporary file.

## Verification

Unit tests cover Apple raw JSON, base64(JSON), a PEM/base64/path `authKey`,
malformed or incomplete objects, and an error that does not echo secret input.
Google tests cover raw JSON, a relative file, base64(JSON), invalid base64, and
a failure that does not echo the supplied value. Action-schema tests cover the
all-or-nothing Apple legacy triplet and the mutually exclusive new object. The
complete Rust test suite and Clippy run before release.

## Release

This is additive and backward compatible, so the release is `v0.3.0`. The
release changes `Cargo.toml`, `CHANGELOG.md`, version-pinned documentation,
and the plan status. Publishing happens only from a pushed `v0.3.0` tag after
the local package and test checks succeed; GitHub Actions creates release
artifacts, and crates.io publishes only when its repository token is present.
