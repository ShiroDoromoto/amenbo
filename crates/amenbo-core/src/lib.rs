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
pub mod agent_models;
pub mod agent_sessions;
pub mod agents;
pub mod archive;
pub mod binding;
pub mod blob;
pub mod build_stamp;
pub mod config;
pub mod delivery_log;
pub mod doctor;
pub mod due;
pub mod env;
pub mod error;
pub mod hooks;
pub mod identity;
pub mod export;
pub mod frames;
pub mod handover;
pub mod harness;
pub mod idref;
pub mod lifecycle;
pub mod lint;
pub mod memo;
pub mod mcp;
pub mod mcp_apps;
pub mod mcp_bundle;
pub mod mcp_probe;
pub mod mcp_request;
pub mod migrate;
pub mod model;
pub mod notify_dispatch;
pub mod notify_mail;
pub mod notify_wording;
mod notify_wording_table;
pub mod notify_slack;
pub mod notify_smtp;
pub mod nudge;
pub mod store_engine;
pub mod sys;
pub mod ops;
pub mod order;
pub mod outbox_drive;
pub mod overview;
pub mod perf;
pub mod progress;
pub mod project_teardown;
pub mod query;
pub mod reach;
pub mod read_receipts;
pub mod refscan;
pub mod run_wording;
pub mod self_update;
pub mod session;
pub mod skin;
pub mod skin_contrast;
pub mod skin_official;
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
pub mod viewer;
pub mod wake;
pub mod worktree;
pub mod worktree_cut;

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
