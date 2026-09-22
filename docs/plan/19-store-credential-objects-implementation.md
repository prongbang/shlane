# Store Credential Objects Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Release shlane v0.3.0 with secure JSON/base64 credential inputs for App Store Connect and Google Play.

**Architecture:** Centralize Apple credential-object parsing, schema declaration, and cross-field validation in `actions::asc`; all Apple actions use it instead of independently reading three values. Extend the existing Google `ServiceAccount` resolver so its established raw-JSON/path contract gains a final base64(JSON) fallback without creating credential files.

**Tech Stack:** Rust 2021, serde/serde_yaml, shlane action schemas, Cargo tests and Clippy.

**Spec:** `docs/plan/18-asc-api-key-object.md`

## Global Constraints

- `api_key` is one sensitive string containing raw JSON or standard base64(JSON) with exactly `keyId`, `issuerId`, and `authKey`.
- The existing Apple `key_id`, `issuer_id`, `key` triplet stays compatible; it and `api_key` are mutually exclusive.
- Every built-in App Store Connect action has the same credential contract.
- `play_store.service_account_json` keeps raw JSON and path support, then accepts base64(JSON); Firebase remains unchanged.
- Never include a supplied credential or a decoded credential byte in an error or log.
- Release version is exactly `0.3.0`; do not push, tag, publish, or alter remote state without fresh verification and user authorization at that boundary.

## Review Focus

- Base64 text that happens to name a missing file must decode as a credential rather than becoming a filesystem error; Task 3 tests this.
- An Apple step that mixes `api_key` with a legacy field must fail validation before a lane starts; Task 2 tests this.
- A partial legacy Apple triplet must name all missing fields without treating `api_key` as required; Task 2 tests this.
- Invalid credential input must not appear in diagnostics even when it has invalid base64 characters; Tasks 1 and 3 test this.
- Existing raw Apple PEM/base64/path and Google raw JSON/path inputs must remain usable; Tasks 1 and 3 test this.

---

### Task 1: Apple credential object and action-validation hook

**Files:**
- Modify: `src/actions/mod.rs:75-119`
- Modify: `src/actions/asc.rs:12-45,124-182`
- Test: `src/actions/mod.rs:212-260`
- Test: `src/actions/asc.rs:124-182`

**Interfaces:**
- Produces: `Action::validate_args(&BTreeMap<String, String>) -> Vec<String>` with an empty default.
- Produces: `asc::credential_args() -> Vec<ArgSpec>`, `asc::credential_problems(&BTreeMap<String, String>) -> Vec<String>`, and `asc::load_credential(&Args, &Path) -> Result<ApiKey, String>`.
- Consumes: `google::decode_base64`, `serde_yaml`, and the current `ApiKey::load` PEM/base64/path resolver.

- [ ] **Step 1: Write failing tests for raw and base64 Apple objects, validation, and redaction**

```rust
#[test]
fn reads_an_apple_key_object_from_base64() {
    let encoded = encode_base64(br#"{"keyId":"K","issuerId":"I","authKey":"-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----"}"#);
    let key = load_object(&encoded, Path::new(".")).expect("valid object");
    assert_eq!(key.key_id, "K");
    assert_eq!(key.issuer_id, "I");
}

#[test]
fn apple_credential_errors_do_not_echo_the_input() {
    let secret = "not-base64-secret";
    let error = load_object(secret, Path::new(".")).expect_err("invalid input");
    assert!(!error.contains(secret), "{error}");
}
```

- [ ] **Step 2: Run the focused tests and verify they fail because the helper API does not exist**

Run: `cargo test actions::asc::tests::reads_an_apple_key_object_from_base64 actions::asc::tests::apple_credential_errors_do_not_echo_the_input`

Expected: FAIL with unresolved `load_object` or equivalent missing helper errors.

- [ ] **Step 3: Implement the shared object loader and the default action hook**

