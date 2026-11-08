# 22 — Remediate Security Scan Findings

## Objective

Address the eight findings from the standard Codex Security scan of revision
`154ac9886b8867890679edc4399b89b0496b4b03`: remove shell interpretation of action
inputs, require plugin integrity checks before execution, and bound downloaded response
bodies. Preserve the existing CLI and action contracts where they are safe to preserve.

## Scope

| Task | Findings | Main files |
|---|---|---|
| 1. Shell-safe action execution | `which_tool`, `git_commit.paths`, keychain timeouts, iOS `xcargs`, Gradle flags | `src/actions/context.rs`, `src/runtime/shell.rs`, `src/actions/core/{shell,git,ci,ios,android}.rs` |
| 2. GitHub Action input handling | Composite action input injection | `action.yml`, action-related tests or workflow checks |
| 3. Plugin lock enforcement | Unlocked plugin execution | `src/plugin/mod.rs`, `src/cli/plugins.rs`, `tests/cli.rs` |
| 4. Bounded downloads | Unbounded HTTP response buffering | `src/actions/core/http.rs`, `src/actions/core/files.rs` |

The service-account `token_uri` candidate was deferred by the scan because no
independent lower-trust control of that field was established. Re-triage it if a
separate untrusted input path is identified; it is not included as a confirmed fix.

## Task 1: Keep built-in action values out of shell source

### Steps

1. Add or use an `ActionContext` process invocation that accepts an executable and
   individual arguments, and returns the same success, output, and error information
   as the existing shell helpers.
2. Convert `which_tool` probes to argument-based invocations. Treat both `name` and
   `version_arg` as individual arguments.
3. Convert `git_commit` staging to argument-based invocation. Preserve the documented
   space-separated paths contract while passing each path separately after `--`.
4. Parse `setup_ci.timeout` and `keychain.timeout` as bounded integer seconds before
   invocation; reject malformed or out-of-range values.
5. Replace free-form `xcargs` and Gradle `flags` shell concatenation with a structured
   argument representation. Preserve supported xcodebuild and Gradle options without
   re-evaluating shell syntax.
6. Add regression tests for arguments containing spaces, quotes, semicolons, command
   substitutions, empty values, and invalid timeout values. Assert the child receives
   literal arguments and no extra command is invoked.

### Acceptance criteria

- The listed actions execute commands through argument boundaries rather than forming
  shell source from their values.
- Timeouts accept valid documented integer values and reject invalid input before any
  process starts.
- Existing safe lane configurations continue to work, including multi-path staging and
  documented Gradle/xcodebuild options.
- Focused unit/integration tests prove metacharacters remain literal.

## Task 2: Pass GitHub Action inputs safely

### Steps

1. Remove direct `${{ inputs.* }}` expansion from the `Run the lane` shell program.
2. Pass lane, params, and extra args through environment variables or another
   non-code input channel, then build a Bash argument array without `eval` or a second
   shell parse.
3. Preserve the documented action input behavior for `key=value` params and extra
   CLI arguments; update the input descriptions if the safe representation changes.
4. Add a regression check for the action definition that rejects direct expression
   interpolation in `run:` and covers shell metacharacters in supplied values.

### Acceptance criteria

- Action inputs are data and cannot alter the shell program.
- Lane, params, and args still reach `shlane run` in their documented positions.
- A test or static action-definition check covers the injection regression without
  requiring a live GitHub Actions run.

## Task 3: Require a lock entry for every executable plugin

### Steps

1. In `verify_against_lockfile`, fail with a clear configuration error when a loaded
   plugin has no checksum entry.
2. Keep `plugin install` usable for fetching, but require `plugin lock` before a normal
   `shlane run` can load that plugin. Make the command output explain this sequence.
3. Update CLI integration coverage so an installed but unlocked plugin is refused, a
   locked plugin runs, and changing a locked plugin is still refused.
4. Update README and plugin docs to describe the enforced lock-before-execution rule.

### Acceptance criteria

- Every configured plugin has a matching checksum before action registration or plugin
  code execution.
- Tests cover missing lock entries, correct lock entries, and changed plugin contents.
- User-facing docs match actual CLI behavior.

## Task 4: Bound HTTP download memory use

### Steps

1. Enforce and document a 512 MiB maximum response size for byte downloads while
   reading the response, before buffering beyond that limit.
2. Return a bounded, non-sensitive error when the response exceeds the limit; ensure
   `Download` does not write a partial output as if the transfer succeeded.
3. Add local HTTP fixture tests for a response below the limit, exactly at the limit,
   and over the limit. Keep the fixture isolated and avoid external network access.
4. Confirm any other callers of `fetch_bytes` share the intended limit or explicitly
   document their separate bound.

### Acceptance criteria

- Response memory is capped by a finite limit.
- Over-limit responses fail cleanly and leave no successful/partial destination file.
- Boundary-size and failure behavior are covered by local tests.

## Integration and completion

1. Run focused tests for each task, then the full Rust test suite, formatting, and
   Clippy checks required by the project.
2. Review the complete diff and docs for compatibility changes, especially the parsing
   of `params`, `args`, `xcargs`, Gradle flags, and path lists.
3. Run a focused `security-audit` review on the final diff covering shell execution,
   plugin integrity, and response-size enforcement.
4. Mark this plan complete only after all acceptance criteria and required validation
   results are recorded.

## Security

- **Trust boundaries:** GitHub Action and lane parameters enter command execution;
  plugin sources and lockfiles control code execution; remote HTTP responses consume
  local memory and become downloaded files.
- **Attack classes and guards:** shell metacharacter injection is guarded by argv-based
  process execution and numeric parsing; plugin substitution is guarded by requiring a
  checksum for every plugin before loading; resource exhaustion is guarded by a finite
  streaming/read limit and atomic failure behavior. Action inputs are passed as data,
  not shell source.
- Run a focused `security-audit` review on the final diff before calling the work done.

## Status

Implementation changes and the focused read-only security review are complete in the
working tree. Formatting and diff checks pass. In an isolated, network-disabled Docker
container, `cargo test --all-targets --locked --offline -- --test-threads=1` passed:
307 unit tests, 156 CLI tests, 1 action test, 5 docs tests, and 5 schema tests. The
download boundary cases use a local loopback HTTP fixture. The zip round-trip used
temporary, read-only shims backed by Python's standard ZIP library because the
container lacks `zip`; native utility behavior remains unverified. Clippy is not
installed in the local Linux toolchain image and cannot be installed offline. Native
macOS and Windows runtime checks also remain unavailable. This plan is not marked
complete.
