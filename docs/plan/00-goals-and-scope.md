# 00 — Goals and scope

## Why replace fastlane at all

| The problem | What it costs |
|---|---|
| Needs Ruby, Bundler and a full set of gems | rbenv/rvm to manage, and Ruby versions that differ between a developer's machine and CI |
| `bundle install` on every CI run | 30–120 seconds per job when it is not cached |
| Dependency conflicts | plugin gems colliding with each other is routine |
| Slow cold start | `fastlane` takes ~3–8 seconds before it does anything |
| The Fastfile is a Ruby DSL | readable while it is short, hard to debug once it is not, and nothing checks it before it runs |

What `shlane` is aiming at: **one binary, no runtime dependencies, starts immediately,
and a config with a schema that can be checked.**

## Goals

1. **A single static binary** — download it and it runs; nothing else to install.
2. **Cover the 80% of workflows people actually have** — build, test, sign, bump a
   version, upload to TestFlight or Play, tell Slack.
3. **Config before code** — YAML first, Rhai when YAML is not enough.
4. **Checkable before it runs** — `shlane validate` finds config errors without
   executing anything.
5. **A clear way across** — tooling and a mapping table for an existing Fastfile.
6. **Fast** — under 100ms from the command being typed to the first step starting.

## Non-goals

- **Not 400 actions.** Only the ones people use; `run:` or a plugin covers the rest.
- **Not running a Fastfile directly.** It can be converted, best-effort. No embedded
  Ruby interpreter.
- **No GUI or web dashboard.**
- **Not a CI server.** shlane is something CI calls, not a replacement for it.
- **No Windows support for the iOS actions** — that is Xcode's constraint, not ours —
  but the core should run there.

## What "can replace fastlane" means

It is true when one real project can do all of this **with its `Gemfile` and
`fastlane/` deleted**:

- [ ] `shlane run beta` builds an iOS app, signs it with a certificate from CI, and
      uploads it to TestFlight
- [ ] `shlane run beta` on Android builds an AAB, signs it, and uploads it to the Play
      Store internal track
- [ ] `shlane run test` runs the unit tests and emits a JUnit report the CI can read
- [ ] a version or build number can be bumped, committed, tagged and pushed
- [ ] secrets come from the CI's environment, and none of them reach the log
- [ ] a failing lane turns the CI job red with the right exit code, and the error says
      which step failed
- [ ] the pipeline is no slower than it was under fastlane

## Design principles

1. **Explicit over implicit** — no magic global state like `lane_context`, where
   nothing says who set what (see [02](02-architecture.md)).
2. **Fail fast, fail loud** — a mistake should name the line in the YAML.
3. **Every action supports `--dry-run`** — print what would happen without doing it.
4. **Always an escape hatch** — when an action does not cover an option, a raw command
   still can.
