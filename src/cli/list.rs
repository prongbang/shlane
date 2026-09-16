//! `shlane list`.

use crate::config::model::Config;
use std::path::Path;

pub fn print(config: &Config, path: &Path) {
    println!("{}\n", path.display());

    if config.lanes.is_empty() {
        println!("  no lanes defined");
        return;
    }

    let width = config
        .lanes
        .keys()
        .map(|name| name.chars().count())
        .max()
        .unwrap_or(4);

    for (name, lane) in &config.lanes {
        let mut tags = Vec::new();
        if let Some(platform) = &lane.platform {
            tags.push(platform.clone());
        }
        if lane.private {
            tags.push("private".to_string());
        }
        let tags = if tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", tags.join(", "))
        };

        let description = lane.description.as_deref().unwrap_or("");
        println!("  {name:<width$}{tags}  {description}");

        for (param, spec) in &lane.params {
            let mut notes = vec![spec.param_type.as_str().to_string()];
            if spec.required {
                notes.push("required".to_string());
            }
            if let Some(default) = &spec.default {
                notes.push(format!("default: {default}"));
            }
            if let Some(values) = &spec.values {
                notes.push(format!("one of: {}", values.join(", ")));
            }
            let description = spec
                .description
                .as_deref()
                .map(|text| format!(" — {text}"))
                .unwrap_or_default();
            println!(
                "  {:width$}    {param} ({}){description}",
                "",
                notes.join(", ")
            );
        }
    }

    let runnable = config.public_lane_names();
    if !runnable.is_empty() {
        println!("\nRun one with: shlane run {}", runnable[0]);
    }
}
