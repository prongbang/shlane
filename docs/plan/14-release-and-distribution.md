# 14 — Releasing and distributing it

"One binary, nothing to install" only sells if installing it really is that easy.

## The targets to build

| Target | For |
|---|---|
| `aarch64-apple-darwin` | Mac M1 and later — the most important one, it is what mobile developers use |
| `x86_64-apple-darwin` | Intel Macs |
| `x86_64-unknown-linux-gnu` | ordinary CI |
| `aarch64-unknown-linux-gnu` | ARM runners |
| `x86_64-unknown-linux-musl` | containers without glibc |
| `x86_64-pc-windows-msvc` | Windows, for the core and Android only — needs a POSIX shell, which Git for Windows provides |

[`cargo-dist`](https://opensource.axo.dev/cargo-dist/) produces the release workflow, the
installer script, the checksums and the release notes in one go.

## How it can be installed

| Channel | Command | Priority |
|---|---|---|
| An install script | `curl -fsSL https://shlane.dev/install.sh \| sh` | P0 |
| GitHub Releases | download the tarball directly | P0 |
| A Homebrew tap | `brew install prongbang/tap/shlane` | P0 |
| crates.io | `cargo install shlane` | P1 |
| A GitHub Action | see [11](11-ci-integration.md) | P0 |
| Docker | `ghcr.io/prongbang/shlane` | P2 |
| A mise / asdf plugin | — | P2 |

## What has to be true before publishing to crates.io

`Cargo.toml` declares `readme = "README.md"`, but **that file does not exist**, so
`cargo publish` fails.

To do in M0:

- [ ] write `README.md`
- [ ] check `cargo package --list` for files that should not be in there
- [ ] add `LICENSE` — Cargo.toml says Apache-2.0, but there is no file in the repository
- [ ] get `cargo publish --dry-run` to pass

## Versioning

- Semantic versioning.
- Before 1.0 the schema may break, but every break goes in the CHANGELOG, and
  `shlane validate` has to say how to fix it.
- `version: 1` in `shlane.yaml` (see [03](03-config-schema.md)) is what lets the schema
  change later without breaking what exists.
- `min_shlane:` lets a config state the lowest version it works with.

## Keeping the artifacts safe

- publish `SHA256SUMS` with every release
- sign the artifacts, with minisign or keyless cosign through GitHub OIDC
- the install script checks the checksum before installing
- turn on GitHub artifact attestation

## The CHANGELOG

Generated from Keep a Changelog plus conventional commits. The repository's first commit
is `feat: initial`, which already fits, so commitlint can go into CI straight away.

## Binary size

`Cargo.toml` already sets `lto = true`, `codegen-units = 1` and `strip = true`, which is
right. But `panic = "abort"` has to go (see [02](02-architecture.md)) — a slightly larger
binary is worth an error message that is actually usable.

The target: **under 15 MB** per target, with a CI job that warns when a single PR grows
it by more than 10%.
