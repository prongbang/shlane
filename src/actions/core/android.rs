//! Android actions (`docs/plan/08-actions-android.md`).
//!
//! Everything that decides *what* to run is a plain function, so it can be
//! tested without an Android SDK; only the spawning needs one.

use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use crate::runtime::secrets::is_sensitive_name;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Parse `key=value` lines, as used by `properties:` and `flags:`.
fn parse_pairs(raw: &str) -> Vec<(String, String)> {
    raw.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .filter(|(key, _)| !key.is_empty())
        .collect()
}

fn split_flags(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_string).collect()
}

/// `prod` + `release` -> `bundleProdRelease`.
fn gradle_task(prefix: &str, flavor: Option<&str>, build_type: &str, suffix: &str) -> String {
    let mut task = String::from(prefix);
    if let Some(flavor) = flavor.filter(|flavor| !flavor.is_empty()) {
        task.push_str(&capitalize(flavor));
    }
    task.push_str(&capitalize(build_type));
    task.push_str(suffix);
    task
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Build the gradle command line.
///
/// A property whose name looks sensitive is left out and passed through the
/// environment instead: anything on a command line is visible in `ps` and in
/// most CI logs. Callers must also register those values as secrets --
/// [`gradle_env_secrets`] returns them -- or the value shlane went out of its
/// way to keep off the command line still appears in gradle's own output.
fn gradle_command(
    exe: &str,
    tasks: &[String],
    properties: &[(String, String)],
    flags: &[String],
) -> (String, BTreeMap<String, String>) {
    let mut command = vec![exe.to_string()];
    let mut env = BTreeMap::new();

    for task in tasks {
        command.push(quote(task));
    }
    for (key, value) in properties {
        if is_sensitive_name(key) {
            env.insert(format!("ORG_GRADLE_PROJECT_{key}"), value.clone());
        } else {
            command.push(format!("-P{key}={}", quote(value)));
        }
    }
    for flag in flags {
        command.push(flag.clone());
    }

    (command.join(" "), env)
}

/// The property values that [`gradle_command`] routed through the environment,
/// which are by definition the sensitive ones.
fn gradle_env_secrets(properties: &[(String, String)]) -> Vec<String> {
    properties
        .iter()
        .filter(|(key, _)| is_sensitive_name(key))
        .map(|(_, value)| value.clone())
        .collect()
}

/// `./gradlew` if the project has a wrapper, otherwise whatever is on PATH.
fn gradle_executable(project_dir: &Path, wrapper: bool) -> String {
    if wrapper && project_dir.join("gradlew").is_file() {
        return "./gradlew".to_string();
    }
    "gradle".to_string()
}

fn project_dir(ctx: &ActionContext<'_>, args: &Args) -> PathBuf {
    match args.get("project_dir") {
        // The default is ".", and joining it produces paths full of "/./".
        Some(dir) if dir != "." => ctx.workdir().join(dir),
        _ => ctx.workdir().to_path_buf(),
    }
}

/// Find files under `root` with one of `extensions`, newest first.
fn find_artifacts(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(root, extensions, &mut found, 0);
    found.sort_by_key(|path| {
        std::cmp::Reverse(
            fs::metadata(path)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });
    found
}

fn collect(dir: &Path, extensions: &[&str], found: &mut Vec<PathBuf>, depth: usize) {
    // Deep enough for build/outputs/apk/<flavor>/<type>/, shallow enough not to
    // walk an entire monorepo.
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, extensions, found, depth + 1);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.contains(&ext))
        {
            found.push(path);
        }
    }
}

pub struct Gradle;

