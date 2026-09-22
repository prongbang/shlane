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
        let mut command = Command::new(env!("CARGO_BIN_EXE_shlane"));
        command.args(args).current_dir(self.path.join(subdir));
        clear_ambient(&mut command);
        Run::new(command.output().expect("shlane binary should be runnable"))
    }

    /// Run with extra environment variables, for the actions that behave
    /// differently on CI.
    fn run_with_env(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = Command::new(env!("CARGO_BIN_EXE_shlane"));
        command.args(args).current_dir(&self.path);
        clear_ambient(&mut command);
        for (key, value) in env {
            command.env(key, value);
        }
        Run::new(command.output().expect("shlane binary should be runnable"))
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

/// Every variable shlane looks at to decide it is on CI, from
/// `src/runtime/ci.rs`.
const CI_SIGNATURES: &[&str] = &[
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "BITRISE_IO",
    "CIRCLECI",
    "JENKINS_URL",
    "BUILDKITE",
    "TRAVIS",
    "TEAMCITY_VERSION",
    "TF_BUILD",
];

/// Take the environment the test runner happens to be in out of the picture.
///
/// These tests run on CI, so without this the runner's own `GITHUB_ACTIONS`
/// reaches the shlane under test: a test asserting that nothing is annotated
/// off CI fails, and one that sets `BUILDKITE` gets `github` back because the
/// real variable wins. What the sandbox sees has to come from the test alone.
fn clear_ambient(command: &mut Command) {
    command.env_remove("SHLANE_CONFIG");
    command.env_remove("SHLANE_SHELL");
    for name in CI_SIGNATURES {
        command.env_remove(name);
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
fn an_unknown_action_is_reported_by_validate() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - action: teleport_app\n");

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("does not exist");
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

// ---------------------------------------------------------------------------
// M2: environment, secrets, step outputs, the script API and output modes
// ---------------------------------------------------------------------------

#[test]
fn env_files_are_loaded() {
    let sandbox = Sandbox::new(
        "env_files: [.env]\nlanes:\n  a:\n    steps:\n      - run: echo from-file-$FROM_FILE\n",
    );
    sandbox.write(".env", "FROM_FILE=yes\n");

    sandbox
        .run(&["run", "a"])
        .assert_code(0)
        .assert_stdout_contains("from-file-yes");
}

#[test]
fn a_profile_selects_an_env_file() {
    let sandbox = Sandbox::new(
        "env_files: [.env, \".env.${SHLANE_PROFILE}\"]\nlanes:\n  a:\n    steps:\n      - run: echo stage-$STAGE\n",
    );
    sandbox.write(".env", "STAGE=base\n");
    sandbox.write(".env.ci", "STAGE=ci\n");

    sandbox
        .run(&["run", "a"])
        .assert_code(0)
        .assert_stdout_contains("stage-base");

    sandbox
        .run(&["--env", "ci", "run", "a"])
        .assert_code(0)
        .assert_stdout_contains("stage-ci");
}

#[test]
fn a_missing_env_file_is_not_an_error() {
    let sandbox = Sandbox::new(
        "env_files: [.env, \".env.${SHLANE_PROFILE}\"]\nlanes:\n  a:\n    steps:\n      - run: echo fine\n",
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(0)
        .assert_stdout_contains("fine");
}

#[test]
fn lane_and_step_env_override_the_config() {
    let sandbox = Sandbox::new(
        r#"
env:
  STAGE: config
lanes:
  a:
    env:
      STAGE: lane
    steps:
      - run: echo saw-$STAGE
      - run: echo saw-$STAGE
        env:
          STAGE: step
"#,
    );

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(0)
        .assert_stdout_contains("saw-lane")
        .assert_stdout_contains("saw-step");
}

#[test]
fn secrets_are_masked_everywhere() {
    let sandbox = Sandbox::new(
        r#"
env:
  API_TOKEN: supersecret123
  APP_ENV: production
lanes:
  a:
    steps:
      - run: echo using ${API_TOKEN}
      - run: echo stage is $APP_ENV
      - run: echo $API_TOKEN >&2
      - run: exit 1
        name: fails with ${API_TOKEN}
"#,
    );

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(1);

    let everything = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !everything.contains("supersecret123"),
        "the secret leaked:\n{everything}"
    );
    assert!(
        everything.contains("***"),
        "nothing was masked:\n{everything}"
    );
    // A value whose name is not sensitive is left alone.
    assert!(
        everything.contains("production"),
        "ordinary values should not be masked:\n{everything}"
    );
}

#[test]
fn values_listed_under_secrets_are_masked() {
    let sandbox = Sandbox::new(
        r#"
env:
  LICENCE: plain-looking-value
secrets:
  - ${env.LICENCE}
lanes:
  a:
    steps:
      - run: echo ${LICENCE}
"#,
    );

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(0);
    assert!(
        !run.stdout.contains("plain-looking-value"),
        "an explicitly declared secret leaked:\n{}",
        run.stdout
    );
}

#[test]
fn step_outputs_feed_later_steps() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - id: version
        run: echo 1.4.2
      - run: echo building ${steps.version.stdout}
      - run: echo exit-was ${steps.version.code}
"#,
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(0)
        .assert_stdout_contains("building 1.4.2")
        .assert_stdout_contains("exit-was 0");
}

#[test]
fn scripts_can_capture_command_output() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    script: |
      let version = capture("echo 9.9.9");
      print("captured=" + version);
      let result = try_run("exit 4");
      print("code=" + result.code + " success=" + result.success);
      set_output("built", "yes");
      print("output=" + output("a", "built"));
"#,
    );

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(0)
        .assert_stdout_contains("captured=9.9.9")
        .assert_stdout_contains("code=4 success=false")
        .assert_stdout_contains("output=yes");
}

#[test]
fn a_failing_command_in_a_script_stops_the_lane() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    script: |\n      run(\"exit 5\");\n      print(\"must not reach here\");\n",
    );

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(1)
        .assert_stderr_contains("command failed with exit code 5");
    assert!(
        !run.stdout.contains("must not reach here"),
        "the script kept going after a failed command:\n{}",
        run.stdout
    );
}

#[test]
fn scripts_can_set_environment_for_later_steps() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    script: |
      set_env("BUILD_ID", "42");
    after:
      - echo build-$BUILD_ID
"#,
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(0)
        .assert_stdout_contains("build-42");
}

#[test]
fn scripts_can_declare_a_secret_at_runtime() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    script: |
      secret("rotating-value");
      print("leaked? rotating-value");
"#,
    );

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(0);
    assert!(
        !run.stdout.contains("rotating-value"),
        "a runtime secret leaked:\n{}",
        run.stdout
    );
}

#[test]
fn json_mode_emits_one_event_per_line() {
    let sandbox =
        Sandbox::new("lanes:\n  a:\n    steps:\n      - run: echo hi\n        name: greet\n");

    let run = sandbox.run(&["--json", "run", "a"]);
    run.assert_code(0);

    let events: Vec<&str> = run
        .stdout
        .lines()
        .filter(|line| line.starts_with('{'))
        .collect();
    assert!(
        events.iter().any(|line| line.contains("\"lane_started\"")),
        "no lane_started event:\n{}",
        run.stdout
    );
    assert!(
        events
            .iter()
            .any(|line| line.contains("\"step_finished\"") && line.contains("greet")),
        "no step_finished event:\n{}",
        run.stdout
    );
    assert!(
        !run.stdout.contains("Running:") && !run.stdout.contains("Summary"),
        "json mode should not print human output:\n{}",
        run.stdout
    );
}

#[test]
fn quiet_hides_progress_but_keeps_the_command_output() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - run: echo the-output\n");

    let run = sandbox.run(&["--quiet", "run", "a"]);
    run.assert_code(0).assert_stdout_contains("the-output");
    assert!(
        !run.stdout.contains("Running:") && !run.stdout.contains("Summary"),
        "--quiet should hide shlane's own chatter:\n{}",
        run.stdout
    );
}

#[test]
fn verbose_shows_where_a_step_runs() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - run: echo hi\n");

    sandbox
        .run(&["--verbose", "run", "a"])
        .assert_code(0)
        .assert_stdout_contains("in ");
}

#[cfg(unix)]
#[test]
fn ctrl_c_stops_the_run_and_reports_130() {
    use std::thread;
    use std::time::Duration;

    let sandbox = Sandbox::new(
        r#"
error:
  - echo error-hook-ran
lanes:
  slow:
    steps:
      - run: sleep 30
        name: slow step
"#,
    );

    let child = Command::new(env!("CARGO_BIN_EXE_shlane"))
        .args(["run", "slow"])
        .current_dir(sandbox.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("shlane should start");

    // Give it long enough to have spawned the step.
    thread::sleep(Duration::from_millis(400));
    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("kill should run");
    assert!(killed.success(), "could not signal shlane");

    let output = child.wait_with_output().expect("shlane should exit");
    let run = Run::new(output);

    run.assert_code(130)
        .assert_stderr_contains("interrupted")
        .assert_stdout_contains("error-hook-ran");
}

// ---------------------------------------------------------------------------
// M3: actions
// ---------------------------------------------------------------------------

/// Turn a sandbox into a git repository with one commit.
fn init_repo(sandbox: &Sandbox) {
    for args in [
        vec!["init", "-q", "."],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "shlane test"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(sandbox.path())
            .output()
            .expect("git should run");
        assert!(status.status.success(), "git {args:?} failed");
    }
}

fn git(sandbox: &Sandbox, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(sandbox.path())
        .output()
        .expect("git should run");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn action_list_and_show_describe_the_built_ins() {
    let sandbox = Sandbox::new("lanes: {}\n");

    sandbox
        .run(&["action", "list"])
        .assert_code(0)
        .assert_stdout_contains("git_commit")
        .assert_stdout_contains("bump_version");

    sandbox
        .run(&["action", "show", "git_push"])
        .assert_code(0)
        .assert_stdout_contains("remote")
        .assert_stdout_contains("default: origin");

    sandbox
        .run(&["action", "show", "nope"])
        .assert_code(1)
        .assert_stderr_contains("no such action");
}

#[test]
fn the_sh_action_reports_what_the_command_printed() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - id: greet
        action: sh
        with:
          command: echo hello-from-action
      - run: echo saw ${steps.greet.stdout}
"#,
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(0)
        .assert_stdout_contains("saw hello-from-action");
}

#[test]
fn ensure_env_vars_fails_before_the_work_starts() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - action: ensure_env_vars
        with:
          names: DEFINITELY_NOT_SET_XYZ, ALSO_NOT_SET_XYZ
      - run: touch should-not-exist
"#,
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(1)
        .assert_stderr_contains("DEFINITELY_NOT_SET_XYZ");

    assert!(
        !sandbox.path().join("should-not-exist").exists(),
        "later steps should not have run"
    );
}

