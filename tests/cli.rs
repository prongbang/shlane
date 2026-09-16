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
        self.run_in(".", args)
    }

    /// Run with the working directory set to `subdir` inside the sandbox.
    fn run_in(&self, subdir: &str, args: &[&str]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_shlane"))
            .args(args)
            .current_dir(self.path.join(subdir))
            .env_remove("SHLANE_CONFIG")
            .output()
            .expect("shlane binary should be runnable");
        Run::new(output)
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("directory should be creatable");
        }
        fs::write(path, contents).expect("file should be writable");
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

// ---------------------------------------------------------------------------
// M1: schema v1, lane calls, discovery and the new commands
// ---------------------------------------------------------------------------

#[test]
fn config_is_found_from_a_subdirectory() {
    let sandbox = Sandbox::new("lanes:\n  where:\n    steps:\n      - run: pwd\n");
    sandbox.write("deep/nested/keep", "");

    let run = sandbox.run_in("deep/nested", &["run", "where"]);
    run.assert_code(0);
    assert!(
        !run.stdout.contains("deep/nested"),
        "steps should run from the config's directory, not the caller's:\n{}",
        run.stdout
    );
}

#[test]
fn the_file_flag_selects_a_config() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - run: echo from-default\n");
    sandbox.write(
        "other.yaml",
        "lanes:\n  a:\n    steps:\n      - run: echo from-other\n",
    );

    sandbox
        .run(&["--file", "other.yaml", "run", "a"])
        .assert_code(0)
        .assert_stdout_contains("from-other");
}

#[test]
fn list_shows_lanes_with_descriptions_and_params() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  beta:
    description: "ship to testers"
    platform: ios
    params:
      target:
        type: string
        required: true
        values: [staging, production]
  helper:
    private: true
"#,
    );

    sandbox
        .run(&["list"])
        .assert_code(0)
        .assert_stdout_contains("beta")
        .assert_stdout_contains("ship to testers")
        .assert_stdout_contains("ios")
        .assert_stdout_contains("required")
        .assert_stdout_contains("staging, production")
        .assert_stdout_contains("private");
}

#[test]
fn validate_accepts_a_good_config() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - run: \"true\"\n");

    sandbox
        .run(&["validate"])
        .assert_code(0)
        .assert_stdout_contains("is valid");
}

#[test]
fn validate_reports_problems_without_running_anything() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    steps:\n      - run: touch should-not-exist\n      - lane: ghost\n",
    );

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("calls lane 'ghost'");

    assert!(
        !sandbox.path().join("should-not-exist").exists(),
        "validate must not execute steps"
    );
}

#[test]
fn run_refuses_a_config_that_does_not_validate() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - lane: ghost\n");

    sandbox
        .run(&["run", "a"])
        .assert_code(2)
        .assert_stderr_contains("does not exist");
}

#[test]
fn lanes_that_call_each_other_in_a_loop_are_rejected() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    steps:\n      - lane: b\n  b:\n    steps:\n      - lane: a\n",
    );

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("loop");
}

#[test]
fn init_writes_a_config_and_refuses_to_clobber_one() {
    let sandbox = Sandbox::empty();
    sandbox.write("Cargo.toml", "[package]\nname = \"demo\"\n");

    sandbox
        .run(&["init"])
        .assert_code(0)
        .assert_stdout_contains("Rust");

    let written = fs::read_to_string(sandbox.path().join("shlane.yaml"))
        .expect("init should have written a config");
    assert!(written.contains("cargo test"), "got:\n{written}");

    sandbox
        .run(&["init"])
        .assert_code(2)
        .assert_stderr_contains("already exists");

    sandbox.run(&["init", "--force"]).assert_code(0);
}

#[test]
fn a_generated_config_validates_and_runs() {
    let sandbox = Sandbox::empty();

    sandbox.run(&["init"]).assert_code(0);
    sandbox.run(&["validate"]).assert_code(0);
    sandbox
        .run(&["run", "hello"])
        .assert_code(0)
        .assert_stdout_contains("hello world");
}

#[test]
fn private_lanes_cannot_be_started_from_the_command_line() {
    let sandbox = Sandbox::new(
        "lanes:\n  public:\n    steps:\n      - lane: helper\n  helper:\n    private: true\n    steps:\n      - run: echo from-helper\n",
    );

    sandbox
        .run(&["run", "helper"])
        .assert_code(3)
        .assert_stderr_contains("private");

    sandbox
        .run(&["run", "public"])
        .assert_code(0)
        .assert_stdout_contains("from-helper");
}

#[test]
fn a_lane_step_passes_its_own_parameters() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  outer:
    params:
      who:
        default: world
    steps:
      - lane: inner
        with:
          name: ${who}
      - run: echo back-in-outer-${who}
  inner:
    private: true
    params:
      name:
        required: true
    steps:
      - run: echo inner-sees-${name}
"#,
    );

    let run = sandbox.run(&["run", "outer", "who=team"]);
    run.assert_code(0)
        .assert_stdout_contains("inner-sees-team")
        .assert_stdout_contains("back-in-outer-team");
}

#[test]
fn required_parameters_are_enforced() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    params:\n      target:\n        required: true\n    steps:\n      - run: echo ${target}\n",
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(4)
        .assert_stderr_contains("is required");
}

#[test]
fn parameter_values_and_types_are_checked() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    params:
      target:
        values: [staging, production]
      count:
        type: int
        default: 1
    steps:
      - run: echo ${target}-${count}
"#,
    );

    sandbox
        .run(&["run", "a", "target=dev"])
        .assert_code(4)
        .assert_stderr_contains("must be one of");

    sandbox
        .run(&["run", "a", "target=staging", "count=many"])
        .assert_code(4)
        .assert_stderr_contains("expected an integer");

    sandbox
        .run(&["run", "a", "target=staging"])
        .assert_code(0)
        .assert_stdout_contains("staging-1");
}

