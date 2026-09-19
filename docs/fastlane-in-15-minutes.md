# Move off fastlane in 15 minutes

For an ordinary project: a Fastfile of a few lanes that test, build, upload and post to
Slack. Everything below was run against the Fastfile in step 2, and the output shown is
what shlane printed.

You do not have to move everything at once. A lane you have not moved yet can still call
fastlane (step 5), so start with the lanes that are easy.

## 1. Install (1 minute)

```sh
curl -fsSL https://raw.githubusercontent.com/prongbang/shlane/master/install.sh | sh
```

or `cargo install shlane`. There is no Ruby, no Bundler and no `Gemfile`: shlane is one
binary.

## 2. Convert (2 minutes)

In the directory that holds `fastlane/`:

```sh
shlane migrate
```

Given this Fastfile:

```ruby
default_platform(:ios)

platform :ios do
  desc "Run the unit tests"
  lane :test do
    scan(scheme: "MyApp", devices: ["iPhone 16"])
  end

  desc "Ship a build to TestFlight"
  lane :beta do
    ensure_git_status_clean
    increment_build_number
    gym(scheme: "MyApp", export_method: "app-store")
    pilot(skip_waiting_for_build_processing: true)
    slack(message: "iOS beta is on TestFlight", slack_url: ENV["SLACK_URL"])
  end
end

platform :android do
  desc "Run the unit tests"
  lane :test do
    gradle(task: "test")
  end

  desc "Ship to the internal track"
  lane :deploy do |options|
    gradle(task: "bundle", build_type: "Release")
    supply(track: options[:track], json_key: "play-key.json")
  end
end
```

it writes `shlane.yaml` and prints what is left for you:

```
  4 lane(s), 9 action(s) converted, 0 line(s) left for you

What needs a person:
  - `pilot` -> `testflight`: fill in `ipa` (The .ipa to upload); ...
  - `pilot` -> `testflight`: fill in `key_id` (App Store Connect key id); ...
  - `pilot` -> `testflight`: fill in `issuer_id` (App Store Connect issuer id); ...
  - `pilot` -> `testflight`: fill in `key` (The .p8 itself, base64 of it, or a path to it); ...
  - `supply` -> `play_store`: fill in `package_name` (Application id, e.g. com.example.app); ...
  - `fastlane ios test` is `shlane run ios_test`: shlane lane names are not scoped by platform
  - `fastlane android test` is `shlane run android_test`: shlane lane names are not scoped by platform
```

## 3. Fix what it could not know (7 minutes)

```sh
shlane validate
```

```
error: shlane.yaml has 2 problem(s):
  - 'beta': action 'testflight' has no argument 'skip_waiting_for_build_processing' (it takes: ipa, key_id, issuer_id, key, platform)
  - 'deploy': action 'gradle' has no argument 'build_type' (it takes: task, project_dir, properties, flags, wrapper)
```

Between the summary and `validate`, every gap is named. The fixes are nearly always one
of these:

| What you see | What to do |
|---|---|
| `TODO-<name>` | fastlane read it from the Appfile or the environment. Write it in, or use `${NAME}` for an environment variable |
| an argument `validate` rejects | shlane has no such option, or calls it something else. `shlane action show <action>` lists what it takes |
| a value one step produced and another used (`lane_context`) | give the first step an `id:` and use `${steps.<id>.<output>}` |
| `options[:x]` became `${x}` | declare it under `params:` with a default, or pass `x=value` on the command line |
| `gradle(task: "bundle", build_type: "Release")` | `build_android`, which also reports where the bundle landed |

`increment_build_number` needs nothing from you: it becomes `run: agvtool next-version
-all`, which is what fastlane runs.

For the Fastfile above, `beta` and `deploy` end up like this:

```yaml
  beta:
    description: "Ship a build to TestFlight"
    platform: ios
    steps:
      - action: git_status_clean
      - run: agvtool next-version -all
      - id: build
        action: build_ios
        with:
          scheme: "MyApp"
          export_method: "app-store"
      - action: testflight
        with:
          ipa: ${steps.build.ipa}
          key_id: ${ASC_KEY_ID}
          issuer_id: ${ASC_ISSUER_ID}
          key: ${ASC_KEY}             # masked in every log line
      - action: notify_slack
        with:
          text: "iOS beta is on TestFlight"
          webhook: "${SLACK_URL}"

  deploy:
    description: "Ship to the internal track"
    platform: android
    params:
      track:
        type: string
        default: internal
    steps:
      - id: bundle
        action: build_android
      - action: play_store
        with:
          track: "${track}"
          service_account_json: "play-key.json"
          aab: ${steps.bundle.aab}
          package_name: com.example.app
```

`testflight` does not wait for Apple to process the build, so
`skip_waiting_for_build_processing` has nothing to do and can go.

## 4. Try it without touching anything (2 minutes)

```sh
shlane validate
shlane run beta --dry-run
```

```
Would run: agvtool next-version -all
Would run: xcodebuild archive -scheme 'MyApp' -configuration 'Release' -destination 'generic/platform=iOS' -archivePath 'build/MyApp.xcarchive' -allowProvisioningUpdates
Would write build/ExportOptions.plist
Would run: xcodebuild -exportArchive -archivePath 'build/MyApp.xcarchive' ...
Would run: xcrun altool --upload-app -f '<steps.build.ipa>' -t 'ios' --apiKey '...' --apiIssuer '...'
Would post to Slack: {"text":"iOS beta is on TestFlight"}
```

A dry run still does its reads for real — `git status`, reading a version file — and
skips only the changes. So `git_status_clean` fails a dry run on a dirty tree, exactly as
the real run would.

`shlane list` shows the lanes, and `shlane run <lane>` runs one.

## 5. Keep fastlane for what has not moved

A lane that still needs fastlane calls it:

```yaml
  release:
    steps:
      - run: bundle exec fastlane ios release
```

Move one lane at a time: tests first, then builds, then version bumps and changelogs,
and uploads last.

## 6. Run it on CI (3 minutes)

On GitHub Actions:

```yaml
- uses: prongbang/shlane@v0.2.1
  with:
    lane: beta
    args: --report junit:reports/shlane.xml
  env:
    ASC_KEY_ID: ${{ secrets.ASC_KEY_ID }}
    ASC_ISSUER_ID: ${{ secrets.ASC_ISSUER_ID }}
    ASC_KEY: ${{ secrets.ASC_KEY }}
    SLACK_URL: ${{ secrets.SLACK_URL }}
```

Anywhere else, the install line from step 1, then `shlane run beta`. On macOS, add
`- action: setup_ci` as a lane's first step: it makes a temporary keychain and deletes it
when the run ends.

## What does not move

- **`match`, `sigh` and `cert`** can create certificates and profiles. shlane only reads
  them: `codesign_sync` reads an existing match repository, and `provisioning_profile`
  and `certificate` download what Apple already holds. Creating them stays with fastlane
  or the developer portal.
- **`snapshot`, `frameit`, `precheck`, `produce` and `pem`** have no equivalent. Keep
  them behind a `run:` step.
- **Ruby logic** — `if`, loops, your own methods — is left as a `TODO` comment. When a
  conditional block is flattened, `migrate` says so: those steps now always run.

[`migration.md`](migration.md) has every action and what it becomes.

**Worth knowing before you ship with it:** `test_ios`, the `build_ios` archive and the
Android build and signing actions run for real in shlane's own CI. Uploads
(`testflight`, `appstore`, `play_store`) and the `.ipa` export have only been tested
for what they send, not against Apple or Google. Try your first upload with
`--dry-run`, then on a build you do not mind.