impl Action for Gradle {
    fn name(&self) -> &'static str {
        "gradle"
    }

    fn description(&self) -> &'static str {
        "Run a Gradle task"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("task", "Task to run, or several separated by spaces").required(),
            ArgSpec::new("project_dir", "Where the build lives").default("."),
            ArgSpec::new(
                "properties",
                "One `key=value` per line; a sensitive name is passed through the environment",
            ),
            ArgSpec::new("flags", "Extra gradle flags").default("--no-daemon"),
            ArgSpec::new("wrapper", "Use ./gradlew when the project has one").default("true"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let dir = project_dir(ctx, args);
        let exe = gradle_executable(&dir, args.flag("wrapper"));
        let tasks: Vec<String> = args
            .get_or("task", "")
            .split_whitespace()
            .map(str::to_string)
            .collect();

        if tasks.is_empty() {
            return Err(ctx.error(self.name(), "no task given"));
        }

        let properties = parse_pairs(args.get_or("properties", ""));
        for value in gradle_env_secrets(&properties) {
            ctx.mark_secret(&value);
        }

        let (command, env) = gradle_command(
            &exe,
            &tasks,
            &properties,
            &split_flags(args.get_or("flags", "--no-daemon")),
        );

        let previous = std::mem::replace(&mut ctx.workdir, dir);
        let result = ctx.require_with_env(&command, &env);
        ctx.workdir = previous;
        result?;

        Ok(ActionOutput::new().with("task", tasks.join(" ")))
    }
}

pub struct BuildAndroid;

impl Action for BuildAndroid {
    fn name(&self) -> &'static str {
        "build_android"
    }

    fn description(&self) -> &'static str {
        "Assemble an APK or an app bundle"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("format", "aab or apk").default("aab"),
            ArgSpec::new("build_type", "release, debug, ...").default("release"),
            ArgSpec::new("flavor", "Product flavor, if the project has any"),
            ArgSpec::new("project_dir", "Where the build lives").default("."),
            ArgSpec::new("properties", "One `key=value` per line"),
            ArgSpec::new("flags", "Extra gradle flags").default("--no-daemon"),
            ArgSpec::new("wrapper", "Use ./gradlew when the project has one").default("true"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let format = args.get_or("format", "aab");
        let (prefix, extension) = match format {
            "aab" => ("bundle", "aab"),
            "apk" => ("assemble", "apk"),
            other => {
                return Err(ctx.error(
                    self.name(),
                    format!("unknown format '{other}'; use aab or apk"),
                ))
            }
        };

        let task = gradle_task(
            prefix,
            args.get("flavor"),
            args.get_or("build_type", "release"),
            "",
        );
        let dir = project_dir(ctx, args);
        let exe = gradle_executable(&dir, args.flag("wrapper"));
        let properties = parse_pairs(args.get_or("properties", ""));
        for value in gradle_env_secrets(&properties) {
            ctx.mark_secret(&value);
        }

        let (command, env) = gradle_command(
            &exe,
            std::slice::from_ref(&task),
            &properties,
            &split_flags(args.get_or("flags", "--no-daemon")),
        );

        ctx.ui.say(&format!("Gradle task {task}"));

        let previous = std::mem::replace(&mut ctx.workdir, dir.clone());
        let result = ctx.require_with_env(&command, &env);
        ctx.workdir = previous;
        result?;

        if ctx.dry_run {
            return Ok(ActionOutput::new().with("task", task));
        }

        // Gradle does not say where it put things, so look.
        let artifacts = find_artifacts(&dir, &[extension]);
        let Some(newest) = artifacts.first() else {
            return Err(ctx.error(
                self.name(),
                format!(
                    "{task} succeeded but no .{extension} was found under {}",
                    dir.display()
                ),
            ));
        };

        ctx.ui.say(&format!("Built {}", newest.display()));

        let mapping = find_artifacts(&dir, &["txt"])
            .into_iter()
            .find(|path| path.ends_with("mapping.txt"))
            .map(|path| path.display().to_string())
            .unwrap_or_default();

        Ok(ActionOutput::new()
            .with("task", task)
            .with(extension, newest.display().to_string())
            .with("path", newest.display().to_string())
            .with("mapping", mapping))
    }
}

pub struct TestAndroid;

