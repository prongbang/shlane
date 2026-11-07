# 21 — Recover the v0.5.1 release build

## Objective

Use a supported Apple Silicon runner and complete the cancelled `v0.5.1` release without moving its tag.

## Steps

1. Replace retired `macos-14` release runners with supported `macos-15` runners.
2. Make `workflow_dispatch` use its required tag input for version validation, artifact names, release notes, GitHub release, and crates.io publication.
3. Validate the workflow and package version, push the workflow change, then dispatch the release workflow on `master` with `tag=v0.5.1`.
4. Confirm the Apple builds and publication jobs complete for the existing tag.

## Acceptance criteria

- Both Apple target builds use the supported runner and start successfully.
- A manual dispatch from `master` builds and publishes the specified existing version tag.
- The workflow still rejects a tag that does not match the Cargo package version.
- The existing tag remains unchanged.

## Security

- **Trust boundaries:** workflow dispatch input controls release naming and publication; release jobs can access GitHub release permissions and the crates.io token. Validate the selected tag against `Cargo.toml`, pass it through environment/context values rather than interpolating it into shell source, and keep `CARGO_REGISTRY_TOKEN` scoped to the crates.io job.

## Execution status

Commit `00b5e60` moved both Apple builds to `macos-15` and made manual dispatch honor its tag input. Workflow run `42397852300` completed successfully: all six target builds and the GitHub Release assets were published for the existing `v0.5.1` tag, which remains on commit `9f1435b`. The crates.io job skipped publication with a warning because the repository Actions secret `CARGO_REGISTRY_TOKEN` is not configured.