#[test]
fn version_actions_read_and_bump() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  release:
    steps:
      - id: before
        action: read_version
      - id: bumped
        action: bump_version
        with:
          part: minor
      - run: echo ${steps.before.version} became ${steps.bumped.version}
"#,
    );
    sandbox.write(
        "Cargo.toml",
        "[package]\nname = \"demo\"\nversion = \"1.4.2\"\n",
    );

    sandbox
        .run(&["run", "release"])
        .assert_code(0)
        .assert_stdout_contains("1.4.2 became 1.5.0");

    let written = fs::read_to_string(sandbox.path().join("Cargo.toml")).expect("still readable");
    assert!(written.contains("version = \"1.5.0\""), "got:\n{written}");
}

#[test]
fn bump_version_leaves_the_file_alone_on_a_dry_run() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - action: bump_version\n");
    sandbox.write("VERSION", "2.0.0\n");

    sandbox
        .run(&["run", "a", "--dry-run"])
        .assert_code(0)
        .assert_stdout_contains("Would bump 2.0.0 to 2.0.1");

    let written = fs::read_to_string(sandbox.path().join("VERSION")).expect("still readable");
    assert_eq!(written.trim(), "2.0.0");
}

#[test]
fn the_release_lane_the_plan_asked_for_works_without_any_shell() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  release:
    description: "bump, commit, tag"
    steps:
      - action: git_status_clean
      - id: bumped
        action: bump_version
        with:
          part: patch
      - action: git_commit
        with:
          message: "release: ${steps.bumped.version}"
      - action: git_tag
        with:
          name: "v${steps.bumped.version}"
          message: "release ${steps.bumped.version}"
      - id: branch
        action: git_branch
"#,
    );
    sandbox.write("VERSION", "0.9.9\n");
    init_repo(&sandbox);
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(sandbox.path())
        .status()
        .expect("git add")
        .success());
    assert!(Command::new("git")
        .args(["commit", "-qm", "initial"])
        .current_dir(sandbox.path())
        .status()
        .expect("git commit")
        .success());

    let run = sandbox.run(&["run", "release"]);
    run.assert_code(0);

    assert_eq!(
        fs::read_to_string(sandbox.path().join("VERSION"))
            .expect("readable")
            .trim(),
        "0.9.10"
    );
    assert_eq!(git(&sandbox, &["tag", "--list"]), "v0.9.10");
    assert_eq!(
        git(&sandbox, &["log", "-1", "--pretty=%s"]),
        "release: 0.9.10"
    );
}

#[test]
fn git_status_clean_fails_on_a_dirty_tree() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - action: git_status_clean\n");
    init_repo(&sandbox);
    sandbox.write("untracked.txt", "hello\n");

    sandbox
        .run(&["run", "a"])
        .assert_code(1)
        .assert_stderr_contains("uncommitted changes");
}

#[test]
fn changelog_and_last_tag_cope_with_a_repository_without_tags() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  notes:
    steps:
      - id: tag
        action: last_git_tag
      - id: log
        action: changelog_from_commits
      - run: echo found=${steps.tag.found} count=${steps.log.count}
"#,
    );
    init_repo(&sandbox);
    sandbox.write("a.txt", "one\n");
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(sandbox.path())
        .status()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-qm", "first change"])
        .current_dir(sandbox.path())
        .status()
        .expect("git commit");

    sandbox
        .run(&["run", "notes"])
        .assert_code(0)
        .assert_stdout_contains("found=false count=1");
}

#[test]
fn scripts_can_call_actions() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    script: |
      let result = action("sh", #{ command: "echo from-script-action" });
      print("got=" + result.stdout);
      let bad = action("bump_version", #{ part: "sideways" });
"#,
    );
    sandbox.write("VERSION", "1.0.0\n");

    let run = sandbox.run(&["run", "a"]);
    run.assert_code(1)
        .assert_stdout_contains("got=from-script-action");
    run.assert_stderr_contains("unknown part");
}

#[test]
fn a_script_calling_an_unknown_action_says_so() {
    let sandbox =
        Sandbox::new("lanes:\n  a:\n    script: |\n      action(\"teleport_app\", #{});\n");

    sandbox
        .run(&["run", "a"])
        .assert_code(1)
        .assert_stderr_contains("no such action");
}

#[test]
fn a_slack_webhook_never_reaches_the_output() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - action: notify_slack
        with:
          webhook: https://hooks.example.invalid/services/SECRET-PATH-12345
          text: hello
"#,
    );

    let run = sandbox.run(&["run", "a"]);
    // The host does not resolve; what matters is that the URL is not printed.
    let everything = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !everything.contains("SECRET-PATH-12345"),
        "the webhook leaked:\n{everything}"
    );
}

#[test]
fn discord_and_teams_webhooks_never_reach_the_output() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - action: notify_discord
        continue_on_error: true
        with:
          webhook: https://discord.example.invalid/api/webhooks/DISCORD-SECRET-1
          text: hello
      - action: notify_teams
        with:
          webhook: https://teams.example.invalid/workflows/TEAMS-SECRET-2
          title: Release
          text: hello
"#,
    );

    let run = sandbox.run(&["run", "a"]);
    let everything = format!("{}{}", run.stdout, run.stderr);
    for secret in ["DISCORD-SECRET-1", "TEAMS-SECRET-2"] {
        assert!(
            !everything.contains(secret),
            "{secret} leaked:\n{everything}"
        );
    }
}

#[test]
fn http_request_arguments_are_validated_before_running() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    steps:\n      - action: http_request\n        with:\n          urll: http://example.com\n",
    );

    let run = sandbox.run(&["validate"]);
    run.assert_code(2)
        .assert_stderr_contains("needs 'url'")
        .assert_stderr_contains("no argument 'urll'");
}

// ---------------------------------------------------------------------------
// M4: Android actions and reports
// ---------------------------------------------------------------------------

#[test]
fn gradle_actions_are_registered_and_documented() {
    let sandbox = Sandbox::new("lanes: {}\n");

    sandbox
        .run(&["action", "list"])
        .assert_code(0)
        .assert_stdout_contains("build_android")
        .assert_stdout_contains("sign_android")
        .assert_stdout_contains("test_android");

    sandbox
        .run(&["action", "show", "build_android"])
        .assert_code(0)
        .assert_stdout_contains("aab or apk")
        .assert_stdout_contains("default: release");
}

#[test]
fn build_android_runs_the_right_gradle_task() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  build:
    steps:
      - action: build_android
        with:
          format: apk
          flavor: prod
          build_type: release
"#,
    );
    // A stand-in wrapper: it records how it was called and produces an APK
    // where gradle would.
    sandbox.write(
        "gradlew",
        "#!/bin/sh\necho \"called with: $@\"\nmkdir -p build/outputs/apk/prod/release\necho apk > build/outputs/apk/prod/release/app-prod-release.apk\n",
    );
    let status = Command::new("chmod")
        .args(["+x", "gradlew"])
        .current_dir(sandbox.path())
        .status()
        .expect("chmod should run");
    assert!(status.success());

    sandbox
        .run(&["run", "build"])
        .assert_code(0)
        .assert_stdout_contains("called with: assembleProdRelease")
        .assert_stdout_contains("app-prod-release.apk");
}

#[test]
fn build_android_says_so_when_no_artifact_appears() {
    let sandbox = Sandbox::new(
        "lanes:\n  build:\n    steps:\n      - action: build_android\n        with:\n          format: aab\n",
    );
    sandbox.write("gradlew", "#!/bin/sh\nexit 0\n");
    Command::new("chmod")
        .args(["+x", "gradlew"])
        .current_dir(sandbox.path())
        .status()
        .expect("chmod should run");

    sandbox
        .run(&["run", "build"])
        .assert_code(1)
        .assert_stderr_contains("no .aab was found");
}

#[test]
fn gradle_keeps_sensitive_properties_off_the_command_line() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  publish:
    steps:
      - action: gradle
        with:
          task: publish
          properties: |
            SIGNING_PASSWORD=hunter2000
            flavor=prod
"#,
    );
    sandbox.write(
        "gradlew",
        "#!/bin/sh\necho \"args: $@\"\necho \"env: $ORG_GRADLE_PROJECT_SIGNING_PASSWORD\"\n",
    );
    Command::new("chmod")
        .args(["+x", "gradlew"])
        .current_dir(sandbox.path())
        .status()
        .expect("chmod should run");

    let run = sandbox.run(&["run", "publish"]);
    // The shell removes the quoting before gradle sees the argument.
    run.assert_code(0).assert_stdout_contains("-Pflavor=prod");

    // It reached gradle through the environment, and is masked on the way back.
    assert!(
        !run.stdout.contains("hunter2000"),
        "the password leaked:\n{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("env: ***"),
        "the password did not reach gradle:\n{}",
        run.stdout
    );
}

#[cfg(unix)]
#[test]
fn sign_android_removes_the_aligned_intermediate() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  sign:
    steps:
      - action: sign_android
        with:
          input: app-release-unsigned.apk
          output: app-release.apk
          keystore: release.jks
          keystore_password: pw
          key_alias: release
