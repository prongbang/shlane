# 05 — The Rhai scripting API

There are four builtins today: `param`, `env`, `run` and `print`
(`src/main.rs:167-199`).

## What is wrong with them

| Problem | What it costs |
|---|---|
| `run()` returns only an `i32` (`src/main.rs:180`) | the stdout cannot be used, which is most of why someone writes a script |
| `run()` does not stop on failure | the script carries on after a command has already failed |
| `param()` returns `""` for a name that does not exist (`src/main.rs:171`) | a misspelled parameter goes unnoticed |
| `env()` reads the process environment (`src/main.rs:175`) | tied to the global state that is being removed (see [02](02-architecture.md)) |
| Nothing can be handed back to a later step | a script is a dead end |
| No limits | a runaway loop hangs the CI job |

## The API worth having

### Running commands

```rhai
let r = run("git rev-parse HEAD");   // raises on failure, ending the lane
r.stdout      // String
r.stderr      // String
r.code        // int
r.success     // bool

let r = try_run("which gradle");     // returns the failure instead of raising it
let out = capture("git log -1 --pretty=%s");   // stdout, trimmed
```

### Parameters and environment

```rhai
param("target")              // raises when there is no value and no default
param_or("target", "dev")
has_param("target")
env("APP_ENV")               // from the LaneContext, not the process
set_env("BUILD_NUMBER", n)   // applies to later steps in the same lane
```

### Passing values between steps, in place of fastlane's `lane_context`

```rhai
set_output("ipa_path", "build/MyApp.ipa");
output("build", "ipa")       // what the step with id "build" produced
```

### Calling actions and lanes

```rhai
action("build_ios", #{ scheme: "MyApp", configuration: "Release" });
call_lane("notify", #{ channel: "#releases" });
```

This is the important one: it makes Rhai a complete escape hatch. Whatever YAML cannot
express — a complicated condition, a loop — can be written here and still reach the
same actions.

### UI and logging

```rhai
ui_message("...");  ui_success("...");  ui_error("...");  ui_important("...");
ui_confirm("really deploy?")   // on CI, answers itself rather than waiting
```

`print()` currently overrides Rhai's own (`src/main.rs:196`). Keep it, but route it
through the same logging.

### Utilities

```rhai
file_exists(p); read_file(p); write_file(p, s);
json_parse(s); json_stringify(v); yaml_parse(s);
semver_bump("1.2.3", "minor");    // "1.3.0"
now_iso(); git_sha(); git_branch();
```

## Limits

Set on the engine when it is built (`src/main.rs:87`):

```rust
engine.set_max_operations(10_000_000);
engine.set_max_expr_depths(64, 32);
engine.set_max_string_size(10 * 1024 * 1024);
engine.set_max_array_size(100_000);
engine.disable_symbol("eval");
```

And an overall timeout through `on_progress`.

## Reporting errors

Today a script error is printed and the run continues (`src/main.rs:125-127`). Instead:

- a script error fails the lane, unless the step sets `continue_on_error: true`
- the message carries the line in the script, and maps back to the line in
  `shlane.yaml`

## Later

`script_file: ./scripts/release.rhai`, so a long script can leave the YAML for a file
an editor can highlight.

> **What was actually built (M2, M3, M6):** all of the above except `call_lane()`,
> which would mean re-entering the executor from inside a builtin. A `lane:` step does
> the same thing. `action()` exists, but not inside a Rhai plugin: the registry holds
> the plugin, so it cannot be handed the registry back.
