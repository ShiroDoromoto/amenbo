//! The **one audited gate** through which the process environment is read.
//!
//! `disallowed-methods` in `clippy.toml` **bans `std::env::var` and `std::env::var_os` across every
//! crate**, funnelling raw environment reads into this module. An environment variable propagates
//! across process boundaries on its own, so what a process inherits decides behaviour nobody at the
//! call site declared. Gathering every read into a single module keeps the list of process inputs we
//! depend on reviewable, and makes adding one a deliberate act rather than an incidental line.
//!
//! The facet is **not** among them (`AMB-D-408`): it is declared by `--actor` and nowhere else,
//! precisely because it must not be inherited.
//!
//! Note that this is a speed bump, not a sandbox: a read reached through a function pointer slips
//! past it. It holds only so long as `#[allow]` stays on the two functions below and no raw
//! environment read exists anywhere else.

use std::ffi::OsString;

/// The name of [`home`], for the surfaces that **set** it rather than read it — a plugin is handed the
/// store this way ([`crate::plugin_callback`]), and a name that is written in two places is a name that can
/// drift.
pub const HOME_VAR: &str = "AMENBO_HOME";

/// `AMENBO_HOME` — the explicit root that isolates the whole user layer (secrets, config, store)
/// into one place, for tests and dogfooding. It is also how a launched plugin is told which store to call
/// back into ([`crate::plugin_callback`]).
pub fn home() -> Option<OsString> {
    var_os(HOME_VAR)
}

/// The name of [`plugin_reach`], for the runner that sets it on a plugin's process
/// ([`crate::plugin_callback`]).
pub const PLUGIN_REACH_VAR: &str = "AMENBO_PLUGIN_REACH";

/// `AMENBO_PLUGIN_REACH` — how far the plugin amenbo just launched may read (`AMB-D-406`): `all`, or a
/// project's `AMB-P-<n>` ref. Set by amenbo on a plugin's process and read back when that plugin calls
/// `amenbo` again; unset everywhere else, where the facet and the binding decide the reach as usual.
pub fn plugin_reach() -> Option<String> {
    var(PLUGIN_REACH_VAR)
}

/// `AMENBO_PROJECT_DIR` — where the search for the `.amenbo` pointer starts, so the GUI and friends
/// can name a directory other than the CWD.
pub fn project_dir() -> Option<OsString> {
    var_os("AMENBO_PROJECT_DIR")
}

/// `AMENBO_HW_ID` — override the machine UUID, to pose as a different machine during development.
pub fn hw_id() -> Option<OsString> {
    var_os("AMENBO_HW_ID")
}

/// `AMENBO_PERF` — the top-priority override of the perf instrumentation toggle: an EnvFilter string
/// in `RUST_LOG` form (`perf=debug`, `perf=warn`, …). When set, it overrides the config, channel and
/// build defaults. Empty, `off`, `0` and `false` mean explicitly off. The spans are **not compiled
/// out even in a release build**, so this env var can turn instrumentation on locally against a
/// production binary.
pub fn perf() -> Option<String> {
    var("AMENBO_PERF")
}

/// `AMENBO_UPDATE_CHECK` — the environment override for the update check (the static latest.json
/// query). `0` / `off` / `false` / `no` **disable** it, overriding `config.update_check`: a hard kill
/// switch for CI, for privacy, and for tests that must guarantee nothing ever leaves the machine.
/// Any other value, or none, leaves the decision to the config.
pub fn update_check_disabled() -> bool {
    var("AMENBO_UPDATE_CHECK")
        .map(|v| matches!(v.trim(), "0" | "off" | "false" | "no"))
        .unwrap_or(false)
}

/// `AMENBO_UPDATE_JSON_URL` — override the latest.json we query, pointing tests and development at
/// something other than the production URL.
pub fn update_json_url() -> Option<String> {
    var("AMENBO_UPDATE_JSON_URL")
}

/// `AMENBO_PLUGIN_CATALOG_URL` — override the plugin catalog amenbo fetches
/// ([`crate::plugin_catalog::OFFICIAL_CATALOG_URL`]), so development and manual testing can point at a
/// staging catalog without touching the published one.
pub fn plugin_catalog_url() -> Option<String> {
    var("AMENBO_PLUGIN_CATALOG_URL")
}

/// `AMENBO_GITHUB_API_URL` — override the GitHub API base a plugin's detail reads its stars, README
/// and download count from ([`crate::plugin_github::GITHUB_API_URL`]), so development and manual
/// testing can answer those requests locally instead of spending the real API's rate limit.
pub fn github_api_url() -> Option<String> {
    var("AMENBO_GITHUB_API_URL")
}

/// `AMENBO_ALLOW_UNSTAMPED_MIGRATE` — the escape hatch out of the release-stamp gate
/// ([`crate::build_stamp`], `AMB-D-378`): it lets a locally built binary carry the production store
/// forward for **this one run**. It exists for the case the gate cannot help with — a released build
/// that cannot recover the store itself — and lives in the environment rather than in the binary
/// precisely so an accidental launch never has it.
///
/// Off / `0` / `false` / `no` / empty read as not set, so a value that says "no" is not a way in.
pub fn allow_unstamped_migrate() -> bool {
    var("AMENBO_ALLOW_UNSTAMPED_MIGRATE")
        .map(|v| !matches!(v.trim(), "" | "0" | "off" | "false" | "no"))
        .unwrap_or(false)
}

/// `AMENBO_TEST_NETWORK_DIR` — a directory on a **genuine network volume**. Only the GUI's
/// `tests/store_watch.rs` passes it, and only when exercising "can we recognize a network FS from
/// its filesystem type?" against a real mount — mounting one takes manual work, so the test is
/// `#[ignore]`d by default. No product code path reads this value. It is a test-only entry point,
/// but it still keeps the promise of gathering every environment read into one place: adding a raw
/// read would defeat the purpose of this gate.
pub fn test_network_dir() -> Option<OsString> {
    var_os("AMENBO_TEST_NETWORK_DIR")
}

// --- The sole exit for a raw environment read; `allow` lives here and nowhere else. A new
// environment variable gets a typed getter above. ---

#[allow(clippy::disallowed_methods)]
fn var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

#[allow(clippy::disallowed_methods)]
fn var_os(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}