"#,
    );
    // Stand-ins for the build tools: zipalign copies, apksigner writes --out.
    sandbox.write(
        "sdk/build-tools/35.0.0/zipalign",
        "#!/bin/sh\ncp \"$4\" \"$5\"\n",
    );
    sandbox.write(
        "sdk/build-tools/35.0.0/apksigner",
        "#!/bin/sh\nwhile [ \"$1\" != --out ]; do shift; done\ncp \"$3\" \"$2\"\n",
    );
    for tool in ["zipalign", "apksigner"] {
        use std::os::unix::fs::PermissionsExt as _;
        let path = sandbox.path().join("sdk/build-tools/35.0.0").join(tool);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    sandbox.write("app-release-unsigned.apk", "apk");
    sandbox.write("release.jks", "keystore");
    // Through the process environment, which outranks a config's `env:`: a CI
    // runner with the Android SDK already has ANDROID_HOME set.
    let sdk = sandbox.path().join("sdk");
    sandbox
        .run_with_env(
            &["run", "sign"],
            &[("ANDROID_HOME", &sdk.display().to_string())],
        )
        .assert_code(0);
    assert!(sandbox.path().join("app-release.apk").is_file());
    assert!(
        !sandbox
            .path()
            .join("app-release-unsigned.aligned.apk")
            .exists(),
        "the zipaligned intermediate was left behind"
    );
}

#[test]
fn sign_android_needs_a_keystore() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  sign:
    steps:
      - action: sign_android
        with:
          input: app.apk
          keystore_password: secret123
          key_alias: upload
"#,
    );

    sandbox
        .run(&["run", "sign"])
        .assert_code(1)
        .assert_stderr_contains("keystore or keystore_base64");
}

#[test]
fn a_junit_report_is_written_for_a_failed_run() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - run: "true"
        name: first
      - run: exit 1
        name: second
      - run: "true"
        name: never
"#,
    );

    sandbox
        .run(&["run", "a", "--report", "junit:reports/shlane.xml"])
        .assert_code(1);

    let xml = fs::read_to_string(sandbox.path().join("reports/shlane.xml"))
        .expect("the report should exist even though the lane failed");
    assert!(xml.contains("tests=\"2\""), "{xml}");
    assert!(xml.contains("failures=\"1\""), "{xml}");
    assert!(xml.contains("name=\"second\""), "{xml}");
    assert!(xml.contains("<failure"), "{xml}");
}

#[test]
fn json_and_markdown_reports_are_written() {
    let sandbox =
        Sandbox::new("lanes:\n  a:\n    steps:\n      - run: \"true\"\n        name: only\n");

    sandbox
        .run(&[
            "run",
            "a",
            "--report",
            "json:out.json",
            "--report",
            "md:out.md",
        ])
        .assert_code(0);

    let json = fs::read_to_string(sandbox.path().join("out.json")).expect("json written");
    assert!(json.contains("\"result\": \"ok\""), "{json}");
    assert!(json.contains("\"step\": \"only\""), "{json}");

    let md = fs::read_to_string(sandbox.path().join("out.md")).expect("markdown written");
    assert!(md.contains("| only |"), "{md}");
}

#[test]
fn a_bad_report_specification_is_rejected() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - run: \"true\"\n");

    sandbox
        .run(&["run", "a", "--report", "toml:out.toml"])
        .assert_code(2)
        .assert_stderr_contains("unknown report format");
}

// ---------------------------------------------------------------------------
// M5: iOS
// ---------------------------------------------------------------------------

#[test]
fn ios_actions_are_registered_and_documented() {
    let sandbox = Sandbox::new("lanes: {}\n");

    sandbox
        .run(&["action", "list"])
        .assert_code(0)
        .assert_stdout_contains("build_ios")
        .assert_stdout_contains("testflight")
        .assert_stdout_contains("keychain");

    sandbox
        .run(&["action", "show", "build_ios"])
        .assert_code(0)
        .assert_stdout_contains("app-store")
        .assert_stdout_contains("scheme");
}

#[test]
fn build_ios_lets_xcodebuild_resolve_the_directory() {
    // A Swift package has neither a .xcodeproj nor a .xcworkspace to name, and
    // xcodebuild finds it from the working directory.
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    steps:\n      - action: build_ios\n        with:\n          scheme: MyApp\n",
    );

    let run = sandbox.run(&["run", "a", "--dry-run"]);
    run.assert_code(0)
        .assert_stdout_contains("xcodebuild archive");
    assert!(
        !run.stdout.contains("-project") && !run.stdout.contains("-workspace"),
        "neither flag should be passed when neither was given:\n{}",
        run.stdout
    );
}

#[test]
fn a_dry_run_survives_a_step_output_that_does_not_exist_yet() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - id: build
        action: build_ios
        with:
          scheme: MyApp
      - run: echo "shipping ${steps.build.ipa}"
"#,
    );

    sandbox
        .run(&["run", "a", "--dry-run"])
        .assert_code(0)
        .assert_stdout_contains("<steps.build.ipa>");
}

#[test]
fn build_ios_shows_the_commands_it_would_run() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  beta:
    steps:
      - action: build_ios
        with:
          workspace: MyApp.xcworkspace
          scheme: MyApp
          export_method: ad-hoc
          team_id: ABCDE12345
"#,
    );

    let run = sandbox.run(&["run", "beta", "--dry-run"]);
    run.assert_code(0)
        .assert_stdout_contains("xcodebuild archive")
        .assert_stdout_contains("-workspace 'MyApp.xcworkspace'")
        .assert_stdout_contains("-allowProvisioningUpdates")
        .assert_stdout_contains("-exportArchive")
        .assert_stdout_contains("ExportOptions.plist");
}

#[test]
fn build_ios_can_stop_after_the_archive() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  archive:
    steps:
      - action: build_ios
        with:
          project: MyApp.xcodeproj
          scheme: MyApp
          skip_export: true
"#,
    );

    let run = sandbox.run(&["run", "archive", "--dry-run"]);
    run.assert_code(0)
        .assert_stdout_contains("xcodebuild archive");
    assert!(
        !run.stdout.contains("-exportArchive") && !run.stdout.contains("ExportOptions.plist"),
        "skip_export still exports:\n{}",
        run.stdout
    );
}

#[test]
fn testflight_checks_its_arguments_before_anything_else() {
    let sandbox = Sandbox::new(
        "lanes:\n  a:\n    steps:\n      - action: testflight\n        with:\n          ipa: app.ipa\n",
    );

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("needs api_key or key_id, issuer_id and key");
}

#[test]
fn an_app_store_connect_key_that_is_not_a_key_is_reported() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - action: asc_request
        with:
          path: /v1/apps
          key_id: ABC123
          issuer_id: 69a6de7e-0000-0000-0000-000000000000
          key: definitely-not-a-p8
"#,
    );

    sandbox
        .run(&["run", "a"])
        .assert_code(1)
        .assert_stderr_contains("neither PEM, a file, nor base64");
}

#[test]
fn the_ios_keychain_password_never_reaches_the_output() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - action: keychain
        with:
          action: create
          name: shlane-test.keychain-db
          password: keychain-secret-9999
"#,
    );

    // `security` does not exist on Linux, so this fails -- what matters is that
    // the password is not in the failure.
    let run = sandbox.run(&["run", "a"]);
    let everything = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !everything.contains("keychain-secret-9999"),
        "the keychain password leaked:\n{everything}"
    );
}

// ---------------------------------------------------------------------------
// M6: plugins
// ---------------------------------------------------------------------------

/// Write a plugin that speaks the protocol, in the language every machine has.
fn write_plugin(sandbox: &Sandbox, dir: &str, extra_arg: Option<&str>) {
    let extra = extra_arg
        .map(|name| format!("      - name: {name}\n"))
        .unwrap_or_default();

    sandbox.write(
        &format!("{dir}/shlane-plugin.yaml"),
        &format!(
            "name: line-notify\nversion: 0.2.0\nprotocol: 1\nexecutable: notify.sh\nactions:\n  - name: notify_line\n    description: Send a LINE message\n    args:\n      - name: token\n        description: Channel token\n        required: true\n        sensitive: true\n      - name: message\n        description: What to send\n{extra}"
        ),
    );

    sandbox.write(
        &format!("{dir}/notify.sh"),
        r#"#!/bin/sh
request=$(cat)
case "$request" in
  *'"op":"describe"'*)
    printf '{"type":"describe","description":"Send a LINE message","args":[{"name":"token","description":"Channel token","required":true,"sensitive":true},{"name":"message","description":"What to send"}]}\n'
    exit 0
    ;;
esac
case "$request" in
  *'"message":"fail"'*)
    printf '{"type":"result","ok":false,"message":"the channel rejected it"}\n'
    exit 1
    ;;
esac
printf '{"type":"secret","value":"runtime-token-98765"}\n'
printf '{"type":"log","level":"info","message":"sending with runtime-token-98765"}\n'
printf '{"type":"result","ok":true,"outputs":{"id":"msg-1"}}\n'
"#,
    );

    Command::new("chmod")
        .args(["+x", &format!("{dir}/notify.sh")])
        .current_dir(sandbox.path())
        .status()
        .expect("chmod should run");
}

const PLUGIN_CONFIG: &str = r#"
plugins:
  - name: line-notify
    path: ./tools/line-notify
lanes:
  notify:
    steps:
      - id: sent
        action: notify_line
        with:
          token: super-secret-channel-token
          message: hello
      - run: echo "message id ${steps.sent.id}"
"#;

#[test]
fn a_plugin_action_runs_and_reports_its_outputs() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);

    let run = sandbox.run(&["run", "notify"]);
    run.assert_code(0)
        .assert_stdout_contains("message id msg-1");

    let everything = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !everything.contains("super-secret-channel-token"),
        "an argument the manifest marks sensitive leaked:\n{everything}"
    );
    assert!(
        !everything.contains("runtime-token-98765"),
        "a secret the plugin declared at runtime leaked:\n{everything}"
    );
}

#[test]
fn plugin_actions_are_listed_and_validated_like_built_ins() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);

    sandbox
        .run(&["action", "list"])
        .assert_code(0)
        .assert_stdout_contains("notify_line")
        .assert_stdout_contains("Send a LINE message");

    sandbox
        .run(&["action", "show", "notify_line"])
        .assert_code(0)
        .assert_stdout_contains("Channel token")
        .assert_stdout_contains("masked in output");

    sandbox.run(&["validate"]).assert_code(0);
}