#[test]
fn conditions_skip_steps() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    params:
      target:
        default: staging
    steps:
      - run: echo production-only
        if: param("target") == "production"
      - run: echo always
"#,
    );

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(0).assert_stdout_contains("always");
    assert!(
        !run.stdout.contains("production-only\n"),
        "the conditional step should have been skipped:\n{}",
        run.stdout
    );
    run.assert_stdout_contains("skipped");

    sandbox
        .run(&["run", "a", "target=production"])
        .assert_code(0)
        .assert_stdout_contains("production-only");
}

#[test]
fn a_bad_condition_fails_the_lane() {
    let sandbox =
        Sandbox::new("lanes:\n  a:\n    steps:\n      - run: \"true\"\n        if: param(\"x\")\n");

    sandbox
        .run(&["run", "a"])
        .assert_code(1)
        .assert_stderr_contains("true or false");
}

#[test]
fn retry_runs_a_step_again() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  flaky:
    steps:
      - run: |
          count=$(cat attempts 2>/dev/null || echo 0)
          count=$((count + 1))
          echo $count > attempts
          test "$count" -ge 3
        retry: 3
      - run: echo attempts-used-$(cat attempts)
"#,
    );

    sandbox
        .run(&["run", "flaky"])
        .assert_code(0)
        .assert_stdout_contains("attempts-used-3");
}

#[test]
fn continue_on_error_keeps_going_but_marks_the_step() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    steps:\n      - run: exit 7\n        continue_on_error: true\n      - run: echo kept-going\n",
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(0)
        .assert_stdout_contains("kept-going")
        .assert_stdout_contains("FAILED")
        .assert_stderr_contains("continue_on_error");
}

#[test]
fn a_step_that_runs_too_long_is_stopped() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    steps:\n      - run: sleep 30\n        timeout: 1s\n        name: slow\n",
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(1)
        .assert_stderr_contains("still running after 1s");
}

#[test]
fn steps_can_set_their_own_directory_and_environment() {
    let sandbox = Sandbox::new(
        r#"
env:
  GREETING: hello
lanes:
  a:
    steps:
      - run: pwd
        workdir: sub
      - run: echo $GREETING $EXTRA
        env:
          EXTRA: ${GREETING}-again
"#,
    );
    sandbox.write("sub/keep", "");

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(0)
        .assert_stdout_contains("sub")
        .assert_stdout_contains("hello hello-again");
}

#[test]
fn dry_run_prints_without_executing() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - run: touch created\n");

    sandbox
        .run(&["run", "a", "--dry-run"])
        .assert_code(0)
        .assert_stdout_contains("Would run: touch created");

    assert!(
        !sandbox.path().join("created").exists(),
        "--dry-run must not execute anything"
    );
}

#[test]
fn global_hooks_run_around_the_lane() {
    let sandbox = Sandbox::new(
        r#"
before_all:
  - echo hook-before-all
after_all:
  - echo hook-after-all
error:
  - echo hook-error
lanes:
  ok:
    steps:
      - run: echo lane-body
  bad:
    steps:
      - run: exit 1
"#,
    );

    let good = sandbox.run(&["run", "ok"]);
    good.assert_code(0)
        .assert_stdout_contains("hook-before-all")
        .assert_stdout_contains("lane-body")
        .assert_stdout_contains("hook-after-all");
    assert!(
        !good.stdout.contains("hook-error"),
        "error hooks must not run on success:\n{}",
        good.stdout
    );

    let bad = sandbox.run(&["run", "bad"]);
    bad.assert_code(1).assert_stdout_contains("hook-error");
    assert!(
        !bad.stdout.contains("hook-after-all"),
        "after_all must not run when the lane failed:\n{}",
        bad.stdout
    );
}

#[test]
fn the_summary_lists_every_step_with_its_result() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    steps:\n      - run: \"true\"\n        name: first\n      - run: exit 2\n        name: second\n",
    );

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(1)
        .assert_stdout_contains("Summary")
        .assert_stdout_contains("first")
        .assert_stdout_contains("second")
        .assert_stdout_contains("FAILED")
        .assert_stdout_contains("total");
}

#[test]
fn namespaced_references_resolve() {
    let sandbox = Sandbox::new(
        "env:\n  STAGE: prod\nlanes:\n  a:\n    params:\n      x:\n        default: one\n    steps:\n      - run: echo ${params.x}-${env.STAGE}-${shlane.lane}\n",
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(0)
        .assert_stdout_contains("one-prod-a");
}

#[test]
fn action_steps_report_that_they_are_not_implemented_yet() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - action: build_ios\n");

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("not implemented yet");
}

#[test]
fn the_dry_run_flag_works_after_the_parameters() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - run: touch created-${x}\n");

    sandbox
        .run(&["run", "a", "x=1", "--dry-run"])
        .assert_code(0)
        .assert_stdout_contains("Would run: touch created-1");

    assert!(
        !sandbox.path().join("created-1").exists(),
        "--dry-run after a parameter must still be read as a flag"
    );
}

#[test]
fn output_survives_a_closed_pipe() {
    use std::process::Stdio;

    let sandbox = Sandbox::new("lanes:\n  noisy:\n    steps:\n      - run: seq 1 5000\n");

    let child = Command::new(env!("CARGO_BIN_EXE_shlane"))
        .args(["run", "noisy"])
        .current_dir(sandbox.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("shlane should start");

    // Dropping the handle closes the read end while shlane is still writing.
    let output = child.wait_with_output().expect("shlane should finish");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "a closed pipe must not panic:\n{stderr}"
    );
}
