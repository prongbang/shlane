#[cfg(unix)]
#[test]
fn github_action_passes_shell_metacharacters_as_input_data() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let temp = std::env::temp_dir().join(format!("shlane-action-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&temp).expect("temporary directory");
    let shim = temp.join("shlane");
    let capture = temp.join("argv");
    let side_effect = temp.join("injected");
    fs::write(&shim, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE\"\n")
        .expect("write shlane shim");
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).expect("make shim executable");

    let action: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../action.yml")).expect("action metadata is valid YAML");
    let steps = action["runs"]["steps"]
        .as_sequence()
        .expect("composite steps");
    let run = steps
        .iter()
        .find(|step| step["name"] == "Run the lane")
        .and_then(|step| step["run"].as_str())
        .expect("lane step script");
    assert!(
        !run.contains("${{"),
        "input expressions belong in env, not run source"
    );

    let result = Command::new("bash")
        .arg("-c")
        .arg(run)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", temp.display()))
        .env("CAPTURE", &capture)
        .env("SHLANE_LANE", "build; touch")
        .env(
            "SHLANE_PARAMS",
            format!("target=qa; touch {}", side_effect.display()),
        )
        .env(
            "SHLANE_ARGS",
            "--report 'junit:reports/a b.xml' --file \"C:\\reports\\result.xml\"",
        )
        .output()
        .expect("run action step");

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!side_effect.exists(), "shell metacharacters were executed");
    let argv: Vec<_> = fs::read_to_string(&capture)
        .expect("shim captured arguments")
        .lines()
        .map(str::to_string)
        .collect();
    let expected = vec![
        "run".to_string(),
        "build; touch".to_string(),
        "target=qa;".to_string(),
        "touch".to_string(),
        side_effect.display().to_string(),
        "--report".to_string(),
        "junit:reports/a b.xml".to_string(),
        "--file".to_string(),
        "C:\\reports\\result.xml".to_string(),
    ];
    assert_eq!(argv, expected);

    fs::remove_dir_all(temp).expect("remove temporary directory");
}
