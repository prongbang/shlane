//! `shlane migrate`.

use crate::error::{Result, ShlaneError};
use std::fs;
use std::path::{Path, PathBuf};

/// Where a Fastfile usually lives.
const DEFAULT_INPUT: [&str; 2] = ["fastlane/Fastfile", "Fastfile"];

pub fn run(base: &Path, fastfile: Option<&Path>, out: Option<&Path>, force: bool) -> Result<()> {
    let input = match fastfile {
        Some(path) => base.join(path),
        None => DEFAULT_INPUT
            .iter()
            .map(|name| base.join(name))
            .find(|path| path.is_file())
            .ok_or_else(|| ShlaneError::ConfigNotFound {
                path: base.join(DEFAULT_INPUT[0]),
            })?,
    };

    let text = fs::read_to_string(&input).map_err(|source| ShlaneError::ConfigUnreadable {
        path: input.clone(),
        source,
    })?;

    let migration = crate::migrate::convert(&text);
    let output: PathBuf = match out {
        Some(path) => base.join(path),
        None => base.join("shlane.yaml"),
    };

    if output.exists() && !force {
        return Err(ShlaneError::ConfigProblems {
            path: output,
            problems: vec!["already exists; pass --force to overwrite it".to_string()],
        });
    }

    fs::write(&output, &migration.yaml).map_err(|source| ShlaneError::ConfigUnreadable {
        path: output.clone(),
        source,
    })?;

    println!("Read  {}", input.display());
    println!("Wrote {}\n", output.display());
    println!(
        "  {} lane(s), {} action(s) converted, {} line(s) left for you",
        migration.lanes, migration.actions, migration.manual
    );

    if !migration.notes.is_empty() {
        println!("\nWhat needs a person:");
        for note in &migration.notes {
            println!("  - {note}");
        }
    }

    println!(
        "\nThis is a best-effort conversion: a Fastfile is Ruby, and Ruby can do anything.\n\
         Read the result, then run `shlane validate`. Lines marked TODO were not understood."
    );

    Ok(())
}