#[test]
fn a_plugin_step_with_a_misspelled_argument_fails_validation() {
    let sandbox = Sandbox::new(
        r#"
plugins:
  - name: line-notify
    path: ./tools/line-notify
lanes:
  notify:
    steps:
      - action: notify_line
        with:
          mesage: hello
"#,
    );
    write_plugin(&sandbox, "tools/line-notify", None);

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("needs 'token'")
        .assert_stderr_contains("no argument 'mesage'");
}

#[test]
fn a_failing_plugin_fails_the_lane_with_its_own_message() {
    let sandbox = Sandbox::new(
        r#"
plugins:
  - name: line-notify
    path: ./tools/line-notify
lanes:
  notify:
    steps:
      - action: notify_line
        with:
          token: t
          message: fail
"#,
    );
    write_plugin(&sandbox, "tools/line-notify", None);

    sandbox
        .run(&["run", "notify"])
        .assert_code(1)
        .assert_stderr_contains("the channel rejected it");
}

#[test]
fn plugin_list_and_lock_record_the_checksum() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);

    sandbox
        .run(&["plugin", "list"])
        .assert_code(0)
        .assert_stdout_contains("line-notify 0.2.0")
        .assert_stdout_contains("notify_line")
        .assert_stdout_contains("shlane plugin lock");

    sandbox
        .run(&["plugin", "lock"])
        .assert_code(0)
        .assert_stdout_contains("1 plugin(s)");

    let lock = fs::read_to_string(sandbox.path().join("shlane-plugins.lock"))
        .expect("the lockfile should exist");
    assert!(lock.contains("line-notify sha256:"), "{lock}");

    sandbox
        .run(&["plugin", "list"])
        .assert_code(0)
        .assert_stdout_contains("locked    yes");
}

#[test]
fn a_plugin_that_changed_after_being_locked_is_refused() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);
    sandbox.run(&["plugin", "lock"]).assert_code(0);

    // Someone edits the executable after the lockfile was written.
    sandbox.write(
        "tools/line-notify/notify.sh",
        "#!/bin/sh\nprintf '{\"type\":\"result\",\"ok\":true}\\n'\n",
    );

    sandbox
        .run(&["run", "notify"])
        .assert_code(2)
        .assert_stderr_contains("does not match the lockfile");
}

#[test]
fn plugin_verify_catches_a_manifest_that_drifted() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);

    sandbox
        .run(&["plugin", "verify"])
        .assert_code(0)
        .assert_stdout_contains("agree with their manifests");

    // The manifest gains an argument the executable knows nothing about.
    write_plugin(&sandbox, "tools/line-notify", Some("sticker"));

    sandbox
        .run(&["plugin", "verify"])
        .assert_code(2)
        .assert_stderr_contains("the plugin does not report it");
}

#[test]
fn a_plugin_that_has_not_been_fetched_says_so() {
    let sandbox = Sandbox::new(
        "plugins:\n  - name: line-notify\n    source: github:someone/shlane-line-notify@v1\nlanes:\n  a: {}\n",
    );

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("is not installed")
        .assert_stderr_contains("shlane plugin install");
}

#[test]
fn a_plugin_with_neither_path_nor_source_is_reported() {
    let sandbox = Sandbox::new("plugins:\n  - name: line-notify\nlanes:\n  a: {}\n");

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("needs either `path:` or `source:`");
}

/// Turn a plugin directory into a git repository that can be cloned from.
fn make_plugin_repo(sandbox: &Sandbox, dir: &str) -> String {
    for args in [
        vec!["init", "-q", "-b", "main", "."],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "shlane test"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["add", "-A"],
        vec!["commit", "-qm", "the plugin"],
        vec!["tag", "v1.0.0"],
    ] {
        let output = Command::new("git")
            .args(&args)
            .current_dir(sandbox.path().join(dir))
            .output()
            .expect("git should run");
        assert!(output.status.success(), "git {args:?} failed");
    }

    format!("file://{}", sandbox.path().join(dir).display())
}

#[test]
fn a_plugin_is_fetched_from_a_git_host_and_then_runs() {
    let sandbox = Sandbox::empty();
    write_plugin(&sandbox, "upstream", None);
    let url = make_plugin_repo(&sandbox, "upstream");

    sandbox.write(
        "shlane.yaml",
        &format!(
            r#"
plugins:
  - name: line-notify
    source: git:{url}@v1.0.0
lanes:
  notify:
    steps:
      - id: sent
        action: notify_line
        with:
          token: super-secret-channel-token
          message: hello
      - run: echo "message id ${{steps.sent.id}}"
"#
        ),
    );

    // Nothing is fetched as a side effect of running.
    sandbox
        .run(&["run", "notify"])
        .assert_code(2)
        .assert_stderr_contains("shlane plugin install");

    sandbox
        .run(&["plugin", "install"])
        .assert_code(0)
        .assert_stdout_contains("Installed line-notify")
        .assert_stdout_contains("not in the lockfile yet");

    let run = sandbox.run(&["run", "notify"]);
    run.assert_code(0)
        .assert_stdout_contains("message id msg-1");
    assert!(
        !run.stdout.contains("super-secret-channel-token"),
        "a fetched plugin's sensitive argument leaked:\n{}",
        run.stdout
    );

    // The clone's history is not kept: a plugin must not be updatable in place
    // without going through the checksum.
    assert!(
        !sandbox
            .path()
            .join(".shlane/plugins/line-notify/.git")
            .exists(),
        "the plugin kept its git history"
    );
}

#[test]
fn installing_again_does_nothing_unless_asked() {
    let sandbox = Sandbox::empty();
    write_plugin(&sandbox, "upstream", None);
    let url = make_plugin_repo(&sandbox, "upstream");
    sandbox.write(
        "shlane.yaml",
        &format!(
            "plugins:\n  - name: line-notify\n    source: git:{url}@v1.0.0\nlanes:\n  a: {{}}\n"
        ),
    );

    sandbox.run(&["plugin", "install"]).assert_code(0);
    sandbox
        .run(&["plugin", "install"])
        .assert_code(0)
        .assert_stdout_contains("already installed");
    sandbox
        .run(&["plugin", "install", "--force"])
        .assert_code(0)
        .assert_stdout_contains("Installed line-notify");
}

#[test]
fn a_source_without_a_tag_is_flagged_as_floating() {
    let sandbox = Sandbox::empty();
    write_plugin(&sandbox, "upstream", None);
    let url = make_plugin_repo(&sandbox, "upstream");
    sandbox.write(
        "shlane.yaml",
        &format!("plugins:\n  - name: line-notify\n    source: git:{url}\nlanes:\n  a: {{}}\n"),
    );

    sandbox
        .run(&["plugin", "install"])
        .assert_code(0)
        .assert_stdout_contains("nothing pins this plugin");
}

#[test]
fn a_fetched_plugin_must_be_the_one_the_config_named() {
    let sandbox = Sandbox::empty();
    write_plugin(&sandbox, "upstream", None);
    let url = make_plugin_repo(&sandbox, "upstream");
    sandbox.write(
        "shlane.yaml",
        &format!(
            "plugins:\n  - name: something-else\n    source: git:{url}@v1.0.0\nlanes:\n  a: {{}}\n"
        ),
    );

    sandbox
        .run(&["plugin", "install"])
        .assert_code(2)
        .assert_stderr_contains("manifest says 'line-notify'");

    assert!(
        !sandbox
            .path()
            .join(".shlane/plugins/something-else")
            .exists(),
        "a rejected plugin should not be left installed"
    );
}

#[test]
fn a_moved_tag_is_caught_by_the_lockfile() {
    let sandbox = Sandbox::empty();
    write_plugin(&sandbox, "upstream", None);
    let url = make_plugin_repo(&sandbox, "upstream");
    sandbox.write(
        "shlane.yaml",
        &format!(
            "plugins:\n  - name: line-notify\n    source: git:{url}@v1.0.0\nlanes:\n  a: {{}}\n"
        ),
    );

    sandbox.run(&["plugin", "install"]).assert_code(0);
    sandbox.run(&["plugin", "lock"]).assert_code(0);

    // Upstream changes what the tag points at.
    sandbox.write(
        "upstream/notify.sh",
        "#!/bin/sh
echo 'this is not the plugin you locked'
",
    );
    for args in [
        vec!["add", "-A"],
        vec!["commit", "-qm", "sneaky"],
        vec!["tag", "-f", "v1.0.0"],
    ] {
        Command::new("git")
            .args(&args)
            .current_dir(sandbox.path().join("upstream"))
            .output()
            .expect("git should run");
    }

    sandbox
        .run(&["plugin", "install", "--force"])
        .assert_code(2)
        .assert_stderr_contains("does not match the lockfile");
}

#[test]
fn a_source_that_cannot_be_fetched_is_reported() {
    let sandbox = Sandbox::new(
        "plugins:\n  - name: line-notify\n    source: git:file:///definitely/not/a/repo@v1\nlanes:\n  a: {}\n",
    );

    sandbox
        .run(&["plugin", "install"])
        .assert_code(2)
        .assert_stderr_contains("could not fetch");
}

#[test]
fn a_plugin_speaking_another_protocol_is_refused() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);
    sandbox.write(
        "tools/line-notify/shlane-plugin.yaml",
        "name: line-notify\nprotocol: 99\nexecutable: notify.sh\nactions: []\n",
    );

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("speaks protocol 99");
}

// ---------------------------------------------------------------------------
// M6: migrating from fastlane
// ---------------------------------------------------------------------------

const FASTFILE: &str = r#"
default_platform(:ios)

platform :ios do
  desc "Push a new beta build to TestFlight"
  lane :beta do |options|
    ensure_git_status_clean
    increment_build_number
    gym(scheme: "MyApp", export_method: "app-store")
    pilot
    slack(message: "Shipped #{options[:version]}", slack_url: ENV["SLACK_URL"])
  end

  lane :release do
    match(type: "appstore")
    sh "echo done"
  end
end
"#;

