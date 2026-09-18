//! `shlane init`.

use crate::error::{Result, ShlaneError};
use std::fs;
use std::path::Path;

/// What kind of project this looks like, which decides the starter lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Project {
    Rust,
    Node,
    Flutter,
    Android,
    Ios,
    Unknown,
}

pub fn write(dir: &Path, force: bool) -> Result<()> {
    let path = dir.join("shlane.yaml");
    if path.exists() && !force {
        return Err(ShlaneError::ConfigProblems {
            path: path.clone(),
            problems: vec!["already exists; pass --force to overwrite it".to_string()],
        });
    }

    let project = detect(dir);
    let contents = template(project);

    fs::write(&path, contents).map_err(|source| ShlaneError::ConfigUnreadable {
        path: path.clone(),
        source,
    })?;

    println!("Wrote {} for a {} project", path.display(), label(project));
    println!("Next: shlane list");
    Ok(())
}

fn label(project: Project) -> &'static str {
    match project {
        Project::Rust => "Rust",
        Project::Node => "Node",
        Project::Flutter => "Flutter",
        Project::Android => "Android",
        Project::Ios => "iOS",
        Project::Unknown => "generic",
    }
}

fn detect(dir: &Path) -> Project {
    if dir.join("pubspec.yaml").is_file() {
        return Project::Flutter;
    }
    if dir.join("Cargo.toml").is_file() {
        return Project::Rust;
    }
    if dir.join("gradlew").is_file()
        || dir.join("build.gradle").is_file()
        || dir.join("build.gradle.kts").is_file()
    {
        return Project::Android;
    }
    if has_extension(dir, "xcworkspace") || has_extension(dir, "xcodeproj") {
        return Project::Ios;
    }
    if dir.join("package.json").is_file() {
        return Project::Node;
    }
    Project::Unknown
}

fn has_extension(dir: &Path, extension: &str) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .is_some_and(|found| found == extension)
    })
}

fn template(project: Project) -> String {
    let header = "\
# shlane configuration — https://github.com/prongbang/shlane
#
#   shlane list            show every lane
#   shlane validate        check this file without running anything
#   shlane run <lane>      run one
version: 1

";

    let lanes = match project {
        Project::Rust => {
            "lanes:
  test:
    description: \"Run the test suite\"
    steps:
      - run: cargo test

  build:
    description: \"Build a release binary\"
    steps:
      - run: cargo fmt --all --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo build --release
"
        }
        Project::Node => {
            "lanes:
  test:
    description: \"Run the test suite\"
    steps:
      - run: npm test

  build:
    description: \"Build the project\"
    steps:
      - run: npm ci
      - run: npm run build
"
        }
        Project::Flutter => {
            "lanes:
  test:
    description: \"Run the test suite\"
    steps:
      - run: flutter test

  build:
    description: \"Build a release artifact\"
    platform: android
    params:
      format:
        type: string
        default: appbundle
        values: [appbundle, apk]
    steps:
      - run: flutter build ${format}
"
        }
        Project::Android => {
            "lanes:
  test:
    description: \"Run the unit tests\"
    platform: android
    steps:
      - run: ./gradlew test

  build:
    description: \"Assemble a release build\"
    platform: android
    steps:
      - run: ./gradlew bundleRelease
"
        }
        Project::Ios => {
            "lanes:
  test:
    description: \"Run the test suite\"
    platform: ios
    params:
      scheme:
        type: string
        required: true
        description: \"Xcode scheme to test\"
    steps:
      - run: xcodebuild test -scheme ${scheme} -destination 'platform=iOS Simulator,name=iPhone 15'

  build:
    description: \"Archive the app\"
    platform: ios
    params:
      scheme:
        type: string
        required: true
    steps:
      - run: xcodebuild archive -scheme ${scheme} -archivePath build/app.xcarchive
"
        }
        Project::Unknown => {
            "lanes:
  hello:
    description: \"A lane to edit\"
    params:
      name:
        type: string
        default: world
    steps:
      - run: echo \"hello ${name}\"
"
        }
    };

    format!("{header}{lanes}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_parses_and_validates() {
        for project in [
            Project::Rust,
            Project::Node,
            Project::Flutter,
            Project::Android,
            Project::Ios,
            Project::Unknown,
        ] {
            let text = template(project);
            let config = crate::config::loader::parse(&text, Path::new("shlane.yaml"))
                .unwrap_or_else(|err| panic!("{project:?} template should parse: {err}"));
            let problems =
                crate::config::validate::check(&config, &crate::actions::Registry::builtins());
            assert!(problems.is_empty(), "{project:?}: {problems:?}");
            assert!(!config.lanes.is_empty(), "{project:?} has no lanes");
        }
    }
}
