//! `shlane action list` and `shlane action show`.

use crate::actions::Registry;
use crate::error::{Result, ShlaneError};

pub fn list(registry: &Registry) {
    let width = registry
        .iter()
        .map(|action| action.name().chars().count())
        .max()
        .unwrap_or(4);

    println!("{} action(s)\n", registry.len());
    for action in registry.iter() {
        println!("  {:<width$}  {}", action.name(), action.description());
    }
    println!("\nDetails: shlane action show <name>");
}

pub fn show(registry: &Registry, name: &str) -> Result<()> {
    let Some(action) = registry.find(name) else {
        return Err(ShlaneError::Action {
            action: name.to_string(),
            message: format!("no such action (try: {})", registry.names().join(", ")),
        });
    };

    println!("{}  {}\n", action.name(), action.description());

    let schema = action.schema();
    if schema.is_empty() {
        println!("  takes no arguments");
    } else {
        println!("  arguments:");
        for spec in &schema {
            let mut notes = Vec::new();
            if spec.required {
                notes.push("required".to_string());
            }
            if let Some(default) = &spec.default {
                notes.push(format!("default: {default}"));
            }
            if spec.sensitive {
                notes.push("masked in output".to_string());
            }
            let notes = if notes.is_empty() {
                String::new()
            } else {
                format!(" ({})", notes.join(", "))
            };
            println!("    {}{notes}\n      {}", spec.name, spec.description);
        }
    }

    let required: Vec<&crate::actions::ArgSpec> =
        schema.iter().filter(|spec| spec.required).collect();
    println!("\n  steps:\n    - action: {}", action.name());
    if !required.is_empty() {
        println!("      with:");
        for spec in required {
            println!("        {}: ...", spec.name);
        }
    }
    Ok(())
}
