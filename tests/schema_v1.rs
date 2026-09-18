//! The schema v1 freeze (`docs/schema-v1.md`).
//!
//! These tests exist so the document and the code cannot drift apart quietly.
//! A key that stops being accepted, or one that appears without being written
//! down, fails here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Every key the document lists, in one config.
const EVERYTHING: &str = r##"
version: 1
min_shlane: "0.1.0"

env:
  APP_ENV: production
  A_SECRET: hunter2
env_files:
  - .env
  - .env.defaults
secrets:
  - ${env.A_SECRET}

script: |-
  fn shared_helper(name) { name + "!" }

before_all:
  - name: before everything
    run: echo before-all
after_all:
  - run: echo after-all
error:
  - run: echo something-failed

lanes:
  release:
    description: "Everything the schema allows"
    platform: ios
    private: false
    params:
      target:
        type: string
        required: true
        values: [staging, production]
        description: "Where to deploy"
      count:
        type: int
        default: 3
      verbose:
        type: bool
        default: false
    env:
      LANE_ONLY: yes-it-is
    before:
      - run: echo before
    steps:
      - name: a command
        id: first
        run: echo hello ${params.target}
        workdir: .
        timeout: 30s
        retry: 1
        continue_on_error: false
        env:
          STEP_ONLY: "1"

      - name: an action
        id: second
        action: sh
        with:
          command: echo ${steps.first.stdout}
        if: param("target") == "production"

      - name: some rhai
        script: |
          print(shared_helper("done"));

      - name: another lane
        lane: notify
        with:
          channel: "#releases"
    after:
      - run: echo after
    script: |
      print("lane script");

  notify:
    private: true
    params:
      channel:
        type: string
        required: true
    steps:
      - run: echo ${params.channel}
"##;

struct Sandbox {
    path: PathBuf,
}

impl Sandbox {
    fn new(config: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "shlane-schema-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("sandbox");
        fs::write(path.join("shlane.yaml"), config).expect("config");
        Self { path }
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        // The runner's own CI variables would otherwise reach the shlane under
        // test and change what it prints.
        let mut command = Command::new(env!("CARGO_BIN_EXE_shlane"));
        command
            .args(args)
            .current_dir(&self.path)
            .env_remove("SHLANE_CONFIG")
            .env_remove("SHLANE_SHELL");
        for name in [
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
        ] {
            command.env_remove(name);
        }
        let output = command.output().expect("shlane should run");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn every_documented_key_is_accepted() {
    let sandbox = Sandbox::new(EVERYTHING);
    let (code, out, err) = sandbox.run(&["validate"]);
    assert_eq!(code, 0, "the documented schema must validate\n{out}\n{err}");
}

#[test]
fn a_config_using_every_key_actually_runs() {
    let sandbox = Sandbox::new(EVERYTHING);
    let (code, out, err) = sandbox.run(&["run", "release", "target=production"]);
    assert_eq!(code, 0, "stdout:\n{out}\nstderr:\n{err}");
    assert!(out.contains("before-all"), "{out}");
    assert!(out.contains("hello production"), "{out}");
    assert!(
        out.contains("done!"),
        "the shared script's function ran:\n{out}"
    );
    assert!(out.contains("#releases"), "the called lane ran:\n{out}");
    assert!(out.contains("after-all"), "{out}");
}

#[test]
fn a_key_that_is_not_in_the_document_is_refused() {
    // Silently ignoring it is how a lane stops doing something without anyone
    // noticing.
    for config in [
        "lanes:\n  a:\n    steps:\n      - run: echo hi\n    unknown_lane_key: 1\n",
        "lanes:\n  a:\n    steps:\n      - run: echo hi\n        unknown_step_key: 1\n",
        "unknown_top_key: 1\nlanes:\n  a:\n    steps:\n      - run: echo hi\n",
        "lanes:\n  a:\n    params:\n      p:\n        unknown_param_key: 1\n    steps:\n      - run: echo hi\n",
    ] {
        let sandbox = Sandbox::new(config);
        let (code, _, err) = sandbox.run(&["validate"]);
        assert_ne!(code, 0, "this should have been refused:\n{config}");
        assert!(
            err.contains("unknown"),
            "the error should name the key:\n{err}"
        );
    }
}

#[test]
fn only_schema_version_one_is_accepted() {
    let sandbox = Sandbox::new("version: 2\nlanes:\n  a:\n    steps:\n      - run: echo hi\n");
    let (code, _, err) = sandbox.run(&["validate"]);
    assert_ne!(code, 0);
    assert!(err.contains("version 2 is not supported"), "{err}");
}

#[test]
fn the_document_lists_every_key_the_code_accepts() {
    // Reading the source rather than the document: a key added to the model
    // without a line in docs/schema-v1.md is the drift this is here to catch.
    let document = fs::read_to_string(document_path()).expect("docs/schema-v1.md");
    let model =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/config/model.rs"))
            .expect("src/config/model.rs");

    let mut missing = Vec::new();
    for line in model.lines() {
        let Some(field) = declared_field(line) else {
            continue;
        };
        // `kind` is how a step's run/action/script/lane is stored, `param_type`
        // is written as `type`, and `condition` is written as `if`.
        if matches!(field, "kind" | "param_type" | "condition") {
            continue;
        }
        if !document.contains(&format!("`{field}`")) {
            missing.push(field.to_string());
        }
    }

    assert!(
        missing.is_empty(),
        "these keys are accepted but not in docs/schema-v1.md: {}",
        missing.join(", ")
    );
}

/// The name of a field declared on a deserialized struct, if this line is one.
fn declared_field(line: &str) -> Option<&str> {
    let line = line.trim();
    let line = line.strip_prefix("pub ").unwrap_or(line);
    let (name, rest) = line.split_once(':')?;
    if !rest.trim_end().ends_with(',') {
        return None;
    }
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    Some(name)
}

fn document_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/schema-v1.md")
}