```rust
pub trait Action {
    // existing methods
    fn validate_args(&self, _provided: &BTreeMap<String, String>) -> Vec<String> {
        Vec::new()
    }
}

pub fn check_args(action: &dyn Action, provided: &BTreeMap<String, String>) -> Vec<String> {
    // existing required and unknown checks
    problems.extend(action.validate_args(provided));
    problems
}
```

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialObject {
    #[serde(rename = "keyId")]
    key_id: String,
    #[serde(rename = "issuerId")]
    issuer_id: String,
    #[serde(rename = "authKey")]
    auth_key: String,
}
```

Decode only when the trimmed input does not start with `{`; map decode, UTF-8,
and parse failures to constant errors. Reject empty fields before delegating
`auth_key` to `ApiKey::load`.

- [ ] **Step 4: Run focused Apple and action-schema tests**

Run: `cargo test actions::asc::tests actions::tests`

Expected: PASS.

- [ ] **Step 5: Commit the shared Apple credential primitives**

```bash
git add src/actions/mod.rs src/actions/asc.rs
git commit -m "feat(asc): accept JSON credential objects"
```

### Task 2: Adopt the Apple credential contract across all ASC actions

**Files:**
- Modify: `src/actions/core/ios.rs:524-638`
- Modify: `src/actions/core/appstore.rs:45-73,201-209`
- Modify: `src/actions/codesign/fetch.rs:17-37`
- Test: `src/config/validate.rs`

**Interfaces:**
- Consumes: `asc::credential_args`, `asc::credential_problems`, and `asc::load_credential` from Task 1.
- Produces: `testflight`, `asc_request`, `appstore`, `provisioning_profile`, and `certificate` schemas that all accept `api_key` or the legacy triplet.

- [ ] **Step 1: Write failing config-validation tests for the alternative credential shapes**

```rust
#[test]
fn app_store_actions_accept_one_api_key_object() {
    let problems = check(&config(
        "lanes:\n  ship:\n    steps:\n      - action: testflight\n        with:\n          ipa: build/App.ipa\n          api_key: '${ASC_API_KEY}'\n",
    ));
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn app_store_actions_reject_mixed_credentials() {
    let problems = check(&config(
        "lanes:\n  ship:\n    steps:\n      - action: asc_request\n        with:\n          path: /v1/apps\n          api_key: object\n          key_id: old\n",
    ));
    assert!(problems.iter().any(|problem| problem.contains("either api_key or")), "{problems:?}");
}
```

- [ ] **Step 2: Run the focused validation tests and verify they fail because `api_key` is unknown**

Run: `cargo test config::validate::tests::app_store_actions`

Expected: FAIL reporting that `testflight` or `asc_request` has no `api_key` argument.

- [ ] **Step 3: Replace duplicated Apple argument declarations and loaders**

```rust
fn schema(&self) -> Vec<ArgSpec> {
    let mut schema = vec![ArgSpec::new("ipa", "The .ipa to upload").required()];
    schema.extend(credential_args());
    schema
}

fn validate_args(&self, provided: &BTreeMap<String, String>) -> Vec<String> {
    credential_problems(provided)
}
```

Each runtime caller must use `load_credential(args, ctx.workdir())`, including
the `appstore` client and the shared codesign fetch request helper.

- [ ] **Step 4: Run action, validation, and integration tests**

Run: `cargo test actions::core::ios::tests actions::core::appstore::tests actions::codesign::fetch::tests config::validate::tests --lib && cargo test --test cli`

Expected: PASS.

- [ ] **Step 5: Commit the ASC action adoption**

```bash
git add src/actions/core/ios.rs src/actions/core/appstore.rs src/actions/codesign/fetch.rs src/config/validate.rs
git commit -m "feat(asc): share API key input across actions"
```

### Task 3: Base64 Google Play service-account input

**Files:**
- Modify: `src/actions/google.rs:20-49,235-246`
- Modify: `src/actions/core/play.rs:89-95`
- Test: `src/actions/google.rs:235-246`

**Interfaces:**
- Produces: `ServiceAccount::load` accepting raw JSON, an existing relative JSON path, or standard base64(JSON).
- Consumes: `decode_base64` from the same module and the existing `play_store.service_account_json` input.

- [ ] **Step 1: Write failing tests for base64 JSON, an existing file, and secret-safe failures**

```rust
#[test]
fn reads_a_service_account_from_base64_json() {
    let encoded = encode_base64(br#"{"client_email":"a@b.com","private_key":"pem"}"#);
    let account = ServiceAccount::load(&encoded, Path::new(".")).expect("valid");
    assert_eq!(account.client_email, "a@b.com");
}

#[test]
fn invalid_base64_does_not_echo_the_service_account_value() {
    let secret = "bad secret value";
    let error = ServiceAccount::load(secret, Path::new(".")).expect_err("invalid");
    assert!(!error.contains(secret), "{error}");
}
```

- [ ] **Step 2: Run the focused Google tests and verify the base64 case fails as a missing-path read**

Run: `cargo test actions::google::tests::reads_a_service_account_from_base64_json actions::google::tests::invalid_base64_does_not_echo_the_service_account_value`

Expected: FAIL because the base64 value is treated as a file path or its error exposes that path.

- [ ] **Step 3: Add the JSON → existing-path → base64(JSON) resolver**

```rust
let text = if value.trim_start().starts_with('{') {
    value.to_string()
} else if root.join(value.trim()).is_file() {
    std::fs::read_to_string(root.join(value.trim())).map_err(|_| "cannot read service account file")?
} else {
    String::from_utf8(decode_base64(value).map_err(|_| "service account must be JSON, an existing file, or base64 JSON")?)
        .map_err(|_| "service account base64 is not UTF-8 JSON")?
};
```

Keep `service_account_json` sensitive and amend its schema description to name
base64 JSON.

- [ ] **Step 4: Run the focused Google and Play tests**

Run: `cargo test actions::google::tests actions::core::play::tests`

Expected: PASS.

- [ ] **Step 5: Commit Google Play credential support**

```bash
git add src/actions/google.rs src/actions/core/play.rs
git commit -m "feat(play): accept base64 service accounts"
```

### Task 4: Document and package v0.3.0

**Files:**
- Modify: `README.md`
- Modify: `docs/schema-v1.md`
- Modify: `docs/fastlane-in-15-minutes.md`
- Modify: `docs/plan/16-whats-left.md`
- Modify: `docs/plan/18-asc-api-key-object.md`
- Modify: `docs/plan/19-store-credential-objects-implementation.md`
- Modify: `docs/plan/README.md`
- Modify: `CHANGELOG.md`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: the exact behavior verified by Tasks 1–3.
- Produces: user-facing YAML examples that use `api_key: ${ASC_API_KEY}` and `service_account_json: ${PLAY_SERVICE_ACCOUNT_BASE64}` and release metadata for `0.3.0`.

- [ ] **Step 1: Update examples and schema reference without replacing legacy examples**

```yaml
- action: testflight
  with:
    ipa: build/App.ipa
    api_key: ${ASC_API_KEY}

- action: play_store
  with:
    package_name: com.example.app
    aab: build/app.aab
    service_account_json: ${PLAY_SERVICE_ACCOUNT_BASE64}
```

Document the exact Apple field names `keyId`, `issuerId`, and `authKey`, plus
the raw-JSON/base64(JSON) alternative and legacy compatibility.

- [ ] **Step 2: Update release records**

Set `Cargo.toml` to `version = "0.3.0"`. Move the changelog entry into
`## [0.3.0] - 2026-09-22` with Added entries for Apple credential objects and
Play base64 JSON. Keep a blank `## [Unreleased]`; update version-pinned guide
references and the plan status from 0.2.3 to 0.3.0 where they describe latest.

- [ ] **Step 3: Run formatting and package validation**

Run: `cargo fmt --check && cargo package --allow-dirty --list`

Expected: PASS; the package list includes `README.md`, `LICENSE`, and `docs/schema-v1.md` but not planning documents.

- [ ] **Step 4: Commit documentation and release metadata**

```bash
git add README.md docs/schema-v1.md docs/fastlane-in-15-minutes.md docs/plan CHANGELOG.md Cargo.toml
git commit -m "chore(release): 0.3.0"
```

### Task 5: Verify and prepare the release boundary

**Files:**
- Verify only: repository worktree and release workflow

**Interfaces:**
- Consumes: the versioned commits from Tasks 1–4.
- Produces: fresh evidence required before the user is asked to authorize tag, push, GitHub Release, and crates.io publication.

- [ ] **Step 1: Run the full quality gate**

Run: `cargo test && cargo clippy --all-targets --all-features -- -D warnings && cargo build --release && cargo package --allow-dirty`

Expected: all commands exit 0.

- [ ] **Step 2: Verify the versioned CLI and documentation configuration examples**

Run: `cargo run -- --version && cargo run -- action list && git diff --check`

Expected: CLI reports `shlane 0.3.0`, action list contains `testflight` and `play_store`, and no whitespace errors occur.

- [ ] **Step 3: Create the review package and request an independent whole-branch review**

Run: `git diff --check <merge-base>..HEAD`

Expected: clean diff; send the reviewer the spec, this plan, test evidence, and the credential/error redaction focus.

- [ ] **Step 4: Ask for release authorization after review is clean**

Do not tag, push, publish, or create a remote release in this task. Present
the exact commit SHA, tag `v0.3.0`, verification evidence, and reviewer result
to the user, then wait for confirmation before external release operations.
