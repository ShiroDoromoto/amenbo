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

/// **The account's home directory** — not [`home`], which is Amenbo's own root.
///
/// The two sit together on purpose. `AMENBO_HOME` is a root a reader points at to isolate Amenbo's
/// files, and this is where the operating system says the person lives; a caller reaching for one
/// and getting the other is a bug that shows up as Amenbo reading somebody else's folder, so the
/// names are kept apart and the difference is written here rather than left to be inferred.
///
/// It is what a shell handed no directory starts in, which is what makes it the folder a terminal
/// opened with nothing named is actually standing in.
pub fn home_dir() -> Option<std::path::PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

/// The name of [`plugin_reach`], for the runner that sets it on a plugin's process
/// ([`crate::plugin_callback`]).
pub const PLUGIN_REACH_VAR: &str = "AMENBO_PLUGIN_REACH";

/// `AMENBO_PLUGIN_REACH` — how far the plugin Amenbo just launched may read (`AMB-D-406`): `all`, or a
/// project's `AMB-P-<n>` ref. Set by Amenbo on a plugin's process and read back when that plugin calls
/// `amenbo` again; unset everywhere else, where the facet and the binding decide the reach as usual.
pub fn plugin_reach() -> Option<String> {
    var(PLUGIN_REACH_VAR)
}

/// The name of [`path`], for the one surface that **writes** it rather than reads it: a plugin is started
/// with Amenbo's own directory in front of this list ([`crate::plugin_exec`]).
pub const PATH_VAR: &str = "PATH";

/// `PATH` — the OS's own list of directories a bare command name is looked up in. Amenbo reads it for
/// three purposes. One is to hand a plugin that same list with its own directory in front, so the `amenbo`
/// a plugin is told to call (`AMB-D-406`) is there to be found even when the process was started by a
/// scheduler rather than by a shell (`AMB-D-716`). Another is to ask the same question of ourselves: a
/// theme preview on Linux is the one build nothing installs a CLI for, so whether it has a command to
/// name is whether the member put one here ([`crate::config::Paths::command_to_run`]). The third is to do
/// the lookup ourselves rather than leave it to the spawn, which on macOS is the difference between running
/// git and asking the user to install a compiler ([`crate::sys::git`]).
pub fn path() -> Option<OsString> {
    var_os(PATH_VAR)
}

/// `SHELL` — the shell of whoever's session this process was started from. Amenbo reads it in two places.
/// On macOS it asks that shell where git is when [`PATH`](path) cannot say ([`crate::sys::git`]): a `.app`
/// launched from Finder carries only `/usr/bin:/bin:/usr/sbin:/sbin`, and the profile that puts a Homebrew
/// git in front is the shell's to read, not ours to guess at. In the GUI it is the fallback when the account
/// database cannot say what this user's login shell is, so a terminal opened in the window still starts the
/// shell they actually use (`app/src-tauri/launch.rs`).
///
/// There it is a fallback and not the answer, because what it describes is the session rather than the user:
/// a process several launches deep can be carrying one that was inherited from something else. Unset (a
/// process started by a scheduler, or by an installer) reads as `/bin/sh`, which is on every Unix and reads
/// the same `PATH`-setting profile a plain login would.
pub fn shell() -> Option<OsString> {
    var_os("SHELL")
}

/// The name of [`tmpdir`], for the one surface that **removes** it rather than reads it: the startup
/// repair that disowns a throwaway directory the OS has already taken away
/// ([`crate::tmpdir::forget_if_gone`]).
pub const TMPDIR_VAR: &str = "TMPDIR";

/// `TMPDIR` — where the OS says throwaway files go, on Unix. Amenbo reads it for one purpose: to see
/// whether the directory it names is still there, because an inherited one need not be. A process
/// started by the macOS `.pkg` installer's postinstall carries the installer's sandbox
/// (`/private/tmp/PKInstallSandbox.*/tmp`), and that sandbox is removed the moment the install
/// finishes — leaving the app, and every plugin it starts, pointed at a directory that is gone.
///
/// Windows has no `TMPDIR`: `std::env::temp_dir()` reads `TMP`/`TEMP` there, and nothing we ship hands
/// out a vanishing one. This getter answers `None` there, and the repair has nothing to do.
pub fn tmpdir() -> Option<OsString> {
    var_os(TMPDIR_VAR)
}

