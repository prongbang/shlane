//! iOS actions (`docs/plan/07-actions-ios.md`).
//!
//! What to run is decided by plain functions, which are tested here; running
//! it needs Xcode, so the round trip is e2e work on a macOS runner
//! (`docs/plan/13-testing-and-quality.md`).

use crate::actions::asc::{token, ApiKey};
use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// `-workspace X` or `-project Y`, whichever the config gave.
fn project_flag(args: &Args) -> Option<String> {
    if let Some(workspace) = args.get("workspace") {
        return Some(format!("-workspace {}", quote(workspace)));
    }
    args.get("project")
        .map(|project| format!("-project {}", quote(project)))
}

/// `xcodebuild archive ...`
fn archive_command(args: &Args, archive_path: &str) -> String {
    let mut parts = vec!["xcodebuild".to_string()];
    if args.flag("clean") {
        parts.push("clean".to_string());
    }
    parts.push("archive".to_string());

    if let Some(flag) = project_flag(args) {
        parts.push(flag);
    }
    parts.push(format!("-scheme {}", quote(args.get_or("scheme", ""))));
    parts.push(format!(
        "-configuration {}",
        quote(args.get_or("configuration", "Release"))
    ));
    parts.push(format!(
        "-destination {}",
        quote(args.get_or("destination", "generic/platform=iOS"))
    ));
    parts.push(format!("-archivePath {}", quote(archive_path)));

    if let Some(sdk) = args.get("sdk") {
        parts.push(format!("-sdk {}", quote(sdk)));
    }
    if let Some(team) = args.get("team_id") {
        parts.push(format!("DEVELOPMENT_TEAM={}", quote(team)));
    }
    if args.flag("allow_provisioning_updates") {
        parts.push("-allowProvisioningUpdates".to_string());
    }
    if let Some(extra) = args.get("xcargs") {
        parts.push(extra.to_string());
    }

    parts.join(" ")
}

/// `xcodebuild -exportArchive ...`
fn export_command(archive_path: &str, options_path: &str, output_dir: &str, args: &Args) -> String {
    let mut parts = vec![
        "xcodebuild".to_string(),
        "-exportArchive".to_string(),
        format!("-archivePath {}", quote(archive_path)),
        format!("-exportOptionsPlist {}", quote(options_path)),
        format!("-exportPath {}", quote(output_dir)),
    ];
    if args.flag("allow_provisioning_updates") {
        parts.push("-allowProvisioningUpdates".to_string());
    }
    parts.join(" ")
}

/// The plist `-exportArchive` insists on.
///
/// Writing this by hand is the part of `gym` people do not realise they are
/// getting until they try to do without it.
fn export_options_plist(args: &Args) -> String {
    let mut entries = vec![(
        "method".to_string(),
        args.get_or("export_method", "app-store").to_string(),
    )];

    if let Some(team) = args.get("team_id") {
        entries.push(("teamID".to_string(), team.to_string()));
    }

    let mut body = String::new();
    for (key, value) in &entries {
        body.push_str(&format!(
            "\t<key>{}</key>\n\t<string>{}</string>\n",
            escape_xml(key),
            escape_xml(value)
        ));
    }

    // Booleans have their own tags, so they cannot go through the loop above.
    body.push_str(&format!(
        "\t<key>uploadSymbols</key>\n\t<{}/>\n",
        if args.flag("upload_symbols") {
            "true"
        } else {
            "false"
        }
    ));
    body.push_str(&format!(
        "\t<key>uploadBitcode</key>\n\t<{}/>\n",
        if args.flag("upload_bitcode") {
            "true"
        } else {
            "false"
        }
    ));
    if args.flag("allow_provisioning_updates") {
        body.push_str("\t<key>signingStyle</key>\n\t<string>automatic</string>\n");
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The first file with `extension` directly inside `dir`.
fn first_with_extension(dir: &Path, extension: &str) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == extension))
}

fn common_schema() -> Vec<ArgSpec> {
    vec![
        ArgSpec::new("workspace", "Path to the .xcworkspace"),
        ArgSpec::new(
            "project",
            "Path to the .xcodeproj, if there is no workspace",
        ),
        ArgSpec::new("scheme", "Scheme to build").required(),
        ArgSpec::new("configuration", "Build configuration").default("Release"),
        ArgSpec::new("destination", "xcodebuild -destination").default("generic/platform=iOS"),
        ArgSpec::new("sdk", "xcodebuild -sdk"),
        ArgSpec::new("team_id", "Apple developer team id"),
        ArgSpec::new("xcargs", "Anything else to append to the xcodebuild call"),
    ]
}

