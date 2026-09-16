//! End-to-end tests that run the real binary against real config files.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A scratch directory that cleans itself up.
struct Sandbox {
    path: PathBuf,
}

impl Sandbox {
    fn new(config: &str) -> Self {
        let unique = format!(
            "shlane-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("sandbox should be creatable");
        fs::write(path.join("shlane.yaml"), config).expect("config should be writable");
        Self { path }
    }

    /// A sandbox with no config file in it.
    fn empty() -> Self {
        let unique = format!(
            "shlane-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("sandbox should be creatable");
        Self { path }
    }

    fn run(&self, args: &[&str]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_shlane"))
            .args(args)
            .current_dir(&self.path)
            .output()
            .expect("shlane binary should be runnable");
        Run::new(output)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn new(output: Output) -> Self {
        Self {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    fn assert_code(&self, expected: i32) -> &Self {
        assert_eq!(
            self.code, expected,
            "expected exit code {expected}, got {}\nstdout:\n{}\nstderr:\n{}",
            self.code, self.stdout, self.stderr
        );
        self
    }

    fn assert_stdout_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stdout.contains(needle),
            "stdout did not contain {needle:?}\nstdout:\n{}",
            self.stdout
        );
        self
    }

    fn assert_stderr_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stderr.contains(needle),
            "stderr did not contain {needle:?}\nstderr:\n{}",
            self.stderr
        );
        self
    }
}

#[test]
fn runs_hooks_steps_and_script_in_order() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  build:
    before:
      - echo phase-before
    steps:
      - run: echo phase-step
    script: |
      print("phase-script");
    after:
      - echo phase-after
"#,
    );

    let run = sandbox.run(&["run", "build"]);
    run.assert_code(0)
        .assert_stdout_contains("Lane 'build' completed successfully!");

    let order: Vec<&str> = ["phase-before", "phase-step", "phase-script", "phase-after"]
        .into_iter()
        .collect();
    let mut last = 0;
    for phase in order {
        let at = run
            .stdout
            .find(phase)
            .unwrap_or_else(|| panic!("{phase} missing from output:\n{}", run.stdout));
        assert!(at >= last, "{phase} ran out of order:\n{}", run.stdout);
        last = at;
    }
}

#[test]
fn unknown_lane_exits_three_and_lists_lanes_alphabetically() {
    let sandbox = Sandbox::new("lanes:\n  zeta: {}\n  alpha: {}\n");

    let run = sandbox.run(&["run", "nope"]);
    run.assert_code(3)
        .assert_stderr_contains("lane 'nope' not found")
        .assert_stderr_contains("- alpha")
        .assert_stderr_contains("- zeta");

    let alpha = run.stderr.find("- alpha").unwrap_or_default();
    let zeta = run.stderr.find("- zeta").unwrap_or_default();
    assert!(
        alpha < zeta,
        "lanes should be listed in order:\n{}",
        run.stderr
    );
}

#[test]
fn missing_config_exits_three_with_a_hint() {
    let sandbox = Sandbox::empty();

    sandbox
        .run(&["run", "build"])
        .assert_code(3)
        .assert_stderr_contains("no config file found")
        .assert_stderr_contains("hint:");
}

#[test]
fn invalid_yaml_exits_two_and_points_at_the_line() {
    let sandbox = Sandbox::new("lanes:\n  build:\n   - not a map\n");

    sandbox
        .run(&["run", "build"])
        .assert_code(2)
        .assert_stderr_contains("invalid config at");
}

#[test]
fn a_misspelled_key_is_rejected_instead_of_ignored() {
    let sandbox = Sandbox::new("lanes:\n  build:\n    stepz:\n      - run: \"true\"\n");

    sandbox
        .run(&["run", "build"])
        .assert_code(2)
        .assert_stderr_contains("stepz");
}

#[test]
fn a_failing_step_exits_one_and_names_the_step() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  build:
    steps:
      - run: "true"
      - run: exit 3
    after:
      - echo should-not-run
"#,
    );

    let run = sandbox.run(&["run", "build"]);
    run.assert_code(1)
        .assert_stderr_contains("lane 'build': step #2 failed with exit code 3")
        .assert_stderr_contains("command: exit 3");

    assert!(
        !run.stdout.contains("should-not-run"),
        "after hooks should not run once a step has failed:\n{}",
        run.stdout
    );
    assert!(
        !run.stdout.contains("completed successfully"),
        "a failed lane must not report success:\n{}",
        run.stdout
    );
}

