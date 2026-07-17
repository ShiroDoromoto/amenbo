//! `amenbo agent --json` — a thin CLI shell. The source of truth for the spec (philosophy, every
//! command, capabilities, workflow) lives in [`amenbo_core::agent`], so the CLI (this command) and the
//! GUI (the command palette / ⌘K Tauri command) consume the same source. That every subcommand in
//! `cli.rs` shows up in the spec is enforced by this crate's integration tests via `command_names()`.

use serde_json::Value;

pub use amenbo_core::agent::{SCHEMA_VERSION, VERSION};

/// Return the spec.
pub fn build() -> Value {
    amenbo_core::agent::build()
}

/// The entry-point spec: how to work here in full, plus an index of the commands (look one up with
/// [`command_spec`]).
pub fn build_index() -> Value {
    amenbo_core::agent::build_index()
}

/// One command's full spec (`agent --command <name>`). None for an unknown name.
pub fn command_spec(name: &str) -> Option<Value> {
    amenbo_core::agent::command_spec(name)
}

/// Every command name registered in the spec (used by the test that catches unregistered commands).
#[allow(dead_code)]
pub fn command_names() -> Vec<String> {
    amenbo_core::agent::command_names()
}
