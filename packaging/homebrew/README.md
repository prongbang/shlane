# The Homebrew tap

`brew install prongbang/tap/shlane` needs a second repository, which Homebrew
requires to be named `prongbang/homebrew-tap`. It is not part of this one, so it
has to be created by hand once:

1. Create the repository `prongbang/homebrew-tap`, public, with a `Formula/`
   directory in it.
2. Create a token that can push to it — a fine-grained personal access token
   with `Contents: read and write` on that repository is enough — and add it to
   *this* repository as the secret `HOMEBREW_TAP_TOKEN`.

After that the `homebrew` job in [`../../.github/workflows/release.yml`](../../.github/workflows/release.yml)
regenerates `Formula/shlane.rb` on every release and pushes it. Without the
secret the job says so and succeeds: a release should not go red because a
distribution channel nobody configured is missing.

## The formula

[`generate.sh`](generate.sh) prints it, reading the checksums the release
published so the formula cannot claim a hash nobody verified:

```sh
./generate.sh 0.1.0 dist/SHA256SUMS > shlane.rb
```

It covers macOS on Apple silicon and Intel, and Linux on x86-64 and arm64. There
is no Windows in a Homebrew formula and no musl build in it either — the
gnu one is what a Homebrew Linux install wants.

`SHLANE_REPO` overrides the repository the URLs point at, for testing against a
fork.

## Checking it before a release

```sh
brew tap prongbang/tap
brew install --build-from-source --verbose shlane
brew test shlane
brew audit --strict --online prongbang/tap/shlane
```

`brew test` runs a real lane, not just `--version`: a binary that starts and
then cannot run a step is not installed correctly.