pub struct BuildIos;

impl Action for BuildIos {
    fn name(&self) -> &'static str {
        "build_ios"
    }

    fn description(&self) -> &'static str {
        "Archive and export an iOS app (needs Xcode)"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        let mut schema = common_schema();
        schema.extend([
            ArgSpec::new(
                "export_method",
                "app-store, ad-hoc, development or enterprise",
            )
            .default("app-store"),
            ArgSpec::new("output_dir", "Where to put the archive and the .ipa").default("build"),
            ArgSpec::new("clean", "Clean before archiving").default("false"),
            ArgSpec::new(
                "allow_provisioning_updates",
                "Let Xcode manage signing, rather than a synced certificate store",
            )
            .default("true"),
            ArgSpec::new("upload_symbols", "Include dSYMs in the export").default("true"),
            ArgSpec::new("upload_bitcode", "Include bitcode in the export").default("false"),
        ]);
        schema
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        if args.get("workspace").is_none() && args.get("project").is_none() {
            return Err(ctx.error(self.name(), "give either workspace or project"));
        }

        let scheme = args.get_or("scheme", "");
        let output_dir = ctx.workdir().join(args.get_or("output_dir", "build"));
        let archive = output_dir.join(format!("{scheme}.xcarchive"));
        let options = output_dir.join("ExportOptions.plist");

        let archive_command = archive_command(args, &archive.display().to_string());
        let export_command = export_command(
            &archive.display().to_string(),
            &options.display().to_string(),
            &output_dir.display().to_string(),
            args,
        );

        if ctx.dry_run {
            ctx.ui.say(&format!("Would write {}", options.display()));
            ctx.ui.say(&format!("Would run: {archive_command}"));
            ctx.ui.say(&format!("Would run: {export_command}"));
            return Ok(ActionOutput::new().with("archive", archive.display().to_string()));
        }

        fs::create_dir_all(&output_dir).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot create {}: {err}", output_dir.display()),
            )
        })?;
        fs::write(&options, export_options_plist(args)).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot write {}: {err}", options.display()),
            )
        })?;

        ctx.require(&archive_command)?;
        ctx.require(&export_command)?;

        let Some(ipa) = first_with_extension(&output_dir, "ipa") else {
            return Err(ctx.error(
                self.name(),
                format!(
                    "the export succeeded but no .ipa appeared in {}",
                    output_dir.display()
                ),
            ));
        };

        ctx.ui.say(&format!("Exported {}", ipa.display()));

        let dsyms = archive.join("dSYMs");
        Ok(ActionOutput::new()
            .with("ipa", ipa.display().to_string())
            .with("archive", archive.display().to_string())
            .with(
                "dsym",
                if dsyms.is_dir() {
                    dsyms.display().to_string()
                } else {
                    String::new()
                },
            ))
    }
}

pub struct TestIos;

impl Action for TestIos {
    fn name(&self) -> &'static str {
        "test_ios"
    }

    fn description(&self) -> &'static str {
        "Run the test suite in a simulator (needs Xcode)"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        let mut schema = common_schema();
        // Tests run in a simulator, not on a generic device.
        schema.retain(|spec| spec.name != "destination" && spec.name != "configuration");
        schema.extend([
            ArgSpec::new("destination", "xcodebuild -destination")
                .default("platform=iOS Simulator,name=iPhone 15"),
            ArgSpec::new("configuration", "Build configuration").default("Debug"),
            ArgSpec::new("result_bundle", "Where to write the .xcresult")
                .default("build/tests.xcresult"),
            ArgSpec::new("code_coverage", "Collect coverage").default("false"),
        ]);
        schema
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        if args.get("workspace").is_none() && args.get("project").is_none() {
            return Err(ctx.error(self.name(), "give either workspace or project"));
        }

        let bundle = ctx
            .workdir()
            .join(args.get_or("result_bundle", "build/tests.xcresult"));
        let mut parts = vec!["xcodebuild".to_string(), "test".to_string()];
        if let Some(flag) = project_flag(args) {
            parts.push(flag);
        }
        parts.push(format!("-scheme {}", quote(args.get_or("scheme", ""))));
        parts.push(format!(
            "-configuration {}",
            quote(args.get_or("configuration", "Debug"))
        ));
        parts.push(format!(
            "-destination {}",
            quote(args.get_or("destination", "platform=iOS Simulator,name=iPhone 15"))
        ));
        parts.push(format!(
            "-resultBundlePath {}",
            quote(&bundle.display().to_string())
        ));
        if args.flag("code_coverage") {
            parts.push("-enableCodeCoverage YES".to_string());
        }
        if let Some(extra) = args.get("xcargs") {
            parts.push(extra.to_string());
        }

        let command = parts.join(" ");
        if ctx.dry_run {
            ctx.ui.say(&format!("Would run: {command}"));
            return Ok(ActionOutput::new().with("result_bundle", bundle.display().to_string()));
        }

        // The bundle must not already exist, or xcodebuild refuses.
        if bundle.exists() {
            let _ = fs::remove_dir_all(&bundle);
        }

        ctx.require(&command)?;

        Ok(ActionOutput::new().with("result_bundle", bundle.display().to_string()))
    }
}