#[test]
fn migrate_converts_a_fastfile_into_a_config_shlane_can_read() {
    let sandbox = Sandbox::empty();
    sandbox.write("fastlane/Fastfile", FASTFILE);

    let run = sandbox.run(&["migrate"]);
    run.assert_code(0)
        .assert_stdout_contains("2 lane(s)")
        .assert_stdout_contains("What needs a person")
        .assert_stdout_contains("match");

    let yaml = fs::read_to_string(sandbox.path().join("shlane.yaml")).expect("written");
    assert!(yaml.contains("  beta:"), "{yaml}");
    assert!(yaml.contains("action: build_ios"), "{yaml}");
    assert!(yaml.contains("action: testflight"), "{yaml}");
    assert!(yaml.contains("webhook: \"${SLACK_URL}\""), "{yaml}");
    assert!(yaml.contains("# TODO: migrate by hand: match"), "{yaml}");

    // The point of the exercise: what it wrote is a config shlane accepts.
    sandbox
        .run(&["validate"])
        .assert_code(0)
        .assert_stdout_contains("is valid");

    sandbox
        .run(&["list"])
        .assert_code(0)
        .assert_stdout_contains("Push a new beta build to TestFlight");
}

#[test]
fn migrate_refuses_to_clobber_an_existing_config() {
    let sandbox = Sandbox::new("lanes:\n  keep:\n    steps:\n      - run: \"true\"\n");
    sandbox.write("fastlane/Fastfile", FASTFILE);

    sandbox
        .run(&["migrate"])
        .assert_code(2)
        .assert_stderr_contains("already exists");

    sandbox
        .run(&["migrate", "--out", "converted.yaml"])
        .assert_code(0);
    assert!(sandbox.path().join("converted.yaml").is_file());
}

#[test]
fn migrate_says_when_there_is_no_fastfile() {
    let sandbox = Sandbox::empty();

    sandbox
        .run(&["migrate"])
        .assert_code(3)
        .assert_stderr_contains("no config file found");
}

// ---------------------------------------------------------------------------
// M6: reading an existing fastlane match repository
// ---------------------------------------------------------------------------

/// A provisioning profile encrypted by real OpenSSL with "match-passphrase",
/// the way `match` writes them.
const MATCH_PROFILE: &str = "U2FsdGVkX1/15/pI5yH7uEFJbz1lY8Z2zvhRPWDrZUk1kEHiEQoZTp1vLnNUHa/svMJauo0/VXpgWtQZwC0+V19MbR4pLqblubwkZRRR9cC2/72dvBChjOqAVI8gM8lsCDWTpjn2twg0xEjGo9uqGSDMkzTjIyPDhRNQDO/X9ZeM4CzklQCEYJstMxSv3Ix2t8/UMJFKVo2wDEVjwxuxCt9neGQWYHVFLumnktDtHL9ICriotLsZ9kAhH2UivTiq9WOXtgLkDn/whXPq+2tzXXBrpa7GE0ym7un43kd4aro33QLg9q3DkjZ2c0L1TULGrF/oo+8/ugg+Q6FpFi9ZZJtVkc4KynxFX4pvi2fjTLVf7KT+P7rKi418+qJhWytIdb36qMCKQMiXASdUa3l+pumYZh6G0ZPzc9HTOjXaN5mq4WmmKN5my+W3tWBPtSM1LgK8n7G9Qt6484ENuGt1YZlqzMSt5hJ3UarwksiFlQw=";

const MATCH_P12: &str = "U2FsdGVkX18sELh2Kwcyo4rbTLbVWUPe0waKGQDGRk36E1qDgz6hxYgC1DN+1vzJ";

/// Build a git repository shaped like the one `match` maintains.
fn write_match_repo(sandbox: &Sandbox, dir: &str) {
    sandbox.write(
        &format!("{dir}/profiles/appstore/AppStore_com.example.app.mobileprovision"),
        MATCH_PROFILE,
    );
    sandbox.write(&format!("{dir}/certs/distribution/ABC123.p12"), MATCH_P12);
    sandbox.write(&format!("{dir}/certs/distribution/ABC123.cer"), MATCH_P12);

    for args in [
        vec!["init", "-q", "-b", "master", "."],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "shlane test"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["add", "-A"],
        vec!["commit", "-qm", "certificates"],
    ] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(sandbox.path().join(dir))
            .output()
            .expect("git should run");
        assert!(status.status.success(), "git {args:?} failed");
    }
}

#[test]
fn codesign_sync_reads_a_match_repository() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  signing:
    steps:
      - id: certs
        action: codesign_sync
        with:
          git_url: ./certificates
          type: appstore
          app_identifier: com.example.app
          passphrase: match-passphrase
          install: false
      - run: echo "profile ${steps.certs.uuid} team ${steps.certs.team_id}"
"#,
    );
    write_match_repo(&sandbox, "certificates");

    let run = sandbox.run(&["run", "signing"]);
    run.assert_code(0)
        .assert_stdout_contains("AppStore com.example.app")
        .assert_stdout_contains("profile 1a2b3c4d-0000-1111-2222-333344445555 team ABCDE12345");

    // The decrypted files are where the step said they are.
    let decrypted = sandbox
        .path()
        .join(".shlane/codesign-out/1a2b3c4d-0000-1111-2222-333344445555.mobileprovision");
    assert!(decrypted.is_file(), "the profile was not written");

    // A real profile is a signed container, so it is not valid UTF-8 throughout.
    let contents = fs::read(&decrypted).expect("readable");
    assert!(
        contents
            .windows(16)
            .any(|window| window == b"<key>UUID</key>\n"),
        "the decrypted profile does not look like a plist"
    );
}

#[test]
fn a_wrong_match_passphrase_is_reported_clearly() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  signing:
    steps:
      - action: codesign_sync
        with:
          git_url: ./certificates
          app_identifier: com.example.app
          passphrase: not-the-passphrase
          install: false
"#,
    );
    write_match_repo(&sandbox, "certificates");

    let run = sandbox.run(&["run", "signing"]);
    run.assert_code(1).assert_stderr_contains("passphrase");
    assert!(
        !run.stderr.contains("not-the-passphrase"),
        "the passphrase leaked:\n{}",
        run.stderr
    );
}

#[test]
fn codesign_sync_says_when_the_app_is_not_in_the_repository() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  signing:
    steps:
      - action: codesign_sync
        with:
          git_url: ./certificates
          app_identifier: com.example.other
          passphrase: match-passphrase
          install: false
"#,
    );
    write_match_repo(&sandbox, "certificates");

    sandbox
        .run(&["run", "signing"])
        .assert_code(1)
        .assert_stderr_contains("no appstore profile for com.example.other");
}

#[test]
fn codesign_sync_updates_a_clone_it_already_has() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  signing:
    steps:
      - action: codesign_sync
        with:
          git_url: ./certificates
          app_identifier: com.example.app
          passphrase: match-passphrase
          install: false
"#,
    );
    write_match_repo(&sandbox, "certificates");

    sandbox.run(&["run", "signing"]).assert_code(0);
    // The second run takes the other path through fetch().
    sandbox
        .run(&["run", "signing"])
        .assert_code(0)
        .assert_stdout_contains("Updating");
}

#[test]
fn installing_without_macos_says_so_rather_than_failing_obscurely() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  signing:
    steps:
      - action: codesign_sync
        with:
          git_url: ./certificates
          app_identifier: com.example.app
          passphrase: match-passphrase
"#,
    );
    write_match_repo(&sandbox, "certificates");

    let run = sandbox.run(&["run", "signing"]);
    if cfg!(target_os = "macos") {
        // On macOS it gets as far as the keychain, which this test has not set up.
        run.assert_code(1);
    } else {
        run.assert_code(1).assert_stderr_contains("needs macOS");
    }
}

// ---------------------------------------------------------------------------
// M7: CI integration
// ---------------------------------------------------------------------------

#[test]
fn env_shows_what_a_lane_would_see_with_secrets_masked() {
    let sandbox = Sandbox::new(
        "env:\n  APP_ENV: production\n  API_TOKEN: super-secret-value\nenv_files: [.env]\nlanes:\n  a: {}\n",
    );
    sandbox.write(".env", "FROM_FILE=yes\n");

    let run = sandbox.run(&["env"]);
    run.assert_code(0)
        .assert_stdout_contains("APP_ENV=production")
        .assert_stdout_contains("FROM_FILE=yes")
        .assert_stdout_contains("API_TOKEN=***")
        .assert_stdout_contains("inherited from the environment");

    assert!(
        !run.stdout.contains("super-secret-value"),
        "`shlane env` must not be the easiest way to leak a token:\n{}",
        run.stdout
    );

    // By default it shows what the config contributes, not the whole process.
    assert!(
        !run.stdout.contains("PATH="),
        "the default listing should not dump the process environment:\n{}",
        run.stdout
    );
    sandbox
        .run(&["env", "--all"])
        .assert_code(0)
        .assert_stdout_contains("PATH=");
}

#[test]
fn env_reports_whether_this_looks_like_ci() {
    let sandbox = Sandbox::new("lanes:\n  a: {}\n");

    let output = Command::new(env!("CARGO_BIN_EXE_shlane"))
        .arg("env")
        .current_dir(sandbox.path())
        .env_remove("SHLANE_CONFIG")
        .env("GITHUB_ACTIONS", "true")
        .env("CI", "true")
        .output()
        .expect("shlane should run");
    let run = Run::new(output);

    run.assert_code(0)
        .assert_stdout_contains("running on github");
}

#[test]
fn a_failure_on_github_actions_is_annotated() {
    let sandbox = Sandbox::new("lanes:\n  a:\n    steps:\n      - run: exit 1\n");

    let output = Command::new(env!("CARGO_BIN_EXE_shlane"))
        .args(["run", "a"])
        .current_dir(sandbox.path())
        .env_remove("SHLANE_CONFIG")
        .env("GITHUB_ACTIONS", "true")
        .output()
        .expect("shlane should run");
    let run = Run::new(output);

    run.assert_code(1)
        .assert_stderr_contains("::error title=shlane::");

    // Off GitHub, nothing extra is printed.
    let plain = sandbox.run(&["run", "a"]);
    plain.assert_code(1);
    assert!(
        !plain.stderr.contains("::error"),
        "annotations should only appear where they mean something:\n{}",
        plain.stderr
    );
}