#[test]
fn parameters_are_substituted_into_commands() {
    let sandbox = Sandbox::new("lanes:\n  greet:\n    steps:\n      - run: echo hello-${name}\n");

    sandbox
        .run(&["run", "greet", "name=world"])
        .assert_code(0)
        .assert_stdout_contains("hello-world");
}

#[test]
fn parameter_values_cannot_inject_shell_commands() {
    let sandbox = Sandbox::new("lanes:\n  greet:\n    steps:\n      - run: echo ${name}\n");
    let marker = sandbox.path().join("pwned");

    let run = sandbox.run(&[
        "run",
        "greet",
        &format!("name=x; touch {}", marker.display()),
    ]);

    run.assert_code(0);
    assert!(
        !marker.exists(),
        "an injected command was executed:\n{}",
        run.stdout
    );
    run.assert_stdout_contains("x; touch");
}

#[test]
fn undefined_variables_are_an_error_not_a_literal() {
    let sandbox = Sandbox::new("lanes:\n  greet:\n    steps:\n      - run: echo ${missing}\n");

    sandbox
        .run(&["run", "greet"])
        .assert_code(2)
        .assert_stderr_contains("undefined variable '${missing}'");
}

#[test]
fn config_env_reaches_commands() {
    let sandbox = Sandbox::new(
        "env:\n  APP_ENV: production\nlanes:\n  show:\n    steps:\n      - run: echo env-is-$APP_ENV\n",
    );

    sandbox
        .run(&["run", "show"])
        .assert_code(0)
        .assert_stdout_contains("env-is-production");
}

#[test]
fn a_failing_script_fails_the_lane() {
    let sandbox =
        Sandbox::new("lanes:\n  broken:\n    script: |\n      this is not valid rhai(((\n");

    let run = sandbox.run(&["run", "broken"]);
    run.assert_code(1).assert_stderr_contains("script failed");
    assert!(
        !run.stdout.contains("completed successfully"),
        "a lane whose script failed must not report success:\n{}",
        run.stdout
    );
}

#[test]
fn scripts_can_read_params_and_env() {
    let sandbox = Sandbox::new(
        r#"
env:
  APP_ENV: production
lanes:
  show:
    script: |
      print("target=" + param("target"));
      print("app_env=" + env("APP_ENV"));
"#,
    );

    sandbox
        .run(&["run", "show", "target=staging"])
        .assert_code(0)
        .assert_stdout_contains("target=staging")
        .assert_stdout_contains("app_env=production");
}

#[test]
fn a_lane_with_nothing_in_it_succeeds() {
    let sandbox = Sandbox::new("lanes:\n  noop: {}\n");

    sandbox
        .run(&["run", "noop"])
        .assert_code(0)
        .assert_stdout_contains("Lane 'noop' completed successfully!");
}

#[test]
fn a_script_ending_in_an_expression_succeeds() {
    let sandbox = Sandbox::new("lanes:\n  show:\n    script: |\n      run(\"echo from-script\")\n");

    sandbox
        .run(&["run", "show"])
        .assert_code(0)
        .assert_stdout_contains("from-script")
        .assert_stdout_contains("completed successfully");
}

#[test]
fn lane_scripts_can_call_shared_functions() {
    let sandbox = Sandbox::new(
        r#"
script: |-
  fn greet(name) {
      print("hello " + name);
  }
lanes:
  show:
    script: |
      greet("world");
"#,
    );

    sandbox
        .run(&["run", "show"])
        .assert_code(0)
        .assert_stdout_contains("hello world")
        .assert_stdout_contains("completed successfully");
}

#[test]
fn shared_top_level_statements_run_exactly_once() {
    let sandbox = Sandbox::new(
        r#"
script: |-
  print("loaded-shared");
  fn noop() {}
lanes:
  show:
    script: |
      noop();
      print("lane-ran");
"#,
    );

    let run = sandbox.run(&["run", "show"]);
    run.assert_code(0).assert_stdout_contains("lane-ran");
    assert_eq!(
        run.stdout.matches("loaded-shared").count(),
        1,
        "the shared script's statements ran more than once:\n{}",
        run.stdout
    );
}