pub struct Keychain;

impl Action for Keychain {
    fn name(&self) -> &'static str {
        "keychain"
    }

    fn description(&self) -> &'static str {
        "Create, unlock or delete a keychain (macOS)"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("action", "create, unlock or delete").default("create"),
            ArgSpec::new("name", "Keychain name").default("shlane.keychain-db"),
            ArgSpec::new("password", "Keychain password")
                .required()
                .sensitive(),
            ArgSpec::new("timeout", "Lock again after this many seconds").default("3600"),
            ArgSpec::new("make_default", "Put it at the front of the search list").default("true"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let name = args.get_or("name", "shlane.keychain-db");
        let password = args.get_or("password", "");
        // The password goes through the environment: a command line is visible
        // in `ps`, and `security` is happy to read it from one.
        let env = BTreeMap::from([("SHLANE_KEYCHAIN_PASS".to_string(), password.to_string())]);

        match args.get_or("action", "create") {
            "create" => {
                // Recreating is normal on a fresh CI machine, so a delete that
                // finds nothing is not a failure.
                let _ = ctx.sh(&format!("security delete-keychain {}", quote(name)));
                ctx.require_with_env(
                    &format!(
                        "security create-keychain -p \"$SHLANE_KEYCHAIN_PASS\" {}",
                        quote(name)
                    ),
                    &env,
                )?;
                ctx.require(&format!(
                    "security set-keychain-settings -lut {} {}",
                    args.get_or("timeout", "3600"),
                    quote(name)
                ))?;
                ctx.require_with_env(
                    &format!(
                        "security unlock-keychain -p \"$SHLANE_KEYCHAIN_PASS\" {}",
                        quote(name)
                    ),
                    &env,
                )?;
                if args.flag("make_default") {
                    ctx.require(&format!(
                        "security list-keychains -d user -s {} login.keychain",
                        quote(name)
                    ))?;
                }
            }
            "unlock" => {
                ctx.require_with_env(
                    &format!(
                        "security unlock-keychain -p \"$SHLANE_KEYCHAIN_PASS\" {}",
                        quote(name)
                    ),
                    &env,
                )?;
            }
            "delete" => {
                ctx.require(&format!("security delete-keychain {}", quote(name)))?;
            }
            other => {
                return Err(ctx.error(
                    self.name(),
                    format!("unknown action '{other}'; use create, unlock or delete"),
                ))
            }
        }

        Ok(ActionOutput::new().with("name", name))
    }
}

pub struct TestFlight;