#[test]
fn scripts_can_branch_on_ci() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - run: echo on-ci
        if: is_ci()
      - run: echo always
    script: |
      print("provider=" + ci_provider());
"#,
    );

    // Through the sandbox helper, so the runner's own CI variables are cleared
    // first: a real GITHUB_ACTIONS would win over the BUILDKITE set here.
    let run = sandbox.run_with_env(&["run", "a"], &[("BUILDKITE", "true")]);

    run.assert_code(0)
        .assert_stdout_contains("on-ci")
        .assert_stdout_contains("provider=buildkite");

    let local = sandbox.run(&["run", "a"]);
    local.assert_code(0).assert_stdout_contains("always");
    assert!(
        !local.stdout.contains("on-ci\n"),
        "the CI-only step should have been skipped:\n{}",
        local.stdout
    );
}

// ---------------------------------------------------------------------------
// M6: plugins written in Rhai
// ---------------------------------------------------------------------------

/// A plugin that is a script rather than a program.
fn write_rhai_plugin(sandbox: &Sandbox, dir: &str, body: &str) {
    sandbox.write(
        &format!("{dir}/shlane-plugin.yaml"),
        "name: release-helpers\nversion: 0.1.0\nprotocol: 1\nscript: helpers.rhai\nactions:\n  - name: tag_release\n    description: Tag a release and report what it did\n    args:\n      - name: version\n        description: Version to tag\n        required: true\n      - name: token\n        description: An API token\n        sensitive: true\n",
    );
    sandbox.write(&format!("{dir}/helpers.rhai"), body);
}

const RHAI_PLUGIN_CONFIG: &str = r#"
plugins:
  - name: release-helpers
    path: ./tools/helpers
lanes:
  release:
    steps:
      - id: tagged
        action: tag_release
        with:
          version: 1.4.2
          token: super-secret-api-token
      - run: echo "tagged ${steps.tagged.tag} at ${steps.tagged.where}"
"#;

#[test]
fn a_rhai_plugin_runs_with_the_same_builtins_a_lane_script_has() {
    let sandbox = Sandbox::new(RHAI_PLUGIN_CONFIG);
    write_rhai_plugin(
        &sandbox,
        "tools/helpers",
        r#"
fn tag_release(args) {
    ui_message("tagging " + args.version);
    let here = capture("echo from-the-plugin");
    secret(args.token);
    print("token is " + args.token);
    #{ tag: "v" + args.version, where: here }
}
"#,
    );

    let run = sandbox.run(&["run", "release"]);
    run.assert_code(0)
        .assert_stdout_contains("tagging 1.4.2")
        .assert_stdout_contains("tagged v1.4.2 at from-the-plugin");

    let everything = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !everything.contains("super-secret-api-token"),
        "a sensitive argument leaked from a Rhai plugin:\n{everything}"
    );
}

#[test]
fn a_rhai_plugin_is_listed_and_validated_like_any_other() {
    let sandbox = Sandbox::new(RHAI_PLUGIN_CONFIG);
    write_rhai_plugin(
        &sandbox,
        "tools/helpers",
        "fn tag_release(args) { #{ tag: args.version, where: \"here\" } }\n",
    );

    sandbox.run(&["validate"]).assert_code(0);

    sandbox
        .run(&["action", "show", "tag_release"])
        .assert_code(0)
        .assert_stdout_contains("Version to tag")
        .assert_stdout_contains("masked in output");

    sandbox
        .run(&["plugin", "list"])
        .assert_code(0)
        .assert_stdout_contains("helpers.rhai (rhai)")
        .assert_stdout_contains("tag_release");
}

#[test]
fn a_rhai_plugin_that_fails_fails_the_lane() {
    let sandbox = Sandbox::new(RHAI_PLUGIN_CONFIG);
    write_rhai_plugin(
        &sandbox,
        "tools/helpers",
        "fn tag_release(args) { throw \"the tag already exists\"; }\n",
    );

    sandbox
        .run(&["run", "release"])
        .assert_code(1)
        .assert_stderr_contains("the tag already exists");
}

#[test]
fn a_rhai_plugin_can_call_back_into_the_action_registry() {
    let sandbox = Sandbox::new(RHAI_PLUGIN_CONFIG);
    write_rhai_plugin(
        &sandbox,
        "tools/helpers",
        "fn tag_release(args) { let r = action(\"sh\", #{ command: \"echo from-the-plugin\" }); #{ tag: \"v1\", where: r.stdout } }\n",
    );

    // The registry is reached through a weak handle, so the plugin it holds can
    // be handed it back without the two keeping each other alive.
    sandbox
        .run(&["run", "release"])
        .assert_code(0)
        .assert_stdout_contains("from-the-plugin");
}

#[test]
fn a_rhai_plugin_has_no_lane_to_call_back_into() {
    let sandbox = Sandbox::new(RHAI_PLUGIN_CONFIG);
    write_rhai_plugin(
        &sandbox,
        "tools/helpers",
        "fn tag_release(args) { call_lane(\"release\"); #{ tag: \"v1\", where: \"here\" } }\n",
    );

    sandbox
        .run(&["run", "release"])
        .assert_code(1)
        .assert_stderr_contains("not available inside a Rhai plugin");
}

#[test]
fn plugin_verify_checks_a_rhai_script_against_its_manifest() {
    let sandbox = Sandbox::new(RHAI_PLUGIN_CONFIG);
    write_rhai_plugin(
        &sandbox,
        "tools/helpers",
        "fn tag_release(args) { #{ tag: \"v1\" } }\n",
    );

    sandbox
        .run(&["plugin", "verify"])
        .assert_code(0)
        .assert_stdout_contains("agree with their manifests");

    // The script no longer defines what the manifest promises.
    sandbox.write(
        "tools/helpers/helpers.rhai",
        "fn something_else(args) { #{} }\n",
    );
    sandbox
        .run(&["plugin", "verify"])
        .assert_code(2)
        .assert_stderr_contains("the script defines no such function");

    // A script that does not compile is reported rather than left to a lane.
    sandbox.write("tools/helpers/helpers.rhai", "fn broken( {{{\n");
    sandbox.run(&["plugin", "verify"]).assert_code(2);
}

#[test]
fn a_manifest_must_say_how_the_plugin_runs() {
    let sandbox = Sandbox::new(RHAI_PLUGIN_CONFIG);
    write_rhai_plugin(&sandbox, "tools/helpers", "fn tag_release(args) {}\n");
    sandbox.write(
        "tools/helpers/shlane-plugin.yaml",
        "name: release-helpers\nprotocol: 1\nexecutable: run.sh\nscript: helpers.rhai\nactions: []\n",
    );

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("not both");
}

#[test]
fn a_rhai_plugin_returning_the_wrong_shape_is_reported() {
    let sandbox = Sandbox::new(RHAI_PLUGIN_CONFIG);
    write_rhai_plugin(
        &sandbox,
        "tools/helpers",
        "fn tag_release(args) { \"just a string\" }\n",
    );

    sandbox
        .run(&["run", "release"])
        .assert_code(1)
        .assert_stderr_contains("map of outputs");
}

#[test]
fn capture_reads_for_real_under_dry_run() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  a:
    steps:
      - run: echo would-not-run
    script: |
      let version = capture("cat VERSION");
      ui_message("version is " + version);
      let changed = try_run("touch should-not-exist");
      ui_message("change skipped: " + changed.success);
"#,
    );
    sandbox.write("VERSION", "2.1.0\n");

    let run = sandbox.run(&["run", "a", "--dry-run"]);
    run.assert_code(0)
        .assert_stdout_contains("version is 2.1.0")
        .assert_stdout_contains("Would run: echo would-not-run");

    assert!(
        !sandbox.path().join("should-not-exist").exists(),
        "--dry-run must still skip commands that change something"
    );
}

#[test]
fn setup_ci_does_nothing_off_ci() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  prepare:
    steps:
      - id: setup
        action: setup_ci
      - run: echo ci=${steps.setup.ci}
"#,
    );

    // CI, and the provider variables, removed: this has to look like a
    // developer's machine, where taking over the default keychain would lock
    // them out of their own certificates.
    sandbox
        .run_with_env(&["run", "prepare"], &[("CI", ""), ("GITHUB_ACTIONS", "")])
        .assert_code(0)
        .assert_stdout_contains("Not running on CI")
        .assert_stdout_contains("ci=false");
}

#[test]
fn setup_ci_reports_the_provider_it_found() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  prepare:
    steps:
      - action: setup_ci
"#,
    );

    let run = sandbox.run_with_env(
        &["run", "prepare"],
        &[("CI", "true"), ("GITHUB_ACTIONS", "true")],
    );
    run.assert_code(0)
        .assert_stdout_contains("CI detected: github");

    // The keychain half is macOS-only; everywhere else it says so rather than
    // failing on a missing `security`.
    if !cfg!(target_os = "macos") {
        run.assert_stdout_contains("Not macOS");
    }
}

#[test]
fn cache_paths_reports_only_what_the_config_uses() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  build:
    steps:
      - action: build_android
        with: { format: apk }
"#,
    );

    sandbox
        .run(&["cache-paths"])
        .assert_code(0)
        .assert_stdout_contains("~/.gradle/caches");

    // Nothing here touches Xcode, so suggesting its cache would be noise.
    let run = sandbox.run(&["cache-paths"]);
    assert!(
        !run.stdout.contains("DerivedData"),
        "should not suggest an Xcode cache for an Android-only config:\n{}",
        run.stdout
    );
}

#[test]
fn cache_paths_emits_a_json_array() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  build:
    steps:
      - run: ./gradlew assemble
"#,
    );

    sandbox
        .run(&["cache-paths", "--json"])
        .assert_code(0)
        .assert_stdout_contains(r#"["~/.gradle/caches","~/.gradle/wrapper"]"#);
}

#[test]
fn cache_paths_says_so_when_there_is_nothing_to_cache() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  hello:
    steps:
      - run: echo hi
"#,
    );

    sandbox
        .run(&["cache-paths"])
        .assert_code(0)
        .assert_stdout_contains("Nothing in this config downloads anything worth caching");
}

