//! Rhai engine construction.

use super::builtins;
use crate::runtime::context::SharedFrame;
use rhai::Engine;

/// Bounds that stop a runaway script from hanging a CI job. Generous enough
/// that no realistic automation script notices them.
const MAX_OPERATIONS: u64 = 10_000_000;
const MAX_STRING_SIZE: usize = 10 * 1024 * 1024;
const MAX_ARRAY_SIZE: usize = 100_000;

pub fn build(frame: &SharedFrame) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_string_size(MAX_STRING_SIZE);
    engine.set_max_array_size(MAX_ARRAY_SIZE);
    // `print`/`debug` are routed through the engine's handlers. Registering a
    // function named `print` instead makes every script fail with "Output type
    // incorrect", because Rhai uses those overloads for value-to-string
    // conversion -- which is what v0.1.0 did, and why the shipped example's
    // script never actually ran.
    engine.on_print(|text| println!("{text}"));
    engine.on_debug(|text, source, pos| match source {
        Some(source) => eprintln!("{source} @ {pos:?}: {text}"),
        None => eprintln!("{pos:?}: {text}"),
    });

    builtins::register(&mut engine, frame);
    engine
}