impl Action for TestAndroid {
    fn name(&self) -> &'static str {
        "test_android"
    }

    fn description(&self) -> &'static str {
        "Run the unit tests and collect their reports"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("build_type", "release, debug, ...").default("debug"),
            ArgSpec::new("flavor", "Product flavor, if the project has any"),
            ArgSpec::new("project_dir", "Where the build lives").default("."),
            ArgSpec::new("flags", "Extra gradle flags").default("--no-daemon"),
            ArgSpec::new("wrapper", "Use ./gradlew when the project has one").default("true"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let task = match args.get("flavor") {
            Some(_) => gradle_task(
                "test",
                args.get("flavor"),
                args.get_or("build_type", "debug"),
                "UnitTest",
            ),
            None => "test".to_string(),
        };

        let dir = project_dir(ctx, args);
        let exe = gradle_executable(&dir, args.flag("wrapper"));
        let (command, env) = gradle_command(
            &exe,
            std::slice::from_ref(&task),
            &[],
            &split_flags(args.get_or("flags", "--no-daemon")),
        );

        let previous = std::mem::replace(&mut ctx.workdir, dir.clone());
        let result = ctx.require_with_env(&command, &env);
        ctx.workdir = previous;
        result?;

        let reports: Vec<String> = find_artifacts(&dir, &["xml"])
            .into_iter()
            .filter(|path| path.to_string_lossy().contains("test-results"))
            .map(|path| path.display().to_string())
            .collect();

        ctx.ui
            .say(&format!("{} test report file(s)", reports.len()));

        Ok(ActionOutput::new()
            .with("task", task)
            .with("reports", reports.join("\n"))
            .with("report_count", reports.len().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_task_names_from_flavor_and_type() {
        assert_eq!(
            gradle_task("bundle", Some("prod"), "release", ""),
            "bundleProdRelease"
        );
        assert_eq!(gradle_task("assemble", None, "debug", ""), "assembleDebug");
        assert_eq!(
            gradle_task("test", Some("free"), "debug", "UnitTest"),
            "testFreeDebugUnitTest"
        );
        assert_eq!(
            gradle_task("bundle", Some(""), "release", ""),
            "bundleRelease"
        );
    }

    #[test]
    fn quotes_task_and_property_values() {
        let (command, _) = gradle_command(
            "./gradlew",
            &["assembleRelease".to_string()],
            &[("version".to_string(), "1.2 3".to_string())],
            &["--no-daemon".to_string()],
        );
        assert_eq!(
            command,
            "./gradlew 'assembleRelease' -Pversion='1.2 3' --no-daemon"
        );
    }

    #[test]
    fn sensitive_properties_go_through_the_environment() {
        let (command, env) = gradle_command(
            "gradle",
            &["publish".to_string()],
            &[
                ("SIGNING_PASSWORD".to_string(), "hunter2".to_string()),
                ("flavor".to_string(), "prod".to_string()),
            ],
            &[],
        );
        assert!(
            !command.contains("hunter2"),
            "a secret reached the command line: {command}"
        );
        assert_eq!(
            env.get("ORG_GRADLE_PROJECT_SIGNING_PASSWORD")
                .map(String::as_str),
            Some("hunter2")
        );
        assert!(command.contains("-Pflavor='prod'"), "{command}");
    }

    #[test]
    fn env_routed_properties_are_reported_for_masking() {
        let properties = vec![
            ("SIGNING_PASSWORD".to_string(), "hunter2".to_string()),
            ("flavor".to_string(), "prod".to_string()),
        ];
        assert_eq!(gradle_env_secrets(&properties), vec!["hunter2".to_string()]);
    }

    #[test]
    fn parses_property_lines() {
        let pairs = parse_pairs("a=1\nb = two\n\nnonsense\n");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[1], ("b".to_string(), "two".to_string()));
    }

    #[test]
    fn prefers_the_wrapper_when_there_is_one() {
        let dir = std::env::temp_dir().join(format!("shlane-gradle-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        assert_eq!(gradle_executable(&dir, true), "gradle");
        fs::write(dir.join("gradlew"), "#!/bin/sh\n").expect("writable");
        assert_eq!(gradle_executable(&dir, true), "./gradlew");
        assert_eq!(gradle_executable(&dir, false), "gradle");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_artifacts_by_extension() {
        let dir = std::env::temp_dir().join(format!("shlane-art-{}", std::process::id()));
        let nested = dir.join("build/outputs/bundle/release");
        fs::create_dir_all(&nested).expect("creatable");
        fs::write(nested.join("app.aab"), "x").expect("writable");
        fs::write(nested.join("ignored.txt"), "x").expect("writable");

        let found = find_artifacts(&dir, &["aab"]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].ends_with("app.aab"));

        let _ = fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------

/// A file that deletes itself, so a keystore written from a CI secret does not
/// outlive the step that needed it -- including when the step fails.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn write(dir: &Path, name: &str, contents: &[u8]) -> std::io::Result<Self> {
        let path = dir.join(name);
        fs::write(&path, contents)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(Self { path })
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Decode standard base64, so a keystore can travel in an environment variable.
fn decode_base64(text: &str) -> std::result::Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut buffer: u32 = 0;
    let mut bits = 0;
    let mut out = Vec::new();

    for byte in text.bytes() {
        if byte.is_ascii_whitespace() || byte == b'=' {
            continue;
        }
        let Some(value) = ALPHABET.iter().position(|candidate| *candidate == byte) else {
            return Err(format!("'{}' is not base64", byte as char));
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    Ok(out)
}

/// Newest `build-tools/<version>/<tool>` under `$ANDROID_HOME`.
fn android_tool(env: &BTreeMap<String, String>, tool: &str) -> Option<PathBuf> {
    let home = env
        .get("ANDROID_HOME")
        .or_else(|| env.get("ANDROID_SDK_ROOT"))?;
    let build_tools = Path::new(home).join("build-tools");

    let mut versions: Vec<PathBuf> = fs::read_dir(&build_tools)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join(tool).is_file())
        .collect();

    versions.sort();
    versions.pop().map(|version| version.join(tool))
}

pub struct SignAndroid;

impl Action for SignAndroid {
    fn name(&self) -> &'static str {
        "sign_android"
    }

    fn description(&self) -> &'static str {
        "Sign an APK or app bundle with a keystore"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("input", "File to sign").required(),
            ArgSpec::new(
                "output",
                "Where to write the signed file; defaults to in place",
            ),
            ArgSpec::new("keystore", "Path to the keystore"),
            ArgSpec::new("keystore_base64", "The keystore itself, base64 encoded").sensitive(),
            ArgSpec::new("keystore_password", "Keystore password")
                .required()
                .sensitive(),
            ArgSpec::new("key_alias", "Key alias inside the keystore").required(),
            ArgSpec::new(
                "key_password",
                "Key password; defaults to the keystore password",
            )
            .sensitive(),
            ArgSpec::new("zipalign", "Align an APK before signing").default("true"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let input = ctx.workdir().join(args.get_or("input", ""));
        let is_bundle = input.extension().is_some_and(|ext| ext == "aab");

        // Passwords go through the environment: apksigner reads `env:NAME`, and
        // a command line is visible in `ps` and in most CI logs.
        let keystore_password = args.get_or("keystore_password", "");
        let key_password = args.get_or("key_password", keystore_password);
        let mut env = BTreeMap::new();
        env.insert("SHLANE_KS_PASS".to_string(), keystore_password.to_string());
        env.insert("SHLANE_KEY_PASS".to_string(), key_password.to_string());

        // Keep the decoded keystore next to the artifact and remove it after.
        let _temporary;
        let keystore = match (args.get("keystore"), args.get("keystore_base64")) {
            (Some(path), _) => ctx.workdir().join(path),
            (None, Some(encoded)) => {
                let bytes = decode_base64(encoded).map_err(|message| {
                    ctx.error(self.name(), format!("keystore_base64: {message}"))
                })?;
                let file = TempFile::write(ctx.workdir(), ".shlane-keystore.jks", &bytes).map_err(
                    |err| ctx.error(self.name(), format!("cannot write keystore: {err}")),
                )?;
                let path = file.path.clone();
                _temporary = file;
                path
            }
            (None, None) => {
                return Err(ctx.error(self.name(), "give either keystore or keystore_base64"))
            }
        };

        let alias = args.get_or("key_alias", "");
        let output = match args.get("output") {
            Some(output) => ctx.workdir().join(output),
            None => input.clone(),
        };

        if is_bundle {
            // Bundles are jar-signed; apksigner only handles APKs.
            let command = format!(
                "jarsigner -keystore {} -storepass:env SHLANE_KS_PASS -keypass:env SHLANE_KEY_PASS -signedjar {} {} {}",
                quote(&keystore.display().to_string()),
                quote(&output.display().to_string()),
                quote(&input.display().to_string()),
                quote(alias)
            );
            ctx.require_with_env(&command, &env)?;
            return Ok(ActionOutput::new().with("path", output.display().to_string()));
        }

        let mut to_sign = input.clone();
        // Removed when this returns, signed or not: it is an intermediate, and
        // left behind it is one more .apk for the next glob to pick up.
        let _aligned;
        if args.flag("zipalign") && !ctx.dry_run {
            let zipalign = android_tool(ctx.env, "zipalign")
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "zipalign".to_string());
            let aligned = input.with_extension("aligned.apk");
            _aligned = TempFile {
                path: aligned.clone(),
            };
            ctx.require(&format!(
                "{} -p -f 4 {} {}",
                quote(&zipalign),
                quote(&input.display().to_string()),
                quote(&aligned.display().to_string())
            ))?;
            to_sign = aligned;
        }

        let apksigner = android_tool(ctx.env, "apksigner")
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "apksigner".to_string());

        let command = format!(
            "{} sign --ks {} --ks-pass env:SHLANE_KS_PASS --ks-key-alias {} --key-pass env:SHLANE_KEY_PASS --out {} {}",
            quote(&apksigner),
            quote(&keystore.display().to_string()),
            quote(alias),
            quote(&output.display().to_string()),
            quote(&to_sign.display().to_string())
        );
        ctx.require_with_env(&command, &env)?;

        ctx.ui.say(&format!("Signed {}", output.display()));
        Ok(ActionOutput::new().with("path", output.display().to_string()))
    }
}

#[cfg(test)]
mod signing_tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        assert_eq!(decode_base64("aGVsbG8=").expect("valid"), b"hello");
        assert_eq!(
            decode_base64("aGVs\nbG8=").expect("whitespace is ignored"),
            b"hello"
        );
        assert!(decode_base64("not base64!").is_err());
    }

    #[test]
    fn a_temporary_keystore_is_removed() {
        let dir = std::env::temp_dir().join(format!("shlane-ks-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("creatable");
        let path = {
            let file = TempFile::write(&dir, "keystore.jks", b"secret").expect("writable");
            assert!(file.path.is_file());
            file.path.clone()
        };
        assert!(!path.exists(), "the keystore outlived its guard");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_the_newest_build_tools() {
        let home = std::env::temp_dir().join(format!("shlane-sdk-{}", std::process::id()));
        for version in ["30.0.3", "34.0.0"] {
            let dir = home.join("build-tools").join(version);
            fs::create_dir_all(&dir).expect("creatable");
            fs::write(dir.join("apksigner"), "#!/bin/sh\n").expect("writable");
        }
        let env = BTreeMap::from([("ANDROID_HOME".to_string(), home.display().to_string())]);

        let found = android_tool(&env, "apksigner").expect("should find one");
        assert!(found.to_string_lossy().contains("34.0.0"), "{found:?}");
        assert!(android_tool(&BTreeMap::new(), "apksigner").is_none());

        let _ = fs::remove_dir_all(&home);
    }
}