impl Action for TestFlight {
    fn name(&self) -> &'static str {
        "testflight"
    }

    fn description(&self) -> &'static str {
        "Upload a build to TestFlight (needs Xcode's altool)"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("ipa", "The .ipa to upload").required(),
            ArgSpec::new("key_id", "App Store Connect key id").required(),
            ArgSpec::new("issuer_id", "App Store Connect issuer id").required(),
            ArgSpec::new("key", "The .p8 itself, base64 of it, or a path to it")
                .required()
                .sensitive(),
            ArgSpec::new("platform", "ios, appletvos or osx").default("ios"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let ipa = ctx.workdir().join(args.get_or("ipa", ""));
        let key_id = args.get_or("key_id", "");
        let issuer = args.get_or("issuer_id", "");

        let command = format!(
            "xcrun altool --upload-app -f {} -t {} --apiKey {} --apiIssuer {}",
            quote(&ipa.display().to_string()),
            quote(args.get_or("platform", "ios")),
            quote(key_id),
            quote(issuer)
        );

        if ctx.dry_run {
            ctx.ui.say(&format!("Would run: {command}"));
            return Ok(ActionOutput::new().with("ipa", ipa.display().to_string()));
        }

        if !ipa.is_file() {
            return Err(ctx.error(self.name(), format!("{} does not exist", ipa.display())));
        }

        let key = ApiKey::load(key_id, issuer, args.get_or("key", ""), ctx.workdir())
            .map_err(|message| ctx.error(self.name(), message))?;

        // altool looks for AuthKey_<id>.p8 in a directory it is told about,
        // which keeps the key off the command line and out of the home
        // directory.
        let keys_dir = ctx.workdir().join(".shlane-asc-keys");
        fs::create_dir_all(&keys_dir).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot create {}: {err}", keys_dir.display()),
            )
        })?;
        let key_path = keys_dir.join(format!("AuthKey_{key_id}.p8"));
        let _guard = KeyFile::write(&key_path, key.private_key.as_bytes())
            .map_err(|err| ctx.error(self.name(), format!("cannot write the key: {err}")))?;

        let env = BTreeMap::from([(
            "API_PRIVATE_KEYS_DIR".to_string(),
            keys_dir.display().to_string(),
        )]);

        ctx.ui.say(&format!("Uploading {}", ipa.display()));
        ctx.require_with_env(&command, &env)?;

        Ok(ActionOutput::new().with("ipa", ipa.display().to_string()))
    }
}

/// A private key that removes itself, however the step ends.
struct KeyFile {
    path: PathBuf,
}

