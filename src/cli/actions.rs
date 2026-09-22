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

/// `shlane action run`: one action, no config file.
///
/// Prints the outputs to stdout unmasked — asking for them is the point. One
/// output prints as its bare value so `$(shlane action run ...)` captures it.
pub fn run(
    registry: std::rc::Rc<Registry>,
    name: &str,
    params: &[String],
    workdir: &std::path::Path,
    dry_run: bool,
    verbosity: crate::runtime::Verbosity,
    json: bool,
) -> Result<()> {
    use crate::runtime::secrets::Secrets;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    let Some(action) = registry.find(name) else {
        return Err(ShlaneError::Action {
            action: name.to_string(),
            message: format!("no such action (try: {})", registry.names().join(", ")),
        });
    };

    let mut problems: Vec<String> = params
        .iter()
        .filter(|param| !param.contains('='))
        .map(|param| format!("'{param}' is not key=value"))
        .collect();
    let provided = crate::runtime::parse_params(params);
    problems.extend(crate::actions::check_args(action, &provided));
    if !problems.is_empty() {
        return Err(ShlaneError::Action {
            action: name.to_string(),
            message: problems.join("\n  "),
        });
    }
    let args = crate::actions::with_defaults(action, &provided);

    let secrets = Rc::new(RefCell::new(Secrets::new()));
    for spec in action.schema().iter().filter(|spec| spec.sensitive) {
        if let Some(value) = args.get(&spec.name) {
            secrets.borrow_mut().add(value);
        }
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let frame = crate::runtime::context::Frame {
        lane: format!("action:{name}"),
        env: env.clone(),
        workdir: workdir.to_path_buf(),
        dry_run,
        ..Default::default()
    };
    let cleanups: crate::runtime::context::SharedCleanups = Default::default();
    let ui = Rc::new(crate::runtime::ui::Ui::new(
        verbosity,
        json,
        secrets.clone(),
    ));

    let mut ctx = crate::actions::context::ActionContext {
        lane: frame.lane.clone(),
        env: &env,
        workdir: workdir.to_path_buf(),
        dry_run,
        ui: ui.clone(),
        secrets: secrets.clone(),
        frame: Rc::new(RefCell::new(frame)),
        outputs: Default::default(),
        cleanups: cleanups.clone(),
        registry: Rc::downgrade(&registry),
        depth: Default::default(),
    };
    let result = action.run(&mut ctx, &args);

    // Same contract as a lane: cleanups run whatever the result.
    let pending: Vec<_> = cleanups.borrow_mut().drain(..).rev().collect();
    for cleanup in pending {
        if dry_run {
            ui.say(&format!("Would run: {}", cleanup.command));
            continue;
        }
        let outcome = crate::runtime::shell::run(crate::runtime::shell::Spawn {
            command: &cleanup.command,
            env: &env,
            workdir,
            timeout: None,
            quiet: true,
            secrets: &secrets.borrow(),
        });
        if !outcome.is_ok_and(|outcome| outcome.success) {
            ui.warn(&format!("could not clean up {}", cleanup.what));
        }
    }

    let outputs = result?.0;
    if json {
        println!("{}", serde_json::to_string(&outputs).unwrap_or_default());
    } else if let [(_, value)] = outputs.iter().collect::<Vec<_>>()[..] {
        println!("{value}");
    } else {
        for (key, value) in &outputs {
            println!("{key}={value}");
        }
    }
    Ok(())
}
