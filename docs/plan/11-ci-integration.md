# 11 — Running on CI

This is where shlane's main advantage over fastlane sits: there is no `bundle install`.

## Detecting CI

```rhai
is_ci()        // true when CI, GITHUB_ACTIONS, GITLAB_CI, BITRISE_IO, CIRCLECI, JENKINS_URL or BUILDKITE is set
ci_provider()  // "github" | "gitlab" | "bitrise" | ...
```

What changes by itself on CI:

- the spinner and the colours go, except on providers that handle ANSI
- `ui_confirm()` does not wait for input — it takes the default, or fails, depending on
  the flag
- `--verbose` turns itself on when a step fails, reprinting that step's whole log

## `setup_ci`, in place of fastlane's

```yaml
- action: setup_ci
  with:
    keychain_name: shlane_tmp
    timeout: 3600
```

It creates a temporary keychain, unlocks it, makes it the default, and registers the
cleanup that deletes it at the end, **whether the lane succeeded or failed**.

## Reports

| Format | Flag | For |
|---|---|---|
| JUnit XML | `--report junit:./reports/shlane.xml` | any CI that reads a test report |
| JSON | `--report json:./reports/shlane.json` | in-house tooling |
| Markdown summary | `--report md:$GITHUB_STEP_SUMMARY` | the GitHub Actions job summary |

## GitHub Actions annotations

When `ci_provider() == "github"`, emit workflow commands:

```
::group::build_ios
::error file=shlane.yaml,line=42::step 'testflight' failed: invalid API key
::endgroup::
```

That puts the error on the file itself in the PR view — something fastlane does not do
for you.

## A GitHub Action wrapper

In its own repository, `prongbang/shlane-action`:

```yaml
- uses: prongbang/shlane-action@v1
  with:
    version: "0.5.0"      # or "latest"
    lane: beta
    params: "target=production"
```

- downloads the binary for the platform, checks the checksum, and caches it with
  `@actions/tool-cache`
- setup takes one or two seconds, against 30–120 seconds for `bundle install` — **that
  is the number worth advertising**

## An example workflow

```yaml
jobs:
  beta:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: prongbang/shlane-action@v1
        with: { version: "0.5.0" }
      - run: shlane validate
      - run: shlane run beta target=production --report junit:reports/shlane.xml
        env:
          ASC_KEY_ID: ${{ secrets.ASC_KEY_ID }}
          ASC_ISSUER_ID: ${{ secrets.ASC_ISSUER_ID }}
          ASC_KEY_P8: ${{ secrets.ASC_KEY_P8 }}
      - uses: actions/upload-artifact@v4
        if: always()
        with: { name: reports, path: reports/ }
```

## Caching

`shlane` should not cache anything itself, but it should be able to **tell CI what is
worth caching**:

```
shlane cache-paths --json
# → ["~/.gradle/caches", "~/Library/Developer/Xcode/DerivedData", ".shlane/plugins"]
```

## What robustness requires

- every network action retries with exponential backoff, because an unreliable network
  is normal on CI
- there is a timeout per step (see [03](03-config-schema.md)), so a job cannot hang until
  it hits CI's own limit
- the SIGTERM that CI sends as a timeout approaches is caught, the `error` hook runs, and
  the exit is clean