impl KeyFile {
    fn write(path: &Path, contents: &[u8]) -> std::io::Result<Self> {
        fs::write(path, contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for KeyFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct AscRequest;

impl Action for AscRequest {
    fn name(&self) -> &'static str {
        "asc_request"
    }

    fn description(&self) -> &'static str {
        "Call the App Store Connect API with a signed token"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new(
                "path",
                "Path under https://api.appstoreconnect.apple.com, e.g. /v1/apps",
            )
            .required(),
            ArgSpec::new("method", "GET, POST, PATCH, ...").default("GET"),
            ArgSpec::new("body", "Request body, for the methods that take one"),
            ArgSpec::new("key_id", "App Store Connect key id").required(),
            ArgSpec::new("issuer_id", "App Store Connect issuer id").required(),
            ArgSpec::new("key", "The .p8 itself, base64 of it, or a path to it")
                .required()
                .sensitive(),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        use crate::actions::core::http::{send, Payload};

        let url = asc_url(args.get_or("path", ""));
        let method = args.get_or("method", "GET").to_uppercase();

        if ctx.dry_run {
            ctx.ui.say(&format!("Would send {method} {url}"));
            return Ok(ActionOutput::new().with("status", "0"));
        }

        let key = ApiKey::load(
            args.get_or("key_id", ""),
            args.get_or("issuer_id", ""),
            args.get_or("key", ""),
            ctx.workdir(),
        )
        .map_err(|message| ctx.error(self.name(), message))?;

        let bearer = token(&key).map_err(|message| ctx.error(self.name(), message))?;
        ctx.mark_secret(&bearer);

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {bearer}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let payload = match args.get("body") {
            Some(body) => Payload::Text(body),
            None => Payload::Empty,
        };

        ctx.ui.say(&format!("{method} {url}"));
        let response = send(ctx, &method, &url, &headers, payload).map_err(|message| {
            ctx.error(self.name(), format!("{method} {url} failed: {message}"))
        })?;

        if !(200..300).contains(&response.status) {
            return Err(ctx.error(
                self.name(),
                format!(
                    "{method} {url} returned HTTP {}: {}",
                    response.status,
                    response.body.trim().chars().take(500).collect::<String>()
                ),
            ));
        }

        Ok(ActionOutput::new()
            .with("status", response.status.to_string())
            .with("body", response.body))
    }
}

fn asc_url(path: &str) -> String {
    const BASE: &str = "https://api.appstoreconnect.apple.com";
    if path.starts_with("http") {
        return path.to_string();
    }
    if path.starts_with('/') {
        return format!("{BASE}{path}");
    }
    format!("{BASE}/{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> Args {
        Args::new(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<String, String>>(),
        )
    }

    #[test]
    fn archives_a_workspace() {
        let command = archive_command(
            &args(&[
                ("workspace", "MyApp.xcworkspace"),
                ("scheme", "MyApp"),
                ("configuration", "Release"),
                ("destination", "generic/platform=iOS"),
                ("allow_provisioning_updates", "true"),
            ]),
            "build/MyApp.xcarchive",
        );

        assert!(command.starts_with("xcodebuild archive"), "{command}");
        assert!(
            command.contains("-workspace 'MyApp.xcworkspace'"),
            "{command}"
        );
        assert!(command.contains("-scheme 'MyApp'"), "{command}");
        assert!(
            command.contains("-archivePath 'build/MyApp.xcarchive'"),
            "{command}"
        );
        assert!(command.contains("-allowProvisioningUpdates"), "{command}");
        assert!(!command.contains("-project"), "{command}");
    }

    #[test]
    fn a_project_is_used_when_there_is_no_workspace() {
        let command = archive_command(
            &args(&[("project", "MyApp.xcodeproj"), ("scheme", "MyApp")]),
            "out.xcarchive",
        );
        assert!(command.contains("-project 'MyApp.xcodeproj'"), "{command}");
    }

    #[test]
    fn clean_comes_before_archive() {
        let command = archive_command(
            &args(&[
                ("project", "A.xcodeproj"),
                ("scheme", "A"),
                ("clean", "true"),
            ]),
            "out",
        );
        assert!(command.starts_with("xcodebuild clean archive"), "{command}");
    }

    #[test]
    fn a_scheme_with_a_space_stays_one_argument() {
        let command = archive_command(
            &args(&[("project", "A.xcodeproj"), ("scheme", "My App")]),
            "out",
        );
        assert!(command.contains("-scheme 'My App'"), "{command}");
    }

    #[test]
    fn exports_with_the_options_plist() {
        let command = export_command(
            "build/MyApp.xcarchive",
            "build/ExportOptions.plist",
            "build",
            &args(&[("allow_provisioning_updates", "true")]),
        );
        assert!(command.contains("-exportArchive"), "{command}");
        assert!(
            command.contains("-exportOptionsPlist 'build/ExportOptions.plist'"),
            "{command}"
        );
        assert!(command.contains("-exportPath 'build'"), "{command}");
    }

    #[test]
    fn writes_a_plist_with_the_method_and_team() {
        let plist = export_options_plist(&args(&[
            ("export_method", "ad-hoc"),
            ("team_id", "ABCDE12345"),
            ("upload_symbols", "true"),
        ]));

        assert!(plist.starts_with("<?xml version=\"1.0\""), "{plist}");
        assert!(
            plist.contains("<key>method</key>\n\t<string>ad-hoc</string>"),
            "{plist}"
        );
        assert!(
            plist.contains("<key>teamID</key>\n\t<string>ABCDE12345</string>"),
            "{plist}"
        );
        assert!(
            plist.contains("<key>uploadSymbols</key>\n\t<true/>"),
            "{plist}"
        );
        assert!(
            plist.contains("<key>uploadBitcode</key>\n\t<false/>"),
            "{plist}"
        );
    }

    #[test]
    fn automatic_signing_is_declared_when_xcode_manages_it() {
        let plist = export_options_plist(&args(&[("allow_provisioning_updates", "true")]));
        assert!(plist.contains("<key>signingStyle</key>"), "{plist}");

        let manual = export_options_plist(&args(&[]));
        assert!(!manual.contains("signingStyle"), "{manual}");
    }

    #[test]
    fn tests_run_against_a_simulator_by_default() {
        let schema = TestIos.schema();
        let destination = schema
            .iter()
            .find(|spec| spec.name == "destination")
            .expect("a destination");
        assert_eq!(
            destination.default.as_deref(),
            Some("platform=iOS Simulator,name=iPhone 15")
        );
    }

    #[test]
    fn builds_app_store_connect_urls() {
        assert_eq!(
            asc_url("/v1/apps"),
            "https://api.appstoreconnect.apple.com/v1/apps"
        );
        assert_eq!(
            asc_url("v1/builds"),
            "https://api.appstoreconnect.apple.com/v1/builds"
        );
        assert_eq!(asc_url("https://example.com/x"), "https://example.com/x");
    }
}
