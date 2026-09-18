# 09 — Plugins

fastlane has over 300 plugins, all of them Ruby gems. shlane will never ship that many
actions itself, so a plugin system is not optional.

## The options

| Shape | How it works | For | Against |
|---|---|---|---|
| **A. An external executable** | a plugin is a binary or script named `shlane-<name>`, spoken to in JSON over stdin/stdout | written in any language, cannot affect the main binary, a crash stays contained | a process per call, and large payloads are awkward |
| **B. A Rhai module** | a plugin is a `.rhai` file loaded from a path or from git | nothing to compile, very easy to write, can be sandboxed | limited to what the builtins expose, and slow |
| **C. WASM** | a plugin is a `.wasm` component | a tight sandbox, portable | the tooling is not ready, and reaching the filesystem or network is painful |
| **D. A Rust crate, recompiled** | the user builds shlane themselves | the fastest | against the "download it and go" goal |

**The proposal: A and B** — A for the heavy work, B for glue code — and revisit C after
1.0.

## The external plugin protocol (option A)

```
shlane → plugin (stdin, one JSON object per line)
{
  "protocol": 1,
  "op": "run",                   // "describe" | "run" | "dry_run"
  "action": "notify_line",
  "args": { "token": "***", "message": "hi" },
  "context": { "lane": "beta", "workdir": "/repo", "dry_run": false }
}

plugin → shlane (stdout, one JSON event per line)
{"type":"log","level":"info","message":"sending..."}
{"type":"secret","value":"xxx"}                 // please mask this value in the log
{"type":"result","ok":true,"outputs":{"id":"123"}}
```

- `op: "describe"` returns the action's schema, which `shlane validate` and
  `shlane action show` use
- a non-zero exit code, or no `result` event, is a failure
- the plugin's stderr is relayed as warn-level log lines

## The manifest

```yaml
# shlane-plugin.yaml
name: line-notify
version: 0.1.0
protocol: 1
executable: ./bin/shlane-line-notify
platforms: [macos, linux]
actions:
  - notify_line
```

## Declaring one in shlane.yaml

```yaml
plugins:
  - name: line-notify
    source: github:someone/shlane-line-notify@v0.1.0
  - name: internal-tools
    source: path:./tools/shlane-plugins/internal
```

## The commands

```
shlane plugin add github:someone/shlane-line-notify@v0.1.0
shlane plugin list
shlane plugin remove line-notify
shlane plugin verify          # checksum and protocol version
```

- installed into `.shlane/plugins/` in the project, with a `shlane-plugins.lock`
  carrying the checksums, committed
- **the lockfile has to carry a SHA-256.** A plugin is code that runs with full
  privileges on the CI machine holding the app's signing key; resolving a floating
  version is a supply-chain hole.

## Status — M6, done

- external-executable plugins (option A), with all of protocol v1
- `path:` and `source:` (`github:owner/repo@tag`, a git URL, ssh)
- `shlane plugin add / remove / install / list / lock / verify`
- SHA-256 in the lockfile — a tag that has been moved is rejected, not installed over

- Rhai module plugins (option B) — the manifest carries `script:` instead of
  `executable:`, one action per function. They get the same builtins as a lane's script,
  except `action()`: the registry holds the plugin, so it cannot be handed the registry
  back.

## Security requirements

1. `shlane run` never installs a plugin — `plugin add` has to be run deliberately.
2. The lockfile has to carry checksums, and they are checked before every run.
3. `shlane run` prints the plugins it loaded as it starts, so they appear in CI's audit
   log.
4. A plugin does not receive the context's secrets — only the values named in that
   step's `with:`.
