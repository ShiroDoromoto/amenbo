//! Amenbo's core library: the domain model, persistence, operations and export live here, and the CLI,
//! the GUI and AI agents are thin skins over it. There is no central server — the data is portable and
//! local-first — and the conceptual model it exposes is one coherent, CLI- and AI-native whole.

// The big `json!` spec in `agent::build` blows past the default macro recursion limit.
#![recursion_limit = "256"]
// Docs are built with `--document-private-items`, so linking from a public API to a private helper is
// correct here — we only want broken links to warn.
#![allow(rustdoc::private_intra_doc_links)]

pub mod activity;
pub mod activity_log;
pub mod agent;
pub mod agents;
pub mod archive;
pub mod binding;
pub mod blob;
pub mod build_stamp;
pub mod config;
pub mod doctor;
pub mod due;
pub mod env;
pub mod error;
pub mod hooks;
pub mod identity;
pub mod export;
pub mod frames;
pub mod harness;
pub mod idref;
pub mod lint;
pub mod mcp;
pub mod mcp_apps;
pub mod mcp_bundle;
pub mod mcp_probe;
pub mod mcp_request;
pub mod migrate;
pub mod model;
pub mod nudge;
pub mod store_engine;
pub mod sys;
pub mod ops;
pub mod order;
pub mod overview;
pub mod perf;
pub mod plugin_agent;
pub mod plugin_callback;
pub mod plugin_catalog;
pub mod plugin_check;
pub mod plugin_command;
pub mod plugin_compat;
pub mod plugin_config;
pub mod plugin_dispatch;
pub mod plugin_drive;
pub mod plugin_exec;
pub mod plugin_github;
pub mod plugin_hooks;
pub mod plugin_runner;
pub mod plugin_inject;
pub mod plugin_install;
pub mod plugin_installed;
pub mod plugin_invoke;
pub mod plugin_layer;
pub mod plugin_log;
pub mod plugin_manifest;
pub mod plugin_payload;
pub mod plugin_provenance;
pub mod plugin_show;
pub mod plugin_subscribe;
pub mod plugin_trust;
pub mod plugin_when;
pub mod plugin_uninstall;
pub mod plugin_update;
pub mod plugin_validate;
pub mod plugin_wire;
pub mod progress;
pub mod project_teardown;
pub mod query;
pub mod reach;
pub mod read_receipts;
pub mod refscan;
pub mod self_update;
pub mod session;
pub mod session_work;
pub mod slug;
pub mod store;
pub mod swap_lock;
pub mod sync_snapshot;
pub mod tick;
pub mod time;
pub mod tmpdir;
pub mod update_check;
pub mod validate;
pub mod view;
pub mod wake;
pub mod worktree;

pub use error::{Error, ErrorCode, Fields, Msg, Result};
pub use store::Store;

#[cfg(test)]
mod build_profile {
    /// The suite runs with debug assertions on, and this is what says so.
    ///
    /// `Cargo.toml` raises `opt-level` for the binary the end-to-end suite spawns, because starting it
    /// unoptimised costs several hundred milliseconds a thousand times over. `opt-level` is independent
    /// of `debug-assertions`, so that trade buys speed and gives up nothing — but the neighbouring move,
    /// building the suite in the release profile, would take eighteen `debug_assert!`s out of the tree
    /// silently, and `perf` branches on `cfg!(debug_assertions)` for behaviour rather than for speed.
    /// The suite would go on passing while asserting less, which is the one failure a green run cannot
    /// report. So it is asked here instead.
    ///
    /// `overflow-checks` has no `cfg!` to ask, and follows `debug-assertions` unless someone sets the two
    /// apart; this covers it only that far.
    ///
    /// The condition is a compile-time constant, and clippy says so — which is the point rather than a
    /// slip: what is being asked is not a fact about this run but a fact about the build this run was
    /// produced by, and a constant is the only shape that question has.
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn the_suite_runs_with_debug_assertions_on() {
        assert!(
            cfg!(debug_assertions),
            "the tests are being built without debug assertions — `debug_assert!` is compiled out and \
             `perf` takes its other branch, so the suite would pass while checking less. Raise \
             `opt-level` if this is about speed; do not move the suite to a profile that turns these off.",
        );
    }
}