/// A git repository holding a minimal Rhai plugin, to fetch from.
fn plugin_repo(at: &Path) {
    fs::create_dir_all(at).expect("plugin source should be creatable");
    fs::write(
        at.join("shlane-plugin.yaml"),
        "name: greeter\nversion: 0.1.0\nprotocol: 1\nscript: greeter.rhai\nactions:\n  - name: greet\n    description: Say hello\n    args:\n      - name: who\n        description: Who to greet\n        required: true\n",
    )
    .expect("manifest should be writable");
    fs::write(
        at.join("greeter.rhai"),
        "fn greet(args) {\n    ui_message(\"hello \" + args.who);\n    #{ greeted: args.who }\n}\n",
    )
    .expect("script should be writable");

    for args in [
        vec!["init", "-q", "."],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ],
        vec!["tag", "v0.1.0"],
    ] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(at)
            .status()
            .expect("git should be runnable");
        assert!(status.success(), "git {args:?} failed");
    }
}

#[test]
fn plugin_add_fetches_declares_and_locks() {
    let sandbox = Sandbox::new("# keep me\nlanes:\n  hello:\n    steps:\n      - run: echo hi\n");
    let source = sandbox.path().join("source");
    plugin_repo(&source);

    sandbox
        .run(&["plugin", "add", &format!("git:{}@v0.1.0", source.display())])
        .assert_code(0)
        .assert_stdout_contains("Added greeter 0.1.0");

    let config = fs::read_to_string(sandbox.path().join("shlane.yaml")).expect("config readable");
    assert!(config.contains("# keep me"), "comments survive:\n{config}");
    assert!(config.contains("- name: greeter"), "{config}");

    let lock =
        fs::read_to_string(sandbox.path().join("shlane-plugins.lock")).expect("lockfile written");
    assert!(lock.contains("greeter sha256:"), "{lock}");

    // And the action it provides is now callable.
    sandbox.write(
        "shlane.yaml",
        &config.replace(
            "      - run: echo hi",
            "      - run: echo hi\n      - action: greet\n        with:\n          who: world",
        ),
    );
    sandbox
        .run(&["run", "hello"])
        .assert_code(0)
        .assert_stdout_contains("hello world");
}

#[test]
fn plugin_remove_refuses_while_a_lane_still_calls_it() {
    let sandbox = Sandbox::new("lanes:\n  hello:\n    steps:\n      - run: echo hi\n");
    let source = sandbox.path().join("source");
    plugin_repo(&source);
    sandbox
        .run(&["plugin", "add", &format!("git:{}@v0.1.0", source.display())])
        .assert_code(0);

    let config = fs::read_to_string(sandbox.path().join("shlane.yaml")).expect("config readable");
    sandbox.write(
        "shlane.yaml",
        &config.replace(
            "      - run: echo hi",
            "      - run: echo hi\n      - action: greet\n        with:\n          who: world",
        ),
    );

    // Removing it here would leave a config that no longer validates, and the
    // failure would surface later as "no such action".
    sandbox
        .run(&["plugin", "remove", "greeter"])
        .assert_code(2)
        .assert_stderr_contains("still provides actions this config uses");

    assert!(
        sandbox.path().join(".shlane/plugins/greeter").is_dir(),
        "a refused removal must not delete anything"
    );

    sandbox
        .run(&["plugin", "remove", "greeter", "--force"])
        .assert_code(0)
        .assert_stdout_contains("Removed greeter");
    assert!(!sandbox.path().join(".shlane/plugins/greeter").exists());
}

#[test]
fn plugin_remove_drops_the_declaration_and_the_directory() {
    let sandbox = Sandbox::new("lanes:\n  hello:\n    steps:\n      - run: echo hi\n");
    let source = sandbox.path().join("source");
    plugin_repo(&source);
    sandbox
        .run(&["plugin", "add", &format!("git:{}@v0.1.0", source.display())])
        .assert_code(0);

    sandbox
        .run(&["plugin", "remove", "greeter"])
        .assert_code(0)
        .assert_stdout_contains("Removed greeter");

    let config = fs::read_to_string(sandbox.path().join("shlane.yaml")).expect("config readable");
    assert!(!config.contains("greeter"), "{config}");
    // An empty `plugins:` key left behind is untidy, so it goes too.
    assert!(!config.contains("plugins:"), "{config}");
    sandbox.run(&["validate"]).assert_code(0);
}

#[test]
fn plugin_remove_reports_a_name_that_is_not_declared() {
    let sandbox = Sandbox::new("lanes:\n  hello:\n    steps:\n      - run: echo hi\n");
    sandbox
        .run(&["plugin", "remove", "nothing"])
        .assert_code(2)
        .assert_stderr_contains("is not declared in this config");
}

#[test]
fn copy_artifacts_gathers_matches_and_warns_about_the_rest() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  collect:
    steps:
      - id: copied
        action: copy_artifacts
        with:
          paths: "build/**/*.apk, build/mapping.txt, build/nothing-here.txt"
          into: artifacts
      - run: echo count=${steps.copied.count}
"#,
    );
    sandbox.write("build/outputs/apk/app-release.apk", "apk");
    sandbox.write("build/outputs/apk/app-debug.apk", "apk");
    sandbox.write("build/mapping.txt", "map");
    sandbox.write("build/notes.md", "not an artifact");

    let run = sandbox.run(&["run", "collect"]);
    run.assert_code(0).assert_stdout_contains("count=3");
    run.assert_stderr_contains("'build/nothing-here.txt' matched nothing");

    assert!(sandbox.path().join("artifacts/app-release.apk").is_file());
    assert!(sandbox.path().join("artifacts/mapping.txt").is_file());
    assert!(
        !sandbox.path().join("artifacts/notes.md").exists(),
        "a file that matched no pattern must not be copied"
    );
}

#[test]
fn copy_artifacts_can_fail_when_a_pattern_matches_nothing() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  collect:
    steps:
      - action: copy_artifacts
        with:
          paths: "build/*.apk"
          into: artifacts
          fail_on_missing: true
"#,
    );

    sandbox
        .run(&["run", "collect"])
        .assert_code(1)
        .assert_stderr_contains("matched nothing");
}

#[test]
fn clean_build_artifacts_deletes_files_and_directories() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  clean:
    steps:
      - id: cleaned
        action: clean_build_artifacts
        with:
          paths: "build/**/*.ipa, build/App.xcarchive, build/nothing-here"
      - run: echo count=${steps.cleaned.count}
"#,
    );
    sandbox.write("build/out/App.ipa", "ipa");
    sandbox.write("build/App.xcarchive/Info.plist", "plist");
    sandbox.write("build/keep.txt", "keep");

    let run = sandbox.run(&["run", "clean"]);
    run.assert_code(0).assert_stdout_contains("count=2");
    run.assert_stderr_contains("'build/nothing-here' matched nothing");

    assert!(!sandbox.path().join("build/out/App.ipa").exists());
    assert!(!sandbox.path().join("build/App.xcarchive").exists());
    assert!(sandbox.path().join("build/keep.txt").is_file());
}

#[test]
fn clean_build_artifacts_changes_nothing_on_a_dry_run() {
    let sandbox = Sandbox::new(
        "lanes:\n  clean:\n    steps:\n      - action: clean_build_artifacts\n        with:\n          paths: build/App.ipa\n",
    );
    sandbox.write("build/App.ipa", "ipa");

    sandbox
        .run(&["run", "clean", "--dry-run"])
        .assert_code(0)
        .assert_stdout_contains("Would delete");
    assert!(sandbox.path().join("build/App.ipa").is_file());
}

#[test]
fn clean_build_artifacts_refuses_paths_outside_the_project() {
    let sandbox = Sandbox::new(
        "lanes:\n  clean:\n    steps:\n      - action: clean_build_artifacts\n        with:\n          paths: ../elsewhere\n",
    );

    sandbox
        .run(&["run", "clean"])
        .assert_code(1)
        .assert_stderr_contains("refusing to delete");
}

#[test]
fn template_render_uses_the_same_names_as_a_step() {
    let sandbox = Sandbox::new(
        r#"
env:
  APP_VERSION: "1.2.3"
lanes:
  write:
    params:
      target:
        type: string
        required: true
    steps:
      - id: first
        run: echo produced
      - action: template_render
        with:
          template: notes.tmpl
          output: notes.txt
"#,
    );
    sandbox.write(
        "notes.tmpl",
        "version=${env.APP_VERSION}\ntarget=${params.target}\nlane=${shlane.lane}\nfirst=${steps.first.stdout}\n",
    );

    sandbox.run(&["run", "write", "target=prod"]).assert_code(0);

    let out = fs::read_to_string(sandbox.path().join("notes.txt")).expect("rendered file");
    assert_eq!(
        out, "version=1.2.3\ntarget=prod\nlane=write\nfirst=produced\n",
        "rendered:\n{out}"
    );
}

#[test]
fn which_tool_checks_the_version_it_finds() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  ok:
    steps:
      - action: which_tool
        with: { name: git, min_version: "1.0" }
  missing:
    steps:
      - action: which_tool
        with: { name: definitely-not-installed-xyz }
  too_old:
    steps:
      - action: which_tool
        with: { name: git, min_version: "999.0" }
"#,
    );

    sandbox.run(&["run", "ok"]).assert_code(0);
    sandbox
        .run(&["run", "missing"])
        .assert_code(1)
        .assert_stderr_contains("is not installed, or not on PATH");
    sandbox
        .run(&["run", "too_old"])
        .assert_code(1)
        .assert_stderr_contains("needs 999.0 or newer");
}

#[test]
fn zip_and_unzip_round_trip() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  pack:
    steps:
      - action: zip
        with: { path: payload, output: payload.zip }
      - run: rm -rf payload
      - action: unzip
        with: { archive: payload.zip, into: restored }
"#,
    );
    sandbox.write("payload/one.txt", "first");
    sandbox.write("payload/nested/two.txt", "second");

    sandbox.run(&["run", "pack"]).assert_code(0);
    assert_eq!(
        fs::read_to_string(sandbox.path().join("restored/payload/nested/two.txt"))
            .expect("extracted file"),
        "second"
    );
}

