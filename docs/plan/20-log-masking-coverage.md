# 20 — Complete sensitive-value masking in logs

## Objective

Ensure values registered as secrets are masked on every shlane-controlled output path. Preserve ordinary output and the existing `***` replacement.

## Current behavior and scope

- `src/runtime/secrets.rs` stores registered values and masks raw and URL-encoded text.
- `src/runtime/ui.rs` masks human-readable messages and JSON event fields through the shared registry.
- `src/runtime/shell.rs` masks child stdout and stderr before display and before retaining output.
- `src/plugin/action.rs` routes plugin log messages through `Ui`, but processes events in order. A plugin that emits a log before declaring that value with a later `secret` event cannot have that earlier log redacted.
- Existing coverage is split across `src/runtime/secrets.rs` unit tests and `tests/cli.rs` integration tests; `tests/platform-checks.sh` has a basic runtime secret case.

In scope: supported transformed representations of registered values; human and JSON output; child output and errors; plugin logs and plugin event ordering; regression coverage. Do not add a logging framework or attempt heuristic detection of unregistered values.

## Steps

1. Keep masking limited to registered values and the existing URL-encoded representation.
2. Make plugin logging safe when a plugin declares a secret anywhere in its event stream: register secret events before rendering log/result messages, or establish an equally small behavior that guarantees no prior buffered plugin message is printed unmasked.
3. Add focused tests for each transformed form, overlapping values, and end-to-end leaks across normal output, stderr/errors, JSON, child output, and plugin events. Include a control proving ordinary values remain visible.
4. Update plan 10 or its checklist only where needed so its documented masking guarantees match the implemented behavior.
5. Run focused unit and CLI tests, then run a read-only `security-audit` review of the diff for secret disclosure paths.

## Acceptance criteria

- A registered secret and its URL-encoded form are replaced before any shlane-controlled output is emitted or retained.
- Plugin log/result events are not exposed before secrets declared in the same response have been registered.
- Human output, `--json`, child stdout/stderr, and error messages have regression coverage for leakage.
- Non-sensitive output remains unchanged, and no new heuristic masks arbitrary values.
- Focused tests and the security review pass; any remaining limitation is documented next to the behavior it constrains.

## Security

- **Trust boundaries:** environment/config/action inputs, plugin process output, and child-process stdout/stderr flow into CI-visible terminal or JSON output. Applicable risks are accidental credential disclosure, transformed-secret bypass, and plugin event ordering. Keep registration based on existing explicit/sensitive-name sources, mask at the shared output boundary, and buffer plugin events until secret declarations are registered.

## Execution status

The masker handles raw and URL-encoded values, and plugin secret events are registered before plugin output is displayed. Base64 values are not implicitly masked; only the raw registered value and URL-encoded form are.
