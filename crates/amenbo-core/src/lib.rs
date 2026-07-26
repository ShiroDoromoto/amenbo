//! amenbo's core library: the domain model, persistence, operations and export live here, and the CLI,
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
pub mod env;
pub mod error;
pub mod hooks;
pub mod identity;
pub mod export;
pub mod idref;
pub mod lint;
pub mod migrate;
pub mod model;
pub mod store_engine;
pub mod sys;
pub mod ops;
pub mod order;
pub mod overview;
pub mod perf;
pub mod plugin_callback;
pub mod plugin_catalog;
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
pub mod plugin_log;
pub mod plugin_manifest;
pub mod plugin_payload;
pub mod plugin_provenance;
pub mod plugin_secret;
pub mod plugin_subscribe;
pub mod plugin_trust;
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
pub mod slug;
pub mod store;
pub mod swap_lock;
pub mod time;
mod tmpdir;
pub mod update_check;
pub mod validate;
pub mod view;
pub mod worktree;

pub use error::{Error, ErrorCode, Result};
pub use store::Store;