#[test]
fn xcode_settings_changes_every_configuration() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  sign:
    steps:
      - action: xcode_settings
        with:
          project: App.xcodeproj
          team_id: ABCD123456
          code_sign_style: Manual
"#,
    );
    sandbox.write(
        "App.xcodeproj/project.pbxproj",
        "{\n\tbuildSettings = {\n\t\tCODE_SIGN_STYLE = Automatic;\n\t\tDEVELOPMENT_TEAM = \"\";\n\t};\n\tbuildSettings = {\n\t\tCODE_SIGN_STYLE = Automatic;\n\t\tDEVELOPMENT_TEAM = \"\";\n\t};\n}\n",
    );

    sandbox
        .run(&["run", "sign"])
        .assert_code(0)
        .assert_stdout_contains("Changed 4 setting(s)");

    let project = fs::read_to_string(sandbox.path().join("App.xcodeproj/project.pbxproj"))
        .expect("project readable");
    assert_eq!(project.matches("DEVELOPMENT_TEAM = ABCD123456;").count(), 2);
    assert_eq!(project.matches("CODE_SIGN_STYLE = Manual;").count(), 2);
}

#[test]
fn xcode_settings_warns_about_a_setting_the_project_does_not_have() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  sign:
    steps:
      - action: xcode_settings
        with:
          project: App.xcodeproj
          team_id: ABCD123456
          code_sign_identity: "iPhone Distribution"
"#,
    );
    sandbox.write(
        "App.xcodeproj/project.pbxproj",
        "{\n\tbuildSettings = {\n\t\tDEVELOPMENT_TEAM = \"\";\n\t};\n}\n",
    );

    // Silence here would mean a lane that thinks it set a signing identity
    // while the build keeps whatever was there before.
    sandbox
        .run(&["run", "sign"])
        .assert_code(0)
        .assert_stderr_contains("CODE_SIGN_IDENTITY does not appear in this project");
}

#[test]
fn call_lane_runs_another_lane_from_a_script() {
    // r##: the config contains `"#releases"`, and `"#` ends an r#"..."# string.
    let sandbox = Sandbox::new(
        r##"
lanes:
  main:
    params:
      target: { type: string, default: dev }
    steps:
      - script: |
          call_lane("notify", #{ channel: "#releases", target: param("target") });
          print("back in main");

  notify:
    private: true
    params:
      channel: { type: string, required: true }
      target: { type: string, required: true }
    steps:
      - run: echo "notify ${params.channel} for ${params.target}"
"##,
    );

    let run = sandbox.run(&["run", "main", "target=prod"]);
    run.assert_code(0)
        .assert_stdout_contains("notify #releases for prod")
        .assert_stdout_contains("back in main");

    // The called lane lands in the same summary as the caller, the way a
    // `lane:` step does.
    run.assert_stdout_contains("notify");
}

#[test]
fn call_lane_stops_a_lane_that_calls_itself() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  loop:
    steps:
      - script: |
          call_lane("loop");
"#,
    );

    let run = sandbox.run(&["run", "loop"]);
    run.assert_code(1)
        .assert_stderr_contains("lanes nested more than");

    // One line, not one wrapper per level with the reason buried at the end.
    assert_eq!(
        run.stderr.matches("nested more than").count(),
        1,
        "the nesting error should be reported once:\n{}",
        run.stderr
    );
}

#[test]
fn an_action_that_fails_inside_a_script_reports_the_action_not_the_script() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  boom:
    steps:
      - script: |
          action("sh", #{ command: "exit 3" });
"#,
    );

    sandbox
        .run(&["run", "boom"])
        .assert_code(1)
        .assert_stderr_contains("command failed with exit code 3");
}

#[test]
fn call_lane_reports_a_lane_that_does_not_exist() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  main:
    steps:
      - script: |
          call_lane("nope");
"#,
    );

    sandbox
        .run(&["run", "main"])
        .assert_code(3)
        .assert_stderr_contains("nope");
}

#[test]
fn appstore_declares_what_it_needs_before_reaching_apple() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  release:
    steps:
      - action: appstore
        with:
          bundle_id: com.example.app
"#,
    );

    // Caught by validate, so a release lane does not fail after the build.
    let run = sandbox.run(&["validate"]);
    run.assert_code(2)
        .assert_stderr_contains("action 'appstore' needs 'version'")
        .assert_stderr_contains("needs api_key or key_id, issuer_id and key");
}

#[test]
fn appstore_rejects_an_argument_it_does_not_take() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  release:
    steps:
      - action: appstore
        with:
          bundle_id: com.example.app
          version: "1.0"
          key_id: k
          issuer_id: i
          key: pem
          skip_screenshots: true
"#,
    );

    sandbox
        .run(&["validate"])
        .assert_code(2)
        .assert_stderr_contains("skip_screenshots");
}

#[test]
fn the_shell_can_be_chosen_through_the_environment() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  hello:
    steps:
      - run: echo which-shell
"#,
    );

    // The override is what makes shlane work on a machine whose POSIX shell is
    // somewhere other than /bin/sh -- Windows, where it comes with Git and is
    // found on PATH rather than at an absolute path.
    let shell = if cfg!(windows) { "bash" } else { "/bin/sh" };
    sandbox
        .run_with_env(&["run", "hello"], &[("SHLANE_SHELL", shell)])
        .assert_code(0)
        .assert_stdout_contains("which-shell");

    sandbox
        .run_with_env(
            &["run", "hello"],
            &[("SHLANE_SHELL", "/definitely/not/a/shell")],
        )
        .assert_code(5)
        .assert_stderr_contains("/definitely/not/a/shell");
}

#[test]
fn a_value_substituted_into_the_sh_action_is_escaped_like_a_run_step() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  inject:
    params:
      evil:
        type: string
        default: "x; touch PWNED"
    steps:
      - action: sh
        with:
          command: echo ${params.evil}
"#,
    );

    // `run:` escapes what it substitutes; an argument the action runs as a
    // shell command has to do the same, or the guarantee depends on which of
    // the two spellings someone used.
    sandbox
        .run(&["run", "inject"])
        .assert_code(0)
        .assert_stdout_contains("x; touch PWNED");

    assert!(
        !sandbox.path().join("PWNED").exists(),
        "the value was re-read by the shell"
    );
}

#[test]
fn an_action_argument_that_is_not_a_command_is_substituted_literally() {
    let sandbox = Sandbox::new(
        r#"
lanes:
  copy:
    params:
      dir:
        type: string
        default: "a dir with spaces"
    steps:
      - action: copy_artifacts
        with:
          paths: "src/one.txt"
          into: ${params.dir}
"#,
    );
    sandbox.write("src/one.txt", "x");

    // Quoting a path would put the quotes in the path.
    sandbox.run(&["run", "copy"]).assert_code(0);
    assert!(
        sandbox.path().join("a dir with spaces/one.txt").is_file(),
        "the directory should be named without quotes"
    );
}

#[test]
fn plugin_verify_reports_what_a_plugin_said_before_it_died() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);
    // A plugin that fails on startup, the way a missing interpreter or an
    // unreadable credential would.
    sandbox.write(
        "tools/line-notify/notify.sh",
        "#!/bin/sh\necho 'cannot reach the API: no token' >&2\nexit 7\n",
    );

    // "did not answer describe" on its own says nothing about why. Running a
    // plugin relays its stderr; verify has to as well.
    sandbox
        .run(&["plugin", "verify"])
        .assert_code(2)
        .assert_stderr_contains("did not answer `describe` (exit code 7)")
        .assert_stderr_contains("cannot reach the API: no token");
}

#[test]
fn plugin_verify_reports_stdout_that_was_not_a_protocol_event() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);
    // A plugin that answers, but not in the protocol -- a stray print, a stack
    // trace, a JSON library writing something else.
    sandbox.write(
        "tools/line-notify/notify.sh",
        "#!/bin/sh
echo 'this is not json'
exit 1
",
    );

    sandbox
        .run(&["plugin", "verify"])
        .assert_code(2)
        .assert_stderr_contains("this is not json");
}

#[test]
fn plugin_verify_says_so_when_a_plugin_prints_nothing_at_all() {
    let sandbox = Sandbox::new(PLUGIN_CONFIG);
    write_plugin(&sandbox, "tools/line-notify", None);
    sandbox.write(
        "tools/line-notify/notify.sh",
        "#!/bin/sh
exit 1
",
    );

    // Silence is itself the finding, and worth saying out loud rather than
    // leaving the reader to wonder what was trimmed.
    sandbox
        .run(&["plugin", "verify"])
        .assert_code(2)
        .assert_stderr_contains("and printed nothing");
}

#[test]
fn action_run_prints_a_single_output_bare_without_a_config() {
    let sandbox = Sandbox::empty();
    let pem = "key=-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----";

    let run = sandbox.run(&[
        "action",
        "run",
        "asc_api_key",
        "key_id=K",
        "issuer_id=I",
        pem,
    ]);
    run.assert_code(0);
    let value = run.stdout.trim_end();
    assert!(!value.contains('\n'), "one line, got {value:?}");
    assert!(
        value.starts_with("eyJ"),
        "base64 of a JSON object, got {value:?}"
    );

    fs::write(sandbox.path.join("k.p8"), &pem["key=".len()..]).expect("key should be writable");
    let from_path = sandbox.run(&[
        "action",
        "run",
        "asc_api_key",
        "key_id=K",
        "issuer_id=I",
        "key_path=k.p8",
    ]);
    from_path.assert_code(0);
    assert_eq!(
        from_path.stdout, run.stdout,
        "key_path and key give the same api_key"
    );

    sandbox
        .run(&[
            "action",
            "run",
            "asc_api_key",
            "key_id=K",
            "issuer_id=I",
            "key_path=missing.p8",
        ])
        .assert_code(1)
        .assert_stderr_contains("cannot read");
    sandbox
        .run(&["action", "run", "asc_api_key", "key_id=K"])
        .assert_code(1)
        .assert_stderr_contains("needs 'issuer_id'")
        .assert_stderr_contains("needs key or key_path");
}
