# 18 — Store credential objects

## v0.3.0 contract

Every built-in App Store Connect action (`testflight`, `asc_request`,
`appstore`, `provisioning_profile`, and `certificate`) accepts one sensitive
`api_key` value. It is either raw JSON or standard Base64(JSON):

```json
{"keyId":"ABC123","issuerId":"69a6de7e-0000-0000-0000-000000000000","authKey":"-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----"}
```

`authKey` remains compatible with PEM, base64 PEM, and a path relative to the
configuration. The existing `key_id`, `issuer_id`, and `key` triplet remains
supported, but a step must use exactly one credential form. `shlane validate`
rejects a mixed or incomplete form before the lane starts.

`play_store.service_account_json` now accepts raw Google service-account JSON,
an existing JSON file, or Base64(JSON). The decoder retains the parsed object
in memory and never writes decoded service-account JSON to a temporary file.

## Security and compatibility

All aggregate credential inputs are sensitive. Decoder and parser failures use
fixed messages that do not echo secret input. The TestFlight temporary `.p8`
file remains mode `0600` and is removed at the end of the step. Existing Apple
and Play credential formats remain valid.

## Verification

Unit tests cover Apple JSON and Base64(JSON), cross-field validation, migration
placeholders, Google raw JSON/file/Base64(JSON), and redacted invalid input.
The service calls themselves still require live Apple and Google credentials
for end-to-end validation.
