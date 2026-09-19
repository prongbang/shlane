# The plan for replacing fastlane

This is the plan `shlane` is being built to: taking it from what it was — a prototype
of about 200 lines — to a tool that can stand in for
[fastlane](https://fastlane.tools) on real mobile CI/CD.

## Where it started (v0.1.0)

| | |
|---|---|
| Code | one file, `src/main.rs`, 199 lines |
| Features | `shlane run <lane> [k=v]`, YAML lanes (before/steps/script/after), four Rhai builtins |
| Actions | none — only `run:`, which calls `sh -c` |
| Tests | none |
| CI | none |
| Docs | none; `README.md` did not exist, though `Cargo.toml` referred to it |

## Contents

| File | What it covers |
|---|---|
| [00-goals-and-scope.md](00-goals-and-scope.md) | Goals, scope, what is deliberately left out, and what "can replace it" means |
| [01-gap-analysis.md](01-gap-analysis.md) | fastlane's features against shlane's, one by one |
| [02-architecture.md](02-architecture.md) | Module layout, error handling, lane context |
| [03-config-schema.md](03-config-schema.md) | The `shlane.yaml` v1 specification |
| [04-cli-ux.md](04-cli-ux.md) | Commands, flags, exit codes, what the output looks like |
| [05-scripting-rhai.md](05-scripting-rhai.md) | The Rhai API worth having |
| [06-actions-core.md](06-actions-core.md) | The action system, and the actions that are not tied to a platform |
| [07-actions-ios.md](07-actions-ios.md) | Replacing gym / scan / match / pilot / deliver |
| [08-actions-android.md](08-actions-android.md) | Replacing gradle / supply / firebase distribution |
| [09-plugins.md](09-plugins.md) | The plugin system, in place of fastlane's |
| [10-secrets-and-env.md](10-secrets-and-env.md) | Environment, `.env`, secrets, masking |
| [11-ci-integration.md](11-ci-integration.md) | Running on CI, reports, the GitHub Action |
| [12-migration-from-fastlane.md](12-migration-from-fastlane.md) | The tooling and the guide for moving off a Fastfile |
| [13-testing-and-quality.md](13-testing-and-quality.md) | How this gets tested, and how the code is kept honest |
| [14-release-and-distribution.md](14-release-and-distribution.md) | Building and shipping the binary |
| [15-roadmap.md](15-roadmap.md) | Milestones M0–M7, the order of work, and the risks |
| [16-whats-left.md](16-whats-left.md) | What remains at 0.2.1, checked against the code |

## How to read it

- Start with `00` and `01`: why this is worth doing, and how much is enough.
- `02`–`05` are the foundations. They come before any action, because changing them
  later is expensive.
- `06`–`09` are the bulk of the work — the part that makes "replaces fastlane" true.
- `15` is the order to actually do it in. Starting tomorrow means starting at M0.
- `16` is where it stopped: what is left, and what nobody has verified yet.