/// The name of [`mcp_dirs`], for the MCP server that sets it on every run it starts
/// (`crate::mcp`).
pub const MCP_DIRS_VAR: &str = "AMENBO_MCP_DIRS";

/// `AMENBO_MCP_DIRS` — the folders the MCP server this run was started by is serving, one per line
/// (`AMB-D-679`). Set by `amenbo mcp` on the children a tool call re-runs, unset everywhere else.
///
/// It is read by the one thing whose advice depends on the road the call arrived by: the `no_pointer`
/// refusal, whose way out is `init` / `bind` — and neither is served over MCP, so the caller reading
/// it cannot take the road it names. What it needs instead is the folders that *are* served, and the
/// person who can add one.
pub fn mcp_dirs() -> Option<String> {
    var(MCP_DIRS_VAR)
}

/// `AMENBO_SESSION` — the id of the talk window's terminal this process was started inside
/// ([`crate::session::SESSION_VAR`]). Set by the window on the terminal it opens, and inherited by
/// everything started in it, which is the only way a process several levels deep can say which pane it
/// belongs to. Unset everywhere else, and that is what tells the surface layer it is outside the window.
pub fn session() -> Option<String> {
    var(crate::session::SESSION_VAR)
}

/// `AMENBO_SESSION_DIR` — the throwaway directory the talk window reads this run's statements out of
/// ([`crate::session::DIR_VAR`]). Set beside [`session`], on the same terminals, and gone with the run.
pub fn session_dir() -> Option<OsString> {
    var_os(crate::session::DIR_VAR)
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

/// `NO_COLOR` — the cross-tool convention (no-color.org) for "do not emit ANSI escapes". It is the one
/// variable here Amenbo does not name itself, because that is the whole point of it: a person turns
/// colour off once, for every tool they run, rather than per tool.
///
/// **Presence alone is the signal, whatever the value** — that is what the convention says, so an empty
/// `NO_COLOR=` counts too. Hence `Option`, and no parsing: a caller asks `is_some()`.
pub fn no_color() -> Option<String> {
    var("NO_COLOR")
}

/// `TERM` — the OS's own name for what kind of terminal a program is running in, and hence which
/// escape sequences it may use. Amenbo reads it to see whether the launch it is passing on already
/// carries one, because a desktop launch carries none and a program that finds none assumes a
/// terminal that can do nothing (`app/src-tauri/launch.rs`).
pub fn term() -> Option<OsString> {
    var_os("TERM")
}

/// The locale this process was launched with, in the precedence the C library reads them in:
/// `LC_ALL` overrules `LANG`, so a session that set the first has answered whatever is asked of the
/// second. Amenbo reads it for the same reason as [`term`] — to leave a launch that already carries
/// an answer alone, and to name a UTF-8 one for a desktop launch that carries none.
///
/// The value is not parsed and not judged: presence is the whole of the question.
pub fn locale() -> Option<OsString> {
    var_os("LC_ALL").or_else(|| var_os("LANG"))
}

/// `AMENBO_UPDATE_CHECK` — the environment override for the update check (the update endpoint
/// query). `0` / `off` / `false` / `no` **disable** it, overriding `config.update_check`: a hard kill
/// switch for CI, for privacy, and for tests that must guarantee nothing ever leaves the machine.
/// Any other value, or none, leaves the decision to the config.
pub fn update_check_disabled() -> bool {
    var("AMENBO_UPDATE_CHECK")
        .map(|v| matches!(v.trim(), "0" | "off" | "false" | "no"))
        .unwrap_or(false)
}

/// `AMENBO_UPDATE_JSON_URL` — override the manifest URL we query, pointing tests and development at
/// something other than the production endpoint.
///
/// **Read it through [`crate::update_check`], not here.** What this returns is only what the
/// environment says; whether the build may act on it is the update check's to decide, and a build the
/// release workflow stamped may not — a shipped binary that its environment can re-aim is one whose
/// self-update installs whatever that address answers.
pub fn update_json_url() -> Option<String> {
    var("AMENBO_UPDATE_JSON_URL")
}

/// `AMENBO_PLUGIN_CATALOG_URL` — override the plugin catalog Amenbo fetches
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
