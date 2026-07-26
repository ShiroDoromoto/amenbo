//! Entry point for the amenbo CLI: parse with clap, delegate to the core (amenbo-core) operations.
//! The CLI is a thin skin. Output comes in two layers: human-readable, and `--json`.

// `commands()` in `agent.rs` is one huge `json!([...])` literal, and every entry added pushes it further
// past the default recursion limit (128). Raise the limit so the spec can stay a single array.
#![recursion_limit = "256"]

mod agent;
mod cli;
mod doctor_text;
mod output;
mod validate_text;

use std::io::IsTerminal;
use std::sync::OnceLock;

use chrono::NaiveDate;
use clap::Parser;
use serde_json::json;

use amenbo_core::config::Paths;
use amenbo_core::model::{ActorKind, Attachment, AttachmentTarget, Priority, TaskStatus, View};
use amenbo_core::ops::Position;
use amenbo_core::plugin_drive::Face;
use amenbo_core::plugin_installed;
use amenbo_core::plugin_subscribe::EnabledSubscribers;
use amenbo_core::reach::Reach;
use amenbo_core::worktree;
use amenbo_core::{activity_log, ops, query, time, Store};

use cli::*;
use output::{
    confirm, human, print_json, render_error, set_setup_report, warn_body, write_envelope,
    CliError, CliErrorCode, Flags,
};

/// The effective project id picked by an explicit override (`--project`). It is a process-wide setting
/// decided once from argv, hence `OnceLock` (one CLI run is one process). Unset means the binding
/// (`.amenbo`) decides.
static PROJECT_OVERRIDE: OnceLock<i64> = OnceLock::new();

fn main() {
    restore_sigpipe();
    let code = real_main();
    std::process::exit(code);
}

/// Hand SIGPIPE back to the kernel, which the Rust runtime otherwise ignores on startup.
///
/// With the signal ignored, a write to a pipe whose reader has gone returns EPIPE instead of ending
/// the process, and every printing macro panics on a write it cannot make. `amenbo task list | head`
/// is enough: the reader leaves at its second line, and the writer dies at `stdout` with a panic
/// message and exit 101. That is not a broken export or a lost store — the output simply had nowhere
/// left to go — but it reads as a crash, and it fires on the most ordinary pipe there is.
///
/// Default disposition is what a Unix tool does here: the process ends where it wrote, at once, with
/// no message and the shell's usual 141. It also covers every printing site in one move rather than
/// one at a time, and the ones that matter are not confined to any single path — `--json` on stdout
/// and the progress lines on stderr fail the same way.
///
/// Windows has no SIGPIPE. A closed pipe surfaces there as an ordinary write error, so there is
/// nothing to restore and nothing to do.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: `signal` is async-signal-safe, and this runs before any thread is spawned. SIG_DFL is
    // the disposition the process would have had if the runtime had not changed it.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn real_main() -> i32 {
    let parsed = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => return handle_parse_error(e),
    };
    // facet (actor kind): `--actor` and nothing else (`AMB-D-408`). An operation that uses the facet —
    // stamping who acted, or drawing how far an AI reaches — must declare one, and gets `facet_required`
    // when it does not. An operation that uses none passes without one and never touches a facet again.
    // Nothing is inferred from the context of the call: an environment variable would propagate into
    // every process amenbo starts, and a human default would let an undeclared AI write as a person and
    // read past its binding.
    let actor = match decide_facet(parsed.actor.as_deref(), uses_facet(&parsed.command)) {
        Ok(a) => a,
        Err(err) => {
            // No Flags yet, so render the error with a minimal set.
            let probe = Flags { json: parsed.json, yes: false, quiet: false, no_color: false, actor: None };
            return render_error(&probe, &misplaced_flags_hint(&parsed.command, err));
        }
    };
    let flags = Flags {
        json: parsed.json,
        yes: parsed.yes,
        quiet: parsed.quiet,
        no_color: parsed.no_color,
        actor,
    };
    match run(parsed, &flags) {
        Ok(code) => code,
        Err(err) => render_error(&flags, &err),
    }
}

/// Resolve the facet, as a pure function over `--actor` alone. An explicit value is taken as given; an
/// invalid one is `invalid_value` (exit 2). Nothing given is `None` — **the facet stays unspecified**
/// rather than becoming a default — unless the command uses one ([`uses_facet`], folded into `require` by
/// the caller), in which case it fails loud with `facet_required`. Keeping the judgement in the caller
/// leaves this function side-effect free and unit-testable.
fn decide_facet(flag: Option<&str>, require: bool) -> Result<Option<ActorKind>, CliError> {
    match flag {
        None | Some("") => {
            if require {
                Err(CliError::facet_required())
            } else {
                Ok(None)
            }
        }
        Some(s) => ActorKind::parse(s).map(Some).ok_or_else(|| CliError {
            code: "invalid_value",
            message: format!("--actor value '{s}' is invalid (specify human or ai)"),
            hint: Some("AI agents use --actor ai.".to_string()),
            exit: 2,
        }),
    }
}

/// amenbo's own flags, as they are spelled on a command line. After `plugin run <name>` these are not
/// amenbo's any more — every word there is the plugin's (`AMB-D-346`) — which is what the hint below
/// exists to say out loud.
const OWN_FLAGS: &[&str] = &["--actor", "--json", "--quiet", "--yes", "--no-color"];

/// The flag that takes a value; the rest stand alone. It is also the only one whose misplacement
/// **explains** a failure, which is why it is what the hint triggers on.
const FACET_FLAG: &str = "--actor";

/// Say where a flag went, when that is the answer to the failure a person is looking at.
///
/// Everywhere else in amenbo `--json` goes on the end, so that is the habit people bring to `plugin
/// run` — where the end belongs to the plugin. The `--actor` they typed is then sitting in the
/// plugin's argv, amenbo never saw a facet, and what comes back is "facet is unspecified", which
/// says nothing about the words they actually wrote. This adds the missing half to the hint: the flag
/// went to the plugin, and here is the same command with amenbo's flags where amenbo can see them.
///
/// It fires on that pairing alone. A plugin is entitled to a `--json` of its own, so a flag amenbo
/// happens to share a spelling with is not a mistake — only a facet that was typed and never arrived
/// is, and that is the one this can be sure about.
fn misplaced_flags_hint(cmd: &Option<Command>, err: CliError) -> CliError {
    if err.code != CliErrorCode::FacetRequired.as_str() {
        return err;
    }
    let Some(Command::Plugin { sub: PluginCmd::Run { name, args } }) = cmd else { return err };
    let (own, plugins) = split_own_flags(args);
    if !own.iter().any(|f| f.starts_with(FACET_FLAG)) {
        return err;
    }
    // Every one of amenbo's flags found is hoisted, not just the facet: leaving a `--json` behind
    // would hand back a corrected line that still does not answer in JSON.
    let corrected = [
        vec![Paths::command_name().to_string()],
        own.clone(),
        vec!["plugin".to_string(), "run".to_string(), name.clone()],
        plugins,
    ]
    .concat()
    .join(" ");
    CliError {
        hint: Some(format!(
            "`{}` went to the plugin, not to amenbo — after `plugin run {name}` every word is the plugin's. Put amenbo's flags before it:\n  {corrected}",
            own.join(" ")
        )),
        ..err
    }
}

/// Split what was handed to the plugin into amenbo's own flags (with the value `--actor` carries) and
/// everything else, each in the order it was written. `--actor=ai` counts as one word, and a bare
/// `--actor` at the very end counts as itself — a person who wrote either meant amenbo to read it.
fn split_own_flags(args: &[String]) -> (Vec<String>, Vec<String>) {
    let (mut own, mut rest) = (Vec::new(), Vec::new());
    let mut it = args.iter().peekable();
    while let Some(arg) = it.next() {
        let head = arg.split_once('=').map_or(arg.as_str(), |(k, _)| k);
        if !OWN_FLAGS.contains(&head) {
            rest.push(arg.clone());
            continue;
        }
        own.push(arg.clone());
        // `--actor ai` is two words unless it was written as one; the value follows it.
        if head == FACET_FLAG && !arg.contains('=') {
            if let Some(value) = it.next_if(|v| !v.starts_with('-')) {
                own.push(value.clone());
            }
        }
    }
    (own, rest)
}

/// Does this command **use** the facet (human/ai)? There are two consumers, and either one counts
/// (`AMB-D-408`):
///
/// - it **stamps** the facet — into created_by / assign / activity — which every write does;
/// - it **draws the reach** from it — an `ai` facet is confined to the bound project, a human sees the
///   device — which every read that surfaces store content does, `task list` / `show` / `activity` /
///   `status` / `export` / `doctor` among them.
///
/// False is the narrow set that touches neither: the faces that answer about this build or this machine
/// (version / update / agent / whoami / config), the ones that place the pointer or read text handed to
/// them (bind / lint / the git hooks), and the two entry points amenbo starts itself with a store and a
/// window already named (plugin-runner, `plugin validate`). Those never reach a facet, so there is
/// nothing for an undeclared one to go wrong in. Everything else defaults to true (**fail-closed**: a
/// variant missed here surfaces `facet_required`, which beats acting on a facet nobody declared).
fn uses_facet(cmd: &Option<Command>) -> bool {
    // No args = discover, which lists this project's work — store content, so it draws the reach.
    let Some(c) = cmd else { return true };
    match c {
        // Facts about this build and this machine's own settings; no store content either way.
        Command::Agent { .. }
        | Command::Version
        | Command::Update { .. }
        | Command::Whoami
        | Command::Config { .. } // settings live in the user layer, outside any project
        | Command::Bind { .. } // only writes the `.amenbo` pointer
        | Command::Lint { .. } // reads the text it is handed; no store to reach into
        | Command::GithookPreCommit // the hook's face of `lint`; reads the staged diff, no store
        | Command::GithookCommitMsg { .. } // the hook's face of `lint <file>`; reads the message file, no store
        // A runner fires the hooks a facet's own writes already queued: it creates nothing, assigns
        // nothing, and was handed the store to work (`AMB-T-2175`). So was a plugin calling amenbo back,
        // whose window comes from the gate it fired through rather than from a facet (`AMB-D-406`).
        | Command::PluginRunner { .. }
        // `validate` reads a manifest file the author names and touches no store at all — unlike the rest
        // of the group, which moves this machine's plugin state and the plugin's own per-project rows.
        | Command::Plugin { sub: PluginCmd::Validate { .. } } => false,
        // Everything else — every write, and every read that surfaces store content.
        _ => true,
    }
}

/// Map a clap parse error onto an exit code. With `--json`, emit the error as JSON.
fn handle_parse_error(e: clap::Error) -> i32 {
    use clap::error::ErrorKind;
    match e.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            print!("{e}");
            return 0;
        }
        _ => {}
    }
    let wants_json = std::env::args().any(|a| a == "--json");
    let code = match e.kind() {
        ErrorKind::InvalidSubcommand | ErrorKind::UnknownArgument => "unknown_command",
        ErrorKind::MissingRequiredArgument => "missing_required_flag",
        _ => "invalid_value",
    };
    if wants_json {
        let obj = json!({ "error": {
            "code": code,
            "message": e.to_string().lines().next().unwrap_or("argument error"),
            "hint": format!("Run `{} agent --json` to see the available commands.", Paths::command_name())
        }});
        eprintln!("{}", serde_json::to_string_pretty(&obj).unwrap());
    } else {
        eprint!("{e}");
    }
    2
}

/// Does this command require a pointer (`.amenbo`)? This is the exec guard. The exceptions are the commands
/// that place or name the marker (init / bind) and the faces that can answer without opening the store
/// (version / update); everything else (task/project/activity/config/agent/discover …) needs a pointer,
/// including `None` — bare discover. version / update get through because neither ever reads store content:
/// version states facts about this build, and update just looks up this OS's installer URL in the published
/// latest.json. With no self-update, `update` is the only route for someone who wants a newer build, and
/// answering "run init first" would shut out exactly the people stuck on an old version or a fresh install.
/// Containing the AI is not this guard's job — `Reach::for_ai` does it (no binding, empty reach) — and
/// neither command exposes any user data to an AI, so there is nothing here for a location guard to protect
/// twice. `agent` (the AI's own entry point) is not an exception.
fn requires_pointer(cmd: &Option<Command>) -> bool {
    !matches!(
        cmd,
        Some(Command::Init { .. })
            | Some(Command::Bind { .. })
            | Some(Command::Version)
            | Some(Command::Update { .. })
    )
}

/// Which folder would this invocation use amenbo in — the one [`refuse_a_nested_worktree`] judges — or
/// `None` when the command is outside the guard's reach. Usually the CWD, but `bind --dir <path>` places its
/// pointer elsewhere, and the hazard belongs to the folder that receives the pointer rather than to the one
/// the command was typed in. A `--dir` that names no directory is left to `bind` to report.
///
/// Out of reach are the commands that place no pointer and read no store (`version` / `update` / `lint`),
/// and `unbind` — the way *out*. Refusing that one would strand a pointer an older build wrote, leaving a
/// text editor as the only way to remove it, and its single store write forgets this folder's registration:
/// that cleans the binding up rather than driving the backlog with it. So is `plugin-runner`: it is handed
/// the store to work and inherits only its launcher's directory, so the walk this guard makes would answer
/// about a folder it never consulted — and refusing it would stall that queue over where a command was typed.
fn nested_guard_target(cmd: &Option<Command>) -> Option<std::path::PathBuf> {
    match cmd {
        Some(Command::Version)
        | Some(Command::Update { .. })
        | Some(Command::Lint { .. })
        | Some(Command::GithookPreCommit)
        | Some(Command::GithookCommitMsg { .. })
        | Some(Command::PluginRunner { .. })
        | Some(Command::Plugin { sub: PluginCmd::Validate { .. } })
        | Some(Command::Unbind { .. }) => None,
        Some(Command::Bind { dir: Some(d), .. }) => {
            let p = std::path::PathBuf::from(d);
            p.is_dir().then(|| std::fs::canonicalize(&p).unwrap_or(p))
        }
        _ => std::env::current_dir().ok(),
    }
}

/// Refuse to use amenbo in a git worktree cut inside an amenbo-managed folder, the sibling of the pointer
/// guard: that one asks whether a binding exists, this one whether the checkout is a place to use one. Such
/// a worktree inherits the project's `.amenbo` through the upward walk while the store it writes to sits in
/// app-data and outlives the checkout, so a throwaway environment would drive the real backlog.
///
/// `bind` and `init` are held to it too, though they carry no pointer to inherit — they *write* one, and the
/// asymmetry is theirs all the same: `init --force` raises a project in the real store, which no
/// `git worktree remove` takes back, and `bind --force` upserts a managed block into CLAUDE.md/AGENTS.md,
/// which in most repositories are tracked. `--force` on either means "overwrite the pointer already there"
/// and says nothing about this hazard, so it buys no passage here. What is refused is only a worktree nested
/// inside a managed tree; parking one beside the project is the way to have a bound one.
///
/// It answers before any dispatch, so a refused invocation neither forward-migrates a store nor raises a
/// project — being ahead of the pointer guard costs that one nothing, since a nested worktree has a bound
/// ancestor and so can never be the bare directory it reports on.
fn refuse_a_nested_worktree(cmd: &Option<Command>) -> Result<(), CliError> {
    match nested_guard_target(cmd).and_then(|dir| worktree::nested(&dir)) {
        Some(nested) => Err(CliError::nested_worktree(
            &nested.worktree_root.to_string_lossy(),
            &nested.bound_dir.to_string_lossy(),
        )),
        None => Ok(()),
    }
}

/// Is there a `.amenbo` pointer in the current directory (or above it)? An explicit AMENBO_HOME /
/// AMENBO_PROJECT_DIR is the caller's business to allow for; this is nothing but the pointer search.
fn pointer_present() -> bool {
    std::env::current_dir()
        .ok()
        .map(|cwd| amenbo_core::binding::find_upward(&cwd).is_some())
        .unwrap_or(false)
}

/// Is the bound folder — the one holding the `.amenbo` this invocation resolved — inside a git checkout?
/// This is what gates the agent spec's `worktree` cycle: git advice reaches only the people git reaches. It
/// asks about that folder rather than the CWD because the binding is what says where the work lives, and a
/// caller with no pointer at all has no such folder, so the answer is no.
fn bound_dir_is_under_git() -> bool {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| amenbo_core::binding::find_upward(&cwd))
        .is_some_and(|(dir, _)| worktree::under_git(&dir))
}

/// Can this invocation reach a store at all — is one named, by a pointer or by env (AMENBO_HOME /
/// AMENBO_PROJECT_DIR)? If not, `Store::open()` would quietly create a new one, so "may we open?" is asked
/// in exactly one place: both the exec guard and the faces that answer without opening (version/update)
/// consult this.
fn store_reachable() -> bool {
    amenbo_core::env::home().is_some()
        || amenbo_core::env::project_dir().is_some()
        || pointer_present()
}

/// Nudge a Linux user off an older *system-wide* install. The retired `.deb`/`.rpm` left the GUI and CLI
/// under `/usr/bin` (root-owned); the per-user build cannot retire those, and by policy it advises
/// rather than auto-strips them. On stderr (so `--json` stdout stays clean), no-op off Linux or once the
/// packages are gone — mirroring the "newer version available" advisory it sits beside. The package name
/// is the productName (`amenbo`); `apt` covers Debian/Ubuntu, `dpkg -r` / `rpm -e` the rest.
fn advise_linux_system_orphan() {
    if amenbo_core::self_update::linux_system_orphan_present() {
        eprintln!(
            "⚠ An older system-wide amenbo is still installed under /usr/bin. Remove it with your \
             package manager: `sudo apt remove amenbo` (or `dpkg -r amenbo` / `rpm -e amenbo`)."
        );
    }
}

/// `version` outside a binding: report only what this build knows about itself. Two things are dropped, and
/// both for the same reason — they cannot be answered without opening a store. `format_version` (the version
/// the store records) has nothing to read, and opening one would create it; and whether to query the upstream
/// latest.json is a store setting (`config.update_check`), so fetching it with the default ON while unable to
/// read that setting would trample the user's opt-out. Both are dropped silently, with zero network traffic.
/// Where a binding exists this function is not reached: the `Command::Version` path answers with both fields.
fn version_unbound(flags: &Flags) -> Result<i32, CliError> {
    let channel = amenbo_core::config::Paths::APP_NAME;
    if flags.json {
        print_json(&json!({
            "version": agent::VERSION,
            "schema_version": agent::SCHEMA_VERSION,
            "channel": channel,
            // No store was opened, so claim no store-derived fact — do not pad these with 0 or false.
            "format_version": serde_json::Value::Null,
            "max_supported_format": amenbo_core::model::FORMAT_VERSION,
            "latest_version": serde_json::Value::Null,
            "update_available": serde_json::Value::Null,
        }));
    } else {
        let suffix = if channel == "amenbo" { String::new() } else { format!(" ({channel})") };
        human(flags, format!("amenbo {}{}", agent::VERSION, suffix));
        human(flags, format!("format: this build opens up to store v{}", amenbo_core::model::FORMAT_VERSION));
    }
    Ok(0)
}

/// The explicit update route: look up this OS's all-in-one installer URL in latest.json and open it. There
/// is no self-update — opening is all it does. Because the user asked for it, the lookup runs regardless of
/// the config toggle, and falls back to the latest-release page if the fetch fails or the OS is not listed.
/// `upstream` is whatever startup already fetched (which does honour the config); reuse it when present,
/// query otherwise (a warm cache means no traffic). Callable from outside a binding, so it never touches
/// the store.
fn update_cmd(
    flags: &Flags,
    upstream: Option<amenbo_core::update_check::LatestRelease>,
    print: bool,
) -> Result<i32, CliError> {
    let latest = match upstream {
        Some(rel) => Some(rel),
        None => amenbo_core::update_check::check(true),
    };
    let url = latest
        .as_ref()
        .map(|r| r.update_url().to_string())
        .unwrap_or_else(|| amenbo_core::update_check::LATEST_RELEASE_PAGE.to_string());
    let newer = latest
        .as_ref()
        .filter(|r| r.is_newer_than(agent::VERSION))
        .map(|r| r.version.clone());
    if flags.json {
        print_json(&json!({
            "action": "update",
            "current_version": agent::VERSION,
            "latest_version": latest.as_ref().map(|r| r.version.clone()),
            "update_available": newer.is_some(),
            "url": url,
            "opened": !print,
        }));
    } else {
        // `--print` is the face that opens nothing (headless / scripts), so it must not say it will.
        match (&newer, print) {
            (Some(v), _) => human(flags, format!("A newer amenbo ({v}) is available (this build is {}).", agent::VERSION)),
            (None, false) => human(flags, format!("This build is {} — no newer version detected (opening the installer anyway).", agent::VERSION)),
            (None, true) => human(flags, format!("This build is {} — no newer version detected.", agent::VERSION)),
        }
        human(flags, format!("Installer: {url}"));
    }
    if !print {
        os_open(&url)?;
    }
    Ok(0)
}

/// `amenbo update --apply`: self-update the standalone CLI in place. Downloads this platform's CLI
/// archive over TLS, checks version monotonicity, and swaps the running binary — no installer, no
/// elevation. Reuses whatever startup already fetched (`upstream`), querying otherwise. Callable from
/// outside a binding (a CLI-only user updates without a store), so it never touches the store. The two
/// "correctly declined" outcomes (already current / GUI-managed) are reported as plain messages with a
/// zero exit; genuine failures (download, extract, swap, no archive) are errors.
fn self_update_cmd(
    flags: &Flags,
    upstream: Option<amenbo_core::update_check::LatestRelease>,
) -> Result<i32, CliError> {
    use amenbo_core::self_update::{self, SelfUpdateError};
    let latest = match upstream {
        Some(rel) => Some(rel),
        None => amenbo_core::update_check::check(true),
    };
    let Some(latest) = latest else {
        return Err(CliError {
            code: "io_error",
            message: "could not reach the release manifest to check for an update".to_string(),
            hint: Some(format!("check your connection, or run `{} update` to open the installer.", Paths::command_name())),
            exit: 1,
        });
    };

    match self_update::apply(&latest) {
        Ok(done) => {
            if flags.json {
                print_json(&json!({
                    "action": "self_update",
                    "updated": true,
                    "from": done.from,
                    "to": done.to,
                    "path": done.path.display().to_string(),
                    "backup": done.backup.display().to_string(),
                }));
            } else {
                human(flags, format!("Updated amenbo: {} → {}.", done.from, done.to));
                human(flags, "Restart amenbo to run the new version.");
                human(flags, format!("The previous binary is kept at {} — undo with `{} update --rollback`.", done.backup.display(), Paths::command_name()));
            }
            Ok(0)
        }
        // Not failures: already current, or a GUI-managed CLI that the desktop app updates. Report
        // plainly with a zero exit.
        Err(e @ (SelfUpdateError::UpToDate { .. } | SelfUpdateError::GuiManaged { .. })) => {
            if flags.json {
                let (updated, reason) = match &e {
                    SelfUpdateError::UpToDate { .. } => (false, "up_to_date"),
                    SelfUpdateError::GuiManaged { .. } => (false, "gui_managed"),
                    _ => unreachable!(),
                };
                print_json(&json!({
                    "action": "self_update",
                    "updated": updated,
                    "reason": reason,
                    "current_version": agent::VERSION,
                    "latest_version": latest.version,
                    "message": e.to_string(),
                }));
            } else {
                human(flags, e.to_string());
                if matches!(e, SelfUpdateError::GuiManaged { .. }) {
                    human(flags, format!("Run `{} update` to open the desktop installer instead.", Paths::command_name()));
                }
            }
            Ok(0)
        }
        // Genuine failures — e.g. no CLI archive listed for this platform (fall back to the installer).
        Err(e) => {
            let hint = match e {
                SelfUpdateError::NoArchive { .. } => {
                    Some(format!("run `{} update` to open the installer instead.", Paths::command_name()))
                }
                _ => Some(format!("try again, or run `{} update` to open the installer.", Paths::command_name())),
            };
            Err(CliError { code: "io_error", message: e.to_string(), hint, exit: 1 })
        }
    }
}

/// `amenbo update --rollback`: undo the last `--apply` by restoring the binary it retained beside the
/// running one — offline and instant, no download and no version check (a rollback is a deliberate
/// downgrade). Touches no store, like `--apply`. `NoBackup` (nothing was retained) and `GuiManaged` (the
/// desktop app owns a bundled CLI, so self-replace does not apply) are reported plainly with a zero exit;
/// a failed restore is a genuine error.
fn self_rollback_cmd(flags: &Flags) -> Result<i32, CliError> {
    use amenbo_core::self_update::{self, SelfUpdateError};
    match self_update::rollback() {
        Ok(done) => {
            let restored = done.restored.clone();
            if flags.json {
                print_json(&json!({
                    "action": "self_rollback",
                    "rolled_back": true,
                    "from": done.from,
                    "restored": restored,
                    "path": done.path.display().to_string(),
                }));
            } else {
                match &restored {
                    Some(v) => human(flags, format!("Rolled back amenbo: {} → {}.", done.from, v)),
                    None => human(flags, format!("Rolled back amenbo from {} to the previous version.", done.from)),
                }
                human(flags, "Restart amenbo to run the restored version.");
            }
            Ok(0)
        }
        // Not failures: nothing retained to roll back to, or a GUI-managed CLI that does not self-replace
        // here. Report plainly with a zero exit.
        Err(e @ (SelfUpdateError::NoBackup { .. } | SelfUpdateError::GuiManaged { .. })) => {
            if flags.json {
                let reason = match &e {
                    SelfUpdateError::NoBackup { .. } => "no_backup",
                    SelfUpdateError::GuiManaged { .. } => "gui_managed",
                    _ => unreachable!(),
                };
                print_json(&json!({
                    "action": "self_rollback",
                    "rolled_back": false,
                    "reason": reason,
                    "current_version": agent::VERSION,
                    "message": e.to_string(),
                }));
            } else {
                human(flags, e.to_string());
                if matches!(e, SelfUpdateError::GuiManaged { .. }) {
                    human(flags, "The desktop app owns updates for this CLI — use its own version history.");
                }
            }
            Ok(0)
        }
        // A genuine failed restore.
        Err(e) => Err(CliError {
            code: "io_error",
            message: e.to_string(),
            hint: Some(format!("try again, or run `{} update` to reinstall from the installer.", Paths::command_name())),
            exit: 1,
        }),
    }
}

/// `amenbo lint`: report the amenbo refs in text on its way out of this store, and exit non-zero if there
/// are any. The exit code is the whole verdict, because the callers that matter are machines: a git hook
/// and CI both judge by it, and an AI runs this before it commits — so a hit is not rendered as a
/// `CliError`, since the run succeeded and what it found is the finding. `--quiet` silences the report and
/// leaves the code, for a caller that wants only the verdict; the hook amenbo installs does not pass it,
/// because a person whose commit was just refused has to be told what refused it. It opens no store: it
/// reads the text it is handed and judges it on the `AMB-` prefix alone.
fn lint_cmd(flags: &Flags, paths: Vec<String>, stdin: bool) -> Result<i32, CliError> {
    let (hits, scanned) = if stdin {
        use std::io::Read;
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text).map_err(|e| CliError {
            code: "io_error",
            message: format!("Cannot read from stdin: {e}"),
            hint: None,
            exit: 1,
        })?;
        (amenbo_core::lint::scan_text(STDIN_LABEL, &text), STDIN_LABEL.to_string())
    } else if !paths.is_empty() {
        let mut hits = Vec::new();
        for path in &paths {
            let text = std::fs::read_to_string(path).map_err(|e| CliError {
                code: "io_error",
                message: format!("Cannot read {path}: {e}"),
                hint: None,
                exit: 1,
            })?;
            hits.extend(amenbo_core::lint::scan_text(path, &text));
        }
        (hits, paths.join(", "))
    } else {
        // The default: what `git commit` is about to record, from wherever the caller stands (a hook runs at
        // the repo root, a person may not).
        let cwd = std::env::current_dir().map_err(|e| CliError {
            code: "io_error",
            message: format!("Cannot read the current directory: {e}"),
            hint: None,
            exit: 1,
        })?;
        let diff = amenbo_core::lint::staged_diff(&cwd).map_err(CliError::from)?;
        (amenbo_core::lint::scan_diff(&diff), "the staged diff".to_string())
    };

    if flags.json {
        print_json(&json!({ "ok": hits.is_empty(), "scanned": scanned, "count": hits.len(), "hits": hits }));
    } else if hits.is_empty() {
        human(flags, format!("lint: ok — no amenbo refs in {scanned}."));
    } else {
        for h in &hits {
            human(flags, format!("{}:{}: {}", h.path, h.line, h.reference));
        }
        human(
            flags,
            format!(
                "✗ lint: {} amenbo ref(s) in {scanned}. An id resolves only in this store — remove them, or spell out what they say.",
                hits.len()
            ),
        );
    }
    Ok(if hits.is_empty() { 0 } else { 1 })
}

/// What the report calls piped text — no path to name it by, so name the stream.
const STDIN_LABEL: &str = "<stdin>";

/// `plugin validate <path>` — check a manifest file against the catalog rules (`AMB-D-354`), the
/// author-facing face of the very validator the door uses ([`amenbo_core::plugin_validate`]). A `.json`
/// path is read as JSON (the aggregated `catalog.json` form), anything else as YAML (the `.yaml` form
/// authored in the catalog repo). A parse failure is itself a fail-closed refusal — a manifest missing a
/// required field is the shape half of the door — so it is reported as a problem, not surfaced as a crash.
/// Exits non-zero when the manifest is invalid, dropping cleanly into a pre-submit check.
///
/// On `--json` a passing manifest also carries the manifest amenbo read, in three shapes: `manifest`, the
/// whole serde body (`AMB-T-2109`), and `entry` / `detail`, that same body split into the two documents the
/// catalog serves (`AMB-D-385`). Either way the catalog aggregator publishes what amenbo hands it rather
/// than keeping its own list of fields to copy, which silently drops a field amenbo later adds. All three
/// ride back only when the manifest passes: a parse error read nothing, and a rule-breaking manifest is
/// refused at the door.
fn plugin_validate_cmd(flags: &Flags, path: String) -> Result<i32, CliError> {
    let text = std::fs::read_to_string(&path).map_err(|e| CliError {
        code: "io_error",
        message: format!("Cannot read {path}: {e}"),
        hint: None,
        exit: 1,
    })?;
    let is_json =
        std::path::Path::new(&path).extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
    let parsed: Result<amenbo_core::plugin_manifest::Manifest, String> = if is_json {
        serde_json::from_str(&text).map_err(|e| e.to_string())
    } else {
        serde_norway::from_str(&text).map_err(|e| e.to_string())
    };
    let manifest = match parsed {
        Ok(m) => m,
        Err(e) => {
            if flags.json {
                print_json(&json!({ "ok": false, "path": path, "parse_error": e, "problems": [] }));
            } else {
                human(flags, format!("✗ {path}: not a valid manifest — {e}"));
            }
            return Ok(1);
        }
    };

    let problems = amenbo_core::plugin_validate::validate_manifest(&manifest);
    if flags.json {
        let arr: Vec<_> = problems
            .iter()
            .map(|p| {
                json!({
                    "location": p.location,
                    "code": p.code.as_str(),
                    "message": p.message.en(),
                    "message_ja": p.message.ja(),
                })
            })
            .collect();
        let mut out = json!({ "ok": problems.is_empty(), "path": path, "count": problems.len(), "problems": arr });
        // When the manifest passes, hand the caller the manifest amenbo *read* — the whole serde shape,
        // so a consumer (the catalog's aggregator) writes install entries from it without keeping its own
        // list of which fields to copy, a list that silently drops any field amenbo later adds (`AMB-T-2105`
        // lost `scope`/`events` that way). Present exactly when `ok`: a parse error leaves nothing to read,
        // and a manifest that broke a rule is refused at the door, so neither carries a `manifest` here.
        // `skip_serializing_if` keeps an omitted optional field omitted, so the emitted body round-trips
        // what the author wrote.
        if problems.is_empty() {
            out["manifest"] = serde_json::to_value(&manifest).unwrap();
            // …and the same manifest split into the two documents the catalog serves (`AMB-D-385`): the
            // `entry` everyone fetches to draw the list, and the `detail` fetched for one plugin at a time.
            // The split is amenbo's (`amenbo_core::plugin_wire`) for the same reason the body above is —
            // an aggregator that decided which half a field belongs to would be keeping the list of fields
            // all over again. `entry` carries `added_at`, `detail_sum` and `featured` as empty slots the
            // catalog CI fills; none of them is knowable from a manifest alone.
            let (entry, detail) = amenbo_core::plugin_wire::split(&manifest);
            out["entry"] = serde_json::to_value(&entry).unwrap();
            out["detail"] = serde_json::to_value(&detail).unwrap();
        }
        print_json(&out);
    } else if problems.is_empty() {
        human(flags, format!("plugin validate: ok — {path} is a valid manifest."));
    } else {
        for p in &problems {
            human(flags, format!("{}: {}", p.location, p.message.en()));
        }
        human(flags, format!("✗ plugin validate: {} problem(s) in {path}.", problems.len()));
    }
    Ok(if problems.is_empty() { 0 } else { 1 })
}

/// The store-opening half of the `plugin` group: this machine's installed plugins and their gates
/// (`AMB-D-350`/`AMB-D-351`). `validate` is not here — it opens no store and is answered before the store
/// is ever opened.
fn plugin_cmd(store: &mut Store, flags: &Flags, sub: PluginCmd) -> Result<i32, CliError> {
    match sub {
        PluginCmd::Validate { .. } => unreachable!("handled before open"),
        PluginCmd::List => plugin_list_cmd(store, flags),
        PluginCmd::Install { name } => plugin_install_cmd(store, flags, &name),
        PluginCmd::Enable { name } => plugin_enable_cmd(store, flags, &name),
        PluginCmd::Disable { name } => plugin_disable_cmd(store, flags, &name),
        PluginCmd::Uninstall { name } => plugin_uninstall_cmd(store, flags, &name),
        PluginCmd::Run { name, args } => plugin_run_cmd(store, flags, &name, &args),
        PluginCmd::Log { name } => plugin_log_cmd(store, flags, name.as_deref()),
        PluginCmd::Update { name, check, all } => {
            plugin_update_cmd(store, flags, name.as_deref(), check, all)
        }
        PluginCmd::Rollback { name } => plugin_rollback_cmd(store, flags, &name),
        PluginCmd::Config { sub } => match sub {
            PluginConfigCmd::Set { name, key, value, scope } => {
                plugin_config_set_cmd(store, flags, &name, &key, value, &scope)
            }
            PluginConfigCmd::Get { name, key, scope } => {
                plugin_config_get_cmd(store, flags, &name, &key, &scope)
            }
        },
        PluginCmd::Catalog { sub } => match sub {
            PluginCatalogCmd::List => plugin_catalog_list_cmd(store, flags),
            PluginCatalogCmd::Add { url } => plugin_catalog_add_cmd(store, flags, &url),
            PluginCatalogCmd::Remove { url } => plugin_catalog_remove_cmd(store, flags, &url),
        },
    }
}

/// `plugin catalog list` — the catalogs that make up the browsing view (`AMB-T-1980`): the official
/// catalog first, then each registered third-party one in registration order, with how many plugins each
/// offers and whether it answered. Reads caches the incidental way (`plugin_catalog::discover`) — a
/// catalog fresh on disk answers with no request — so a listing is cheap and works offline.
fn plugin_catalog_list_cmd(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    let discovery = amenbo_core::plugin_catalog::discover(&store.paths);
    if flags.json {
        let sources: Vec<_> = discovery
            .sources
            .iter()
            .map(|s| {
                json!({
                    "url": s.url,
                    "official": s.official,
                    "reachable": s.reachable,
                    "offered": s.offered,
                })
            })
            .collect();
        print_json(&json!({
            "ok": true,
            "action": "plugin.catalog.list",
            "plugins_total": discovery.entries.len(),
            "dropped": discovery.dropped.len(),
            "sources": sources,
        }));
    } else {
        human(flags, format!("Catalogs — {} plugins after merge:", discovery.entries.len()));
        for s in &discovery.sources {
            let tag = if s.official { "official" } else { "third-party" };
            let state =
                if s.reachable { format!("{} plugins", s.offered) } else { "unreachable".to_string() };
            human(flags, format!("  [{tag}] {} — {state}", s.url));
        }
    }
    Ok(0)
}

/// `plugin catalog add <url>` — register a third-party catalog and warm its cache so the first browse is
/// ready (`AMB-T-1980`). Registering only widens what discovery *shows*, never what `install` accepts
/// (`AMB-D-371`). An already-registered URL is a no-op; an unreachable one still registers and is retried
/// on the next browse.
fn plugin_catalog_add_cmd(store: &Store, flags: &Flags, url: &str) -> Result<i32, CliError> {
    let added = amenbo_core::plugin_catalog::add_source(&store.paths, url).map_err(CliError::from)?;
    if !added {
        human(flags, format!("Already registered: {url}"));
        if flags.json {
            print_json(&json!({
                "ok": true, "action": "plugin.catalog.add", "url": url, "added": false,
            }));
        }
        return Ok(0);
    }
    // Warm the cache and report what it holds — discovery fetches each source once, so the source we just
    // added is fetched here. Unreachable is not a failure: it stays registered.
    let discovery = amenbo_core::plugin_catalog::discover(&store.paths);
    let (reachable, offered) = discovery
        .sources
        .iter()
        .find(|s| s.url == url)
        .map(|s| (s.reachable, s.offered))
        .unwrap_or((false, 0));
    human(flags, format!("Registered catalog: {url}"));
    if reachable {
        human(flags, format!("  {offered} plugins available to browse."));
    } else {
        human(flags, "  Not reachable yet — it will be retried on the next browse.");
    }
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "plugin.catalog.add", "url": url, "added": true,
            "reachable": reachable, "offered": offered,
        }));
    }
    Ok(0)
}

/// `plugin catalog remove <url>` — unregister a third-party catalog and drop its cached copy
/// (`AMB-T-1980`). An unregistered URL is a no-op.
fn plugin_catalog_remove_cmd(store: &Store, flags: &Flags, url: &str) -> Result<i32, CliError> {
    let removed =
        amenbo_core::plugin_catalog::remove_source(&store.paths, url).map_err(CliError::from)?;
    human(
        flags,
        if removed { format!("Unregistered catalog: {url}") } else { format!("Not registered: {url}") },
    );
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "plugin.catalog.remove", "url": url, "removed": removed,
        }));
    }
    Ok(0)
}

/// Which tier `--scope` names (`AMB-D-356`/`AMB-D-350`) — the same two for a config value and for the
/// enable gate. `machine` is the default that applies everywhere; `project` is this project's override,
/// and *which* project is never named here — it is the effective context ([`bound_project`]): the binding,
/// or a human's `--project`. An AI cannot name one, so for it the binding is the only answer, which is
/// exactly the reach the store enforces.
fn plugin_scope(store: &Store, scope: &str) -> Result<amenbo_core::plugin_config::Scope, CliError> {
    use amenbo_core::plugin_config::Scope;
    match scope {
        "machine" => Ok(Scope::MachineDefault),
        "project" => bound_project(store).map(Scope::Project).ok_or_else(|| project_required(store)),
        other => Err(CliError {
            code: "invalid_value",
            message: format!("unknown --scope '{other}'"),
            hint: Some("Pass --scope machine (the default) or --scope project.".to_string()),
            exit: 2,
        }),
    }
}

/// The declared field this key names, or a refusal that lists the keys the author *did* declare. The
/// manifest is the only thing that says whether a value is a secret (`AMB-D-356`), so a key it does not
/// declare has no storage rule and cannot be written — guessing one is precisely what amenbo must not do.
fn plugin_config_field(
    plugin: &amenbo_core::plugin_subscribe::InstalledPlugin,
    key: &str,
) -> Result<amenbo_core::plugin_manifest::ConfigField, CliError> {
    if let Some(f) = plugin.manifest.config.iter().find(|f| f.key == key) {
        return Ok(f.clone());
    }
    let declared: Vec<&str> = plugin.manifest.config.iter().map(|f| f.key.as_str()).collect();
    let known = if declared.is_empty() { "none".to_string() } else { declared.join(", ") };
    Err(CliError::from(amenbo_core::Error::invalid(
        format!("plugin '{}' declares no setting '{key}' (it declares: {known})", plugin.name),
        format!("プラグイン '{}' に設定 '{key}' はありません（宣言されているのは: {known}）", plugin.name),
    )))
}

/// The value to store: as given, or read whole from stdin when it is `-`. The stdin route exists for
/// secrets — a token on argv is visible in the process list and lands in shell history — so it drops the
/// trailing newline a pipe adds, and nothing else: whitespace inside a value can be significant, and the
/// write boundary stores what it is handed verbatim.
fn plugin_config_value(value: String) -> Result<String, CliError> {
    if value != "-" {
        return Ok(value);
    }
    if std::io::stdin().is_terminal() {
        return Err(CliError {
            code: "invalid_value",
            message: "`-` says the value comes in on stdin, but stdin is a terminal".to_string(),
            hint: Some(format!("Pipe the value in (`… | {} plugin config set … -`), or pass it directly.", Paths::command_name())),
            exit: 2,
        });
    }
    use std::io::Read;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| CliError {
        code: "io_error",
        message: format!("Cannot read the value from stdin: {e}"),
        hint: None,
        exit: 1,
    })?;
    Ok(s.strip_suffix('\n').map(|t| t.strip_suffix('\r').unwrap_or(t)).unwrap_or(&s).to_string())
}

/// `plugin config set <name> <key> <value>` — the CLI face of the one config write boundary
/// ([`amenbo_core::plugin_config::set`]), which is where the safe floor and the secret routing live
/// (`AMB-D-356`). This side does two things and no more: read the installed manifest off disk to find the
/// field the key names, and turn `--scope` into a tier. **The value is never echoed back**, secret or not —
/// there is nothing to confirm that the caller did not just type.
fn plugin_config_set_cmd(
    store: &mut Store,
    flags: &Flags,
    name: &str,
    key: &str,
    value: String,
    scope: &str,
) -> Result<i32, CliError> {
    let plugin = amenbo_core::plugin_installed::read(&store.paths, name).map_err(CliError::from)?;
    let field = plugin_config_field(&plugin, key)?;
    let scope = plugin_scope(store, scope)?;
    let value = plugin_config_value(value)?;
    let cleared = value.is_empty();
    amenbo_core::plugin_config::set(store, &field, name, &value, scope).map_err(CliError::from)?;

    let where_ = plugin_config_tier(&field, scope);
    human(
        flags,
        if cleared {
            format!("Cleared {name}.{key} ({where_})")
        } else {
            format!("Set {name}.{key} ({where_})")
        },
    );
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "plugin.config.set", "plugin": name, "key": key,
            "secret": field.secret, "scope": where_, "cleared": cleared,
        }));
    }
    Ok(0)
}

/// `plugin config get <name> <key>` — read one setting back at the tier `--scope` names. A secret's value
/// does not come out here: the face reports that one is set and stops, because a `get` that prints a token
/// puts it in the terminal, the scrollback and the shell's history. Injection reads secrets whole, at run
/// time, into the plugin's environment and nowhere else (`AMB-D-356`).
fn plugin_config_get_cmd(
    store: &mut Store,
    flags: &Flags,
    name: &str,
    key: &str,
    scope: &str,
) -> Result<i32, CliError> {
    let plugin = amenbo_core::plugin_installed::read(&store.paths, name).map_err(CliError::from)?;
    let field = plugin_config_field(&plugin, key)?;
    let scope = plugin_scope(store, scope)?;
    let value = amenbo_core::plugin_config::get(store, &field, name, scope).map_err(CliError::from)?;

    let where_ = plugin_config_tier(&field, scope);
    let set = value.is_some();
    if field.secret {
        human(flags, format!("{name}.{key} ({where_}): {}", if set { "set (not shown)" } else { "not set" }));
    } else {
        human(flags, format!("{name}.{key} ({where_}): {}", value.as_deref().unwrap_or("(not set)")));
    }
    if flags.json {
        let mut out = json!({
            "ok": true, "action": "plugin.config.get", "plugin": name, "key": key,
            "secret": field.secret, "scope": where_, "set": set,
        });
        // A secret's value never leaves through this door, --json included: a machine reader wants to know
        // whether the setting is filled, and injection is the only thing that needs the value itself.
        if !field.secret {
            out["value"] = json!(value);
        }
        print_json(&out);
    }
    Ok(0)
}

/// Where a value actually lands, for the caller to read back. It is not simply what `--scope` said: a
/// secret ignores the tiers entirely and goes to the user-area secret file, and saying `machine` there
/// would describe a place the value is not in.
fn plugin_config_tier(
    field: &amenbo_core::plugin_manifest::ConfigField,
    scope: amenbo_core::plugin_config::Scope,
) -> &'static str {
    use amenbo_core::plugin_config::Scope;
    if field.secret {
        "secret file"
    } else if matches!(scope, Scope::Project(_)) {
        "project"
    } else {
        "machine"
    }
}

/// `plugin install <name>` — resolve the name in the catalog, fetch its asset, verify its provenance,
/// and lay it down under the app-data `plugins/` directory ([`amenbo_core::plugin_install`]). The one
/// command in this group that touches the network.
///
/// The closing line is not decoration: `install ≠ enable` (`AMB-D-351`), so a caller who stops here has a
/// plugin that will never fire, and the next step is named rather than assumed.
fn plugin_install_cmd(store: &Store, flags: &Flags, name: &str) -> Result<i32, CliError> {
    let installed =
        amenbo_core::plugin_install::install(&store.paths, name).map_err(CliError::from)?;

    human(flags, format!("Installed plugin: {name} — {}", installed.manifest.desc));
    human(flags, format!("It is not enabled yet: `{} plugin enable {name}` opens its gate.", Paths::command_name()));
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "plugin.install", "plugin": name,
            "desc": installed.manifest.desc,
            "author": installed.manifest.author,
            "official": installed.manifest.official,
            "events": installed.manifest.events,
            "home": installed.home.display().to_string(),
            "program": installed.program.display().to_string(),
            "program_bytes": installed.program_bytes,
            "enabled": false,
        }));
    }
    Ok(0)
}

/// `plugin list` — what is installed under the app-data `plugins/` directory, and whose gate is open.
/// The two facts side by side, because `install ≠ enable` (`AMB-D-351`) is the thing a reader most often
/// gets wrong: an installed plugin that never fires is the *normal* state, not a fault.
///
/// Each plugin has exactly one switch, at the level its author declared (`AMB-D-379`), so the listing says
/// **which** level answered as well as what it answered — "on here" and "on for this device" are different
/// facts, and a reader who cannot tell them apart cannot tell where to go and change one. A project-scoped
/// plugin read from outside any project has no answer at all rather than a made-up one.
///
/// **An open gate is not the same as a plugin that fires**, so the listing carries the compatibility
/// verdict beside it (`AMB-D-359`). The dispatch resolver warns and drops a plugin this build cannot speak
/// to — and amenbo updates underneath an install, so a plugin enabled while it was compatible can stop
/// firing without anyone touching it. Left to "enabled" alone, that state is readable only in the log.
///
/// It also carries the "update available" mark (`AMB-D-359`): the last-fetched catalog holds a different
/// build of an installed plugin. Read from the **cache** (`plugin_update::available_cached`), never a
/// fetch — the listing stays network-free and answers the same offline. Refreshing the catalog and
/// applying the update are the explicit `plugin update --check` / `plugin update <name>`; the listing only
/// surfaces the fact, quietly.
fn plugin_list_cmd(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    use amenbo_core::plugin_compat;
    use amenbo_core::plugin_manifest::Scope;
    use amenbo_core::plugin_trust::{effective_enabled_in, gate_for};

    let installed =
        amenbo_core::plugin_installed::installed(&store.paths).map_err(CliError::from)?;
    // Which installs the cached catalog's list says something has moved about — best-effort, no network:
    // an absent or unreadable cache is simply no marks, and `plugin update --check` is the surface that
    // refreshes it and reads what actually moved.
    let updatable: std::collections::HashSet<String> =
        amenbo_core::plugin_update::available_cached(&store.paths)
            .into_iter()
            .map(|c| c.name)
            .collect();
    let here = bound_project(store);
    // `None` = this plugin's switch cannot be answered from where we stand (a project-scoped plugin, and
    // no project in context).
    let gate_of = |scope: Scope, name: &str| -> Result<Option<bool>, CliError> {
        match gate_for(scope, here) {
            Ok(gate) => Ok(Some(effective_enabled_in(store, name, gate).map_err(CliError::from)?)),
            Err(_) => Ok(None),
        }
    };

    if flags.json {
        let mut rows = Vec::with_capacity(installed.len());
        for p in &installed {
            let why = plugin_compat::check(&p.manifest).err();
            rows.push(json!({
                "name": p.name,
                "desc": p.manifest.desc,
                "author": p.manifest.author,
                "official": p.manifest.official,
                "scope": p.manifest.scope.as_str(),
                "enabled": gate_of(p.manifest.scope, &p.name)?,
                "compatible": why.is_none(),
                "incompatible_reason": why.map(|why| why.to_string()),
                "update_available": updatable.contains(&p.name),
                "consented": store.config.plugin_consented(&p.name),
                "events": p.manifest.events,
                "program": p.program.display().to_string(),
            }));
        }
        print_json(&json!({
            "count": rows.len(),
            "plugins_dir": store.paths.plugins_dir().display().to_string(),
            "plugins": rows,
        }));
    } else if installed.is_empty() {
        human(flags, format!("No plugins installed ({}).", store.paths.plugins_dir().display()));
    } else {
        for p in &installed {
            let where_ = match p.manifest.scope {
                Scope::Machine => "this device",
                Scope::Project => "this project",
            };
            let open = gate_of(p.manifest.scope, &p.name)?;
            let gate = match open {
                Some(true) => format!("enabled ({where_})"),
                Some(false) => format!("disabled ({where_})"),
                None => "per project — open a project to see".to_string(),
            };
            let badge = if p.manifest.official { " [official]" } else { "" };
            // A quiet badge, not a nag (`AMB-D-359`): the fact sits on the line, and applying it is the
            // explicit `plugin update <name>`.
            let update = if updatable.contains(&p.name) { " [update available]" } else { "" };
            human(flags, format!("{}  {gate}{badge}{update}  {}", p.name, p.manifest.desc));
            if let Err(why) = plugin_compat::check(&p.manifest) {
                // The consequence, not just the verdict: an open gate reads as "this one is working"
                // until the line says otherwise, and that gap is the whole point of showing this here.
                let effect = match open {
                    Some(true) => "enabled, but nothing fires",
                    _ => "cannot run against this amenbo",
                };
                human(flags, format!("    {effect}: {why}"));
            }
        }
    }
    Ok(0)
}

/// `plugin log` — the execution log, read back (`AMB-D-361`).
///
/// The write side has been landing runs since the dispatcher started firing; this is the first thing that
/// reads them. It exists because a hook is fire-and-forget (`AMB-D-352`): nobody waits on it and nothing
/// fails when it fails, so "my plugin did nothing" has no answer anywhere else. What answers it is the
/// plugin's own stderr (`AMB-D-353`), so a run that did not end cleanly carries that text under its line
/// rather than only into `--json`.
///
/// No paging, no window flags: the log is bounded by construction (the last runs of each installed
/// plugin), so the whole file *is* the recent window.
///
/// It leads with the **dispatch cursor**, and the face that last advanced it (`AMB-D-380`), because the
/// questions this command is opened for are answered by the two together: the log says what ran, the cursor
/// says how far delivery got, and a double fire or a miss is the disagreement between them. Reading them
/// apart would leave a reader correlating two commands by hand — so this one reads the store's two meta rows
/// as well as the machine-local file. The face is a stamp for that correlation and nothing else: it names
/// who delivered a span, never whose turn is next.
fn plugin_log_cmd(store: &Store, flags: &Flags, name: Option<&str>) -> Result<i32, CliError> {
    use amenbo_core::plugin_log::{self, Outcome};

    let path = store.paths.plugin_log_file();
    let cursor = amenbo_core::plugin_drive::persisted_cursor(store.read_model())?;
    let cursor_face = amenbo_core::plugin_drive::persisted_cursor_face(store.read_model())?;
    // Newest first either way — the run a reader is looking for is nearly always the last one.
    let lines = match name {
        Some(name) => plugin_log::recent(&path, name),
        None => {
            let mut all = plugin_log::read(&path);
            all.reverse();
            all
        }
    };

    if flags.json {
        let rows: Vec<_> = lines
            .iter()
            .map(|l| {
                json!({
                    "at": l.at.to_rfc3339_z(),
                    "plugin": l.plugin,
                    "event": l.event,
                    "outcome": l.outcome.as_str(),
                    "code": l.code,
                    "elapsed_ms": l.elapsed_ms,
                    "stderr": l.stderr,
                })
            })
            .collect();
        print_json(&json!({
            "count": rows.len(),
            "dispatch": {
                "cursor": cursor,
                "cursor_face": cursor_face.map(|f| f.as_str()),
            },
            "log": path.display().to_string(),
            "plugin": name,
            "runs": rows,
        }));
        return Ok(0);
    }

    human(flags, dispatch_cursor_line(cursor, cursor_face));
    if lines.is_empty() {
        match name {
            Some(name) => human(flags, format!("No runs recorded for plugin '{name}'.")),
            None => human(flags, format!("No plugin runs recorded ({}).", path.display())),
        }
    } else {
        for l in &lines {
            let at = l.at.to_rfc3339_z();
            if l.outcome == Outcome::Gap {
                // Not a run: it names no plugin and no event, so a row of dashes would be six columns of
                // nothing. What was lost cannot be named — the fact and its instant are the whole content.
                human(flags, format!("{at}  gap — events fired that reached nobody (they aged out before the dispatcher read them)"));
                continue;
            }
            let code = match l.code {
                Some(code) => format!("exit {code}"),
                None => "no exit code".to_string(),
            };
            human(
                flags,
                format!(
                    "{at}  {}  {}  {}  {code}  {}ms",
                    l.plugin,
                    l.event,
                    l.outcome.as_str(),
                    l.elapsed_ms
                ),
            );
            // The diagnosis, where the author put it. Held back for a clean run so a listing stays
            // scannable — `--json` carries it either way, for a reader that wants all of it.
            if l.outcome != Outcome::Ok {
                for text in l.stderr.lines() {
                    human(flags, format!("    {text}"));
                }
            }
        }
    }
    Ok(0)
}

/// The dispatch-cursor line `plugin log` leads with: how far this store's outbox has been fanned out onto
/// the plugins' queues, and which face took it there (`AMB-D-380`, `AMB-D-399`).
///
/// A cursor of `0` with no face is a store nothing has ever been handed out from, which is a different fact
/// from an empty log — a plugin that never fired and a dispatcher that never ran read the same in the runs
/// below, and this line is what tells them apart. A cursor standing at some id with no face beside it is the
/// third shape: fanned out by a build that did not stamp one.
///
/// Split out from the command so the wording is one string, testable without a store, and so the two callers
/// (`--json` and the listing) cannot drift into saying different things.
fn dispatch_cursor_line(cursor: i64, face: Option<amenbo_core::plugin_drive::Face>) -> String {
    if cursor == 0 && face.is_none() {
        return "dispatch cursor 0 — nothing has been delivered from this store yet".to_string();
    }
    match face {
        Some(face) => format!("dispatch cursor {cursor} · last advanced by {}", face.as_str()),
        None => format!("dispatch cursor {cursor} · advanced by an unrecorded face"),
    }
}

/// `plugin update` — the one command, and which of its three jobs this invocation asked for
/// (`AMB-D-359`).
///
/// Reporting and applying are the same subject and deliberately not the same act, so the form has to say
/// which: `--check` reports, a name applies one, `--all` applies every one. Nothing is the fourth case and
/// it is refused rather than guessed at — defaulting a bare `plugin update` to either side would make the
/// safe reading and the replacing one a typo apart.
fn plugin_update_cmd(
    store: &Store,
    flags: &Flags,
    name: Option<&str>,
    check: bool,
    all: bool,
) -> Result<i32, CliError> {
    let cmd = Paths::command_name();
    let misuse = |message: String, hint: String| CliError {
        code: "invalid_value",
        message,
        hint: Some(hint),
        exit: 2,
    };
    match (check, all, name) {
        (true, false, None) => plugin_update_check_cmd(store, flags),
        (true, _, _) => Err(misuse(
            "--check reports every install and applies nothing".to_string(),
            format!("Pass --check on its own, or drop it to apply: `{cmd} plugin update <name>` / `--all`."),
        )),
        (false, true, Some(name)) => Err(misuse(
            format!("--all is every installed plugin, so it cannot also name '{name}'"),
            format!("Pass one or the other: `{cmd} plugin update <name>`, or `{cmd} plugin update --all`."),
        )),
        (false, true, None) => plugin_update_all_cmd(store, flags),
        (false, false, Some(name)) => plugin_update_apply_cmd(store, flags, name),
        (false, false, None) => Err(misuse(
            "say what to update".to_string(),
            format!("`{cmd} plugin update --check` to see what there is, `<name>` or `--all` to apply it."),
        )),
    }
}

/// `plugin update --check` — which installed plugins the catalog holds a different build of
/// (`AMB-D-359`).
///
/// Reports and stops there. Applying is a separate, explicit act, and keeping the two apart is what lets
/// this be offered freely: nothing here downloads, verifies or replaces anything, so the worst a check
/// costs is one fetch of the whole index — and none at all inside the freshness window, or with nothing
/// installed.
///
/// A plugin the catalog does not list is not reported: installed by hand, or delisted since, and neither
/// is something an update could answer.
fn plugin_update_check_cmd(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    let updates =
        amenbo_core::plugin_update::available(&store.paths).map_err(CliError::from)?;
    let here = amenbo_core::plugin_manifest::Platform::here();

    if flags.json {
        let rows: Vec<_> = updates
            .iter()
            .map(|u| {
                // This machine's distributable on both sides (`AMB-D-381`/`AMB-D-384`) — the digests the
                // detection actually compared, resolved os-arch then os, not another platform's.
                let installed = here.and_then(|p| u.installed.asset_for(p));
                let available = here.and_then(|p| u.available.asset_for(p));
                json!({
                    "name": u.name,
                    "desc": u.available.desc,
                    "installed_checksum": installed.map(|a| a.checksum),
                    "available_checksum": available.as_ref().map(|a| a.checksum.clone()),
                    "url": available.map(|a| a.url),
                })
            })
            .collect();
        print_json(&json!({ "count": rows.len(), "updates": rows }));
    } else if updates.is_empty() {
        human(flags, "Everything installed matches what the catalog publishes.");
    } else {
        for u in &updates {
            human(flags, format!("{}  update available  {}", u.name, u.available.desc));
        }
    }
    Ok(0)
}

/// `plugin update <name>` — put the catalog's build of one plugin in place (`AMB-D-359`).
///
/// A plugin already on that build is reported as such and is not a failure: the command's promise is
/// "this plugin is now what the catalog publishes", and there is a way of meeting it that fetches
/// nothing. What is said afterwards is what a reader most needs to know they did *not* just lose — the
/// gate and the settings are still there — and where the build that was replaced went.
fn plugin_update_apply_cmd(store: &Store, flags: &Flags, name: &str) -> Result<i32, CliError> {
    let applied = amenbo_core::plugin_update::apply(&store.paths, name, |available| {
        refuse_update_leaving_required_unset(store, available)
    })
    .map_err(CliError::from)?;

    match applied {
        None => {
            human(flags, format!("Plugin '{name}' is already the build the catalog publishes."));
            if flags.json {
                print_json(&json!({
                    "ok": true, "action": "plugin.update", "plugin": name, "applied": false,
                }));
            }
        }
        Some(r) => {
            human(flags, format!("Updated plugin: {name} — {}", r.to.desc));
            human(flags, "Its gate, settings and secrets are unchanged.");
            human(flags, format!("The build it replaced is kept at {}.", r.backup.display()));
            if flags.json {
                print_json(&json!({
                    "ok": true, "action": "plugin.update", "plugin": name, "applied": true,
                    "desc": r.to.desc,
                    "program": r.program.display().to_string(),
                    "program_bytes": r.program_bytes,
                    "backup": r.backup.display().to_string(),
                }));
            }
        }
    }
    Ok(0)
}

/// `plugin update --all` — every update the catalog holds, applied one plugin at a time (`AMB-D-359`).
///
/// Best-effort across plugins: one whose asset will not verify is reported and the rest are still
/// applied, because a single bad entry holding back every other update is the worse failure. It still
/// exits non-zero when anything failed — the run is not a success just because most of it worked.
fn plugin_update_all_cmd(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    use amenbo_core::plugin_update::Outcome;

    let outcomes = amenbo_core::plugin_update::apply_all(&store.paths, |available| {
        refuse_update_leaving_required_unset(store, available)
    })
    .map_err(CliError::from)?;
    let failed = outcomes.iter().filter(|o| matches!(o, Outcome::Failed { .. })).count();

    if outcomes.is_empty() {
        human(flags, "Everything installed matches what the catalog publishes.");
    }
    for outcome in &outcomes {
        match outcome {
            Outcome::Replaced(r) => {
                human(flags, format!("{}  updated  {}", r.name, r.to.desc));
            }
            Outcome::Failed { name, error } => {
                human(flags, format!("{name}  not updated  {} (it is as it was)", error.message_en()));
            }
        }
    }
    if flags.json {
        let rows: Vec<_> = outcomes
            .iter()
            .map(|o| match o {
                Outcome::Replaced(r) => json!({
                    "name": r.name, "applied": true, "desc": r.to.desc,
                    "program_bytes": r.program_bytes,
                    "backup": r.backup.display().to_string(),
                }),
                Outcome::Failed { name, error } => json!({
                    "name": name, "applied": false,
                    "error": error.code(), "message": error.message_en(),
                }),
            })
            .collect();
        print_json(&json!({
            "ok": failed == 0, "action": "plugin.update",
            "count": rows.len(), "failed": failed, "updates": rows,
        }));
    }
    Ok(if failed == 0 { 0 } else { 1 })
}

/// `plugin rollback <name>` — restore the build the last update replaced (`AMB-D-359`).
///
/// Says what a reader most needs to know afterwards: which build is running again, and that the gate and
/// settings did not move with it. The refusals — not installed, or nothing retained — come up from the
/// core with their own wording, so nothing here has to guess which case it is.
fn plugin_rollback_cmd(store: &Store, flags: &Flags, name: &str) -> Result<i32, CliError> {
    let rolled = amenbo_core::plugin_update::rollback(&store.paths, name).map_err(CliError::from)?;

    human(flags, format!("Rolled back plugin: {name} — {}", rolled.restored.desc));
    human(flags, "Its gate, settings and secrets are unchanged.");
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "plugin.rollback", "plugin": name,
            "desc": rolled.restored.desc,
            "program": rolled.program.display().to_string(),
        }));
    }
    Ok(0)
}

/// The gate one installed plugin's declaration names, from where this command is standing
/// (`AMB-D-379`): a `machine` plugin's device-wide switch, or a `project` plugin's switch in the bound
/// project. The refusal for a project-scoped plugin outside any project comes from the boundary itself, so
/// the CLI does not restate the rule.
fn plugin_gate(
    store: &Store,
    scope: amenbo_core::plugin_manifest::Scope,
) -> Result<amenbo_core::plugin_trust::Gate, CliError> {
    amenbo_core::plugin_trust::gate_for(scope, bound_project(store)).map_err(CliError::from)
}

/// How a gate reads back to its caller — the level, never a tier a user would have to pick.
fn plugin_gate_level(gate: amenbo_core::plugin_trust::Gate) -> &'static str {
    match gate {
        amenbo_core::plugin_trust::Gate::Machine => "this device",
        amenbo_core::plugin_trust::Gate::Project(_) => "this project",
    }
}

/// Which config tier the `required` probe should count for this gate (`AMB-D-356`): a project's enable
/// counts that project's overrides on top of the machine defaults; the device's counts the defaults alone.
fn plugin_value_tier(gate: amenbo_core::plugin_trust::Gate) -> amenbo_core::plugin_config::Scope {
    match gate {
        amenbo_core::plugin_trust::Gate::Machine => amenbo_core::plugin_config::Scope::MachineDefault,
        amenbo_core::plugin_trust::Gate::Project(id) => amenbo_core::plugin_config::Scope::Project(id),
    }
}

/// The config re-check an update runs before it replaces a build (`AMB-D-359`), handed to
/// [`amenbo_core::plugin_update::apply`] / `apply_all` as their `approve` gate. It re-judges the **new**
/// manifest's `required` settings the same way `plugin enable` does (`AMB-D-351`/`AMB-D-356`): if the new
/// schema declares a `required` field that no value answers at a gate the plugin is enabled at, the update
/// is held back and the working build stays — the reason names the fields to set first. Aligned with the
/// apply side's fail-before-write posture: the safe reading is to refuse the replacement, not to leave an
/// enabled plugin missing a value its own author marked required.
///
/// The folder this ran in is not part of that: an update replaces the build for every project at once, so
/// the gates judged are all of them (`AMB-D-379`), bound folder or not.
///
/// Which build is held back is [`amenbo_core::plugin_config::required_unset_for_update`]'s call, shared
/// with the GUI's gate; what a terminal is told about it is this command's — hence the `amenbo plugin
/// config set` line, which is the way out from here and nowhere else.
fn refuse_update_leaving_required_unset(
    store: &Store,
    available: &amenbo_core::plugin_manifest::Manifest,
) -> amenbo_core::error::Result<()> {
    let name = available.name.as_str();
    let missing = amenbo_core::plugin_config::required_unset_for_update(store, available)?;
    if missing.is_empty() {
        return Ok(());
    }
    Err(amenbo_core::error::Error::invalid(
        format!(
            "the new build of '{name}' needs setting(s) not provided: {}. Set them first, then update — the build in place is unchanged: `{} plugin config set {name} <key> <value>`",
            missing.join(", "),
            Paths::command_name()
        ),
        format!(
            "'{name}' の新しい版は未入力の必須設定を要求します（{}）。先に設定してから更新してください——今の版はそのまま変わりません：`{} plugin config set {name} <key> <value>`",
            missing.join("、"),
            Paths::command_name()
        ),
    ))
}

/// `plugin enable <name>` — record consent and open **the** gate, through the one boundary that moves that
/// state ([`amenbo_core::plugin_trust`]). There is no `--scope`: the plugin's author declared which switch
/// it has (`AMB-D-379`), so this command always means one thing, and the message says which level it moved.
/// Fail-closed twice over: on the plugin's compatibility declarations
/// ([`amenbo_core::plugin_compat`], `AMB-D-359` — a plugin this amenbo cannot speak to is refused before
/// any consent is recorded), and on the author's `required` settings, probed at the tier that gate reads.
fn plugin_enable_cmd(store: &mut Store, flags: &Flags, name: &str) -> Result<i32, CliError> {
    let plugin = amenbo_core::plugin_installed::read(&store.paths, name).map_err(CliError::from)?;
    amenbo_core::plugin_compat::check(&plugin.manifest)
        .map_err(|incompatible| CliError::from(incompatible.into_error(name)))?;
    let gate = plugin_gate(store, plugin.manifest.scope)?;
    let fields = plugin.manifest.config.clone();
    let tier = plugin_value_tier(gate);
    let satisfied = amenbo_core::plugin_config::satisfied_keys(store, name, &fields, tier)
        .map_err(CliError::from)?;
    let has_value = |f: &amenbo_core::plugin_manifest::ConfigField| {
        satisfied.iter().any(|k| k == &f.key)
    };

    amenbo_core::plugin_trust::enable(store, name, gate, &fields, has_value)
        .map_err(CliError::from)?;

    let where_ = plugin_gate_level(gate);
    human(flags, format!("Enabled plugin: {name} ({where_})"));
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "plugin.enable", "plugin": name,
            "enabled": true, "scope": plugin.manifest.scope.as_str(), "level": where_,
        }));
    }
    Ok(0)
}

/// `plugin disable <name>` — close the gate, keeping the consent (`disable ≠ uninstall`, `AMB-D-357`).
///
/// Deliberately does **not** require the plugin to still read as installed: this is the way to stop a
/// plugin firing, and a broken install is exactly when that is most needed. Without a manifest there is no
/// declaration to read, so it closes **every** gate the name could hold — closing one that was never open
/// costs nothing, and leaving one open because a file would not parse is the failure that matters.
fn plugin_disable_cmd(store: &mut Store, flags: &Flags, name: &str) -> Result<i32, CliError> {
    use amenbo_core::plugin_trust::{disable, effective_enabled_in, Gate};

    let declared = amenbo_core::plugin_installed::read(&store.paths, name)
        .ok()
        .map(|p| p.manifest.scope);
    let Some(scope) = declared else {
        let mut closed = false;
        let mut dropped = 0;
        for gate in [Some(Gate::Machine), bound_project(store).map(Gate::Project)].into_iter().flatten() {
            closed |= effective_enabled_in(store, name, gate).map_err(CliError::from)?;
            dropped += disable(store, name, gate).map_err(CliError::from)?.queued;
        }
        human(
            flags,
            format!("Disabled plugin: {name} (its manifest is unreadable, so every gate it could hold was closed)"),
        );
        say_dropped(flags, dropped);
        if flags.json {
            print_json(&json!({
                "ok": true, "action": "plugin.disable", "plugin": name,
                "enabled": false, "scope": null, "noop": !closed, "dropped_queued": dropped,
            }));
        }
        return Ok(0);
    };

    let gate = plugin_gate(store, scope)?;
    let was_enabled = effective_enabled_in(store, name, gate).map_err(CliError::from)?;
    let stopped = disable(store, name, gate).map_err(CliError::from)?;

    let where_ = plugin_gate_level(gate);
    human(
        flags,
        if was_enabled {
            format!("Disabled plugin: {name} ({where_})")
        } else {
            format!("Plugin already disabled: {name} ({where_})")
        },
    );
    say_dropped(flags, stopped.queued);
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "plugin.disable", "plugin": name,
            "enabled": false, "scope": scope.as_str(), "level": where_, "noop": !was_enabled,
            "dropped_queued": stopped.queued,
        }));
    }
    Ok(0)
}

/// Say what a stop threw away, when it threw anything away (`AMB-D-399`). Silence for nothing dropped: a
/// plugin with an empty queue is the ordinary case, and a line saying so every time would train the reader
/// to skip the one that matters. The events are gone for good — the user is owed the number.
fn say_dropped(flags: &Flags, queued: usize) {
    if queued > 0 {
        human(flags, format!("  {queued} queued event(s) were dropped — a disabled plugin is not caught up afterwards."));
    }
}

/// `plugin uninstall <name>` — remove the plugin and every trace of it (`AMB-D-357`). The confirmation
/// names what goes beyond the binary, because settings and secrets are the part a user does not picture:
/// they are gone device-wide, in every project, and a re-install does not bring them back.
fn plugin_uninstall_cmd(store: &mut Store, flags: &Flags, name: &str) -> Result<i32, CliError> {
    if !confirm(
        flags,
        &format!(
            "uninstall plugin '{name}' (its settings in every project and its secrets go too; a re-install starts clean)"
        ),
    )? {
        return Ok(1);
    }
    let removed = amenbo_core::plugin_uninstall::uninstall(store, name).map_err(CliError::from)?;

    if removed.anything() {
        human(flags, format!("Uninstalled plugin: {name}"));
    } else {
        human(flags, format!("Nothing to uninstall: {name} is not on this machine."));
    }
    if flags.json {
        print_json(&json!({
            "ok": true, "action": "plugin.uninstall", "plugin": name,
            "removed_anything": removed.anything(),
            "removed": {
                "was_enabled": removed.was_enabled,
                "queued": removed.queued,
                "consent": removed.consent,
                "machine_defaults": removed.machine_defaults,
                "secrets": removed.secrets,
                "project_overrides": removed.project_overrides,
                "directory": removed.directory,
                "runs_log": removed.runs_log,
            },
        }));
    }
    Ok(0)
}

/// `plugin run <name> [args...]` — call a plugin's command face and relay what it returned
/// (`AMB-D-353`).
///
/// **This command's stdout belongs to the plugin.** No courtesy line of amenbo's is printed there: the
/// return value is meant to be consumed (`eval "$(…)"`), and anything mixed in would corrupt it. amenbo's
/// own voice goes to stderr, where the plugin's diagnostics are relayed too — first, so they read as
/// context ahead of the value rather than commentary after it. Under `--json` stdout is the document, as
/// everywhere else, and the return value rides inside it.
///
/// A plugin that exits non-zero is a failed call: its return value is discarded (`AMB-D-354`) and this
/// exits 1 — amenbo's own "something went wrong" code, not the plugin's number. Relaying that number
/// instead would collide with the exit codes amenbo itself contracts (2 is bad arguments, whatever the
/// plugin meant by it), so it is reported in the message and in `--json` instead of impersonated.
fn plugin_run_cmd(
    store: &Store,
    flags: &Flags,
    name: &str,
    args: &[String],
) -> Result<i32, CliError> {
    use amenbo_core::plugin_command::CommandOutcome;

    let outcome = amenbo_core::plugin_invoke::call(store, name, args, bound_project(store))
        .map_err(CliError::from)?;

    match outcome {
        CommandOutcome::Returned { value, diagnostic } => {
            eprint!("{diagnostic}");
            if flags.json {
                print_json(&json!({
                    "ok": true, "action": "plugin.run", "plugin": name,
                    "args": args, "value": value, "diagnostic": diagnostic, "code": 0,
                }));
            } else {
                print!("{value}");
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            Ok(0)
        }
        CommandOutcome::Failed { code, diagnostic } => {
            eprint!("{diagnostic}");
            let how = match code {
                Some(code) => format!("exited {code}"),
                None => "was killed by a signal".to_string(),
            };
            Err(CliError::from(amenbo_core::Error::invalid(
                format!("plugin '{name}' {how} — its return value was discarded"),
                format!(
                    "プラグイン '{name}' は{}——戻り値は使いませんでした",
                    match code {
                        Some(code) => format!("終了コード {code} で終わりました"),
                        None => "シグナルで終了しました".to_string(),
                    }
                ),
            )))
        }
    }
}

fn run(cli: Cli, flags: &Flags) -> Result<i32, CliError> {
    // The **plugin face** (`AMB-D-406`): amenbo launched a plugin, and that plugin is calling amenbo back to
    // read what its payload only named. The store to open and the window to read it through were handed over
    // in the environment, so both are read here, once, ahead of everything that would otherwise consult this
    // directory — a plugin's is whatever its launcher happened to be in, and nothing here is decided by it.
    // `None` is every other invocation: nothing launched this as a plugin, and the facet and the binding
    // decide the reach as they always have.
    let plugin_window = amenbo_core::plugin_callback::reach_from_env().map_err(CliError::from)?;
    // Whether this checkout is a place to use amenbo at all is asked before any dispatch: `init` raises a
    // project in the real store and returns below without ever reaching the guards further down, so a
    // refusal that came later would arrive after the damage it exists to prevent. A plugin is outside it for
    // the reason `plugin-runner` is (see `nested_guard_target`): the store it works was named to it, so the
    // upward walk this guard makes would judge a folder that decides nothing here.
    if plugin_window.is_none() {
        refuse_a_nested_worktree(&cli.command)?;
    }
    // Init creates the store itself, so do not open one first.
    match &cli.command {
        Some(Command::Init { name, language, force }) => return init_cmd(flags, name.clone(), language.clone(), *force),
        // unbind is the command that *removes* the marker. Handle it ahead of the pointer exec guard so
        // `--dir` can unbind from a CWD that has no pointer of its own (it deletes `.amenbo` and strips the
        // managed block; cleaning the binding registry does open the store).
        Some(Command::Unbind { dir }) => return unbind_cmd(flags, dir.clone()),
        // Outside a binding, version / update answer without opening a store. Opening one would either
        // create a new store on the spot (the very thing the exec guard below prevents) or forward-migrate an
        // existing store nobody asked us to touch. Neither command needs store content: facts about this
        // build plus the published latest.json are enough. With a binding they fall through to the normal
        // path below, which carries the store's format_version and honours config.update_check.
        // lint reads the text it is handed and nothing else: the `AMB-` prefix is self-declaring, so no
        // store is opened and no id resolved. It therefore sits ahead of the exec guard — the
        // guard's job is to stop a store being created by accident, and a command that opens none has
        // nothing to guard. That is not a concession: CI is exactly where this must run, and there is no
        // `.amenbo` there to find.
        Some(Command::Lint { paths, stdin }) => return lint_cmd(flags, paths.clone(), *stdin),
        // The hook's own entry points, same store-free footing as `lint`: `pre-commit` lints the staged
        // diff (no paths), `commit-msg` lints the message file git hands over.
        Some(Command::GithookPreCommit) => return lint_cmd(flags, Vec::new(), false),
        Some(Command::GithookCommitMsg { path }) => return lint_cmd(flags, vec![path.clone()], false),
        // A plugin runner: amenbo launched this process to work one queue (`AMB-T-2175`). It opens the store
        // it was handed, so it sits ahead of every guard that asks about *this* directory — its own is
        // whatever its launcher happened to be in, and it was never asked to answer for it.
        Some(Command::PluginRunner { plugin, owner, store }) => {
            amenbo_core::plugin_runner::run_process(store.into(), plugin, owner);
            return Ok(0);
        }
        // `plugin validate` reads a manifest file the author points at — no store, no binding, no facet, on
        // the same store-free footing as `lint`. It sits ahead of the exec guard so an author can run it in
        // any directory (their plugin's, a CI checkout), not only a bound one.
        Some(Command::Plugin { sub: PluginCmd::Validate { path } }) => return plugin_validate_cmd(flags, path.clone()),
        Some(Command::Version) if !store_reachable() => {
            advise_linux_system_orphan();
            return version_unbound(flags);
        }
        Some(Command::Update { print, apply, rollback }) if !store_reachable() => {
            advise_linux_system_orphan();
            return if *rollback {
                self_rollback_cmd(flags)
            } else if *apply {
                self_update_cmd(flags, None)
            } else {
                update_cmd(flags, None, *print)
            };
        }
        _ => {}
    }

    // Exec guard (strict). Reaching here means the command neither places the marker (init) nor removes it
    // (unbind), and is not one of the faces that answer without opening a store (version/update, handled
    // above). In a bare directory — no pointer (.amenbo), no AMENBO_HOME/AMENBO_PROJECT_DIR — do not quietly
    // create the single store; tell the user to run init. bind is the exception (it is what places a
    // pointer). `agent` still requires a pointer: the AI's entry point is stopped by location, on purpose.
    // The folderless side door is `--project <name or id>`, which names a project on this same device.
    if requires_pointer(&cli.command) && !store_reachable() {
        // Offer the projects that actually exist on this device as candidates. Do not open the store: listing
        // candidates is no reason to forward-migrate a store the user never asked us to touch.
        let paths = amenbo_core::config::Paths::resolve().map_err(CliError::from)?;
        let candidates: Vec<String> = amenbo_core::store_engine::probe_live_projects(&paths.store_file)
            .into_iter()
            .map(|(id, name)| format!("{}  # {name}", project_label(&id)))
            .collect();
        return Err(CliError::no_pointer(&candidates));
    }
    // Restore sits after the exec guard (it needs to know where this device's store is) and ahead of the
    // migration and the open, because it is the one command that replaces the truth source **wholesale** and
    // therefore never has to read the one it replaces. Both of the steps below would be wrong here:
    //
    // - The **open** refuses a store this build cannot read — including the too-new one
    //   (`store::open::ensure_format_supported`). There is no downgrade, so restoring the pre-migration
    //   backup is the *only* way back from a store a newer build carried past this one — refusing it here
    //   would deny the recovery on precisely the store the recovery exists for.
    // - The **migration** would carry a store forward that this command is about to throw away, and, worse,
    //   the pre-migration backup it takes on the way sweeps the older ones (one rewind point per kind) —
    //   possibly the very archive being restored.
    //
    // What the restore replaces is guarded where the replacing happens: the swap holds the store's swap lock,
    // and the archive's own gates (layout, generation) still refuse what this build cannot carry.
    if let Some(Command::Restore { path }) = &cli.command {
        return run_restore(flags, path.clone());
    }
    // Migration runs here — before the store is opened, before the command matters. There is no saying
    // whether the CLI or the GUI comes up first on this device, so both enter through the same door
    // (`migrate::at_startup`), and whichever arrives second waits there while the other runs. A store already
    // at the current version (the normal case) leaves immediately without even taking the lock.
    migrate_at_startup(flags)?;

    let store = Store::open().map_err(CliError::from)?;

    // Clone detection: warn that this store may have been copied to another machine.
    if store.forked {
        eprintln!(
            "⚠ This store may have been copied to a different machine (hardware identity changed)."
        );
    }

    // The startup kick (`AMB-D-399`), ahead of the command and of the network the update check does: a
    // delivery a previous run left standing is picked up now, whatever this invocation was called to do.
    // A plugin calling amenbo back is not a startup — it is a read from inside a run that was already
    // driven — so it makes none.
    if plugin_window.is_none() {
        resume_dispatch(&store);
    }

    // Ask the upstream (the published latest.json) for the newest version, once. Infrastructure traffic only:
    // no user data goes out. On by default, honours the `update_check` config, has a timeout, fails silently,
    // and is cached for 24h so not every command talks to the network. Fetched once here and reused by
    // `version` / `agent`. `None` means disabled, not fetched, or failed — never a reason to block the work.
    let upstream = amenbo_core::update_check::check(store.config.update_check);

    // If a newer build has been published, say so once, on any command. Non-blocking, and on stderr so
    // `--json` stdout stays clean. Point at `amenbo update` (which opens this OS's all-in-one installer),
    // and give the per-OS installer URL.
    if let Some(rel) = upstream.as_ref() {
        if rel.is_newer_than(agent::VERSION) {
            eprintln!(
                "⚠ A newer amenbo ({}) is available (this build is {}). Run `{} update` to install it, or see {}",
                rel.version,
                agent::VERSION,
                Paths::command_name(),
                rel.update_url(),
            );
        }
    }

    // A one-time nudge for a Linux user who migrated off the old `.deb`/`.rpm` but still has the retired
    // `/usr/bin` copy. Self-clearing and no-op off Linux, so it sits harmlessly beside the version advisory.
    advise_linux_system_orphan();

    // If this invocation is inside a bound folder, whatever that folder still carries on disk — an outdated
    // managed block, a legacy `.amenbo` — is brought up to the current form here: resolving is what repairs
    // it (`binding::resolve_upward`). Run for every actor, once: it is the AI that reads stale guidance, but
    // the only chance to fix it is "amenbo ran in that folder", and who ran it is beside the point. Outside a
    // binding (an `AMENBO_HOME` sandbox, say) nothing happens.
    if let Ok(cwd) = std::env::current_dir() {
        amenbo_core::binding::resolve_upward(&store, &cwd);
    }

    // Decide this surface's reach once, here — from what amenbo handed a plugin it launched (`plugin_window`,
    // above), and otherwise from the facet and the binding. An AI (`--actor ai`) is confined to the project
    // the `.amenbo` points at; a human sees the whole device
    // (the overview is the human's place). Reach is drawn from the binding alone — `--project` never widens
    // it: the resolution below is checked against exactly this reach, and naming an outside project is
    // rejected as out_of_reach. With no binding, an AI's reach is empty, so refuse here. A CWD with neither a
    // pointer nor `AMENBO_HOME` was already stopped by the exec guard above, which leaves the cases "opened
    // via env" and "the pointer names no project, or names one that is gone" — in each, confinement has
    // nothing to bite on, and falling back to All would reduce the binding to decoration. init (which creates
    // the binding) and migrate/unbind (which do not surface store content) are handled before this point and
    // never arrive here; that is the shape of the exceptions.
    let mut store = match plugin_window {
        // The plugin face answers first, and for either facet (`AMB-D-406`). A plugin is neither a human nor
        // an AI — it is a program amenbo started — so its window is not drawn from a facet's default but from
        // the gate it fires through: the device for a `machine` plugin, one project for a `project` one. It
        // cannot come from the binding either, since the folder a plugin runs in has none to read.
        Some(reach) => store.with_reach(reach),
        None => match flags.actor {
            Some(ActorKind::Ai) => {
                let reach = Reach::for_ai(binding_project(&store)).map_err(CliError::from)?;
                store.with_reach(reach)
            }
            // A human sees the device, which is the store's own reach — nothing to narrow. So is a command
            // that declared no facet, and that arm is not a human default in disguise: `uses_facet` let
            // through only the commands that surface no store content (version / agent / whoami / config /
            // bind …), so the reach they keep is never consulted.
            Some(ActorKind::Human) | None => store,
        },
    };

    // Read-only integrity check at startup. Problems are reported as warnings and never repaired
    // automatically (repair is the explicit `amenbo doctor --fix`); the check itself has no side effects.
    // Turn it off with `amenbo config set startup_integrity_check false`.
    //
    // Count only after the reach is fixed. The tally taken at open (`startup_check`) knows nothing of reach —
    // it looks at the whole device — so reading it out inside a closed reach would tell an AI the number of
    // issues that `doctor` will not show it, and send it looking for something it cannot see. Only when we
    // know something is there do we count again, within the reach.
    if store.startup_check.as_ref().is_some_and(|h| h.has_warnings()) {
        let doctor = store.doctor().map_err(CliError::from)?;
        let n = doctor.issues.len();
        if n > 0 {
            let cmd = Paths::command_name();
            eprintln!(
                "⚠ Startup integrity check found {n} issue(s) (error {} / warning {}). Run `{cmd} doctor` for details (repair: `{cmd} doctor --fix`).",
                doctor.summary.error, doctor.summary.warning
            );
        }
    }

    // An AI does not get to choose a project — the binding does. If it passes `--project`, refuse, even when
    // it names the bound project itself: neither ignore it silently nor silently fall back to the binding,
    // because either would teach the AI that it has a choice. This bites only where the reach is closed —
    // an AI facet in a bound CWD — and never constrains a human.
    if let Some(named) = named_project_flag(&cli) {
        store.reach().refuse_project_choice(named).map_err(CliError::from)?;
    }

    // Explicit override: `--project <name|id>` replaces the effective project context (`#n` resolution, the
    // default project) that the binding's `.amenbo` would otherwise supply. Precedence is `--project` >
    // `.amenbo` > error; nothing is guessed, and an unknown or archived project fails loud instead of
    // falling back. Resolved and validated once, against the opened store.
    if let Some(p) = cli.project.as_deref() {
        let pid = store.resolve_project_ref(p).map_err(CliError::from)?;
        let live = store.project(pid).map_err(CliError::from)?.is_some();
        if !live {
            return Err(CliError {
                code: "invalid_value",
                message: format!("project '{p}' is archived or deleted — cannot use it as an explicit --project context"),
                hint: Some(format!("Pass a live project (see `{} project list`).", Paths::command_name())),
                exit: 2,
            });
        }
        let _ = PROJECT_OVERRIDE.set(pid);
    }

    if !matches!(cli.command, Some(Command::Hooks { .. })) {
        lint_hook_setup(&mut store, flags);
    }

    let Some(command) = cli.command else {
        // No arguments: discover.
        let result = store.discover().map_err(CliError::from)?;
        if flags.json {
            print_json(&result);
        } else {
            render_discover(&result);
        }
        return Ok(0);
    };

    match command {
        Command::Agent { command: name, full } => {
            // Drill down: return the full spec of one command — where the entry index leads.
            if let Some(name) = name {
                let Some(spec) = agent::command_spec(&name) else {
                    let names = agent::command_names();
                    // Catch typos and half-remembered names by substring, and offer those as candidates
                    // (with no hits, everything is a candidate).
                    let near: Vec<String> =
                        names.iter().filter(|n| n.contains(&name) || name.contains(*n)).cloned().collect();
                    let candidates = if near.is_empty() { names } else { near };
                    return Err(CliError {
                        code: "unknown_command",
                        message: format!("No command named '{name}' in the agent spec."),
                        hint: Some(format!("Did you mean: {}", candidates.join(", "))),
                        exit: 2,
                    });
                };
                print_json(&spec);
                return Ok(0);
            }
            // Attach the opened store's version / format state to the static spec as runtime information.
            // The spec proper (the command definitions) stays core's truth source, untouched. Whether an
            // update exists comes from the upstream latest.json.
            let vs = store.version_status().with_upstream(upstream.as_ref());
            // The default is the entry point: how to work, in full; commands, as an index. `--full` piles on
            // every command's spec.
            let mut spec = if full { agent::build() } else { agent::build_index() };
            if let serde_json::Value::Object(map) = &mut spec {
                // Fill in the static spec's `updateAvailable` (false by default) with what the upstream
                // actually says, so an AI can learn that an update is out.
                map.insert(
                    "updateAvailable".to_string(),
                    serde_json::Value::Bool(vs.update_available),
                );
                map.insert(
                    "store_status".to_string(),
                    serde_json::to_value(&vs).unwrap_or(serde_json::Value::Null),
                );
                // Where git is not in play, drop the `worktree` cycle outright rather than letting it arrive
                // with a "if you use git" caveat: a caveat still spends the reader's context, and what the
                // spec advises here is not a thing they can do. The question is about the **bound folder** —
                // the pointer's, not wherever the caller stands — since that is the checkout the work happens
                // in. Core stays a static builder; this is the same runtime seam the two fields above use.
                if !bound_dir_is_under_git() {
                    if let Some(serde_json::Value::Object(cycles)) = map.get_mut("cycles") {
                        cycles.remove("worktree");
                    }
                }
            }
            print_json(&spec);
        }
        Command::Version => {
            // channel = the app-data name (`amenbo` in production, `amenbo-dev` in development), so the two
            // are never mistaken for each other.
            let channel = amenbo_core::config::Paths::APP_NAME;
            // Surface format_version — how far this build can open. The upstream latest.json is what sets
            // `update_available` / `latest_version`.
            let vs = store.version_status().with_upstream(upstream.as_ref());
            if flags.json {
                print_json(&json!({
                    "version": agent::VERSION,
                    "schema_version": agent::SCHEMA_VERSION,
                    "channel": channel,
                    "format_version": vs.format_version,
                    "max_supported_format": vs.max_supported_format,
                    "latest_version": vs.latest_version,
                    "update_available": vs.update_available,
                }));
            } else {
                let suffix = if channel == "amenbo" { String::new() } else { format!(" ({channel})") };
                human(flags, format!("amenbo {}{}", agent::VERSION, suffix));
                human(flags, format!("format: store v{} (this build opens up to v{})", vs.format_version, vs.max_supported_format));
                if let Some(latest) = vs.latest_version.as_deref() {
                    human(flags, format!("latest published: {latest}"));
                }
                if vs.update_available {
                    human(flags, format!(
                        "update available — a newer amenbo ({}) is out. Run `{} update` to get the installer.",
                        vs.newer_version.as_deref().unwrap_or("—"),
                        Paths::command_name(),
                    ));
                }
            }
        }
        Command::Update { print, apply, rollback } => {
            return if rollback {
                self_rollback_cmd(flags)
            } else if apply {
                self_update_cmd(flags, upstream)
            } else {
                update_cmd(flags, upstream, print)
            };
        }
        Command::Whoami => return whoami(&store, flags),
        Command::Bind { project, dir, force } => return bind_cmd(&store, flags, project, dir, force),
        Command::Init { .. } => {
            unreachable!("handled before open")
        }
        Command::Unbind { .. } => {
            unreachable!("handled before open")
        }
        Command::Lint { .. } => {
            unreachable!("handled before open")
        }
        Command::GithookPreCommit | Command::GithookCommitMsg { .. } => {
            unreachable!("handled before open")
        }
        Command::PluginRunner { .. } => {
            unreachable!("handled before open")
        }
        Command::Plugin { sub } => return plugin_cmd(&mut store, flags, sub),
        Command::Config { sub } => return config(&mut store, flags, sub),
        Command::Status { scope } => {
            let result = store.status(&scope).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                if let Some(loc) = location_header(&store) {
                    human(flags, loc);
                }
                render_status(&result);
            }
        }
        Command::Doctor { fix } => {
            if fix {
                // Rewrite broken `.amenbo` pointers (legacy form, or gone) into the current form. Only for
                // folders with exactly one owner — an ambiguous one is left alone for a human to settle with
                // `bind --project`. Same core path as the repair button on the GUI's health banner, so the
                // two surfaces never fix different things.
                let repair = amenbo_core::binding::repair_pointers(&store);
                if !repair.repaired.is_empty() {
                    human(
                        flags,
                        format!("✓ Rewrote {} folder pointer(s) to the current format.", repair.repaired.len()),
                    );
                }
                for dir in &repair.unresolved {
                    human(
                        flags,
                        format!("⚠ {dir}: no single live project claims this folder — run `{} bind --project <id>` there.", Paths::command_name()),
                    );
                }

                // Every cleanup below is non-destructive, so `--fix` asks for no confirmation.

                // Sweep blobs nothing references. Each delete op already reclaims the blobs it orphaned, so
                // what lands here is only what slipped through: blobs still too young to collect at the time
                // (`GC_MIN_AGE`), and bytes left behind by an interrupted delete or restore. The full scan
                // costs a pass over every blob, which is why it lives in a manual repair.
                match store.gc_blobs(amenbo_core::blob::GC_MIN_AGE) {
                    Ok(gc) if gc.removed > 0 => human(
                        flags,
                        format!(
                            "✓ Reclaimed {} unreferenced attachment file(s) ({} bytes).",
                            gc.removed, gc.freed_bytes
                        ),
                    ),
                    Ok(_) => human(flags, "doctor --fix: No cleanup targets (unreferenced attachment files)."),
                    Err(e) => human(flags, format!("⚠ Could not reclaim unreferenced attachment files: {e}")),
                }

                // Forget index rows for folders no live project claims. This touches neither the folder nor
                // its `.amenbo` — it is index housekeeping — so it asks for no confirmation.
                match store.forget_orphan_dirs() {
                    Ok(0) => human(flags, "doctor --fix: No cleanup targets (orphan folder bindings)."),
                    Ok(n) => human(flags, format!("✓ Forgot {n} orphan folder binding(s).")),
                    Err(e) => human(flags, format!("⚠ Could not forget orphan folder bindings: {e}")),
                }
            }
            // What doctor covers — the store's internal consistency plus this device's environment — is
            // assembled in core (`doctor::report`). The GUI's Settings > Integrity reads the same thing, so
            // the two surfaces never raise different issues. The prose is not in core: core returns a kind
            // and params, and the CLI writes the sentence (`doctor_text`).
            let result = amenbo_core::doctor::report(&store).map_err(CliError::from)?;
            // Surface the version / format state. Informational only — it is not counted as an issue.
            let vs = store.version_status();
            if flags.json {
                let mut body = serde_json::to_value(&result).unwrap_or(serde_json::Value::Null);
                if let serde_json::Value::Object(map) = &mut body {
                    // Hand the sentence to the `--json` reader (an AI) too — this is still the CLI surface.
                    // The kind and params ride along exactly as core serialized them, so a machine can branch
                    // without ever reading the prose.
                    if let Some(serde_json::Value::Array(issues)) = map.get_mut("issues") {
                        for (slot, issue) in issues.iter_mut().zip(&result.issues) {
                            if let serde_json::Value::Object(o) = slot {
                                o.insert("message".to_string(), doctor_text::message(issue).into());
                                o.insert("fix_hint".to_string(), doctor_text::fix_hint(issue).into());
                            }
                        }
                    }
                    map.insert(
                        "version_status".to_string(),
                        serde_json::to_value(&vs).unwrap_or(serde_json::Value::Null),
                    );
                }
                print_json(&body);
            } else {
                human(flags, format!("doctor: {} issue(s) (error {} / warning {})", result.issues.len(), result.summary.error, result.summary.warning));
                doctor_text::print_grouped(&result.issues, |line| human(flags, line));
                human(flags, format!("format: store v{} (this build opens up to v{})", vs.format_version, vs.max_supported_format));
            }
        }
        Command::SyncGuide { dir } => return sync_guide(&store, flags, dir.clone()),
        Command::Validate { ids } => {
            // The prose is not in core: core returns the rule and the delta, and the CLI writes the sentence
            // (`validate_text`, shaped like doctor's).
            let result = store.validate(&ids).map_err(CliError::from)?;
            if flags.json {
                let mut body = serde_json::to_value(&result).unwrap_or(serde_json::Value::Null);
                if let serde_json::Value::Object(map) = &mut body {
                    // Hand the sentence to the `--json` reader (an AI) too — this is still the CLI surface.
                    // The rule and delta ride along exactly as core serialized them, so a machine can branch
                    // without ever reading the prose.
                    if let Some(serde_json::Value::Array(issues)) = map.get_mut("issues") {
                        for (slot, issue) in issues.iter_mut().zip(&result.issues) {
                            if let serde_json::Value::Object(o) = slot {
                                o.insert("fix_hint".to_string(), validate_text::fix_hint(issue).into());
                            }
                        }
                    }
                }
                print_json(&body);
            } else {
                human(flags, format!("validate: ok={} checked={} (error {} / warning {})", result.ok, result.checked, result.summary.error, result.summary.warning));
                for i in &result.issues {
                    human(flags, format!("  [{}] {} {}: {}", i.severity, i.target, i.field, validate_text::fix_hint(i)));
                }
            }
        }
        Command::Activity { task, project, since, kind, by, for_scope, limit, offset } => {
            return activity_cmd(&store, flags, task, project, since, kind, by, for_scope, limit, offset)
        }
        Command::Project { sub } => return project(&mut store, flags, sub),
        Command::Dimension { sub } => return dimension(&mut store, flags, sub),
        // The three command groups that append observation events to the outbox (`AMB-D-367`): drive the
        // dispatcher once after each, at the short-lived CLI's write seam (`with_dispatch`).
        Command::Task { sub } => return with_dispatch(&mut store, |s| task(s, flags, sub)),
        Command::Comment { sub } => return with_dispatch(&mut store, |s| comment(s, flags, sub)),
        Command::Decision { sub } => return with_dispatch(&mut store, |s| decision(s, flags, sub)),
        Command::Attach { sub } => return attach(&mut store, flags, sub),
        Command::Export { out } => return export(&store, flags, out),
        Command::Backup { path } => return run_backup(&store, flags, path),
        Command::HardErase { sub } => return hard_erase(&mut store, flags, sub),
        Command::Restore { .. } => {
            unreachable!("handled before open")
        }
        Command::Hooks { sub } => return hooks_cmd(&mut store, flags, sub),
    }
    Ok(0)
}

/// `amenbo hooks …`: the explicit faces of the lint hook, usable whatever the device answered — a `no`
/// closes the question, not the door.
///
/// **They speak for the repository they are run in, and for nothing else.** Each does the two things
/// together, but they are not mirror images, because the feature and a repository are not the same scale:
///
/// - `install` is an explicit yes to the lint — as much a yes as clicking it in the modal — so it records
///   the **device** answer ([`amenbo_core::config::Config::hook_consent`]) as `Yes` and clears any opt-out
///   here. Wanting the lint in this repository but not another is the very thing the one-question design
///   says nobody wants, so a yes given anywhere is a yes to the feature, and the other bound repositories
///   are wired at their next startup. One that genuinely wants out says so with `uninstall`.
/// - `uninstall` is **not** a device-wide no — that would stop the feature everywhere over one repository.
///   It removes the hook here and opts *this* repository out, so a device-wide yes does not put it back at
///   the next startup, and it leaves the device answer alone.
fn hooks_cmd(store: &mut Store, flags: &Flags, sub: HooksCmd) -> Result<i32, CliError> {
    use amenbo_core::hooks::{self, HookConsent};

    let cwd = std::env::current_dir().map_err(|e| CliError {
        code: "io_error",
        message: format!("Cannot read the current directory: {e}"),
        hint: None,
        exit: 1,
    })?;
    let cmd = Paths::command_name();
    let project = binding_project(store);

    match sub {
        HooksCmd::Install => {
            let done = hooks::install(&cwd, cmd).map_err(CliError::from)?;
            record_optout(store, project, false);
            store.config.hook_consent = Some(HookConsent::Yes);
            let _ = store.save_config();
            if flags.json {
                print_json(&json!({
                    "ok": true,
                    "installed": done.installed.iter().map(|(slot, what)| json!({
                        "hook": slot.name(),
                        "action": match what { hooks::Installed::Wrote => "installed", hooks::Installed::Rewrote => "updated" },
                    })).collect::<Vec<_>>(),
                    "refused": done.refused.iter().map(|slot| json!({
                        "hook": slot.name(),
                        "add_line": hooks::guidance_line(*slot, cmd),
                    })).collect::<Vec<_>>(),
                }));
            } else {
                human(flags, render_install(&done, cmd));
            }
        }
        HooksCmd::Uninstall => {
            let removed = hooks::uninstall(&cwd).map_err(CliError::from)?;
            record_optout(store, project, true);
            if flags.json {
                print_json(&json!({
                    "ok": true,
                    "removed": removed.iter().map(|slot| slot.name()).collect::<Vec<_>>(),
                }));
            } else if removed.is_empty() {
                human(flags, "hooks: nothing of ours to remove — recorded that you do not want them.");
            } else {
                let names = removed.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ");
                human(flags, format!("hooks: {names} removed. Reinstall any time with `{cmd} hooks install`."));
            }
        }
        HooksCmd::Status => {
            let states = hooks::probe(&cwd);
            let consent = store.config.hook_consent;
            let opted_out = project.is_some_and(|p| store.hook_opted_out(p).unwrap_or(false));
            if flags.json {
                print_json(&json!({
                    "in_git_repo": states.is_some(),
                    "hooks": states.map(|s| s.iter().map(|(slot, state)| json!({
                        "hook": slot.name(),
                        "state": state,
                    })).collect::<Vec<_>>()),
                    "consent": consent,
                    "opted_out": opted_out,
                }));
            } else {
                human(flags, render_hook_status(states, consent, opted_out, cmd));
            }
        }
    }
    Ok(0)
}

/// What an install did, slot by slot — a slot another tool holds gets the block alongside its body, so the
/// only line here that is not a plain "installed" is the rare slot amenbo could not read as text and so
/// could not join.
fn render_install(done: &amenbo_core::hooks::InstallReport, cmd: &str) -> String {
    use amenbo_core::hooks::{guidance_line, Installed};

    let mut lines = Vec::new();
    for (slot, what) in &done.installed {
        let verb = match what {
            Installed::Wrote => "installed",
            Installed::Rewrote => "updated",
        };
        lines.push(format!("hooks: {} {verb}.", slot.name()));
    }
    for slot in &done.refused {
        lines.push(format!(
            "hooks: {} is not amenbo's to change (git tracks it, or it will not read back as text), so amenbo left it alone. Add this line yourself:\n    {}",
            slot.name(),
            guidance_line(*slot, cmd)
        ));
    }
    lines.push("Bypass one commit with `git commit --no-verify`.".to_string());
    lines.join("\n")
}

/// The facts, the disk's a line per slot and the answer's its own — which is the whole point: the answer
/// is not a mirror of the disk, so a reader has to be able to see them disagree. The answer's line says
/// *this device* because that is its scale, and the opt-out gets a line of its own only when there is one:
/// it is the rarer fact, and a line saying a repository is not opted out would be noise on every other run.
fn render_hook_status(
    states: Option<amenbo_core::hooks::HookStates>,
    consent: Option<amenbo_core::hooks::HookConsent>,
    opted_out: bool,
    cmd: &str,
) -> String {
    use amenbo_core::hooks::{HookConsent, HookState};

    let on_disk = match states {
        None => "  not a git repository — nothing to hook".to_string(),
        Some(states) => states
            .iter()
            .map(|(slot, state)| {
                let name = slot.name();
                match state {
                    HookState::Unwired => format!("  {name}: not there (install: `{cmd} hooks install`)"),
                    HookState::Managed { version } => format!("  {name}: amenbo's block (marker v{version})"),
                    HookState::Foreign => format!(
                        "  {name}: another tool's hook, no amenbo block yet (add one alongside: `{cmd} hooks install`)"
                    ),
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    };
    let answered = match consent {
        None => "not asked yet",
        Some(HookConsent::Yes) => "yes — every repository, including ones bound later",
        Some(HookConsent::No) => "no (not asked again; `hooks install` still works, here)",
    };
    let mut out = format!("hooks on disk:\n{on_disk}\nthis device: {answered}");
    if opted_out {
        out.push_str(&format!(
            "\nthis repository: opted out by `{cmd} hooks uninstall` — amenbo will not wire it here on its own"
        ));
    }
    out
}

/// Write the veto down, and let a failure to do so pass: the row is a convenience that decides whether
/// amenbo acts here again, the hook itself is already where the user asked for it, and failing the command
/// over the note about it would undo nothing and help no one.
fn record_optout(store: &Store, project: Option<i64>, opted_out: bool) {
    if let Some(pid) = project {
        let _ = store.set_hook_optout(pid, opted_out);
    }
}

/// The one moment amenbo asks to write into the user's git plumbing, run before the command the user came
/// for — because the moment worth asking at is "amenbo ran in this repository at a terminal", not any one
/// command. `hooks` is not routed here at all: its argv already answered the question, its own faces say
/// what this would say, and skipping it keeps `hooks status` to the one [`amenbo_core::hooks::probe`] it
/// came for and read-only as its spec promises — this can record consent, and a read command must not.
/// The question is asked **once for this device**, on a surface where an answer can be had, and never when
/// the facts leave nothing to ask: [`amenbo_core::hooks::reconcile`] holds that judgment and this is only
/// its hands. Everything here is best-effort — a hook is a convenience, so nothing it does can fail the
/// command the user actually ran.
///
/// It acts on the repository it is standing in, and does not sweep the other bound folders a `yes` also
/// covers. That is not the answer being narrower than it says: those folders are wired the first time
/// amenbo runs in them — this same path, asking nothing, because by then the device has answered — and the
/// GUI sweeps them all at its next startup. Sweeping from here would mean a `git` spawn per bound folder on
/// the way to every `amenbo task list`, which is a cost the CLI pays on every command to finish sooner
/// something that finishes anyway.
fn lint_hook_setup(store: &mut Store, flags: &Flags) {
    use amenbo_core::hooks;

    let Some(project) = binding_project(store) else { return };
    let Ok(cwd) = std::env::current_dir() else { return };
    let consent = store.config.hook_consent;
    let opted_out = store.hook_opted_out(project).unwrap_or(false);
    let can_ask = !flags.json && flags.actor != Some(ActorKind::Ai) && std::io::stdin().is_terminal();
    let states = hooks::probe(&cwd);

    let answered = offer_lint_hook(store, &cwd, states, consent, opted_out, can_ask);
    // Heal a block of ours that was left damaged or stale — the corruption reconcile (inside the offer)
    // steps past, because any marker reads to it as a managed slot. Runs under the answer just given or the
    // one already on record. A stale block (an older version) is upgraded silently; only genuine damage —
    // something changed or half-removed the block — is returned, and so is the only thing said out loud.
    let restored = hooks::restore_blocks(&cwd, Paths::command_name(), answered.or(consent), opted_out);
    if !restored.is_empty() && !flags.quiet {
        let names = restored.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ");
        eprintln!("⚠ amenbo's lint block in {names} had been changed or removed — restored it.");
    }
    report_unfinished_setup(flags, &cwd, answered, states, consent, opted_out);
}

/// Report that the lint is not actually running, on every response until it is — a standing signal, where
/// [`offer_lint_hook`] is a one-time question. It is what reaches an AI, which never sees a prompt and is
/// the reader most likely to leak a ref: it lands in `--json` as a field on the answer the caller already
/// parses, so nothing has to be read or remembered for it to arrive. It is a warning and never an error —
/// the command the user ran succeeds regardless — and text goes to stderr so stdout stays pipeable. The
/// state is re-read when the offer just acted, so an install accepted a moment ago does not get reported
/// as missing in the same breath.
fn report_unfinished_setup(
    flags: &Flags,
    cwd: &std::path::Path,
    answered: Option<amenbo_core::hooks::HookConsent>,
    states: Option<amenbo_core::hooks::HookStates>,
    consent: Option<amenbo_core::hooks::HookConsent>,
    opted_out: bool,
) {
    use amenbo_core::hooks;

    let (states, consent) = match answered {
        Some(fresh) => (hooks::probe(cwd), Some(fresh)),
        None => (states, consent),
    };
    let Some(notice) = hooks::setup_notice(states, consent, opted_out) else { return };
    let cmd = Paths::command_name();
    if flags.json {
        // Empty slots only — the ones install is sure to wire. A stranger's slot is not reported: install
        // either already coexisted with it (under a yes) or refuses it (a tracked hook), so "run install"
        // there would be a promise it cannot keep.
        set_setup_report(json!({
            "unwired": notice.unwired.iter().map(|slot| json!({
                "hook": slot.name(),
                "fix": format!("{cmd} hooks install"),
            })).collect::<Vec<_>>(),
        }));
    } else if !flags.quiet {
        let slots = notice.unwired.iter().map(|slot| slot.name()).collect::<Vec<_>>().join(", ");
        eprintln!("⚠ `{cmd} lint` is not running on your commits ({slots}).");
        eprintln!("  Wire it up: {cmd} hooks install");
    }
}

/// Carry out what [`amenbo_core::hooks::reconcile`] says about the repository we are standing in, and
/// return the answer if one was just given — the caller re-reads the disk under it, so an install accepted
/// a moment ago is not reported as missing in the same breath.
///
/// The answer to a `HookAction::Ask` is the **device's**, and is written to the config as such: this is the
/// one place the question is put, so it is the one place that records it. Failing to persist it is let
/// pass, like every other note here — the hooks are already where the user asked for them, and the cost of
/// a lost note is being asked once more.
fn offer_lint_hook(
    store: &mut Store,
    cwd: &std::path::Path,
    states: Option<amenbo_core::hooks::HookStates>,
    consent: Option<amenbo_core::hooks::HookConsent>,
    opted_out: bool,
    can_ask: bool,
) -> Option<amenbo_core::hooks::HookConsent> {
    use amenbo_core::hooks::{self, HookAction, HookConsent};

    let cmd = Paths::command_name();
    match hooks::reconcile(&hooks::HookContext { states, consent, opted_out, can_ask }) {
        HookAction::Nothing => None,
        HookAction::Install => {
            // A slot another tool held, with no block of ours in it, most likely had its hook regenerated —
            // wiping the block amenbo put there. Re-wiring it is a restoration, so say so rather than report
            // a plain first-time install.
            let vanished = states.map(|s| s.slots_in(hooks::HookState::Foreign)).unwrap_or_default();
            match hooks::install(cwd, cmd) {
                Ok(done) => {
                    let names = done.installed.iter().map(|(slot, _)| slot.name()).collect::<Vec<_>>().join(", ");
                    if vanished.is_empty() {
                        eprintln!("✓ {names} hook installed, under the answer you already gave. Not here? `{cmd} hooks uninstall`.");
                    } else {
                        let gone = vanished.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ");
                        eprintln!("⚠ amenbo's lint block was gone from {gone} (another tool may have replaced its hook) — restored it under the answer you already gave.");
                    }
                }
                Err(e) => eprintln!("⚠ Could not install the hook: {e}"),
            }
            None
        }
        HookAction::Ask => {
            let yes = ask_yes_no(&offer_prompt(states))?;
            if yes {
                match hooks::install(cwd, cmd) {
                    Ok(done) => eprintln!("{}", render_install(&done, cmd)),
                    Err(e) => {
                        eprintln!("⚠ Could not install the hooks: {e}");
                        return None;
                    }
                }
            } else {
                eprintln!("Not installed. amenbo will not ask again — `{cmd} hooks install` when you want it.");
            }
            let answer = if yes { HookConsent::Yes } else { HookConsent::No };
            store.config.hook_consent = Some(answer);
            let _ = store.save_config();
            Some(answer)
        }
    }
}

/// The one question there is, worded from what the slots actually hold: amenbo offers to wire the lint into
/// every slot — an empty one becomes a small standalone hook, a slot another tool holds gains amenbo's
/// block alongside its body, so there is one action and no hand-off.
///
/// It says the answer is asked once and covers every repository, because that is what answering it does —
/// a prompt that named only "this repository" would be collecting a wider consent than it admitted to. What
/// it does *not* do is list those repositories: the answer is about the lint, not about a set of folders,
/// and a list would hand the reader something to check over a question that has one sensible answer.
fn offer_prompt(states: Option<amenbo_core::hooks::HookStates>) -> String {
    use amenbo_core::hooks::HookState;

    let Some(states) = states else { return String::new() };
    let mut prompt = String::from(
        "amenbo can keep amenbo refs (AMB-T-…) out of your commits by linting them on the way out.\n",
    );
    let has_foreign = !states.slots_in(HookState::Foreign).is_empty();
    prompt.push_str(if has_foreign {
        "It adds a small block to your git hooks, keeping any hook already there and running alongside it.\n"
    } else {
        "It writes a small git hook, and touches nothing else.\n"
    });
    prompt.push_str("Asked once: your answer covers the repositories amenbo works in, now and later.\n");
    prompt.push_str("Wire it up?");
    prompt
}

/// Ask on the terminal, where `None` means no answer was read (a closed stdin, an EOF) and is not a `no`:
/// nothing is recorded and the question stays live for the next run. The prompt goes to stderr so a caller
/// reading stdout gets it unpolluted, which costs nothing here — this is only ever reached on an
/// interactive terminal, where the two are the same screen anyway.
fn ask_yes_no(prompt: &str) -> Option<bool> {
    use std::io::{BufRead, Write};
    eprint!("{prompt} [y/N]: ");
    std::io::stderr().flush().ok();
    let mut buf = String::new();
    if std::io::stdin().lock().read_line(&mut buf).ok()? == 0 {
        return None;
    }
    Some(matches!(buf.trim(), "y" | "Y" | "yes"))
}

// ───────────────────────── helpers ─────────────────────────

/// The count header on any listing. When paging returns only part of the matches (count < total_matched),
/// name the total too: `3 task(s)`, or `3 of 42 task(s)` on a page.
fn count_header(count: usize, total_matched: usize, noun: &str) -> String {
    if count < total_matched {
        format!("{count} of {total_matched} {noun}(s)")
    } else {
        format!("{count} {noun}(s)")
    }
}

fn parse_date_opt(s: &Option<String>) -> Result<Option<NaiveDate>, CliError> {
    match s {
        Some(v) => Ok(Some(
            time::parse_date(v, time::today()).map_err(CliError::from)?,
        )),
        None => Ok(None),
    }
}

/// How this face re-runs itself as a plugin runner (`AMB-T-2175`): the hidden `plugin-runner` command, which
/// core follows with the plugin, the lease's owner and the store to work. The CLI's own spelling of the
/// entry point, named where it is dispatched.
const RUNNER_ARGV: &[&str] = &["plugin-runner"];

/// Run a mutating command group, then drive the plugin observation dispatcher once at the short-lived
/// CLI's write seam (`AMB-T-2033`). After the command committed, drain the outbox from the persisted
/// cursor onto the subscribed plugins' queues, persist where it advanced, and launch a runner process for
/// each queue nobody is already working — waiting for none of them, because a runner is not this process's
/// to cut short (`AMB-D-367` / `AMB-D-399` / `AMB-T-2175`). Only on success: if the command errored its
/// mutation rolled back, so there is nothing new to dispatch.
///
/// Who fires is [`EnabledSubscribers`]'s answer, over the plugins installed on this machine
/// ([`plugin_installed::installed`]) read once per drive: the resolver is a pure function of the state it
/// is handed, and this mount is what hands it (`AMB-T-2032`). With nothing installed it resolves nobody,
/// and the cursor still walks and persists, so a plugin installed later starts from what fires *next*, not
/// the whole backlog. A dispatch failure is a warning, never the command's exit: the mutation is already
/// committed.
fn with_dispatch(
    store: &mut Store,
    op: impl FnOnce(&mut Store) -> Result<i32, CliError>,
) -> Result<i32, CliError> {
    let code = op(store)?;
    dispatch(store, |store, subs| {
        store.drive_plugins_persisted(Face::Cli, subs, RUNNER_ARGV).map(Some)
    });
    Ok(code)
}

/// Pick up what a previous run left half-delivered, before this command does anything of its own
/// (`AMB-D-399`). The CLI's whole life *is* a startup, so this is where a fan-out or a runner that was cut
/// short is noticed — and it is noticed on a read as much as on a write, which is the point: the write that
/// would otherwise carry those rows out may be days away.
///
/// It costs a command with nothing pending two reads and no write lock — the guard is core's
/// ([`Store::resume_plugin_delivery`]), so both faces make the same judgement.
fn resume_dispatch(store: &Store) {
    dispatch(store, |store, subs| store.resume_plugin_delivery(Face::Cli, subs, RUNNER_ARGV));
}

/// The half both dispatch mounts share: resolve who is installed, hand the resolver to `drive`, and relay
/// whatever came back. Never fails a command — a mutation behind it is already committed, and a startup
/// kick has no command's outcome to speak for.
fn dispatch(
    store: &Store,
    drive: impl FnOnce(
        &Store,
        &dyn amenbo_core::plugin_dispatch::Subscribers,
    ) -> amenbo_core::Result<Option<amenbo_core::plugin_dispatch::Delivered>>,
) {
    // A directory that will not read is not "nothing is installed": drive nothing rather than walk the
    // cursor past events no subscriber was ever offered. The events stay in the outbox, and the next run
    // reads the directory again and delivers them.
    let installed = match plugin_installed::installed(&store.paths) {
        Ok(installed) => installed,
        Err(e) => {
            eprintln!("warning: could not read the installed plugins, so none was dispatched: {e}");
            return;
        }
    };
    let subscribers = EnabledSubscribers::new(&installed, store);
    match drive(store, &subscribers) {
        // A `reply:true` hook (worktree advice, `AMB-D-383`) ran synchronously; relay its stderr to the
        // caller — the AI reads it off this command's stderr and decides, named by the plugin that gave
        // it. The queues are a runner's, and this command waits for none of it (`AMB-T-2175`): a runner
        // is a process, so it is not cut short by this one returning.
        Ok(Some(delivered)) => {
            for reply in &delivered.replies {
                eprintln!("[{}] {}", reply.plugin, reply.stderr.trim_end());
            }
        }
        // Nothing was pending, so nothing was driven.
        Ok(None) => {}
        Err(e) => eprintln!("warning: could not dispatch plugin observation hooks: {e}"),
    }
}

/// Emit a system event into the ledger, under our own facet. Call it after the mutation wrapper has
/// committed. Activity is not the system of record, so a failed write must not fail the command: warn and
/// carry on, erring towards a missing line.
fn emit_event(store: &mut Store, flags: &Flags, target_id: i64, event: serde_json::Value) {
    // Every caller sits behind a mutation, and a mutation declared its facet — so there is always one to
    // record the line under. With none there is no author to name, which this treats the way it treats a
    // failed write: warn, and err towards the missing line.
    let Ok(actor) = flags.facet() else {
        eprintln!("warning: could not record the activity event: no facet was declared");
        return;
    };
    if let Err(e) = store.add_system_event(actor, target_id, event) {
        eprintln!("warning: could not record the activity event: {e}");
    }
}

/// The live tasks that just became ready because `blocker_id` stopped blocking them; empty if the read
/// fails. All this read feeds is the `task.unblocked` activity line — readiness itself is derived from the
/// dependency edges on every query, so a dependent becomes ready whether or not the signal is emitted. So it
/// takes the same stance as [`emit_event`]: never fail the command, warn, and err towards the missing line
/// (activity is not the system of record). What may be dropped is the line, not the fact of the failure.
fn newly_ready_or_warn(store: &Store, blocker_id: i64) -> Vec<i64> {
    store.newly_ready_by(blocker_id).unwrap_or_else(|e| {
        eprintln!("warning: could not tell which tasks this unblocked: {e}");
        Vec::new()
    })
}

/// After blocker `blocker_id` goes done, send `task.unblocked` to every dependent that just became ready.
fn emit_unblocks(store: &mut Store, flags: &Flags, blocker_id: i64) {
    let blocker = blocker_id.to_string();
    for tid in newly_ready_or_warn(store, blocker_id) {
        emit_event(store, flags, tid, activity_log::event::task_unblocked(&blocker));
    }
}

/// Warn the changer when a premise (a blocker, or a linked decision) is newly placed on a task that is
/// already reserved (`in_progress`). Such a task silently drops to `ready:no` — its reservation is not
/// revoked, and its holder gets no interrupt, so they only notice on their next command (`AMB-D-366`, the
/// changer side; surfacing it to the holder is a separate task). This does not forbid the edge, only breaks
/// the silence. `todo` / `blocked` say nothing; a `done` target is reopen's business, not a premise change,
/// so it is left alone here. A failure to read the status never fails the caller — like [`emit_event`], it
/// warns and moves on.
fn warn_if_premise_added_to_reserved(store: &Store, id: i64, what: &str) {
    match store.task(id) {
        Ok(Some(t)) if t.status == TaskStatus::InProgress => eprintln!(
            "⚠ {task} is reserved (in progress) — {what}. Its holder is not notified now; they will see it on their next amenbo command.",
            task = task_label(id),
        ),
        Ok(_) => {}
        Err(e) => eprintln!("warning: could not check whether {} is reserved: {e}", task_label(id)),
    }
}

/// Warn the changer when unsettling a decision takes the ground out from under a task that is already
/// reserved (`in_progress`). It is the other direction of the same silence
/// [`warn_if_premise_added_to_reserved`] breaks: there a premise is newly placed on a running task, here a
/// premise the task already rests on stops being settled (`AMB-D-373`, the changer side). Either way the
/// task drops to `ready:no` without losing its reservation and its holder gets no interrupt.
///
/// Two acts reach this, and `act` names which one is speaking: a `reopen` (the decision goes back to
/// proposed) and a `supersede` (it stays accepted but stops being current). Both leave the premise
/// unsettled, which is what `ready` reads — so both are made audible, and neither is forbidden. An
/// idempotent one settles nothing anew and so says nothing, which is why every caller warns only when its
/// write reported a change. `detail` is the **unsettled** decision's card — the old side of a supersede,
/// not the new one — since those are the tasks whose ground moved.
fn warn_if_unsettled_under_reserved(did: i64, detail: &amenbo_core::view::DecisionDetail, act: &str) {
    for t in detail.linked_tasks.iter().filter(|t| t.status == TaskStatus::InProgress) {
        eprintln!(
            "⚠ {task} is reserved (in progress) and rests on {decision} — {act} leaves that premise unsettled. Its holder is not notified now; they will see it on their next amenbo command.",
            task = task_label(t.id),
            decision = decision_label(did),
        );
    }
}

// ───────────────────────── E guardrails (local execution policy) ─────────────────────────
// These prevent accidents on this device. They assume an honest actor — the facet is self-declared and can
// be spoofed — so they are not a security boundary. actor=human is unconstrained.

/// An AI may not run a project's destructive or hiding ops (project archive/delete). Off by default, and a
/// config setting can allow it. The point is to stop irreversible destruction and the scrambling of a human's
/// structure, so it covers exactly archive (hides from the default view) and delete (destroys). The
/// reversible, constructive and restoring directions (add / update / move / unarchive) are not gated —
/// the same asymmetry as gating delete but allowing add, gating hard-erase but never the way back, and
/// letting an AI delete only tasks it created: the safe direction is always open.
fn guard_ai_project_ops(store: &Store, flags: &Flags) -> Result<(), CliError> {
    if flags.actor == Some(ActorKind::Ai) && !store.config.ai_allow_project_ops {
        return Err(CliError::ai_guardrail(
            "AI cannot archive or delete projects (destructive/hiding project ops) (E guardrail).",
        ));
    }
    Ok(())
}

/// An AI may not hard-erase (physically destroy append-only content). It is an unrecoverable, destructive
/// maintenance op, so it is human-gated — with no config setting to open it: the refusal is unconditional.
fn guard_ai_hard_erase(flags: &Flags) -> Result<(), CliError> {
    if flags.actor == Some(ActorKind::Ai) {
        return Err(CliError::ai_guardrail(
            "AI cannot hard-erase content: physically destroying store content is a human-gated maintenance op (E guardrail).",
        ));
    }
    Ok(())
}

/// An AI may delete only the tasks it created as the AI facet; human-created tasks and legacy rows are
/// refused. The facet is the only notion of actor there is, so "did the AI make this?" is exactly
/// `created_by_kind == Ai`.
fn guard_ai_task_delete(store: &Store, flags: &Flags, task_id: i64) -> Result<(), CliError> {
    if flags.actor != Some(ActorKind::Ai) {
        return Ok(());
    }
    let self_made = store
        .task(task_id)
        .ok()
        .flatten()
        .map(|t| t.created_by_kind == Some(ActorKind::Ai))
        .unwrap_or(false);
    if self_made {
        Ok(())
    } else {
        Err(CliError::ai_guardrail(
            "AI can only delete tasks it created as AI (deleting tasks created by others is not allowed; E guardrail).",
        ))
    }
}

fn parse_priority(s: &str) -> Result<Priority, CliError> {
    match s {
        "high" => Ok(Priority::High),
        "medium" => Ok(Priority::Medium),
        "low" => Ok(Priority::Low),
        other => Err(CliError {
            code: "invalid_value",
            message: format!("--priority value '{other}' is invalid."),
            hint: Some("Specify one of: high | medium | low.".to_string()),
            exit: 2,
        }),
    }
}

fn parse_view(s: &str) -> Result<View, CliError> {
    match s {
        "list" => Ok(View::List),
        "board" => Ok(View::Board),
        "calendar" => Ok(View::Calendar),
        "timeline" => Ok(View::Timeline),
        other => Err(CliError {
            code: "invalid_value",
            message: format!("--view value '{other}' is invalid."),
            hint: Some("Specify one of: list | board | calendar | timeline.".to_string()),
            exit: 2,
        }),
    }
}

/// The reorder position (`--top`/`--bottom`/`--before`/`--after`). The anchor is an id (an integer key)
/// within the same ordering.
fn pos_from_keys(top: bool, bottom: bool, before: Option<i64>, after: Option<i64>) -> Result<Position, CliError> {
    Position::from_flags(top, bottom, before, after).map_err(CliError::from)
}

// ───────────────────────── identity ─────────────────────────

/// The quiet first line of `status`/`whoami`: which project, and which folder, this run is operating in.
/// Searches upward for `.amenbo` and returns `Project: <name>  (this folder: <the folder holding
/// .amenbo>)` — anchored on the bound folder, exactly as `bind` displays it. With no binding (an
/// `AMENBO_HOME` sandbox store, say) it is None and no header is printed.
fn location_header(store: &Store) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let (dir, binding) = amenbo_core::binding::resolve_upward(store, &cwd)?;
    let name = binding
        .project_id
        .and_then(|pid| {
            store
                .project(pid)
                .ok()
                .flatten()
                
                .map(|p| p.name)
        })
        .unwrap_or_else(|| "(no project set)".to_string());
    let header = format!("Project: {}  (this folder: {})", name, dir.to_string_lossy());
    // Report a mismatch in the cross-check right here, so it is seen at the very start of a session.
    Some(match slug_mismatch_warning(store, &binding) {
        Some(warning) => format!("{header}\n{warning}"),
        None => header,
    })
}

fn whoami(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    let id = &store.identity;
    let live = amenbo_core::identity::live_hw();
    let mismatch = id.hw_mismatch();
    // The facet is the only actor there is, so the display name comes from config (`human_name`).
    if flags.json {
        print_json(&json!({
            "human_name": store.config.human_display_name(),
            "bound_hw": id.bound_hw,
            "live_hw": live,
            "hw_mismatch": mismatch,
        }));
    } else {
        if let Some(loc) = location_header(store) {
            human(flags, loc);
        }
        human(flags, format!("human: {}", store.config.human_display_name()));
        human(flags, format!("hardware check: {}", if mismatch { "⚠ mismatch (suspected copy to another machine)" } else { "ok" }));
    }
    Ok(0)
}

/// The guidance for AI agents that `amenbo init` leaves in the current folder.
/// Idempotently upserts the managed block of generic amenbo guidance (Class A) into **both** `AGENTS.md` and
/// `CLAUDE.md` under `dir`. Only what lies between the markers is ours; the user's own Class P content is
/// preserved (no file → create, markers present → replace the block, markers absent → append at the end).
/// The command reference is not duplicated here — `{CMD} agent --json` is its single source of truth, which
/// keeps this from rotting and keeps it out of commit diffs. `{CMD}` follows `Paths::command_name` (`amenbo` in
/// production, `amenbo-dev` on the dev channel) so a dev build never points people at production. With no
/// `lang_code`, English. Returns only the files whose content actually changed (created or updated), so
/// calling it again is a no-op.
fn upsert_agent_guidance(dir: &std::path::Path, lang_code: Option<&str>) -> Vec<&'static str> {
    // Picking the language — the fallback, and keeping an existing block's language — is `upsert_into_dir`'s.
    amenbo_core::agents::upsert_into_dir(dir, lang_code, Paths::command_name())
}

/// When a new binary raises the managed-block template version, folders bound earlier keep the old one. A
/// folder somebody walks into repairs itself — running amenbo there brings it forward
/// ([`amenbo_core::binding::resolve_upward`]). This command is for the folders nobody walks into, and for
/// blocks that could not be rewritten at the time (the leftovers `doctor` keeps flagging as
/// `stale_managed_block`): it resyncs the `CLAUDE.md` / `AGENTS.md` of every bound folder to the current
/// version in one go. `upsert_into_dir` writes only when the content actually changes, so this causes no
/// churn, and the language label is kept from each folder's existing block (`lang_code = None`) rather than
/// degraded. Without `--dir` it covers every bound folder on this device, matching what `doctor` looks at.
/// Paths that were deleted or renamed are skipped silently — `doctor` surfaces those as stale.
fn sync_guide(store: &Store, flags: &Flags, dir: Option<String>) -> Result<i32, CliError> {
    use amenbo_core::agents::{resync_bound_blocks, MANAGED_BLOCK_VERSION};
    // The resync itself is core's shared path (`resync_bound_blocks`, the same one the GUI uses); all that
    // happens here is formatting it for the terminal or for JSON. The bound folders come from the binding
    // table in the consolidated store.
    let report = resync_bound_blocks(&store.bindings(), dir.as_deref(), Paths::command_name());
    let mut updated: Vec<serde_json::Value> = Vec::new();
    for (d, f) in &report.updated {
        human(flags, format!("✓ {d}: {f} resynced to the current version (v{MANAGED_BLOCK_VERSION})"));
        updated.push(json!({ "dir": d, "file": f }));
    }
    if report.updated.is_empty() {
        human(
            flags,
            format!(
                "every managed block is at the current version (v{MANAGED_BLOCK_VERSION}) — {} folder(s) checked, nothing changed.",
                report.scanned
            ),
        );
    }
    if flags.json {
        print_json(&json!({
            "ok": true,
            "action": "sync_guide",
            "version": MANAGED_BLOCK_VERSION,
            "scanned": report.scanned,
            "updated": updated,
        }));
    }
    Ok(0)
}

fn init_cmd(flags: &Flags, name: Option<String>, language: Option<String>, force: bool) -> Result<i32, CliError> {
    let paths = Paths::resolve().map_err(CliError::from)?;
    // There is one store. `init` does not raise a new one — it creates a new project in this folder and
    // places the pointer — and only doubles as genesis when no store exists yet. Whether one exists is
    // answered by peeking at the truth-source file directly: merely opening with `Store::open_at` writes
    // genesis and stamps `format_version`, i.e. it would touch a store we have not decided to touch.
    let store_exists = amenbo_core::store_engine::probe_is_populated(&paths.store_file);
    // Clobber guard: if this folder (or one above it) is already bound to another project by a `.amenbo`,
    // do not create a new project and overwrite the pointer without asking. Same "respect what is already
    // there" rule that `if !agents.exists()` gives AGENTS.md, applied to `.amenbo`. Only `--force` overrides.
    if !force {
        if let Ok(cwd) = std::env::current_dir() {
            if let Some((_, binding)) = amenbo_core::binding::find_upward(&cwd) {
                // If the store already holds data (tasks/projects), refuse all the harder and spell out how
                // to recover.
                let has_data = !amenbo_core::store::store_file_is_content_empty(&paths.store_file)
                    .unwrap_or(true);
                let pid = binding
                    .project_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "(no project)".to_string());
                return Err(CliError::init_pointer_exists(
                    &cwd.to_string_lossy(),
                    &pid,
                    has_data,
                ));
            }
            // No pointer (we did not return above), but this CWD already has a CLAUDE.md/AGENTS.md carrying
            // an amenbo managed block. A marker alone is not grounds to hard-block: it is a thin, borrowed
            // surface that a clone, a copy or a sync carries along, so it proves nothing about ownership.
            // Ownership lives in amenbo's own artifacts (`.amenbo` plus the bindings registry), so branch on
            // what the registry's reverse lookup says: which live projects claim this cwd.
            if store_exists && amenbo_core::agents::dir_has_managed_block(&cwd) {
                let store = Store::open_at(paths.clone()).map_err(CliError::from)?;
                let owners = amenbo_core::binding::live_projects_claiming(&store, &cwd);
                match owners.as_slice() {
                    // (a) Exactly one live project: recover the missing/broken `.amenbo` (a bind, in effect —
                    // never a silent init).
                    [project_id] => {
                        return recover_lost_pointer(flags, &cwd, *project_id, &store);
                    }
                    // (c) Several claim it: which lost pointer to recover is not determined, so stop and
                    // offer the candidates.
                    many if many.len() > 1 => {
                        let ids: Vec<String> = many.iter().map(|pid| pid.to_string()).collect();
                        return Err(CliError::init_ambiguous_owners(&cwd.to_string_lossy(), &ids));
                    }
                    // (b) None: a stale marker left by a clone, a copy, or a leftover. Do not hard-block —
                    // carry on with init. A new project is created below and `upsert_agent_guidance`
                    // regenerates the managed block idempotently (anything outside the markers is kept).
                    _ => {}
                }
            }
        }
    }
    // Get the store (the single DB) ready: open it if it is already there (`Store::init` rejects an
    // initialized store as a conflict), otherwise raise it with genesis. Neither the store nor any secret
    // goes in the project directory — that keeps them out of iCloud-synced trees, which corrupt them, and
    // out of the AI's project context.
    let mut store = if store_exists {
        if name.is_some() {
            human(flags, format!("  (--name is ignored: this device already holds an amenbo store; change your display name with `{} config set human_name <name>`)", Paths::command_name()));
        }
        Store::open_at(paths).map_err(CliError::from)?
    } else {
        Store::init(paths, name.as_deref()).map_err(CliError::from)?
    };
    // `--language`, if given, sets the global user language (stored in the user-layer config).
    if let Some(lang) = &language {
        store.config.set("language", lang).map_err(CliError::from)?;
    }
    // The language embedded in AGENTS.md is the global setting — what `--language` just set, or what was
    // already there.
    let lang_code = store.config.language.clone();
    // init mirrors the GUI's "new project in a folder" (`provision_project_store`): create one initial
    // project named after the folder (the CWD basename; a language-specific default when that is empty or
    // unusable) and stamp its project_id into `.amenbo`. A folder init'd from the CLI then shows up in the
    // GUI sidebar (which renders the project list) on its own, and `task add` works there right away (the
    // required project comes from the pointer's project_id). Project creation happens on this CLI path only:
    // `Store::init` is shared with the GUI's provisioning, so putting it there would create it twice.
    let cwd = std::env::current_dir().ok();
    let project_name = cwd
        .as_deref()
        .and_then(|c| c.file_name())
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| amenbo_core::config::default_project_name(lang_code.as_deref()));
    let project = store
        .project_add(amenbo_core::ops::project::NewProject {
            name: project_name,
            view: store.config.default_view,
            notes: String::new(),
            color: None,
        })
        .map_err(CliError::from)?;
    let project_id = project.id;
    let project_name = project.name.clone();
    // The project was already committed by the write seam. All that is flushed here is the config that
    // `--language` touched.
    store.save_config().map_err(CliError::from)?;
    // Place `.amenbo` (the dir→project pointer) and the AI guidance (AGENTS.md) in the current folder.
    let mut placed: Vec<&str> = Vec::new();
    if let Some(cwd) = cwd {
        // The pointer names a project and nothing else — `.amenbo` plays no part in choosing the store.
        let pointer = amenbo_core::binding::pointer_for(&store, project_id);
        if pointer.write(&cwd).is_ok() {
            placed.push(".amenbo");
            // Record the project→folder reverse lookup (what the settings screen lists, and the index a lost
            // pointer is recovered from). As in the GUI's provisioning and in CLI bind, `set` (the primary
            // directory) and `record_project_ref` are called as a pair. Best-effort: a failure to record does
            // not fail init, same as the pointer write.
            let mut registry = store.bindings();
            registry.set(project_id, cwd.to_string_lossy());
            registry.record_project_ref(project_id, cwd.to_string_lossy());
            let _ = store.save_bindings(&registry);
        }
        // Idempotently upsert the generic amenbo guidance (the managed block) into both AGENTS.md and
        // CLAUDE.md. Only the space between the markers is ours; the user's Class P content is preserved.
        // English base plus a directive naming the user's language.
        placed.extend(upsert_agent_guidance(&cwd, lang_code.as_deref()));
    }
    let human_name = store.config.human_display_name();
    // Success reports what you can now do and what to do next, not what machinery ran. The `.amenbo` and
    // AGENTS.md that were placed get a light mention in parentheses.
    if flags.json {
        write_envelope(flags, "init", "identity",
            json!({ "human_name": human_name, "project_id": project_id, "project_name": project_name, "placed": placed }),
            None, false, "");
    } else {
        human(flags, format!("✓ Ready — project '{}' is set up; an AI launched in this folder can now operate amenbo (you are {}).", project_name, human_name));
        human(flags, format!("  Next: {} status", Paths::command_name()));
        if !placed.is_empty() {
            human(flags, format!("  (placed {})", placed.join(", ")));
        }
        // init writes the managed block mid-session, and a block written mid-session does not bind the
        // session that is running — it takes effect at the next one. So tell the AI that just ran init to run
        // `agent` right now, and close that gap. The command name follows the channel (amenbo / amenbo-dev).
        human(flags, format!(
            "  AI agents: run `{} agent --json` now and follow it — the managed guidance just written to CLAUDE.md/AGENTS.md does not take effect until your next session.",
            Paths::command_name(),
        ));
    }
    Ok(0)
}

/// When the `.amenbo` is gone but the bindings registry's reverse lookup names exactly one live project as
/// this folder's owner, recover the pointer instead of creating a new project — no silent init. It amounts
/// to a `bind --project`: rewrite the pointer and the bindings index, and idempotently upsert the managed
/// block (anything outside the markers is kept).
fn recover_lost_pointer(
    flags: &Flags,
    cwd: &std::path::Path,
    project_id: i64,
    store: &Store,
) -> Result<i32, CliError> {
    // Rewrite `.amenbo` (this is a bind, in effect).
    amenbo_core::binding::pointer_for(store, project_id).write(cwd).map_err(CliError::from)?;
    // Update the bindings index idempotently (the primary directory plus the many-to-one reverse lookup).
    {
        let mut reg = store.bindings();
        reg.set(project_id, cwd.to_string_lossy().to_string());
        reg.record_project_ref(project_id, cwd.to_string_lossy());
        let _ = store.save_bindings(&reg);
    }
    // Regenerate the managed block idempotently (outside the markers is kept). The upsert keeps an existing
    // block's language label.
    upsert_agent_guidance(cwd, store.config.language.as_deref());

    // Answer with the same facet keys as the init envelope — the same display name must not come back under
    // two different key names.
    let human_name = store.config.human_display_name();
    let project_name = project_name(store, Some(project_id))?;
    if flags.json {
        write_envelope(flags, "init", "identity",
            json!({ "human_name": human_name, "project_id": project_id, "project_name": project_name, "recovered": true }),
            None, false, "");
    } else {
        human(flags, format!(
            "✓ Recovered — this folder was already linked to project '{}' but its .amenbo pointer was missing; rewrote it (you are {}).",
            project_name.clone().unwrap_or_else(|| project_id.to_string()), human_name,
        ));
        human(flags, format!("  Next: {} status", Paths::command_name()));
    }
    Ok(0)
}

/// The project→folders reverse lookup, as a JSON array. Lists the folders recorded in the registry (the
/// binding table of the consolidated store) in ascending order, each with its absolute path and what
/// inspecting it found. An absolute path goes stale when the folder is moved or renamed, so `exists=false`
/// is a cleanup candidate. The inspection carries the same three findings as the GUI's folder list
/// (`BoundFolderDto`): `legacy` (an old-format pointer), `pointer_missing` (the folder is there but the
/// `.amenbo` is not), and `mismatch` (the pointer's slug disagrees with the store — it belongs to another
/// one). Every one of those judgements goes through core's shared path.
fn bound_folders_json(store: &Store, project_id: i64) -> serde_json::Value {
    use amenbo_core::binding;
    let folders: Vec<serde_json::Value> = store
        .bindings()
        .dirs_for_project(project_id)
        .into_iter()
        .map(|dir| {
            let path = std::path::Path::new(dir);
            let mismatch = binding::read_pointer(path)
                .and_then(|b| binding::slug_mismatch(store, &b))
                .map(|m| json!({ "project_id": m.project_id, "recorded": m.recorded, "actual": m.actual }));
            json!({
                "path": dir,
                "exists": path.is_dir(),
                "legacy": binding::is_legacy_pointer(path),
                "pointer_missing": binding::is_pointer_missing(path),
                "mismatch": mismatch,
            })
        })
        .collect();
    serde_json::Value::Array(folders)
}

/// Resolve the folder `bind` acts on: the directory `--dir <path>` names, or the CWD when it is omitted.
/// bind places a `.amenbo` inside that folder, so anything that is not an existing directory — a file, or
/// nothing at all — is refused.
fn resolve_bind_target(dir: Option<String>) -> Result<std::path::PathBuf, CliError> {
    match dir {
        Some(d) => {
            let p = std::path::PathBuf::from(&d);
            if !p.is_dir() {
                return Err(CliError {
                    code: "io_error",
                    message: format!("--dir target does not exist or is not a directory: {d}"),
                    hint: Some("Pass an existing directory to place its `.amenbo` pointer in.".to_string()),
                    exit: 1,
                });
            }
            // Registrations made from the CWD are absolute paths. Canonicalize so a relative `--dir` does not
            // put a different string in the registry (it exists — checked above — so this cannot fail).
            Ok(std::fs::canonicalize(&p).unwrap_or(p))
        }
        None => std::env::current_dir()
            .map_err(|e| CliError { code: "io_error", message: e.to_string(), hint: None, exit: 1 }),
    }
}

fn bind_cmd(store: &Store, flags: &Flags, project: Option<String>, dir: Option<String>, force: bool) -> Result<i32, CliError> {
    use amenbo_core::binding::find_upward_ancestor;
    // With `--dir <path>`, the `.amenbo` goes in that folder rather than the CWD — binding from outside it.
    let cwd = resolve_bind_target(dir)?;
    let mut registry = store.bindings();

    if let Some(p) = project {
        // Nested-binding guard: binding inside a subdirectory of an already-managed tree (an ancestor holds a
        // `.amenbo`) would shadow the parent with the pointer placed here, and scatter `.amenbo`/AGENTS.md/
        // CLAUDE.md through the source tree. Same "respect the tree that is already there" rule as `init`'s
        // clobber guard. A deliberate subdir bind gets through with `--force`.
        if !force {
            if let Some((dir, _)) = find_upward_ancestor(&cwd) {
                return Err(CliError::binding_nested_tree(&dir.to_string_lossy()));
            }
        }
        // Bind: resolve the project in the store and place the `.amenbo` pointer (its project_id). Several
        // directories may point at the same project_id, which makes the relation many-to-one.
        let pid = store.resolve_project_ref(&p).map_err(CliError::from)?;
        amenbo_core::binding::pointer_for(store, pid).write(&cwd).map_err(CliError::from)?;
        registry.set(pid, cwd.to_string_lossy().to_string());
        // Record it in the project→folders reverse lookup too (many-to-one; what the settings screen lists).
        registry.record_project_ref(pid, cwd.to_string_lossy());
        store.save_bindings(&registry).map_err(CliError::from)?;
        // Upsert the guidance's managed block into both files as well (existing content is kept).
        upsert_agent_guidance(&cwd, store.config.language.as_deref());
        let name = project_name(store, Some(pid))?.unwrap_or_default();
        // For a human: what you can now do, and what to do next.
        if flags.json {
            write_envelope(flags, "bind", "binding",
                json!({ "project_id": pid, "project_name": name, "dir": cwd.to_string_lossy() }),
                None, false, "");
        } else {
            human(flags, format!("✓ Linked this folder to project '{name}' — an AI launched here can now operate this project."));
            human(flags, format!("  Next: {} status", Paths::command_name()));
        }
        return Ok(0);
    }

    // Display: search upward for `.amenbo` (a legacy pointer is read compatibly and rewritten lazily) and
    // check the registered path for staleness.
    match amenbo_core::binding::resolve_upward(store, &cwd) {
        Some((dir, b)) => {
            // A project→dir registration whose path has vanished is binding_stale.
            if let Some(pid) = b.project_id {
                registry.resolve_dir(pid).map_err(CliError::from)?;
            }
            let name = project_name(store, b.project_id)?;
            // A recorded slug that disagrees with the store means this pointer likely came from another one.
            let mismatch = slug_mismatch_warning(store, &b);
            if flags.json {
                print_json(&json!({ "ok": true, "action": "bind.show", "binding": {
                    "found_in": dir.to_string_lossy(),
                    "project_id": b.project_id, "project_name": name,
                    "slug": b.slug, "slug_mismatch": mismatch.is_some() } }));
            } else {
                human(flags, format!("Binding: {} ({})", name.clone().unwrap_or_else(|| "(no project set)".to_string()), dir.to_string_lossy()));
                if let Some(warning) = &mismatch {
                    human(flags, warning);
                }
                human(flags, format!("To link a project, run `{} bind --project <name or ID>`.", Paths::command_name()));
            }
        }
        None => {
            if flags.json {
                print_json(&json!({ "ok": true, "action": "bind.show", "binding": null }));
            } else {
                human(flags, format!("No .amenbo found in this directory (or above). Link one with `{} bind --project <name or ID>`.", Paths::command_name()));
            }
        }
    }
    Ok(0)
}

/// `amenbo unbind`: undo this folder's `.amenbo` binding (the CWD by default, any folder with `--dir`). It
/// removes that folder's pointer and nothing else — the relation is many-to-one, so other folders pointing at
/// the same project are untouched — and it never deletes the store: this is the inverse of `bind`, not a
/// teardown. It also strips the managed block from the AGENTS.md / CLAUDE.md that bind/init wrote (the user's
/// Class P content is preserved) and forgets this folder in the registry, keeping the orphan-detection index
/// consistent. It never unbinds an ancestor: with no `.amenbo` directly in this folder, the tree above is not
/// dragged in — `unbind_no_binding` says where to run it instead.
fn unbind_cmd(flags: &Flags, dir: Option<String>) -> Result<i32, CliError> {
    use amenbo_core::binding::{find_upward, DirBinding};
    let target = match dir {
        Some(d) => std::path::PathBuf::from(d),
        None => std::env::current_dir()
            .map_err(|e| CliError { code: "io_error", message: e.to_string(), hint: None, exit: 1 })?,
    };
    let marker = target.join(".amenbo");
    if !marker.is_file() {
        // Not here. If an ancestor is bound, say so — but do not silently unbind it.
        let ancestor = find_upward(&target).map(|(d, _)| d.to_string_lossy().to_string());
        return Err(CliError::unbind_no_binding(&target.to_string_lossy(), ancestor.as_deref()));
    }
    if !confirm(flags, &format!("unbind this folder ({}) from amenbo (removes .amenbo and amenbo's managed blocks; the store is kept)", target.to_string_lossy()))? {
        return Ok(0);
    }
    // Read the pointer before removing it, to report its project_id (best-effort).
    let pointer: Option<DirBinding> = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());
    // Delete `.amenbo`.
    std::fs::remove_file(&marker)
        .map_err(|e| CliError { code: "io_error", message: format!("could not remove {}: {e}", marker.display()), hint: None, exit: 1 })?;
    let mut removed: Vec<String> = vec![".amenbo".to_string()];
    // Strip the managed block from AGENTS.md / CLAUDE.md (Class P is preserved; a file that was nothing but
    // the block is deleted).
    removed.extend(amenbo_core::agents::remove_from_dir(&target).into_iter().map(String::from));
    // Forget this folder in the binding registry (the binding table of the consolidated store). Drop every
    // registration by folder rather than by the pointer's id (`forget_dir`), so the index is cleaned even
    // when the pointer was stale. If there is no store yet (no store file), there is no index either: remove
    // the pointer and the managed block and stop — never silently genesis a store, same rule as the exec
    // guard.
    let dir_str = target.to_string_lossy().to_string();
    let mut forgot = 0usize;
    let paths = Paths::resolve().map_err(CliError::from)?;
    if amenbo_core::store_engine::probe_is_populated(&paths.store_file) {
        let store = Store::open_at(paths).map_err(CliError::from)?;
        let mut registry = store.bindings();
        forgot = registry.forget_dir(&dir_str);
        // The CWD is already canonical, but a non-canonical path passed to `--dir` can disagree with the
        // registered string (the CWD as it read at bind time). Try the canonical form too, if it differs.
        if let Ok(canon) = std::fs::canonicalize(&target) {
            let canon_str = canon.to_string_lossy().to_string();
            if canon_str != dir_str {
                forgot += registry.forget_dir(&canon_str);
            }
        }
        if forgot > 0 {
            store.save_bindings(&registry).map_err(CliError::from)?;
        }
    }
    let project_id = pointer.as_ref().and_then(|b| b.project_id);
    write_envelope(
        flags,
        "unbind",
        "binding",
        json!({ "dir": dir_str, "project_id": project_id, "removed": removed, "registry_entries_forgotten": forgot }),
        None,
        false,
        format!("✓ Unbound {} (removed: {}). The project is kept.", dir_str, removed.join(", ")),
    );
    Ok(0)
}

// ───────────────────────── config ─────────────────────────

fn config(store: &mut Store, flags: &Flags, sub: Option<ConfigCmd>) -> Result<i32, CliError> {
    if let Some(ConfigCmd::Set { key, value }) = sub {
        store.config.set(&key, &value).map_err(CliError::from)?;
        store.save_config().map_err(CliError::from)?;
        // `default_workspace` is accepted for forward compatibility but ignored (a store owns a single
        // workspace) — say so honestly instead of "Updated".
        let deprecated = key == "default_workspace";
        if deprecated {
            human(flags, format!("Ignored deprecated setting: {key} (a store owns a single workspace)"));
        } else {
            human(flags, format!("Updated setting: {key} = {value}"));
        }
        // When the language changes, resync the managed block in the CWD's AGENTS.md and CLAUDE.md so the
        // language directive follows — otherwise the GUI switches language while the AI keeps writing in the
        // old one.
        if key == "language" {
            if let Ok(cwd) = std::env::current_dir() {
                upsert_agent_guidance(&cwd, store.config.language.as_deref());
            }
        }
        if flags.json {
            print_json(&json!({ "ok": true, "action": "config.set", "noop": deprecated, "key": key, "value": value }));
        }
        return Ok(0);
    }

    let members = query::members(&store.config).count;

    let value = json!({
        "app_version": agent::VERSION,
        "schema_version": agent::SCHEMA_VERSION,
        "paths": {
            "config_file": store.paths.config_file.display().to_string(),
            "data_dir": store.paths.base_dir.display().to_string(),
            "store_file": store.paths.store_file.display().to_string(),
        },
        "settings": {
            "default_view": store.config.default_view.as_str(),
            "language": store.config.language,
            "date_locale": store.config.date_locale,
            "human_name": store.config.human_name,
            "ai_name": store.config.ai_name,
            "human_display_name": store.config.human_display_name(),
            "ai_display_name": store.config.ai_display_name(),
            "ai_allow_project_ops": store.config.ai_allow_project_ops,
            "onboarded": store.config.onboarded,
            "startup_integrity_check": store.config.startup_integrity_check,
            "update_check": store.config.update_check,
        },
        "sync": {
            // This build ships no sync transport (local-first).
            "enabled": false,
            "members": members,
        },
        "export": { "default_format": "json" }
    });
    if flags.json {
        print_json(&value);
    } else {
        human(flags, format!("store file: {}", store.paths.store_file.display()));
        human(flags, format!("default view: {}", store.config.default_view.as_str()));
        human(flags, "sync: not available in this build (local-first)");
        human(flags, format!("startup integrity check (startup_integrity_check): {} (read-only doctor at open; warnings only)", if store.config.startup_integrity_check { "on" } else { "off" }));
        human(flags, format!("update check (update_check): {} (checks a static latest.json for a newer release; infra-side only, no user data; timeout + silent-fail + cached; AMENBO_UPDATE_CHECK=0 overrides)", if store.config.update_check { "on" } else { "off" }));
    }
    Ok(0)
}

// ───────────────────────── project ─────────────────────────

fn project(store: &mut Store, flags: &Flags, sub: ProjectCmd) -> Result<i32, CliError> {
    match sub {
        ProjectCmd::Add { name, view, notes, color } => {
            // No `--view` is not "board": it is "whatever this store was configured to open a new
            // project on". The setting exists to be the answer here, so reading it anywhere else —
            // or defaulting past it — is what would leave it a value nothing acts on.
            let view = match view {
                Some(v) => parse_view(&v)?,
                None => store.config.default_view,
            };
            let p = store.project_add(ops::project::NewProject { name, view, notes, color }).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.add", "project", serde_json::to_value(&detail).unwrap(), None, false, format!("✓ Created project: {} ({})", p.name, p.id));
        }
        ProjectCmd::List { archived } => {
            let result = store.project_list(archived).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, format!("{} project(s)", result.count));
                for p in &result.projects {
                    let a = if p.archived { " [archived]" } else { "" };
                    human(flags, format!("  {}  {} (tasks: {}){}", amenbo_core::idref::project(p.id), p.name, p.num_tasks, a));
                }
            }
        }
        ProjectCmd::Show { id } => {
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let detail = store.project_detail(pid).map_err(CliError::from)?;
            // The project→folders reverse lookup. The `.amenbo` files are scattered across the filesystem and
            // cannot be enumerated from app-data, so this comes from what bind recorded in the registry (the
            // binding table of the consolidated store). Absolute paths go stale when a folder is moved or
            // renamed, so every folder carries an `exists` check.
            let bound_folders = bound_folders_json(store, pid);
            if flags.json {
                let mut v = serde_json::to_value(&detail).unwrap();
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("bound_folders".to_string(), bound_folders.clone());
                }
                print_json(&v);
            } else {
                human(flags, format!("{}  {}", amenbo_core::idref::project(detail.id), detail.name));
                human(flags, format!("view: {} / tasks: {} (completed {})", detail.default_view.as_str(), detail.task_counts.total, detail.task_counts.completed));
                let folders = bound_folders.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                if folders.is_empty() {
                    human(flags, "folders: (none bound)");
                } else {
                    human(flags, "folders:");
                    for f in folders {
                        let path = f["path"].as_str().unwrap_or("");
                        // Say what the inspection found on the same line (same material as the JSON; the
                        // order runs strongest first, and "the folder is gone" outranks the rest).
                        let mark = if !f["exists"].as_bool().unwrap_or(false) {
                            "  (missing)".to_string()
                        } else if f["pointer_missing"].as_bool().unwrap_or(false) {
                            format!("  (no .amenbo — run `{} init` there to relink)", Paths::command_name())
                        } else if f["legacy"].as_bool().unwrap_or(false) {
                            "  (legacy .amenbo)".to_string()
                        } else if !f["mismatch"].is_null() {
                            "  (.amenbo points at another store)".to_string()
                        } else {
                            String::new()
                        };
                        human(flags, format!("  {path}{mark}"));
                    }
                }
            }
        }
        ProjectCmd::Update { id, name, notes, view, color } => {
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let mut changed = Vec::new();
            if name.is_some() { changed.push("name".to_string()); }
            if notes.is_some() { changed.push("notes".to_string()); }
            if view.is_some() { changed.push("default_view".to_string()); }
            if color.is_some() { changed.push("color".to_string()); }
            let view = match view { Some(v) => Some(parse_view(&v)?), None => None };
            let p = store.project_update(pid, ops::project::ProjectPatch { name, notes, view, color }).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.update", "project", serde_json::to_value(&detail).unwrap(), Some(changed), false, format!("✓ Updated project: {}", p.id));
        }
        ProjectCmd::Move { id, before, after, top, bottom } => {
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let before = before.map(|b| store.resolve_project_ref(&b)).transpose().map_err(CliError::from)?;
            let after = after.map(|a| store.resolve_project_ref(&a)).transpose().map_err(CliError::from)?;
            let pos = pos_from_keys(top, bottom, before, after)?;
            let p = store.project_move(pid, pos).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.move", "project", serde_json::to_value(&detail).unwrap(), Some(vec!["order_key".to_string()]), false, format!("✓ Moved project: {}", p.id));
        }
        ProjectCmd::Archive { id } => {
            guard_ai_project_ops(store, flags)?;
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let p = store.project_set_archived(pid, true).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.archive", "project", serde_json::to_value(&detail).unwrap(), Some(vec!["archived".to_string()]), false, format!("✓ Archived project: {}", p.id));
        }
        ProjectCmd::Unarchive { id } => {
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let p = store.project_set_archived(pid, false).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.unarchive", "project", serde_json::to_value(&detail).unwrap(), Some(vec!["archived".to_string()]), false, format!("✓ Unarchived project: {}", p.id));
        }
        ProjectCmd::Delete { id } => {
            guard_ai_project_ops(store, flags)?;
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            if !confirm(flags, "delete project")? {
                return Ok(0);
            }
            store.project_delete(pid, flags.facet()?).map_err(CliError::from)?;
            // Delete is destructive (the same shape as the GUI's `project_delete`). Release this project's
            // folder bindings: the `.amenbo` pointers, the managed blocks, the registry rows. The teardown is
            // best-effort — a failure there does not fail the delete.
            let _ = amenbo_core::project_teardown::teardown_deleted_project(store, pid);
            write_envelope(flags, "project.delete", "project", json!({ "id": pid, "deleted": true }), None, false, format!("✓ Deleted project: {pid}"));
        }
    }
    Ok(0)
}

/// The CLI surface of the unified dimension model. The axes themselves (purely user-defined), their values,
/// and their assignment to tasks are all delegated to `ops::dimension`. An axis resolves by id prefix or by
/// name (`resolve_in`); a value resolves within the dimension it belongs to (`resolve_value_in`), because a
/// value's name is only unique inside its own axis.
fn dimension(store: &mut Store, flags: &Flags, sub: DimensionCmd) -> Result<i32, CliError> {
    use amenbo_core::model::{DimensionCardinality, DimensionRole};
    use amenbo_core::ops::dimension::NewDimension;
    // A dimension's kind on one human-readable line (single, ordered, time-axis).
    fn kind_line(cardinality: DimensionCardinality, ordered: bool, role: DimensionRole) -> String {
        let mut s = cardinality.as_str().to_string();
        if ordered {
            s.push_str(", ordered");
        }
        if matches!(role, DimensionRole::TimeAxis) {
            s.push_str(", time-axis");
        }
        s
    }
    /// A value's period `[start_on, end_on]` (both ends inclusive) on one human-readable line. An open end
    /// reads `…` at the start and `ongoing` at the finish. With neither end set there is no period at all,
    /// and nothing is shown.
    fn period_line(v: &amenbo_core::model::DimensionValue) -> Option<String> {
        let (s, e) = (v.start_on, v.end_on);
        if s.is_none() && e.is_none() {
            return None;
        }
        let fmt = |d: Option<NaiveDate>, open: &str| d.map(|d| d.to_string()).unwrap_or_else(|| open.to_string());
        Some(format!("[{} → {}]", fmt(s, "…"), fmt(e, "ongoing")))
    }
    /// A period is the payload of the time_axis role, not a general feature of every axis. Core writes the
    /// physical columns as told, so the CLI surface is what guards the role.
    fn ensure_time_axis(store: &Store, dimension_id: i64) -> Result<(), CliError> {
        let role = store.dimension(dimension_id).map_err(CliError::from)?.map(|d| d.role);
        if matches!(role, Some(DimensionRole::TimeAxis)) {
            return Ok(());
        }
        Err(CliError::from(amenbo_core::Error::invalid(
            "only a time-axis dimension's values carry a period; mark the axis with --time-axis first",
            "期間を持てるのは時間軸の次元の値だけです。先に --time-axis で軸に印を付けてください",
        )))
    }
    /// Build a value's new period from `--start`/`--end` (a new end) and `--clear-*` (open an end). An end
    /// given by neither keeps its current value, which makes this a partial update.
    fn merged_period(
        cur: &amenbo_core::model::DimensionValue,
        start: Option<NaiveDate>,
        end: Option<NaiveDate>,
        clear_start: bool,
        clear_end: bool,
    ) -> (Option<NaiveDate>, Option<NaiveDate>) {
        let s = if clear_start { None } else { start.or(cur.start_on) };
        let e = if clear_end { None } else { end.or(cur.end_on) };
        (s, e)
    }
    match sub {
        DimensionCmd::Add { project, name, notes, ordered, time_axis } => {
            let pid = project_or_bound(store, project)?;
            let new = NewDimension {
                name,
                notes,
                // A classification axis is single-select, always.
                cardinality: DimensionCardinality::Single,
                ordered,
                role: if time_axis { DimensionRole::TimeAxis } else { DimensionRole::None },
            };
            let d = store.dimension_add(pid, new).map_err(CliError::from)?;
            write_envelope(flags, "dimension.add", "dimension", serde_json::to_value(&d).unwrap(), None, false, format!("✓ Created dimension: {} ({})", d.name, dimension_label(d.id)));
        }
        DimensionCmd::List { project } => {
            let pid = project_or_bound(store, project)?;
            let dims: Vec<_> = store.dimensions(pid).map_err(CliError::from)?;
            if flags.json {
                let mut out: Vec<serde_json::Value> = Vec::with_capacity(dims.len());
                for d in &dims {
                    let values = store.dimension_values(d.id).map_err(CliError::from)?;
                    out.push(json!({ "dimension": serde_json::to_value(d).unwrap(), "values": values }));
                }
                print_json(&json!({ "count": out.len(), "dimensions": out }));
            } else {
                human(flags, format!("{} dimension(s)", dims.len()));
                for d in &dims {
                    let vals = store.dimension_values(d.id).map_err(CliError::from)?;
                    human(flags, format!("  {}  {} [{}]  {} value(s)", dimension_label(d.id), d.name, kind_line(d.cardinality, d.ordered, d.role), vals.len()));
                    for v in &vals {
                        let period = period_line(v).map(|p| format!("  {p}")).unwrap_or_default();
                        human(flags, format!("      {}  {}{}", dimension_value_label(v.id), v.name, period));
                    }
                }
            }
        }
        DimensionCmd::Show { id } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            let d = store
                .dimension(did)
                .map_err(CliError::from)?
                
                .ok_or_else(|| { let r = dimension_label(did); CliError::from(amenbo_core::Error::not_found(format!("dimension '{r}' not found"), format!("次元 '{r}' が見つかりません"))) })?;
            let vals: Vec<_> = store.dimension_values(did).map_err(CliError::from)?;
            if flags.json {
                print_json(&json!({ "dimension": serde_json::to_value(&d).unwrap(), "values": serde_json::to_value(&vals).unwrap() }));
            } else {
                human(flags, format!("{}  {}", dimension_label(d.id), d.name));
                human(flags, format!("kind: {}", kind_line(d.cardinality, d.ordered, d.role)));
                if d.notes.trim().is_empty() {
                    human(flags, "notes: (none)");
                } else {
                    human(flags, format!("notes:\n{}", d.notes));
                }
                human(flags, format!("{} value(s)", vals.len()));
                for v in &vals {
                    let period = period_line(v).map(|p| format!("  {p}")).unwrap_or_default();
                    human(flags, format!("  {}  {}{}", dimension_value_label(v.id), v.name, period));
                }
            }
        }
        DimensionCmd::Rename { id, name } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            let d = store.dimension_update(did, Some(&name), None, None, None).map_err(CliError::from)?;
            write_envelope(flags, "dimension.rename", "dimension", serde_json::to_value(&d).unwrap(), Some(vec!["name".to_string()]), false, format!("✓ Renamed dimension: {}", dimension_label(d.id)));
        }
        DimensionCmd::Update { id, name, notes, ordered, time_axis } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            let mut changed = Vec::new();
            if name.is_some() {
                changed.push("name".to_string());
            }
            if notes.is_some() {
                changed.push("notes".to_string());
            }
            if ordered.is_some() {
                changed.push("ordered".to_string());
            }
            if time_axis.is_some() {
                changed.push("role".to_string());
            }
            let role = time_axis
                .map(|on| if on { DimensionRole::TimeAxis } else { DimensionRole::None });
            let d = store.dimension_update(did, name.as_deref(), notes.as_deref(), ordered, role).map_err(CliError::from)?;
            write_envelope(flags, "dimension.update", "dimension", serde_json::to_value(&d).unwrap(), Some(changed), false, format!("✓ Updated dimension: {}", dimension_label(d.id)));
        }
        DimensionCmd::Move { id, before, after, top, bottom } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            let before = before.map(|b| store.resolve_dimension(None, &b)).transpose().map_err(CliError::from)?;
            let after = after.map(|a| store.resolve_dimension(None, &a)).transpose().map_err(CliError::from)?;
            let pos = pos_from_keys(top, bottom, before, after)?;
            let d = store.dimension_move(did, pos).map_err(CliError::from)?;
            write_envelope(flags, "dimension.move", "dimension", serde_json::to_value(&d).unwrap(), Some(vec!["order_key".to_string()]), false, format!("✓ Moved dimension: {}", dimension_label(d.id)));
        }
        DimensionCmd::Rm { id } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            if !confirm(flags, "delete dimension")? {
                return Ok(0);
            }
            store.dimension_delete(did).map_err(CliError::from)?;
            write_envelope(flags, "dimension.rm", "dimension", json!({ "id": did, "deleted": true }), None, false, format!("✓ Deleted dimension: {}", dimension_label(did)));
        }
        DimensionCmd::ValueAdd { dimension, name, start, end } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let start_on = parse_date_opt(&start)?;
            let end_on = parse_date_opt(&end)?;
            let dated = start_on.is_some() || end_on.is_some();
            if dated {
                ensure_time_axis(store, did)?;
            }
            let period = dated.then_some((start_on, end_on));
            let v = store.dimension_value_add(did, &name, period).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-add", "dimension_value", serde_json::to_value(&v).unwrap(), None, false, format!("✓ Added value: {} ({})", v.name, dimension_value_label(v.id)));
        }
        DimensionCmd::ValueRename { dimension, value, name } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let v = store.dimension_value_update(vid, Some(&name), None).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-rename", "dimension_value", serde_json::to_value(&v).unwrap(), Some(vec!["name".to_string()]), false, format!("✓ Renamed value: {}", dimension_value_label(v.id)));
        }
        DimensionCmd::ValueUpdate { dimension, value, name, start, end, clear_start, clear_end } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let start_on = parse_date_opt(&start)?;
            let end_on = parse_date_opt(&end)?;
            let mut changed = Vec::new();
            if name.is_some() {
                changed.push("name".to_string());
            }
            if start.is_some() || clear_start {
                changed.push("start_on".to_string());
            }
            if end.is_some() || clear_end {
                changed.push("end_on".to_string());
            }
            let touches_period = start.is_some() || end.is_some() || clear_start || clear_end;
            if touches_period {
                ensure_time_axis(store, did)?;
            }
            let cur = store.dimension_value(vid).map_err(CliError::from)?.ok_or_else(|| {
                CliError::from(amenbo_core::Error::not_found(format!("dimension value '{vid}' not found"), format!("次元の値 '{vid}' が見つかりません")))
            })?;
            let period = touches_period
                .then(|| merged_period(&cur, start_on, end_on, clear_start, clear_end));
            let v = store.dimension_value_update(vid, name.as_deref(), period).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-update", "dimension_value", serde_json::to_value(&v).unwrap(), Some(changed), false, format!("✓ Updated value: {}", dimension_value_label(v.id)));
        }
        DimensionCmd::ValueMove { dimension, value, before, after, top, bottom } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let before = before.map(|b| store.resolve_dimension_value(did, &b)).transpose().map_err(CliError::from)?;
            let after = after.map(|a| store.resolve_dimension_value(did, &a)).transpose().map_err(CliError::from)?;
            let pos = pos_from_keys(top, bottom, before, after)?;
            let v = store.dimension_value_move(vid, pos).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-move", "dimension_value", serde_json::to_value(&v).unwrap(), Some(vec!["order_key".to_string()]), false, format!("✓ Moved value: {}", dimension_value_label(v.id)));
        }
        DimensionCmd::ValueRm { dimension, value } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            if !confirm(flags, "delete dimension value")? {
                return Ok(0);
            }
            store.dimension_value_delete(vid).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-rm", "dimension_value", json!({ "id": vid, "deleted": true }), None, false, format!("✓ Deleted value: {}", dimension_value_label(vid)));
        }
        DimensionCmd::Set { task, dimension, value } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let (tv, changed) = store.set_task_dimension_value(tid, vid).map_err(CliError::from)?;
            write_envelope(flags, "dimension.set", "task_dimension_value", serde_json::to_value(&tv).unwrap(), None, !changed, format!("✓ Set value on task {}", task_label(tid)));
        }
        DimensionCmd::Unset { task, dimension, value } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let removed = store.unset_task_dimension_value(tid, vid).map_err(CliError::from)?;
            write_envelope(flags, "dimension.unset", "task_dimension_value", json!({ "task_id": tid, "value_id": vid, "removed": removed }), None, !removed, format!("✓ Cleared value on task {}", task_label(tid)));
        }
    }
    Ok(0)
}

// ───────────────────────── task ─────────────────────────

/// Use the binding's (`.amenbo`) default project as the context for ref resolution — but only while that
/// project is live. A legacy pointer is read compatibly by `resolve_upward`, which rewrites it into the
/// current form on the spot.
fn bound_project(store: &Store) -> Option<i64> {
    // An explicit override (`--project`) wins. It was validated as live against this store when it was set,
    // so return it as is.
    if let Some(pid) = PROJECT_OVERRIDE.get() {
        return Some(*pid);
    }
    binding_project(store)
}

/// Does this invocation name a project with `--project` — either the global flag or a sub-command that
/// carries one? What an AI is forbidden is the naming itself, so which project it names is not looked at:
/// naming the bound project is refused just the same.
fn named_project_flag(cli: &Cli) -> Option<&'static str> {
    let named = cli.project.is_some()
        || match &cli.command {
            Some(Command::Bind { project, .. }) | Some(Command::Activity { project, .. }) => {
                project.is_some()
            }
            Some(Command::Task { sub }) => matches!(
                sub,
                TaskCmd::Add { project: Some(_), .. }
                    | TaskCmd::List { project: Some(_), .. }
                    | TaskCmd::Move { project: Some(_), .. }
            ),
            Some(Command::Decision { sub }) => matches!(
                sub,
                DecisionCmd::Add { project: Some(_), .. }
                    | DecisionCmd::List { project: Some(_), .. }
                    | DecisionCmd::Promote { project: Some(_), .. }
            ),
            Some(Command::Dimension { sub }) => matches!(
                sub,
                DimensionCmd::Add { project: Some(_), .. } | DimensionCmd::List { project: Some(_), .. }
            ),
            _ => false,
        };
    named.then_some("--project")
}

/// Resolve an explicit `--project`, and fill the slot from the binding when none is given. With neither,
/// fail loud rather than guess — nothing gets created without a project. An AI cannot pass `--project`
/// ([`named_project_flag`]), so for an AI this is the only route: the binding decides where things land.
fn project_or_bound(store: &Store, project: Option<String>) -> Result<i64, CliError> {
    match project {
        Some(p) => store.resolve_project_ref(&p).map_err(CliError::from),
        None => bound_project(store).ok_or_else(|| project_required(store)),
    }
}

/// The error for "there is nowhere to put this". Lists the existing projects, so the answer is to pick one.
fn project_required(store: &Store) -> CliError {
    let projects = store.project_list(false).map(|r| r.projects).unwrap_or_default();
    if projects.is_empty() {
        return CliError::from(amenbo_core::Error::invalid(
            "--project is required, but no projects exist yet — create one first",
            "--project は必須ですが、プロジェクトがまだありません — まず1つ作成してください",
        ));
    }
    let en = projects.iter().map(|p| format!("{} ({})", p.name, p.id)).collect::<Vec<_>>().join(", ");
    let ja = projects.iter().map(|p| format!("「{}」({})", p.name, p.id)).collect::<Vec<_>>().join("、");
    CliError::from(amenbo_core::Error::invalid(
        format!("--project is required. existing projects: {en}. pass --project <id|name>"),
        format!("--project は必須です。既存プロジェクト: {ja}。--project <id|名前> を指定してください"),
    ))
}

/// Resolve `--dim <axis>=<value>` pairs into the value ids to file a new task under, in the order given.
/// The axis is looked up **inside the task's own project** — axes are per-project, so a name two projects
/// share must not resolve to the neighbour's — and the value inside that axis, the same rules `dimension
/// set` uses.
///
/// Two refusals, both before anything is written:
/// - a pair that is not `<axis>=<value>` (split on the first `=`, so a value may contain one);
/// - the same axis named twice. An axis holds one value, so the second would silently replace the first,
///   and which one the caller meant is not ours to pick.
///
/// `=none` is not accepted here, unlike the `dim:` filter: there it selects the tasks with no value on
/// that axis, and clearing an axis that was never set is what a new task already is.
fn resolve_dim_pairs(store: &Store, project_id: i64, pairs: &[String]) -> Result<Vec<i64>, CliError> {
    let mut value_ids = Vec::with_capacity(pairs.len());
    let mut axes: Vec<i64> = Vec::new();
    for pair in pairs {
        let Some((axis, value)) = pair.split_once('=') else {
            return Err(CliError::from(amenbo_core::Error::invalid(
                format!("--dim takes <axis>=<value> (e.g. --dim \"Category=bug\"), got `{pair}`"),
                format!("--dim は <軸>=<値> の形式です（例: --dim \"カテゴリー=バグ\"）。受け取った値: `{pair}`"),
            )));
        };
        let dimension_id = store.resolve_dimension(Some(project_id), axis).map_err(CliError::from)?;
        if axes.contains(&dimension_id) {
            return Err(CliError::from(amenbo_core::Error::invalid(
                format!("--dim names the axis `{axis}` twice — an axis holds one value, so pass it once"),
                format!("--dim で軸「{axis}」を2度指定しています。軸は単一選択なので1度だけ指定してください"),
            )));
        }
        axes.push(dimension_id);
        value_ids.push(store.resolve_dimension_value(dimension_id, value).map_err(CliError::from)?);
    }
    Ok(value_ids)
}

/// The live project this CWD's `.amenbo` points at — the binding itself, with no override folded in. An AI
/// facet's reach is drawn from here: if `--project` could widen it, the binding would decay into decoration
/// that merely says which store to open.
fn binding_project(store: &Store) -> Option<i64> {
    let cwd = std::env::current_dir().ok()?;
    let (_, binding) = amenbo_core::binding::resolve_upward(store, &cwd)?;
    let pid = binding.project_id?;
    store
        .project(pid)
        .ok()
        .flatten()
        .is_some()
        .then_some(pid)
}

/// The warning for when the slug recorded in `.amenbo` disagrees with the project its `project_id` names.
/// Resolution is not stopped: the id is authoritative and the slug is only a cross-check. What the
/// disagreement means is that the pointer came from another store — the folder was copied, or imported from
/// another environment — and that the id may now quietly name something else entirely. This warning is the
/// only sign of it, so it goes on the surfaces every session passes through first: the location header of
/// `status`/`whoami`, and what `bind` displays.
fn slug_mismatch_warning(store: &Store, binding: &amenbo_core::binding::DirBinding) -> Option<String> {
    let m = amenbo_core::binding::slug_mismatch(store, binding)?;
    Some(format!(
        "warning: this folder's .amenbo names project '{}', but {} is '{}' — the pointer looks \
         like it came from another store. Re-link it with `{} bind --project <name or ID>`.",
        Paths::command_name(),
        m.recorded,
        amenbo_core::idref::project(m.project_id),
        m.actual.as_deref().unwrap_or("(no slug)")
    ))
}

/// The blast radius: the decisions standing on this one (the reverse lookup of all three edge kinds, one hop
/// only). It exists to name the decisions that want revisiting when this one is superseded, rejected or
/// deleted; it never blocks the operation. Currency is not cascaded — the non-transitive rule stands, and a
/// longer chain can be walked but is never followed automatically. All three edge kinds count because
/// `supersedes` and `amends` imply `builds_on`: whatever corrects a decision necessarily stands on it. If the
/// detail cannot be read (the decision is already deleted, say), the result is empty — the suggestion simply
/// disappears, and the operation goes ahead.
fn standing_on(store: &Store, id: i64) -> Vec<amenbo_core::view::DecisionRef> {
    let Ok(d) = store.decision_detail(id) else {
        return Vec::new();
    };
    let mut out = d.superseded_by;
    out.extend(d.amended_by);
    out.extend(d.built_on_by);
    out
}

/// Carry the blast radius to a machine reader (`--json`): add a `revisit` field to the resource that was
/// operated on. Nothing is added when it is empty — an operation on a decision nothing stands on should not
/// be dressed up with "0 to revisit".
fn attach_revisit(resource: &mut serde_json::Value, standing: &[amenbo_core::view::DecisionRef]) {
    if !standing.is_empty() {
        resource["revisit"] = serde_json::to_value(standing).unwrap_or_else(|_| json!([]));
    }
}

/// Show the blast radius to a human, after the success line. A suggestion only — nothing is stopped here.
fn note_revisit(flags: &Flags, target: i64, standing: &[amenbo_core::view::DecisionRef]) {
    if standing.is_empty() {
        return;
    }
    human(flags, format!("note: these decisions stand on {} — revisit them:", decision_label(target)));
    for s in standing {
        human(flags, format!("  {} {}", decision_label(s.id), decision_ref_name(&s.name)));
    }
}

/// The holder-side surface of `AMB-D-366`: premises a task acquired **after it was reserved** — a blocker or
/// an unsettled decision pinned on since `in_progress` began, silently dropping `ready`. Read-only; the
/// reaction is the caller's (a quiet note on `task show`, a firm warn at completion). Only the reservation
/// holder is at risk, so callers gate on `status == in_progress`. A read error yields "nothing changed" —
/// this is additive context, never a reason to fail the command.
fn premise_change(store: &Store, tid: i64) -> amenbo_core::view::PremiseChange {
    store.premise_change_since(tid).unwrap_or_else(|_| no_premise_change())
}

/// The empty change — what a site reports when it did not look, or could not.
fn no_premise_change() -> amenbo_core::view::PremiseChange {
    amenbo_core::view::PremiseChange {
        added_blockers: Vec::new(),
        added_decisions: Vec::new(),
        reopened_decisions: Vec::new(),
    }
}

/// `premise_change` when `applies`, an empty change otherwise — so a safety-net site reads the premises only
/// on the transition that matters (leaving `in_progress`) and skips the query on every other status change.
fn premise_change_when(store: &Store, tid: i64, applies: bool) -> amenbo_core::view::PremiseChange {
    if applies {
        premise_change(store, tid)
    } else {
        no_premise_change()
    }
}

/// The premise-change lines, one per added premise, shared by the quiet and the firm surface.
fn premise_change_lines(pc: &amenbo_core::view::PremiseChange) -> Vec<String> {
    let mut out = Vec::new();
    for b in &pc.added_blockers {
        out.push(format!("  blocker {} {}", task_label(b.id), b.name));
    }
    for d in &pc.added_decisions {
        out.push(format!("  decision {} {} (not settled)", decision_label(d.id), decision_ref_name(&d.name)));
    }
    // The reopen axis (`AMB-D-373`): the link is not new, the decision's settlement is what went away.
    for d in &pc.reopened_decisions {
        out.push(format!("  decision {} {} (no longer settled)", decision_label(d.id), decision_ref_name(&d.name)));
    }
    out
}

/// The **firm** surface — the safety net that must not be missed: on a status change out of `in_progress`
/// (completing, blocking), warn that the reservation's premises shifted underneath. On stderr, so it reaches
/// both a human and a `--json` caller without touching stdout; `attach_premise_change` folds the same fact
/// into the JSON envelope. It never blocks the transition — `D-366` surfaces the change, it does not forbid
/// finishing (the holder may still ship the part that stands on its own).
fn warn_premise_change(pc: &amenbo_core::view::PremiseChange) {
    if !pc.any() {
        return;
    }
    eprintln!("⚠ Premises changed after you reserved this task — readiness was silently withdrawn (AMB-D-366):");
    for line in premise_change_lines(pc) {
        eprintln!("{line}");
    }
    eprintln!("  Finish only the part that stands on its own, or hand it back with `{} task status <id> todo`.", Paths::command_name());
}

/// Fold the premise change into a write command's JSON resource, so a `--json` caller sees it structurally
/// (not only as a stderr line). Absent when nothing changed, so the key appears exactly when it matters.
fn attach_premise_change(resource: &mut serde_json::Value, pc: &amenbo_core::view::PremiseChange) {
    if pc.any() {
        if let Some(obj) = resource.as_object_mut() {
            obj.insert("premise_change".to_string(), serde_json::to_value(pc).unwrap_or(json!(null)));
        }
    }
}

/// Resolve a task reference (`AMB-T-n`, or the bare `T-n` / `#n` / `n`). The numbers are globally unique on the device, so no
/// project context is needed. The id **is** the conversational number: nothing is abbreviated and nothing is
/// prefix-matched. The return type mirrors core's `resolve`, so `.map_err(CliError::from)?` works as is.
fn resolve_task(store: &Store, id: &str) -> amenbo_core::Result<i64> {
    store.resolve_task_ref(id)
}

fn task(store: &mut Store, flags: &Flags, sub: TaskCmd) -> Result<i32, CliError> {
    match sub {
        TaskCmd::Add { title, project, due, start, priority, notes, to, ai, dim } => {
            if ai && to.is_none() {
                return Err(CliError::from(amenbo_core::Error::invalid(
                    "--ai requires --to",
                    "--ai は --to と併せて使います",
                )));
            }
            // After the argument checks: a rejected invocation should not have drained the pipe first.
            let notes = body_arg(notes)?;
            // Resolve `--to` to a facet first, so an unknown assignee is refused before the task exists — an
            // error after creation would leave an orphan behind. The assignee is a facet and nothing else:
            // `--ai` means the AI facet, otherwise the token is resolved to one.
            let assignee_kind = match to {
                Some(ref to) => Some(if ai { ActorKind::Ai } else { store.resolve_assignee_facet(to).map_err(CliError::from)? }),
                None => None,
            };
            // Every task belongs to a project. Refuse a project-less create — an unnumbered orphan/inbox
            // task has low discoverability and breaks per-project numbering — and point at the existing
            // projects so the caller can pick one. Enforced here at the CLI write boundary, not in core
            // add_task: backup/migrate must still reconstruct legacy project-less rows (project_id: None),
            // so it is a write policy, not a data invariant.
            //
            // Where the slot comes from is `project_or_bound`'s answer, the same one `decision add` and
            // `dimension add` take: what `--project` named, else the folder's own binding. Not the *reach*,
            // which answers for an AI alone — a reach is what closes an AI to one project, and a human's is
            // the whole device, so reading it would make a human name the project their folder already
            // names. An AI cannot pass `--project`, so for it this is the binding either way; without a
            // binding there is nothing to fill the slot with, and the create is refused.
            let project_id = project_or_bound(store, project)?;
            let due_on = parse_date_opt(&due)?;
            let start_on = parse_date_opt(&start)?;
            let priority = match priority { Some(p) => Some(parse_priority(&p)?), None => None };
            // Resolved before the create, like `--to` above: a misspelled axis or value is an error with
            // no task left behind to go and classify by hand.
            let dimension_values = resolve_dim_pairs(store, project_id, &dim)?;
            let t = store.add_task_with_dimensions(ops::task::NewTask {
                title, project_id: Some(project_id), due_on, start_on, priority, notes,
                created_by_kind: Some(flags.facet()?),
            }, &dimension_values).map_err(CliError::from)?;
            emit_event(store, flags, t.id, activity_log::event::task_created(&t.title));
            // With `--to`, hand it over here as well, folding create→assign into one command. They are two
            // logical operations and therefore two transactions, so the add survives a failing assign.
            if let Some(kind) = assignee_kind {
                store.set_task_assignee(t.id, Some(kind), flags.facet()?).map_err(CliError::from)?;
                emit_event(store, flags, t.id, activity_log::event::task_assigned(Some(kind.as_str())));
            }
            let detail = store.task_detail(t.id).map_err(CliError::from)?;
            warn_body(&detail.notes); // non-blocking readability hint on write (stderr)
            write_envelope(flags, "task.add", "task", serde_json::to_value(&detail).unwrap(), None, false, format!("✓ Created task: {} ({})", t.title, task_label(t.id)));
        }
        TaskCmd::List { project, filter, sort, limit, offset } => {
            let project_id = project.map(|p| store.resolve_project_ref(&p)).transpose().map_err(CliError::from)?;
            let result = store.list_tasks(query::ListParams {
                project_id, filter_expr: filter, sort,
                limit, offset,
                // Reach belongs to the store — the surface does not declare it here; `Store`'s read supply
                // applies it.
            }).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, count_header(result.count, result.total_matched, "task"));
                // An empty mailbox that means "not yet" has to say so on the spot. A start day mistyped far
                // into the future hides a task in the one way nothing else catches — the list is empty and
                // reads as finished — so the count and the first day arrive with the emptiness.
                if let Some(w) = &result.waiting_on_start {
                    human(
                        flags,
                        format!(
                            "  ({} waiting on a start day — earliest {})",
                            w.count,
                            time::date_to_string(w.earliest)
                        ),
                    );
                }
                for t in &result.tasks {
                    let check = if t.completed { "x" } else { " " };
                    let due = t.due_on.map(|d| format!(" due:{}", time::date_to_string(d))).unwrap_or_default();
                    let pri = t.priority.map(|p| format!(" [{}]", p.as_str())).unwrap_or_default();
                    // Why this row is not in the mailbox, said on the row itself. A plain `task list` shows
                    // everything, so a task held back by a start day still ahead has to carry its reason
                    // here or it reads as ordinary work that the mailbox inexplicably skips. Written only
                    // when it is a reason — like `due:`, and unlike the marked-when-empty lines of `task
                    // show`, a listing row states what is so, and a date column of `-` on every other row
                    // buys nothing.
                    let waiting = t.not_started_until
                        .map(|d| format!(" waiting-until:{}", time::date_to_string(d)))
                        .unwrap_or_default();
                    human(flags, format!("  [{check}] {}  {}{}{}{}", task_label(t.id), t.title, due, waiting, pri));
                }
            }
        }
        TaskCmd::Show { id } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            // Close off, structurally, the mistake of reading only the notes and starting work without seeing
            // the comments or the decisions. A single `task show` must put all four in front of the reader —
            // the task, its notes, its linked decisions, its latest comments — so the links and comments are
            // fetched here too. Shipping them in one command's output is more reliable than prompting anyone
            // to go and look.
            let decisions = store.decisions_for_task(tid);
            let comments = store.comment_list(tid, None, None).map(|r| r.comments).unwrap_or_default();
            if flags.json {
                // TaskDetail stays as it is (task at the top level); linked_decisions / recent_comments are
                // added beside it. The existing keys do not move, so this is backwards compatible.
                let mut v = serde_json::to_value(&detail).unwrap_or(json!({}));
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("linked_decisions".to_string(), serde_json::to_value(&decisions).unwrap_or(json!([])));
                    obj.insert("recent_comments".to_string(), serde_json::to_value(&comments).unwrap_or(json!([])));
                }
                // The holder-side surface of `AMB-D-366`: only the reservation holder (in_progress) is at risk
                // of a premise silently pinned on after they reserved, so gate on it and fold in what changed.
                if detail.status == TaskStatus::InProgress {
                    attach_premise_change(&mut v, &premise_change(store, tid));
                }
                print_json(&v);
            } else {
                // One name only: the ref. The id is the conversational number, so there is no second
                // identifier to print alongside it.
                human(flags, format!("{}  {}", detail.r#ref, detail.title));
                let due = detail.due_on.map(time::date_to_string).unwrap_or_else(|| "-".to_string());
                human(flags, format!("completed: {} / due: {} / priority: {}", detail.completed, due, detail.priority.map(|p| p.as_str()).unwrap_or("-")));
                let assignee = match detail.assignee_kind {
                    Some(k) => query::facet_label(Some(k)),
                    None => "-".to_string(),
                };
                human(flags, format!("assignee: {} / comments: {}", assignee, detail.num_comments));
                // Always mark whether the task is placed; never omit the line when empty. Being unplaced
                // is a meaningful state — a task belonging to no project — so say `(none)` out loud.
                match &detail.placement {
                    None => human(flags, "project: (none)"),
                    Some(p) => human(flags, format!("project: {}", p.project.name)),
                }
                // Empty means nothing blocks this task and it can be started; say `(none)`. Omitting the line
                // would leave the reader unable to tell "no dependencies" from "dependencies not checked".
                if detail.blocked_by.is_empty() {
                    human(flags, "blocked by: (none)");
                } else {
                    let waiting = detail.blocked_by.iter()
                        .map(|b| format!("{} {}", task_label(b.id), b.name))
                        .collect::<Vec<_>>().join(", ");
                    human(flags, format!("blocked by: {waiting} (cannot start)"));
                }
                // A premise that is not settled stops the work too. Mark it even when empty, or the reader
                // cannot tell "the premises are settled" from "the premises were never checked".
                if detail.blocked_by_decisions.is_empty() {
                    human(flags, "blocked by decisions: (none)");
                } else {
                    let premises = detail.blocked_by_decisions.iter()
                        .map(|d| format!("{} {}", decision_label(d.id), decision_ref_name(&d.name)))
                        .collect::<Vec<_>>().join(", ");
                    human(flags, format!("blocked by decisions: {premises} (not settled — cannot start)"));
                }
                // The third reason a task is not ready: a start day that has not arrived. Marked even when
                // empty, like the two above — a reader who cannot tell "startable today" from "the start day
                // was never looked at" is back to guessing why the task is not in the mailbox.
                match detail.not_started_until {
                    None => human(flags, "not started until: (none)"),
                    Some(d) => human(
                        flags,
                        format!("not started until: {} (cannot start yet)", time::date_to_string(d)),
                    ),
                }
                // The quiet early-warning surface of `AMB-D-366`: if this task is reserved (in_progress) and a
                // premise was pinned on after the reservation — silently dropping `ready` — say so here, on
                // an ordinary read, so the holder notices long before they try to complete it. Only printed
                // when something actually shifted (nothing to say otherwise).
                if detail.status == TaskStatus::InProgress {
                    let pc = premise_change(store, tid);
                    if pc.any() {
                        human(flags, "premises changed since reserved (readiness withdrawn — AMB-D-366):");
                        for line in premise_change_lines(&pc) {
                            human(flags, line);
                        }
                    }
                }
                // The dependents — what becomes startable once this task is done. Always mark it, printing
                // `blocks: (none)` when empty; leaving the line out reads to an AI as "nothing follows".
                if detail.blocks.is_empty() {
                    human(flags, "blocks: (none)");
                } else {
                    let blocks = detail.blocks.iter()
                        .map(|b| format!("{} {}", task_label(b.id), b.name))
                        .collect::<Vec<_>>().join(", ");
                    human(flags, format!("blocks ({}): {blocks}", detail.blocks.len()));
                }
                // The rest of what must be read before starting, all in this one command: notes, linked
                // decisions, latest comments. Each is marked even when empty (`notes: (none)` and so on), so
                // an absent note is never mistaken for one that went unread.
                if detail.notes.trim().is_empty() {
                    human(flags, "notes: (none)");
                } else {
                    human(flags, format!("notes:\n{}", detail.notes));
                }
                if decisions.is_empty() {
                    human(flags, "decisions: (none)");
                } else {
                    human(flags, format!("decisions ({}):", decisions.len()));
                    for d in &decisions {
                        let r = decision_label(d.id);
                        human(flags, format!("  {r} [{}] {}", d.status, d.title));
                    }
                }
                // Whether comments exist is already marked by the summary line above (`comments: {n}`). This
                // is a preview of their text — the latest three, with the rest in `amenbo comment list <id>`
                // — so an empty one prints nothing and no information is lost.
                if !comments.is_empty() {
                    // comment_list runs oldest first. Reverse it so the newest is on top and cannot be
                    // missed, and show the latest three (the full set is `amenbo comment list <id>`).
                    human(flags, format!("comments ({}, newest first):", comments.len()));
                    for c in comments.iter().rev().take(3) {
                        human(flags, format!("  [{}] {}: {}", c.created_at.to_rfc3339_z(), c.author.name, c.text));
                    }
                }
            }
        }
        TaskCmd::Update { id, title, notes, due, start, priority, clear_due, clear_start, clear_priority } => {
            let notes = body_arg_opt(notes)?;
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let mut changed = Vec::new();
            if title.is_some() { changed.push("title".to_string()); }
            if notes.is_some() { changed.push("notes".to_string()); }
            if due.is_some() || clear_due { changed.push("due_on".to_string()); }
            if start.is_some() || clear_start { changed.push("start_on".to_string()); }
            if priority.is_some() || clear_priority { changed.push("priority".to_string()); }
            let due_on = parse_date_opt(&due)?;
            let start_on = parse_date_opt(&start)?;
            let priority = match priority { Some(p) => Some(parse_priority(&p)?), None => None };
            let t = store.update_task(tid, ops::task::TaskPatch {
                title, notes, due_on, start_on, priority, clear_due, clear_priority, clear_start,
            }).map_err(CliError::from)?;
            let detail = store.task_detail(t.id).map_err(CliError::from)?;
            // Hint only when notes were actually written — an update that touches just the title should not
            // drag the existing notes back up.
            if changed.iter().any(|c| c == "notes") {
                warn_body(&detail.notes);
            }
            write_envelope(flags, "task.update", "task", serde_json::to_value(&detail).unwrap(), Some(changed), false, format!("✓ Updated task: {}", task_label(t.id)));
        }
        TaskCmd::Done { id } => return task_complete(store, flags, &id, true),
        TaskCmd::Reopen { id } => return task_complete(store, flags, &id, false),
        TaskCmd::Status { id, status } => return task_set_status(store, flags, &id, &status),
        TaskCmd::Block { id, reason } => return task_block(store, flags, &id, body_arg_opt(reason)?),
        TaskCmd::Reject { id, reason } => return task_reject(store, flags, &id, body_arg(reason)?),
        TaskCmd::Move { id, project, before, after, top, bottom } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let project_id = project.map(|p| store.resolve_project_ref(&p)).transpose().map_err(CliError::from)?;
            let before = before.map(|b| resolve_task(store, &b)).transpose().map_err(CliError::from)?;
            let after = after.map(|a| resolve_task(store, &a)).transpose().map_err(CliError::from)?;
            let pos = pos_from_keys(top, bottom, before, after)?;
            let project_for_event = project_id.map(|pid| pid.to_string());
            let t = store.move_task(tid, project_id, pos, flags.facet()?).map_err(CliError::from)?;
            emit_event(store, flags, tid, activity_log::event::task_moved(project_for_event.as_deref()));
            let detail = store.task_detail(t.id).map_err(CliError::from)?;
            write_envelope(flags, "task.move", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["placement".to_string()]), false, format!("✓ Moved task: {}", task_label(t.id)));
        }
        TaskCmd::Depend { id, on } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let bid = resolve_task(store, &on).map_err(CliError::from)?;
            let (_edge, created) = store.depend_task(tid, bid, Some(flags.facet()?)).map_err(CliError::from)?;
            if created {
                warn_if_premise_added_to_reserved(store, tid, "you added a blocker it must be done after");
            }
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            write_envelope(flags, "task.depend", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["blocked_by".to_string()]), !created, format!("✓ Added dependency ({} waits on {}): {}", task_label(tid), task_label(bid), task_label(tid)));
        }
        TaskCmd::Undepend { id, on } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let bid = resolve_task(store, &on).map_err(CliError::from)?;
            let changed = store.undepend_task(tid, bid).map_err(CliError::from)?;
            // If dropping the blocker made this task ready, emit the unblock signal.
            if changed && newly_ready_or_warn(store, bid).contains(&tid) {
                emit_event(store, flags, tid, activity_log::event::task_unblocked(&bid.to_string()));
            }
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            write_envelope(flags, "task.undepend", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["blocked_by".to_string()]), !changed, format!("✓ Removed dependency: {}", task_label(tid)));
        }
        TaskCmd::Attach { id, source, url, name } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            return attach_add(store, flags, AttachmentTarget::Task, tid, &source, url, name);
        }
        TaskCmd::Commit { sub } => return task_commit(store, flags, sub),
        TaskCmd::Assign { id, to, ai } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            // The assignee is a facet and nothing else: `--ai` means the AI facet, otherwise the token is
            // resolved to one.
            let kind = if ai { ActorKind::Ai } else { store.resolve_assignee_facet(&to).map_err(CliError::from)? };
            // Assigning the same facet again is an idempotent no-op — skip the write entirely.
            // `set_task_assignee` commits its own transaction, so calling it would move updated_at even when
            // the value does not change.
            let noop = store.task(tid).map_err(CliError::from)?
                .is_some_and(|t| t.assignee_kind == Some(kind));
            if !noop {
                store.set_task_assignee(tid, Some(kind), flags.facet()?).map_err(CliError::from)?;
                emit_event(store, flags, tid, activity_log::event::task_assigned(Some(kind.as_str())));
            }
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            let to_label = if ai { " (to that person's AI)" } else { "" };
            let msg = format!("✓ Assigned{to_label}: {}", task_label(tid));
            write_envelope(flags, "task.assign", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["assignee".to_string()]), noop, msg);
        }
        TaskCmd::Unassign { id } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let noop = store.task(tid).map_err(CliError::from)?.is_some_and(|t| t.assignee_kind.is_none());
            if !noop {
                store.set_task_assignee(tid, None, flags.facet()?).map_err(CliError::from)?;
                emit_event(store, flags, tid, activity_log::event::task_assigned(None));
            }
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            write_envelope(flags, "task.unassign", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["assignee".to_string()]), noop, format!("✓ Unassigned: {}", task_label(tid)));
        }
        TaskCmd::Delete { id } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            guard_ai_task_delete(store, flags, tid)?;
            if !confirm(flags, "delete task")? {
                return Ok(0);
            }
            // Take the label before the delete — the row will not be there afterwards.
            let label = task_label(tid);
            store.delete_task(tid, flags.facet()?).map_err(CliError::from)?;
            write_envelope(flags, "task.delete", "task", json!({ "id": tid, "deleted": true }), None, false, format!("✓ Deleted task: {label}"));
        }
    }
    Ok(0)
}

// ───────────────────────── task commit ─────────────────────────

/// A task's git commit SHAs: record / list / forget. amenbo stores each SHA opaquely — the ops door
/// admits only full-length lower-case hex and folds case, and the `(task_id, sha)` index makes a
/// re-record idempotent. The chain runs history to task, since a public commit carries no store-local
/// reference.
fn task_commit(store: &mut Store, flags: &Flags, sub: TaskCommitCmd) -> Result<i32, CliError> {
    match sub {
        TaskCommitCmd::Add { task, sha } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let (row, created) = store.add_task_commit(tid, &sha, Some(flags.facet()?)).map_err(CliError::from)?;
            let msg = format!("✓ Recorded commit {} on {}", row.sha, task_label(tid));
            write_envelope(flags, "task.commit.add", "task_commit", serde_json::to_value(&row).unwrap(), None, !created, msg);
        }
        TaskCommitCmd::List { task } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let commits = store.task_commits(tid).map_err(CliError::from)?;
            if flags.json {
                print_json(&json!({ "task": tid, "count": commits.len(), "commits": commits }));
            } else {
                human(flags, format!("{} commit(s) — {}", commits.len(), task_label(tid)));
                for c in &commits {
                    human(flags, format!("  {}  [{}]", c.sha, c.created_at.to_rfc3339_z()));
                }
            }
        }
        TaskCommitCmd::Rm { task, sha } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            if !confirm(flags, "forget commit")? {
                return Ok(0);
            }
            let changed = store.remove_task_commit(tid, &sha).map_err(CliError::from)?;
            let data = json!({ "task": tid, "sha": sha, "deleted": changed });
            write_envelope(flags, "task.commit.rm", "task_commit", data, None, !changed, format!("✓ Forgot commit on {}", task_label(tid)));
        }
    }
    Ok(0)
}

// ───────────────────────── comment ─────────────────────────

fn comment(store: &mut Store, flags: &Flags, sub: CommentCmd) -> Result<i32, CliError> {
    match sub {
        CommentCmd::Add { task, text } => {
            let text = body_arg(text)?;
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            // The author is our own facet; add_comment's author argument is the trace string for the audit log.
            let s = store.add_task_comment(tid, flags.facet()?, &text).map_err(CliError::from)?;
            warn_body(&text); // non-blocking readability hint on write (stderr)
            write_envelope(flags, "comment.add", "comment", serde_json::to_value(&s).unwrap(), None, false, format!("✓ Added comment: {}", task_label(tid)));
        }
        CommentCmd::List { task, limit, offset } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let result = store.comment_list(tid, offset, limit).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, format!("{} — {}", count_header(result.count, result.total_matched, "comment"), result.task.name));
                for c in &result.comments {
                    human(flags, comment_line(amenbo_core::idref::RefKind::TaskComment, c));
                }
            }
        }
        CommentCmd::Rm { comment } => {
            let cid = resolve_live_task_comment(store, &comment)?;
            if !confirm(flags, "delete comment")? {
                return Ok(0);
            }
            let changed = store.remove_task_comment(cid, flags.facet()?).map_err(CliError::from)?;
            write_envelope(flags, "comment.rm", "comment", json!({ "id": cid, "deleted": true }), None, !changed, format!("✓ Deleted comment: {}", task_comment_label(cid)));
        }
        CommentCmd::Edit { comment, text } => {
            let text = body_arg(text)?;
            let cid = resolve_live_task_comment(store, &comment)?;
            let c = store.edit_task_comment(cid, &text).map_err(CliError::from)?;
            warn_body(&text); // non-blocking readability hint on write (stderr)
            write_envelope(flags, "comment.edit", "comment", serde_json::to_value(&c).unwrap(), Some(vec!["text".to_string()]), false, format!("✓ Edited comment: {}", task_comment_label(cid)));
        }
        CommentCmd::Attach { comment, source, url, name } => {
            // Look only in the task-comment table (as `comment rm` / `comment edit` do) — which table an id
            // belongs to is said by the command, not by the id.
            let cid = resolve_live_task_comment(store, &comment)?;
            return attach_add(store, flags, AttachmentTarget::TaskComment, cid, &source, url, name);
        }
    }
    Ok(0)
}

// ───────────────────────── decision ─────────────────────────

/// Resolve a decision reference (`AMB-D-n`, or the bare `D-n` / `#n` / `n`). The numbers are globally unique on the device — a
/// number space of their own, separate from tasks — so no project context is needed. The id **is** the
/// conversational number: nothing is abbreviated and nothing is prefix-matched.
fn resolve_decision(store: &Store, id: &str) -> amenbo_core::Result<i64> {
    store.resolve_decision_ref(id)
}

/// What a decision is called (`AMB-D-<n>`). The id is the conversational number, so reading the row would
/// tell us nothing the ref does not already carry — like [`task_label`], it is built straight from the id.
fn decision_label(id: i64) -> String {
    amenbo_core::idref::decision(id)
}

/// The display name of a decision reference. `None` means the target dangles — a forward edge onto a
/// decision no longer live, whose title cannot be read — and the CLI (English-fixed) composes the
/// placeholder the core deliberately withholds.
fn decision_ref_name(name: &Option<String>) -> &str {
    name.as_deref().unwrap_or("(unknown)")
}

/// What a task is called (`AMB-T-<n>`). The id is the conversational number, so the truth source is never
/// queried.
fn task_label(id: i64) -> String {
    amenbo_core::idref::task(id)
}

/// What a task comment is called (`AMB-TC-<n>`), and what a decision comment is called (`AMB-DC-<n>`). A
/// comment carries no conversational number of its own, so this ref is the only handle `comment rm` /
/// `comment attach` can be given — and the two tables number independently, so which spelling a caller
/// reaches for is decided by the table it just wrote, never by the id (`AMB-D-377`).
fn task_comment_label(id: i64) -> String {
    amenbo_core::idref::task_comment(id)
}

fn decision_comment_label(id: i64) -> String {
    amenbo_core::idref::decision_comment(id)
}

/// What a dimension is called (`AMB-DIM-<n>`), and one of its values (`AMB-DIMV-<n>`). Both resolve by name
/// too, so the ref is the tie-breaker rather than the everyday handle.
fn dimension_label(id: i64) -> String {
    amenbo_core::idref::render(amenbo_core::idref::RefKind::Dimension, id)
}

fn dimension_value_label(id: i64) -> String {
    amenbo_core::idref::render(amenbo_core::idref::RefKind::DimensionValue, id)
}

/// What a project is called (`AMB-P-<n>`), taken from the id as the probe reports it (a decimal string). An
/// unreadable id is shown as it came, rather than dressed up as a ref it is not.
fn project_label(id: &str) -> String {
    match id.parse::<i64>() {
        Ok(n) => amenbo_core::idref::project(n),
        Err(_) => id.to_string(),
    }
}

/// The project name shown alongside a binding. `None` when the id names no record — a `.amenbo` can go on
/// pointing at a project that is gone.
fn project_name(store: &Store, project_id: Option<i64>) -> Result<Option<String>, CliError> {
    let Some(pid) = project_id else { return Ok(None) };
    Ok(store.project(pid).map_err(CliError::from)?.map(|p| p.name))
}

fn decision(store: &mut Store, flags: &Flags, sub: DecisionCmd) -> Result<i32, CliError> {
    match sub {
        DecisionCmd::Add { title, body, project } => {
            let body = body_arg(body)?;
            let project_id = project_or_bound(store, project)?;
            let d = store.add_decision(ops::decision::NewDecision {
                title, body, project_id,
            }).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            warn_body(&detail.body); // non-blocking readability hint on write (stderr)
            write_envelope(flags, "decision.add", "decision", serde_json::to_value(&detail).unwrap(), None, false, format!("✓ Recorded decision: {} ({})", d.title, decision_label(d.id)));
        }
        DecisionCmd::List { project, filter, sort, limit, offset, with_body } => {
            let project_id = project.map(|p| store.resolve_project_ref(&p)).transpose().map_err(CliError::from)?;
            let result = store.decision_list(query::DecisionListParams {
                // `text` is the structural term, for a caller with no grammar to spell it in. The CLI has
                // one — `--filter "text:…"` — so it goes on saying it there.
                project_id, filter_expr: filter, text: None, sort, limit, offset, with_body,
            }).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, count_header(result.count, result.total_matched, "decision"));
                for d in &result.decisions {
                    // "Superseded" is not a status, so a decision that has been overturned is marked on the
                    // currency side instead.
                    let state =
                        if d.current { d.status.as_str().to_string() } else { format!("{}, superseded", d.status.as_str()) };
                    human(flags, format!("  {}  [{}] {} (tasks: {})", d.r#ref, state, d.title, d.linked_task_count));
                    // `--with-body`: follow with the body, indented — a body column on a narrowed page.
                    if let Some(body) = &d.body {
                        for line in body.lines() {
                            human(flags, format!("      {line}"));
                        }
                    }
                }
            }
        }
        DecisionCmd::Show { id } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let detail = store.decision_detail(did).map_err(CliError::from)?;
            if flags.json {
                print_json(&detail);
            } else {
                human(flags, format!("{}  {}", detail.r#ref, detail.title));
                human(flags, format!("status: {}", detail.status.as_str()));
                // Not current means another decision has replaced it; the "superseded by" line below names
                // the successor.
                if !detail.current {
                    human(flags, "current: no");
                }
                // Each edge kind is a set — one decision may supersede or amend several others — so every
                // edge gets its own line.
                for (label, edges) in [
                    ("supersedes", &detail.supersedes),
                    ("superseded by", &detail.superseded_by),
                    ("amends", &detail.amends),
                    ("amended by", &detail.amended_by),
                ] {
                    for s in edges.iter() {
                        human(flags, format!("{label}: {} {}", decision_label(s.id), decision_ref_name(&s.name)));
                    }
                }
                // The premises this decision stands on — read them first. A premise that has been overturned
                // is called out on its own line: this decision stands on rotten ground and wants revisiting
                // (the reason the edge type exists at all).
                for p in detail.builds_on.iter() {
                    let rot = match (p.current, p.superseded_by.as_deref()) {
                        (false, Some(by)) => format!("  ⚠ premise superseded by {by} — revisit this decision"),
                        (false, None) => "  ⚠ premise is no longer current — revisit this decision".to_string(),
                        _ => String::new(),
                    };
                    human(flags, format!("builds on: {} {}{rot}", decision_label(p.id), decision_ref_name(&p.name)));
                }
                // The reverse edge: the decisions that would want revisiting if this one were overturned —
                // its blast radius, one hop out.
                for s in detail.built_on_by.iter() {
                    human(flags, format!("built on by: {} {}", decision_label(s.id), decision_ref_name(&s.name)));
                }
                // Mark both the body and the linked tasks even when they are empty. An empty body means a
                // draft whose conclusion was never written, and printing nothing would leave the reader
                // unable to tell that from having simply missed it.
                if detail.body.is_empty() {
                    human(flags, "body: (none)");
                } else {
                    human(flags, format!("\n{}", detail.body));
                }
                if detail.linked_tasks.is_empty() {
                    human(flags, "linked tasks: (none)");
                } else {
                    // Let the decision show whether the work it spawned is still outstanding. What has
                    // ended recedes behind an `[x]` — carried out or decided against, it is off the list
                    // either way — and everything but `todo`, the default, names its state, so a task that
                    // receded still says which of the two ways it went.
                    human(flags, "linked tasks:");
                    for t in detail.linked_tasks.iter() {
                        let check = if t.status.is_closed() { "x" } else { " " };
                        let state = match t.status {
                            TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::Rejected => {
                                format!(" ({})", t.status.as_str())
                            }
                            TaskStatus::Todo | TaskStatus::Done => String::new(),
                        };
                        human(flags, format!("  [{check}] {} {}{state}", task_label(t.id), t.name));
                    }
                }
            }
        }
        DecisionCmd::Edit { id, title, body } => {
            let body = body_arg_opt(body)?;
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let mut changed = Vec::new();
            if title.is_some() { changed.push("title".to_string()); }
            if body.is_some() { changed.push("body".to_string()); }
            let d = store.update_decision(did, ops::decision::DecisionPatch { title, body }).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            write_envelope(flags, "decision.edit", "decision", serde_json::to_value(&detail).unwrap(), Some(changed), false, format!("✓ Edited decision: {}", decision_label(d.id)));
        }
        DecisionCmd::Accept { id, reason } => {
            let reason = body_arg_opt(reason)?;
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let by = flags.facet()?.as_str().to_string();
            let (d, changed) = store.accept_decision(did, Some(by), flags.facet()?).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            if changed {
                // `--reason` is thin sugar for adding one comment with the reason (the same shape as
                // `task block --reason`). It gets no field of its own. Only on a real acceptance —
                // re-accepting an already-settled decision changes nothing, so a reason must not pile up.
                add_reason_comment(store, flags, did, reason)?;
                write_envelope(flags, "decision.accept", "decision", serde_json::to_value(&detail).unwrap(), Some(vec!["status".to_string()]), false, format!("✓ Accepted decision: {}", decision_label(d.id)));
            } else {
                // Already accepted: say so plainly instead of a bare "✓" that reads as "just now settled".
                // The facet that accepted it is frozen; `reopen` is the sanctioned route to change it.
                write_envelope(flags, "decision.accept", "decision", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("• Decision {} is already accepted{} — no change. To change who accepted it, `reopen` then `accept` again.", decision_label(d.id), accepted_by_suffix(&d)));
            }
        }
        DecisionCmd::Reject { id, reason } => {
            let reason = body_arg_opt(reason)?;
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            // Read the blast radius (one hop) before rejecting. A reject leaves the edges in place, but
            // keeping the order the same gives all three verbs the same shape.
            let standing = standing_on(store, did);
            let (d, changed) = store.reject_decision(did, flags.facet()?).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            let mut resource = serde_json::to_value(&detail).unwrap();
            attach_revisit(&mut resource, &standing);
            if changed {
                // Only attach the reason on a real rejection; a re-reject changes nothing.
                add_reason_comment(store, flags, did, reason)?;
                write_envelope(flags, "decision.reject", "decision", resource, Some(vec!["status".to_string()]), false, format!("✓ Rejected decision: {}", decision_label(d.id)));
                note_revisit(flags, did, &standing);
            } else {
                write_envelope(flags, "decision.reject", "decision", resource, Some(vec![]), true, format!("• Decision {} is already rejected — no change.", decision_label(d.id)));
            }
        }
        DecisionCmd::Reopen { id } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let (d, changed) = store.reopen_decision(did).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            if changed {
                warn_if_unsettled_under_reserved(d.id, &detail, "reopening it");
                write_envelope(flags, "decision.reopen", "decision", serde_json::to_value(&detail).unwrap(), Some(vec!["status".to_string()]), false, format!("✓ Reopened decision: {}", decision_label(d.id)));
            } else {
                // Already proposed: reopening changes nothing, so say so plainly instead of a bare "✓"
                // that reads as "just now reopened" — the same two-branch shape as accept/reject.
                write_envelope(flags, "decision.reopen", "decision", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("• Decision {} is already proposed — no change.", decision_label(d.id)));
            }
        }
        DecisionCmd::Delete { id } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            // Take the label before the delete — the row will not be there afterwards — and read the blast
            // radius up front for the same reason.
            let label = decision_label(did);
            let standing = standing_on(store, did);
            if !confirm(flags, "delete decision")? {
                return Ok(0);
            }
            store.delete_decision(did, flags.facet()?).map_err(CliError::from)?;
            let mut resource = json!({ "id": did, "deleted": true });
            attach_revisit(&mut resource, &standing);
            write_envelope(flags, "decision.delete", "decision", resource, None, false, format!("✓ Deleted decision: {label}"));
            note_revisit(flags, did, &standing);
        }
        DecisionCmd::Supersede { decision: new_ref, replaces } => {
            let new_id = resolve_decision(store, &new_ref).map_err(CliError::from)?;
            let old_id = resolve_decision(store, &replaces).map_err(CliError::from)?;
            // Read the blast radius before drawing the edge: read it afterwards and the supersedes edge just
            // drawn (new_id itself) turns up among the decisions said to want revisiting.
            let standing = standing_on(store, old_id);
            let by = flags.facet()?.as_str().to_string();
            let (d, changed) = store.supersede_decision(new_id, old_id, Some(by), flags.facet()?).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            let mut resource = serde_json::to_value(&detail).unwrap();
            attach_revisit(&mut resource, &standing);
            if changed {
                // The old side is what stopped being current, so its card — not the new decision's — holds
                // the reservations whose ground just moved. Read only when the edge actually landed.
                match store.decision_detail(old_id) {
                    Ok(old) => warn_if_unsettled_under_reserved(old_id, &old, "superseding it"),
                    Err(e) => eprintln!(
                        "warning: could not check what rests on {}: {e}",
                        decision_label(old_id)
                    ),
                }
                write_envelope(flags, "decision.supersede", "decision", resource, Some(vec!["status".to_string(), "supersedes".to_string()]), false, format!("✓ {} supersedes {}", decision_label(new_id), decision_label(old_id)));
                note_revisit(flags, old_id, &standing);
            } else {
                // The edge was already there and the new side already settled: nothing to draw.
                write_envelope(flags, "decision.supersede", "decision", resource, Some(vec![]), true, format!("• {} already supersedes {} — no change.", decision_label(new_id), decision_label(old_id)));
            }
        }
        DecisionCmd::Amend { decision: new_ref, amends } => {
            let new_id = resolve_decision(store, &new_ref).map_err(CliError::from)?;
            let old_id = resolve_decision(store, &amends).map_err(CliError::from)?;
            let d = store.amend_decision(new_id, old_id).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            write_envelope(flags, "decision.amend", "decision", serde_json::to_value(&detail).unwrap(), Some(vec!["amends".to_string()]), false, format!("✓ {} amends {}", decision_label(new_id), decision_label(old_id)));
        }
        DecisionCmd::BuildsOn { decision: new_ref, on: premise_ref } => {
            let new_id = resolve_decision(store, &new_ref).map_err(CliError::from)?;
            let old_id = resolve_decision(store, &premise_ref).map_err(CliError::from)?;
            let d = store.decision_builds_on(new_id, old_id).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            write_envelope(flags, "decision.builds_on", "decision", serde_json::to_value(&detail).unwrap(), Some(vec!["builds_on".to_string()]), false, format!("✓ {} builds on {}", decision_label(new_id), decision_label(old_id)));
        }
        DecisionCmd::Unlink { decision: from_ref, from: to_ref } => {
            let decision_id = resolve_decision(store, &from_ref).map_err(CliError::from)?;
            let target_id = resolve_decision(store, &to_ref).map_err(CliError::from)?;
            let removed = store.unlink_decision_edge(decision_id, target_id).map_err(CliError::from)?;
            write_envelope(flags, "decision.unlink_edge", "decision_edge", json!({ "decision_id": decision_id, "target_decision_id": target_id, "unlinked": removed }), None, !removed, format!("✓ Unlinked {} → {}", decision_label(decision_id), decision_label(target_id)));
        }
        DecisionCmd::Link { decision: d_ref, task, unlink } => {
            let did = resolve_decision(store, &d_ref).map_err(CliError::from)?;
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            if unlink {
                let changed = store.unlink_decision(did, tid).map_err(CliError::from)?;
                write_envelope(flags, "decision.unlink", "decision_task_link", json!({ "decision_id": did, "task_id": tid, "unlinked": changed }), None, !changed, format!("✓ Unlinked {} ⇄ {}", decision_label(did), task_label(tid)));
            } else {
                let (l, created) = store.link_decision(did, tid).map_err(CliError::from)?;
                if created {
                    warn_if_premise_added_to_reserved(store, tid, "you linked a decision it now rests on as a premise");
                }
                write_envelope(flags, "decision.link", "decision_task_link", serde_json::to_value(&l).unwrap(), None, !created, format!("✓ Linked {} ⇄ {}", decision_label(did), task_label(tid)));
            }
        }
        DecisionCmd::Promote { comment, title, project } => {
            let from_task = store.resolve_task_comment(&comment).map_err(CliError::from)?.first().copied();
            let from_decision = store.resolve_decision_comment(&comment).map_err(CliError::from)?.first().copied();
            let (did, source) = match (from_task, from_decision) {
                (Some(a), Some(b)) => return Err(ambiguous_comment(&comment, a, b)),
                (Some(cid), None) => (promote_task_comment(store, cid, title, project)?, task_comment_label(cid)),
                (None, Some(cid)) => (promote_decision_comment(store, cid, title, project)?, decision_comment_label(cid)),
                (None, None) => return Err(comment_not_found(&comment)),
            };
            let detail = store.decision_detail(did).map_err(CliError::from)?;
            write_envelope(flags, "decision.promote", "decision", serde_json::to_value(&detail).unwrap(), None, false, format!("✓ Promoted {source} to decision: {} ({})", detail.title, decision_label(did)));
        }
        DecisionCmd::Comment { sub } => return decision_comment(store, flags, sub),
        DecisionCmd::Attach { id, source, url, name } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            return attach_add(store, flags, AttachmentTarget::Decision, did, &source, url, name);
        }
    }
    Ok(0)
}

/// The task-comment side of `decision promote`: the comment's text becomes the body, its task's project
/// becomes the home, and the new decision is linked back to that task — the decision is that task's
/// premise, which is exactly what the edge says.
fn promote_task_comment(store: &mut Store, cid: i64, title: String, project: Option<String>) -> Result<i64, CliError> {
    let c = store.task_comment(cid).map_err(CliError::from)?.ok_or_else(|| comment_not_found(&task_comment_label(cid)))?;
    let task_id = c.task_id;
    let body = c.text.clone();
    let project_id = match project {
        Some(p) => store.resolve_project_ref(&p).map_err(CliError::from)?,
        None => store.task(task_id).map_err(CliError::from)?
            .and_then(|t| t.project_id)
            .ok_or_else(|| CliError { code: "invalid_value", message: "the comment's task has no project; pass --project".to_string(), hint: None, exit: 2 })?,
    };
    let d = store.add_decision(ops::decision::NewDecision { title, body, project_id }).map_err(CliError::from)?;
    store.link_decision(d.id, task_id).map_err(CliError::from)?;
    Ok(d.id)
}

/// The decision-comment side of `decision promote`: the text becomes the body and the comment's decision
/// gives the home, but **no edge is drawn back to it**. A record raised out of a decision's comment thread
/// is a question that turned into its own, and an automatic link would claim a relation promote cannot
/// know. Where one does hold, its author names it — `builds-on`, `amend`, `supersede`.
fn promote_decision_comment(store: &mut Store, cid: i64, title: String, project: Option<String>) -> Result<i64, CliError> {
    let c = store.decision_comment(cid).map_err(CliError::from)?.ok_or_else(|| comment_not_found(&decision_comment_label(cid)))?;
    let body = c.text.clone();
    let project_id = match project {
        Some(p) => store.resolve_project_ref(&p).map_err(CliError::from)?,
        None => store.decision_detail(c.decision_id).map_err(CliError::from)?
            .project.map(|p| p.id)
            .ok_or_else(|| CliError { code: "invalid_value", message: "the comment's decision has no project; pass --project".to_string(), hint: None, exit: 2 })?,
    };
    let d = store.add_decision(ops::decision::NewDecision { title, body, project_id }).map_err(CliError::from)?;
    Ok(d.id)
}

/// A bare `<n>` handed to `decision promote` when both comment tables hold that key. They number
/// independently, so the number alone names a row in each and the kind code is what disjoins them — the
/// same shape as a bare number that is both a task and a decision. Refused, never guessed at.
fn ambiguous_comment(reference: &str, task_comment_id: i64, decision_comment_id: i64) -> CliError {
    CliError {
        code: "invalid_value",
        message: format!(
            "'{reference}' names both {} and {}; say which",
            task_comment_label(task_comment_id),
            decision_comment_label(decision_comment_id)
        ),
        hint: None,
        exit: 2,
    }
}

/// Record the reason a decision was accepted or rejected as a comment (the same shape as
/// `task block --reason`). An empty or whitespace-only reason is ignored.
/// A `" (by <facet>, <utc>)"` suffix naming who settled an already-accepted decision, empty when the
/// stamps are missing. Shown on the idempotent re-accept so `reopen` is an informed choice, not a guess.
fn accepted_by_suffix(d: &amenbo_core::model::Decision) -> String {
    match (&d.decided_by, &d.decided_at) {
        (Some(who), Some(at)) => format!(" (by {who}, {})", at.to_rfc3339_z()),
        (Some(who), None) => format!(" (by {who})"),
        (None, Some(at)) => format!(" (at {})", at.to_rfc3339_z()),
        (None, None) => String::new(),
    }
}

fn add_reason_comment(store: &mut Store, flags: &Flags, decision_id: i64, reason: Option<String>) -> Result<(), CliError> {
    if let Some(r) = reason.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        // The author is our own facet; the author argument is the trace string for the audit log.
        store.add_decision_comment(decision_id, flags.facet()?, r).map_err(CliError::from)?;
    }
    Ok(())
}

/// `decision comment add/list` — mirrors [`comment`] on the task side.
fn decision_comment(store: &mut Store, flags: &Flags, sub: DecisionCommentCmd) -> Result<i32, CliError> {
    match sub {
        DecisionCommentCmd::Add { decision, text } => {
            let text = body_arg(text)?;
            let did = resolve_decision(store, &decision).map_err(CliError::from)?;
            // The author is our own facet; add_comment's author argument is the trace string for the audit log.
            let c = store.add_decision_comment(did, flags.facet()?, &text).map_err(CliError::from)?;
            warn_body(&text); // non-blocking readability hint on write (stderr)
            write_envelope(flags, "decision.comment.add", "comment", serde_json::to_value(&c).unwrap(), None, false, format!("✓ Added comment: {}", decision_label(did)));
        }
        DecisionCommentCmd::List { decision, limit, offset } => {
            let did = resolve_decision(store, &decision).map_err(CliError::from)?;
            let result = store.decision_comment_list(did, offset, limit).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, format!("{} — {}", count_header(result.count, result.total_matched, "comment"), decision_ref_name(&result.decision.name)));
                for c in &result.comments {
                    human(flags, comment_line(amenbo_core::idref::RefKind::DecisionComment, c));
                }
            }
        }
        DecisionCommentCmd::Rm { comment } => {
            let cid = resolve_live_decision_comment(store, &comment)?;
            if !confirm(flags, "delete comment")? {
                return Ok(0);
            }
            let changed = store.remove_decision_comment(cid).map_err(CliError::from)?;
            write_envelope(flags, "decision.comment.rm", "comment", json!({ "id": cid, "deleted": true }), None, !changed, format!("✓ Deleted comment: {}", decision_comment_label(cid)));
        }
        DecisionCommentCmd::Edit { comment, text } => {
            let text = body_arg(text)?;
            let cid = resolve_live_decision_comment(store, &comment)?;
            let c = store.edit_decision_comment(cid, &text).map_err(CliError::from)?;
            warn_body(&text);
            write_envelope(flags, "decision.comment.edit", "comment", serde_json::to_value(&c).unwrap(), Some(vec!["text".to_string()]), false, format!("✓ Edited comment: {}", decision_comment_label(cid)));
        }
        DecisionCommentCmd::Attach { comment, source, url, name } => {
            // Look only in the decision-comment table (symmetric with the task side of `comment attach`).
            let cid = resolve_live_decision_comment(store, &comment)?;
            return attach_add(store, flags, AttachmentTarget::DecisionComment, cid, &source, url, name);
        }
    }
    Ok(0)
}

/// One comment as a human-readable line, shared by `comment list` and `decision comment list` — which is
/// why the ref's kind is passed in: the two listings read different tables, and the same id names a row in
/// each (`AMB-D-377`). It leads with the comment's ref: a comment carries no conversational number, so this
/// id is the only handle that can be passed to `comment rm` / `comment attach`, and a comment left out of
/// the listing is not addressable at all. It is namespaced like every other exposed ref and pastes straight
/// back — resolution reads `AMB-TC-<n>` / `AMB-DC-<n>` and the bare `<n>` alike. A comment that was edited
/// shows the edit time next to the post time — no revision history is kept, so this is the reader's only
/// clue that the text is not what they read a moment ago. An unedited comment adds nothing.
fn comment_line(kind: amenbo_core::idref::RefKind, c: &amenbo_core::query::CommentItem) -> String {
    let at = c.created_at.to_rfc3339_z();
    let edited = c.edited_at.map(|t| format!(" · edited {}", t.to_rfc3339_z())).unwrap_or_default();
    format!("  {}  [{at}{edited}] {}: {}", amenbo_core::idref::render(kind, c.id), c.author.name, c.text)
}

/// Resolve a live task-comment id (for `comment rm`).
fn resolve_live_task_comment(store: &Store, reference: &str) -> Result<i64, CliError> {
    let hits = store.resolve_task_comment(reference).map_err(CliError::from)?;
    pick_comment(hits, reference)
}

/// Resolve a live decision-comment id — the decision-side counterpart of [`resolve_live_task_comment`].
fn resolve_live_decision_comment(store: &Store, reference: &str) -> Result<i64, CliError> {
    let hits = store.resolve_decision_comment(reference).map_err(CliError::from)?;
    pick_comment(hits, reference)
}

fn pick_comment(hits: Vec<i64>, reference: &str) -> Result<i64, CliError> {
    hits.into_iter().next().ok_or_else(|| comment_not_found(reference))
}

/// A comment reference that names no row — in either comment table. The hint points at both listings: a
/// comment carries no conversational number, so the listing is the only place its id comes from.
fn comment_not_found(reference: &str) -> CliError {
    CliError {
        code: "not_found",
        message: format!("comment '{reference}' not found"),
        hint: Some("list the comments to find the id (`comment list <task>` / `decision comment list <decision>`)".to_string()),
        exit: 1,
    }
}

// ───────────────────────── attach ─────────────────────────

/// The attachment's display label (`att:<id>`).
fn attach_label(a: &Attachment) -> String {
    format!("att:{}", a.id)
}

/// The shared body of `task attach` / `decision attach`: ingest `source` as a blob (the default), or with
/// `--url` register it as an external link. A blob is checked against the per-file size limit before it is
/// ingested. The only creator recorded is the effective facet (`created_by_kind`). Invariant — ingest comes
/// last: the caller must resolve `target_id` before reaching here. Ingest ahead of resolving the target
/// would let a failed attach strand a pinned blob that nothing references. Blob reclamation happens on the
/// delete paths (`attach rm`, deleting a task or a decision), each collecting the orphans it made, so bytes
/// from an attach that never came to be pass through no delete at all: only `doctor --fix`'s full scan will
/// ever pick them up, and until then `blobs/` grows with every failure — and so does every archive, since
/// `backup` packs `blobs/` whole. For the same reason, reading the file's metadata and checking the size
/// limit both go before `ingest_path` (`failed_attach_ingests_nothing`).
fn attach_add(
    store: &mut Store,
    flags: &Flags,
    target_type: AttachmentTarget,
    target_id: i64,
    source: &str,
    url: bool,
    name: Option<String>,
) -> Result<i32, CliError> {
    let a = if url {
        store.attach_url(target_type, target_id, source, name.as_deref(), flags.facet()?)
            .map_err(CliError::from)?
    } else {
        let path = std::path::Path::new(source);
        let meta = std::fs::metadata(path).map_err(|e| CliError {
            code: "not_found",
            message: format!("cannot read file '{source}': {e}"),
            hint: Some("pass a readable file path, or use --url to attach an external link".to_string()),
            exit: 1,
        })?;
        if !meta.is_file() {
            return Err(CliError { code: "invalid_value", message: format!("'{source}' is not a regular file"), hint: None, exit: 2 });
        }
        let filename = name
            .clone()
            .or_else(|| path.file_name().and_then(|n| n.to_str()).map(str::to_string))
            .unwrap_or_else(|| "attachment".to_string());
        let mime = amenbo_core::blob::mime_from_filename(&filename);
        // Check the per-file limit (which varies by type) before ingesting — it is what stops a runaway.
        store.config.attachment_limits.check_per_file(mime, meta.len()).map_err(CliError::from)?;
        let blob = store.blobs().ingest_path(path).map_err(CliError::from)?;
        store.attach_blob(target_type, target_id, &blob.hash, &filename, mime, blob.size_bytes as i64, flags.facet()?)
            .map_err(CliError::from)?
    };
    let what = if url { "link" } else { "file" };
    write_envelope(flags, "attach.add", "attachment", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Attached {what}: {}", attach_label(&a)));
    Ok(0)
}

/// The `attach` group (ls/show/open/rm). Adding lives on `task attach` / `decision attach`.
fn attach(store: &mut Store, flags: &Flags, sub: AttachCmd) -> Result<i32, CliError> {
    match sub {
        AttachCmd::Ls { target, task_comment, decision_comment } => {
            let (target_type, target_id) = resolve_attach_ls_target(store, target.as_deref(), task_comment.as_deref(), decision_comment.as_deref())?;
            let list = store.attachments_for_target(target_type, target_id)?;
            if flags.json {
                print_json(&serde_json::json!({ "count": list.len(), "attachments": list }));
            } else {
                human(flags, format!("{} attachment(s)", list.len()));
                for a in &list {
                    human(flags, format!("  {}", attach_line(a)));
                }
            }
        }
        AttachCmd::Show { id } => {
            let a = resolve_attachment(store, &id)?;
            if flags.json {
                print_json(&a);
            } else {
                human(flags, attach_line(&a));
            }
        }
        AttachCmd::Open { id } => {
            use amenbo_core::model::AttachmentKind;
            let a = resolve_attachment(store, &id)?;
            let target = match (a.kind, a.url.as_deref(), a.blob_hash.as_deref()) {
                // The way in (`ops::attachment::add_url`) allows only web schemes, but rows written before
                // that check existed can still hold anything. os_open interprets whatever it is handed, so
                // check again right before opening: no `file:` reaching a local file, no leading `-` being
                // taken for a command option.
                (AttachmentKind::Url, Some(u), _) if amenbo_core::ops::attachment::is_web_url(u) => u.to_string(),
                (AttachmentKind::Url, Some(u), _) => {
                    return Err(CliError {
                        code: "invalid_value",
                        message: format!("refusing to open '{u}' (only http, https and mailto)"),
                        hint: None,
                        exit: 2,
                    })
                }
                (AttachmentKind::Blob, _, Some(h)) => materialize_blob_temp(store, h, a.filename.as_deref())?,
                _ => {
                    return Err(CliError { code: "invalid_value", message: "attachment has neither a url nor a local blob".to_string(), hint: None, exit: 2 })
                }
            };
            os_open(&target)?;
            write_envelope(flags, "attach.open", "attachment", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Opened {}", attach_label(&a)));
        }
        AttachCmd::Save { id, out, force } => {
            use amenbo_core::model::AttachmentKind;
            let a = resolve_attachment(store, &id)?;
            // Only a blob has bytes to save. A URL attachment records a link, not a file — open it.
            let hash = match (a.kind, a.blob_hash.as_deref()) {
                (AttachmentKind::Blob, Some(h)) => h,
                (AttachmentKind::Url, _) => {
                    return Err(CliError {
                        code: "invalid_value",
                        message: "this attachment is an external link, not a stored file — open it with `attach open`".to_string(),
                        hint: None,
                        exit: 2,
                    })
                }
                _ => {
                    return Err(CliError { code: "invalid_value", message: "attachment has no local blob to save".to_string(), hint: None, exit: 2 })
                }
            };
            if !store.blobs().has(hash) {
                return Err(CliError { code: "not_found", message: format!("blob {hash} is not stored locally"), hint: None, exit: 1 });
            }
            let filename = a.filename.clone().unwrap_or_else(|| "attachment".to_string());
            // `--out` is a file path, unless it names an existing directory — then save under the
            // attachment's own filename inside it. With no `--out`, that filename in the CWD.
            let dest = match out.as_deref() {
                None => std::path::PathBuf::from(&filename),
                Some(p) => {
                    let p = std::path::Path::new(p);
                    if p.is_dir() { p.join(&filename) } else { p.to_path_buf() }
                }
            };
            if dest.exists() && !force {
                return Err(CliError {
                    code: "file_exists",
                    message: format!("{} already exists", dest.display()),
                    hint: Some("pass --force to overwrite".to_string()),
                    exit: 1,
                });
            }
            let bytes = store.blobs().read(hash).map_err(CliError::from)?;
            std::fs::write(&dest, &bytes).map_err(|e: std::io::Error| CliError {
                code: "io_error",
                message: format!("cannot write {}: {e}", dest.display()),
                hint: None,
                exit: 1,
            })?;
            write_envelope(flags, "attach.save", "attachment", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Saved {} → {}", attach_label(&a), dest.display()));
        }
        AttachCmd::Rm { id } => {
            let a = resolve_attachment(store, &id)?;
            if !confirm(flags, "remove attachment")? {
                return Ok(0);
            }
            let changed = store.remove_attachment(a.id).map_err(CliError::from)?;
            write_envelope(flags, "attach.rm", "attachment", serde_json::to_value(&a).unwrap(), None, !changed, format!("✓ Removed {}", attach_label(&a)));
        }
    }
    Ok(0)
}

/// Resolve what `attach ls` lists. A task or a decision is named by its reference in its number space
/// (`AMB-T-n` / `AMB-D-n`); a comment is named by a flag that says which table it is in (`--task-comment` /
/// `--decision-comment`). The two comment tables are numbered independently, so a bare `5` could equally be
/// task comment 5 or decision comment 5. Rather than stamp a kind onto the id, the command says which table
/// it means — the same shape as `comment attach` / `decision comment attach` splitting the tables by
/// namespace, except that here a flag makes the choice.
fn resolve_attach_ls_target(
    store: &Store,
    target: Option<&str>,
    task_comment: Option<&str>,
    decision_comment: Option<&str>,
) -> Result<(AttachmentTarget, i64), CliError> {
    if let Some(id) = task_comment {
        return Ok((AttachmentTarget::TaskComment, resolve_live_task_comment(store, id)?));
    }
    if let Some(id) = decision_comment {
        return Ok((AttachmentTarget::DecisionComment, resolve_live_decision_comment(store, id)?));
    }
    let Some(target) = target else {
        return Err(CliError {
            code: "invalid_args",
            message: "no target given".to_string(),
            hint: Some("pass a task/decision ref (#n / T-n / D-n), or --task-comment <id> / --decision-comment <id>".to_string()),
            exit: 2,
        });
    };
    match store.resolve_any_ref(target)? {
        ops::Ref::Task(id) => Ok((AttachmentTarget::Task, id)),
        ops::Ref::Decision(id) => Ok((AttachmentTarget::Decision, id)),
    }
}

/// Look an attachment up by id, matched exactly. Only live attachments exist to be found — a removed one has
/// no row. The id is the key of a single table, so it matches either nothing or one thing, and can never be
/// ambiguous.
fn resolve_attachment(store: &Store, id: &str) -> Result<Attachment, CliError> {
    let not_found = || CliError {
        code: "not_found",
        message: format!("attachment '{id}' not found"),
        hint: Some(format!("list ids with `{} attach ls <target>`", Paths::command_name())),
        exit: 1,
    };
    let hit = store.resolve_attachment(id)?.first().copied().ok_or_else(not_found)?;
    store.attachment(hit)?.ok_or_else(not_found)
}

/// One attachment, summarized as a line for a human.
fn attach_line(a: &Attachment) -> String {
    use amenbo_core::model::AttachmentKind;
    let label = a.filename.clone().or_else(|| a.url.clone()).unwrap_or_else(|| "(no name)".to_string());
    match a.kind {
        AttachmentKind::Blob => {
            let size = a.size_bytes.unwrap_or(0);
            let mime = a.mime.as_deref().unwrap_or("application/octet-stream");
            format!("{}  blob  {label}  {mime}  {size}B", attach_label(a))
        }
        AttachmentKind::Url => {
            let u = a.url.as_deref().unwrap_or("");
            format!("{}  url   {label}  {u}", attach_label(a))
        }
    }
}

/// The one directory `attach open` puts its temp copies in. Everything it leaves behind lives here, which
/// is what makes the copies sweepable as a set (and identifiable, by anyone wondering what wrote them).
/// On unix it is created 0700: the system temp dir is shared, and an attachment's bytes are the user's.
fn open_temp_dir() -> Result<std::path::PathBuf, std::io::Error> {
    let dir = std::env::temp_dir().join("amenbo-open");
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(&dir)?;
    Ok(dir)
}

/// How long a temp copy is kept before a later `attach open` reclaims it. `attach open` cannot delete the
/// file it just wrote — it hands the path to another application, which is still reading it — so nothing
/// can clean up after a given open except a *later* one. The window is generous because the cost of being
/// wrong runs one way: sweeping a file that is still open breaks something the user is looking at, while
/// keeping one too long costs a few bytes of temp until the next open.
const OPEN_TEMP_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Delete temp copies left by earlier opens that nothing can still plausibly be reading
/// ([`OPEN_TEMP_TTL`]). Best-effort by nature: this is reclaiming garbage, and failing to reclaim it is
/// not worth failing the open the user actually asked for.
fn sweep_open_temp(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .and_then(|t| t.elapsed().map_err(std::io::Error::other))
            .map(|age| age > OPEN_TEMP_TTL)
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Materialize a stored blob into a temp file and return its path — what `attach open` needs first. A blob
/// is stored under its content address and therefore has no extension, which leaves the OS unable to pick a
/// default application; copying it to a temp file that carries the attachment's extension is done for that
/// reason alone.
///
/// The copy is named after the blob, so opening the same attachment twice rewrites one file instead of
/// growing a pile, and it lands in [`open_temp_dir`] — where the next open sweeps whatever is old enough to
/// have been abandoned ([`sweep_open_temp`]). Taking an attachment *out* is `export --out <dir>`; this
/// path only exists to let the OS choose an application, so what it writes is scratch, not a copy anyone
/// is meant to keep.
fn materialize_blob_temp(store: &Store, hash: &str, filename: Option<&str>) -> Result<String, CliError> {
    if !store.blobs().has(hash) {
        return Err(CliError {
            code: "not_found",
            message: format!("blob {hash} is not stored locally"),
            hint: None,
            exit: 1,
        });
    }
    let bytes = store.blobs().read(hash).map_err(CliError::from)?;
    let io_err = |e: std::io::Error| CliError {
        code: "io_error",
        message: format!("cannot write temp file: {e}"),
        hint: None,
        exit: 1,
    };
    let dir = open_temp_dir().map_err(io_err)?;
    sweep_open_temp(&dir);

    let short = &hash[..hash.len().min(16)];
    let name = match filename.and_then(|f| std::path::Path::new(f).extension()).and_then(|e| e.to_str()) {
        Some(ext) => format!("amenbo-{short}.{ext}"),
        None => format!("amenbo-{short}"),
    };
    let tmp = dir.join(name);
    std::fs::write(&tmp, &bytes).map_err(io_err)?;
    Ok(tmp.to_string_lossy().into_owned())
}

/// Open a path or URL in the OS's default application (macOS `open`, Windows `cmd /C start`, otherwise
/// `xdg-open`).
fn os_open(target: &str) -> Result<(), CliError> {
    let mkerr = |e: std::io::Error| CliError { code: "io_error", message: format!("could not open '{target}': {e}"), hint: None, exit: 1 };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("open");
        c.arg(target);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("cmd");
        c.args(["/C", "start", "", target]);
        c
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("xdg-open");
        c.arg(target);
        c
    };
    cmd.status().map_err(mkerr)?;
    Ok(())
}

fn task_complete(store: &mut Store, flags: &Flags, id: &str, completed: bool) -> Result<i32, CliError> {
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let before = store.task(tid).map_err(CliError::from)?;
    let old = before.map(|t| t.status).unwrap_or_default();
    // "Already in the target state" reads differently in each direction, because the two terminals are not
    // one (`AMB-D-397`): `done` has arrived only when the work was carried out, while `reopen` has arrived
    // whenever the task has not ended at all. A task decided against *has* ended, so reopen is the way back
    // from it — reading `completed` here would answer false and make the command silently do nothing.
    let already_there = if completed { old == TaskStatus::Done } else { !old.is_closed() };
    let action = if completed { "task.done" } else { "task.reopen" };
    if already_there {
        // Idempotent: already in the target state. Report success, as a no-op.
        let detail = store.task_detail(tid).map_err(CliError::from)?;
        write_envelope(flags, action, "task", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("(no change) {}", task_label(tid)));
        return Ok(0);
    }
    // Safety net (`AMB-D-366`): completing a reserved task is the moment not to miss — read the premises pinned on
    // after the reservation *before* the transition retires the in_progress clock they are measured against.
    let pc = premise_change_when(store, tid, completed && old == TaskStatus::InProgress);
    let t = store.set_task_completed(tid, completed, flags.facet()?).map_err(CliError::from)?;
    emit_event(store, flags, tid, activity_log::event::task_status_changed(old.as_str(), t.status.as_str()));
    // Ending the task — carried out or decided against — may have made dependents ready; emit the
    // unblock signal if so.
    if t.status.is_closed() {
        emit_unblocks(store, flags, tid);
    }
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let msg = if completed { "✓ Marked done" } else { "✓ Reopened" };
    let mut resource = serde_json::to_value(&detail).unwrap();
    attach_premise_change(&mut resource, &pc);
    write_envelope(flags, action, "task", resource, Some(vec!["completed".to_string(), "status".to_string()]), false, format!("{msg}: {}", task_label(t.id)));
    warn_premise_change(&pc);
    Ok(0)
}

/// `task status <id> <status>`: set the status explicitly (done keeps `completed` in step). This is the only
/// guard against two people starting the same task: `in_progress` reserves it, `todo` gives it back.
fn task_set_status(store: &mut Store, flags: &Flags, id: &str, status: &str) -> Result<i32, CliError> {
    let new_status = TaskStatus::parse(status).ok_or_else(|| CliError {
        code: "invalid_value",
        message: format!("status '{status}' is invalid (todo / in_progress / done / blocked / rejected)"),
        hint: None,
        exit: 2,
    })?;
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let current = store.task(tid).map_err(CliError::from)?.map(|t| t.status);
    // Setting the same status again is a no-op success — except for `in_progress → in_progress`, which must
    // not short-circuit: doing so would defeat the reservation's compare-and-set and let a second session
    // start a task someone is already on. Let that one fall through to `set_status` and come back as
    // `AlreadyReserved`.
    if current == Some(new_status) && new_status != TaskStatus::InProgress {
        // Idempotent: already at the target status. Report success, as a no-op.
        let detail = store.task_detail(tid).map_err(CliError::from)?;
        write_envelope(flags, "task.status", "task", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("(no change) {}", task_label(tid)));
        return Ok(0);
    }
    let old = current.unwrap_or_default();
    // Safety net (`AMB-D-366`): leaving in_progress to complete or block is the not-to-miss moment; read the
    // premises acquired since the reservation before the transition retires the clock. Handing it back to
    // todo needs no warn — the holder is stepping off anyway.
    let pc = premise_change_when(
        store,
        tid,
        old == TaskStatus::InProgress && (new_status.is_closed() || new_status == TaskStatus::Blocked),
    );
    let t = store.set_task_status(tid, new_status, flags.facet()?).map_err(CliError::from)?;
    emit_event(store, flags, tid, activity_log::event::task_status_changed(old.as_str(), new_status.as_str()));
    // Ending the task — carried out or decided against — may have made dependents ready; emit the
    // unblock signal if so.
    if t.status.is_closed() {
        emit_unblocks(store, flags, tid);
    }
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let mut resource = serde_json::to_value(&detail).unwrap();
    attach_premise_change(&mut resource, &pc);
    write_envelope(flags, "task.status", "task", resource, Some(vec!["status".to_string(), "completed".to_string()]), false, format!("✓ Set status to {}: {}", new_status.as_str(), task_label(t.id)));
    warn_premise_change(&pc);
    Ok(0)
}

/// `task block <id> [--reason]`: set the task to blocked, recording the reason as a comment.
fn task_block(store: &mut Store, flags: &Flags, id: &str, reason: Option<String>) -> Result<i32, CliError> {
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let old = store.task(tid).map_err(CliError::from)?.map(|t| t.status).unwrap_or_default();
    // Safety net (`AMB-D-366`): interrupting a reserved task — read the premises acquired since the reservation
    // before the transition retires the in_progress clock.
    let pc = premise_change_when(store, tid, old == TaskStatus::InProgress);
    let t = store.set_task_status(tid, TaskStatus::Blocked, flags.facet()?).map_err(CliError::from)?;
    if old != TaskStatus::Blocked {
        emit_event(store, flags, tid, activity_log::event::task_status_changed(old.as_str(), "blocked"));
    }
    // Keep the reason as a comment when there is one (under our own facet; the author argument is the trace
    // string for the audit log).
    if let Some(r) = reason.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        store.add_task_comment(tid, flags.facet()?, r).map_err(CliError::from)?;
    }
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let mut resource = serde_json::to_value(&detail).unwrap();
    attach_premise_change(&mut resource, &pc);
    write_envelope(flags, "task.block", "task", resource, Some(vec!["status".to_string()]), false, format!("✓ Set to blocked: {}", task_label(t.id)));
    warn_premise_change(&pc);
    Ok(0)
}

/// `task reject <id> --reason <why>`: end a task that will not be done (`AMB-D-397`). The sibling of
/// `task done` — both are terminals, and the difference is only whether the work was carried out.
///
/// The reason is **required**, and it is why the command exists at all: `task status <id> rejected` can
/// reach the same state, but nothing there asks for the reasoning, which is the part worth keeping when a
/// task is closed unfinished. It lands as a comment rather than a column of its own — the same sugar as
/// `task block --reason` and `decision reject --reason`, so free text keeps its one home on the timeline.
fn task_reject(store: &mut Store, flags: &Flags, id: &str, reason: String) -> Result<i32, CliError> {
    // An empty `--reason` passes clap (it is a value) but not the point of the flag: a rejection with no
    // reasoning is the `done`-borrowing this command was added to end.
    let reason = reason.trim().to_string();
    if reason.is_empty() {
        return Err(CliError {
            code: "invalid_value",
            message: "--reason is empty — say why the task will not be done".to_string(),
            hint: Some("A rejection is kept for its reasoning; pass the text, or `-` to read it from stdin.".to_string()),
            exit: 2,
        });
    }
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let old = store.task(tid).map_err(CliError::from)?.map(|t| t.status).unwrap_or_default();
    if old == TaskStatus::Rejected {
        // Idempotent: already decided against. Report success as a no-op, and do **not** pile the reason
        // on — a re-reject changes nothing, so it has nothing new to explain (`decision reject` likewise).
        let detail = store.task_detail(tid).map_err(CliError::from)?;
        write_envelope(flags, "task.reject", "task", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("(no change) {}", task_label(tid)));
        return Ok(0);
    }
    // Safety net (`AMB-D-366`): ending a reserved task is the moment not to miss — read the premises pinned
    // on after the reservation before the transition retires the in_progress clock they are measured against.
    let pc = premise_change_when(store, tid, old == TaskStatus::InProgress);
    let t = store.set_task_status(tid, TaskStatus::Rejected, flags.facet()?).map_err(CliError::from)?;
    emit_event(store, flags, tid, activity_log::event::task_status_changed(old.as_str(), t.status.as_str()));
    // A blocker decided against is a blocker no longer — dependents may have just become ready.
    emit_unblocks(store, flags, tid);
    store.add_task_comment(tid, flags.facet()?, &reason).map_err(CliError::from)?;
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let mut resource = serde_json::to_value(&detail).unwrap();
    attach_premise_change(&mut resource, &pc);
    write_envelope(flags, "task.reject", "task", resource, Some(vec!["status".to_string()]), false, format!("✓ Rejected: {}", task_label(t.id)));
    warn_premise_change(&pc);
    Ok(0)
}

/// `amenbo activity`: the unified timeline (system events plus comments), newest first.
#[allow(clippy::too_many_arguments)]
fn activity_cmd(
    store: &Store,
    flags: &Flags,
    task: Option<String>,
    project: Option<String>,
    since: Option<String>,
    kind: Option<String>,
    by: Option<String>,
    for_scope: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<i32, CliError> {
    let task_id = task.map(|t| resolve_task(store, &t)).transpose().map_err(CliError::from)?;
    let project_id = match project {
        Some(p) => Some(store.resolve_project_ref(&p).map_err(CliError::from)?),
        None => None,
    };
    // `--since`: an opaque cursor from a previous response means incremental mode (walking forward in time);
    // anything else is a date.
    let (since_date, since_cursor) = match since.as_deref() {
        None => (None, None),
        Some(s) if query::looks_like_activity_cursor(s) => {
            let cur = query::parse_activity_cursor(s).ok_or_else(|| CliError {
                code: "invalid_value",
                message: "--since cursor is malformed".to_string(),
                hint: Some("pass a cursor from a previous activity response, or a date (today / +3d / YYYY-MM-DD)".to_string()),
                exit: 2,
            })?;
            (None, Some(cur))
        }
        Some(_) => (parse_date_opt(&since)?, None),
    };
    let cursor_mode = since_cursor.is_some();
    // `--for`: the audience scope. `me` is this invocation's own facet (the one `--actor` declared);
    // human/ai name a facet outright.
    let for_facet = match for_scope.as_deref() {
        None => None,
        Some("me") => Some(flags.facet()?),
        Some(s) => Some(ActorKind::parse(s).ok_or_else(|| CliError {
            code: "invalid_value",
            message: format!("--for must be me / human / ai ('{s}' is invalid)"),
            hint: None,
            exit: 2,
        })?),
    };
    let kind = match kind.as_deref() {
        None => None,
        Some("system") => Some(amenbo_core::activity::Kind::System),
        Some("comment") => Some(amenbo_core::activity::Kind::Comment),
        Some(other) => return Err(CliError { code: "invalid_value", message: format!("--kind must be system / comment ('{other}' is invalid)"), hint: None, exit: 2 }),
    };
    let actor = match by.as_deref() {
        None => None,
        Some(s) => Some(ActorKind::parse(s).ok_or_else(|| CliError { code: "invalid_value", message: format!("--by must be human / ai ('{s}' is invalid)"), hint: None, exit: 2 })?),
    };
    let result = store.activity(query::ActivityParams {
        task_id, project_id, since: since_date, since_cursor, kind, actor, for_facet, limit, offset,
    }).map_err(CliError::from)?;
    if flags.json {
        print_json(&result);
    } else {
        // Incremental mode runs oldest first (it moves forward); history mode runs newest first.
        let order = if cursor_mode { "oldest first" } else { "newest first" };
        human(flags, format!("{} activity item(s) ({order})", result.count));
        for it in &result.items {
            let who = match it.author.kind {
                Some(amenbo_core::model::ActorKind::Ai) => format!("🤖{}", it.author.name),
                _ => it.author.name.clone(),
            };
            let body = if it.kind == "comment" {
                // Say when a comment was edited, in the same words `comment list` uses.
                let edited = it
                    .edited_at
                    .map(|t| format!(" · edited {}", t.to_rfc3339_z()))
                    .unwrap_or_default();
                format!("💬 {}{edited}", it.text.clone().unwrap_or_default())
            } else {
                let kind = it.event.as_ref().and_then(|e| e.get("kind")).and_then(|k| k.as_str()).unwrap_or("event");
                format!("⚙ {kind}")
            };
            // A target whose name could not be recovered — the row carrying it was compacted away, or lies
            // beyond the lookback budget — comes back as an empty string. To a human that would just be a
            // blank after the " — ", so say here that the target is gone (`--json` passes it through raw).
            let title = if it.target.title.is_empty() { "(deleted)" } else { &it.target.title };
            // Some rows still have the name while the target itself is gone: past rows about something later
            // deleted. Printing the name alone makes it indistinguishable from a live target, and `task show`
            // comes back empty — so if it cannot be followed, say so. `--json` does not paraphrase; it hands
            // over `target.live` raw, which is what a machine reads.
            let gone = if it.target.live || it.target.title.is_empty() { "" } else { " (deleted)" };
            human(flags, format!("  [{}] {} {} — {}{gone}", it.at.to_rfc3339_z(), who, body, title));
        }
        // Hand back an opaque cursor that can be passed straight to `--since <cursor>` next time — the seam
        // an incremental subscription is stitched from.
        if let Some(c) = &result.cursor {
            let more = if result.has_more { " (more)" } else { "" };
            human(flags, format!("  ↪ cursor: {c}{more}"));
        }
    }
    Ok(0)
}

// ───────────────────────── export ─────────────────────────

/// Where an export goes when the caller named no destination and the stream shape is not on offer: a fresh,
/// timestamped directory under the current one. The name carries the moment so a second export never lands
/// on the first — `export_bundle` refuses a destination that already exists, and quietly overwriting
/// someone's data is not amenbo's to do.
fn default_export_dir() -> String {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    format!("amenbo-export-{stamp}")
}

/// `export` — one shape only: everything, as JSON. Export exists to hand the data to whatever the user moves
/// to next, and an excerpt or a human-readable table does not serve that, so neither exists. With `--out` it
/// writes the export directory: `export.json` plus `attachments/` holding every attachment's bytes under the
/// target it hangs on — the complete migration artifact, since export is one-way and metadata alone would
/// lose the files. With no `--out` it streams the same JSON to stdout so the dump can be piped; a stream has
/// nowhere to put the bytes, so that shape carries records only and says so. `--json` prints a one-line
/// completion summary (payload always JSON regardless). The stream shape is the one place where the whole
/// device's content lands in the caller's terminal — so a closed reach (an AI) does not get it. It is not
/// refused: taking the data out is the user's, and their AI's, right (no lock-in), and refusing would read
/// as "you cannot export". Instead the destination is chosen for it and the export goes to a file, which
/// returns only a path and a count. What the AI can then do with that file is raw file access, which amenbo
/// does not stop.
fn export(store: &Store, flags: &Flags, out: Option<String>) -> Result<i32, CliError> {
    use amenbo_core::export;
    let out = match out {
        Some(path) => Some(path),
        None if store.reach().project().is_some() => Some(default_export_dir()),
        None => None,
    };
    match out {
        Some(path) => {
            let mut progress = progress_fn(flags);
            let report =
                export::export_bundle(std::path::Path::new(&path), &mut progress).map_err(|e| {
                    CliError {
                        code: "export_error",
                        message: e.to_string(),
                        hint: Some("Pick a destination that does not exist yet.".to_string()),
                        exit: 1,
                    }
                })?;
            human(flags, format!(
                "✓ Export written to {} ({} attachment(s))",
                report.path, report.attachments
            ));
            if report.missing > 0 {
                human(flags, format!(
                    "  ⚠ {} attachment(s) had no bytes left on disk — their export_path is null",
                    report.missing
                ));
            }
            if flags.json {
                print_json(&json!({
                    "ok": true, "action": "export", "noop": false, "out": report.path,
                    "bytes": report.bytes, "attachments": report.attachments, "missing": report.missing,
                }));
            }
        }
        None => {
            let stdout = std::io::stdout();
            let mut w = stdout.lock();
            let mut progress = progress_fn(flags);
            export::export_json(&mut w, &mut progress).map_err(|e| CliError {
                code: "export_error",
                message: e.to_string(),
                hint: None,
                exit: 1,
            })?;
            // stdout carries records only — never let that pass for the whole migration artifact. The
            // note goes to stderr: stdout is the dump itself and must stay pipeable.
            if !flags.json && !flags.quiet {
                eprintln!(
                    "note: attachment files are not in this stream — run `{} export --out <dir>` to take them with you",
                    Paths::command_name()
                );
            }
        }
    }
    Ok(0)
}

/// Backup: stream a verified snapshot of this device's store into one `.amenbo-backup` archive at `path`.
/// A destination is required (the archive is a deliberate, self-placed disaster-recovery file), so an
/// omitted `path` is refused with a hint.
fn run_backup(store: &Store, flags: &Flags, path: Option<String>) -> Result<i32, CliError> {
    use amenbo_core::archive;
    let Some(path) = path else {
        return Err(CliError {
            code: "missing_required_flag",
            message: "backup needs a destination path".to_string(),
            hint: Some(format!("Run `{} backup <path>.{}`.", Paths::command_name(), archive::ARCHIVE_EXT)),
            exit: 2,
        });
    };
    let dest = std::path::Path::new(&path);
    let Some(source) = archive::enumerate_store() else {
        return Err(CliError {
            code: "backup_error",
            message: "found no store to back up on this device".to_string(),
            hint: Some(format!("Create or bind a store first (`{} init`).", Paths::command_name())),
            exit: 1,
        });
    };
    let _ = store; // backup reads the on-disk layout, not the opened store; open is the exec guard.
    let mut progress = progress_fn(flags);
    let report = archive::backup_from(&source, dest, &mut progress).map_err(|e| CliError {
        code: "backup_error",
        message: e.to_string(),
        // A destination that is a directory is the one failure the generic hint misleads on: "pick one
        // that does not exist yet" reads as "delete that folder". Show the shape wanted instead. The
        // check holds after the fact too — a directory could only have been one before the run.
        hint: Some(if dest.is_dir() {
            format!(
                "Give a file path, not a folder — e.g. `{} backup {}/mystore.{}`.",
                Paths::command_name(),
                dest.display(),
                archive::ARCHIVE_EXT
            )
        } else {
            "Pick a destination that does not exist yet.".to_string()
        }),
        exit: 1,
    })?;
    if flags.json {
        print_json(&report);
    } else {
        human(flags, format!(
            "✓ Backup written to {} ({} bytes, {} attachment(s))",
            report.path, report.bytes, report.blobs
        ));
    }
    Ok(0)
}

/// What a phase is called on the progress line. `Verifying` has no name because it has nothing to report:
/// it is a single statement, so its one tick would be a line that is already over by the time it is read.
fn progress_verb(phase: amenbo_core::progress::Phase) -> Option<&'static str> {
    use amenbo_core::progress::Phase;
    match phase {
        Phase::Snapshotting => Some("backing up"),
        Phase::Copying => Some("restoring"),
        Phase::Exporting => Some("exporting"),
        Phase::Migrating => Some("migrating"),
        Phase::Blobs => Some("attachments"),
        Phase::Unpacking => Some("unpacking"),
        Phase::Verifying => None,
    }
}

/// How many lines a phase may spend at most, when its total is known — the budget the throttle divides.
const PROGRESS_LINES_PER_PHASE: u64 = 10;

/// How many units an unbounded phase (a streaming export, which counts rows it has not pre-counted) covers
/// between lines. A row is small, so a line every few hundred is a pulse, not a flood.
const PROGRESS_UNBOUNDED_STEP: u64 = 500;

/// Decides which ticks earn a line — see [`progress_fn`]. Holds the last line's `(phase, done)` so the
/// throttle can tell a phase's first tick from its hundredth.
#[derive(Default)]
struct ProgressLines {
    last: Option<(amenbo_core::progress::Phase, u64)>,
}

impl ProgressLines {
    /// The line this tick is worth, or `None` to stay silent.
    fn line(&mut self, p: &amenbo_core::progress::Progress) -> Option<String> {
        let verb = progress_verb(p.phase)?;
        let entering = self.last.is_none_or(|(phase, _)| phase != p.phase);
        let due = match (p.total, self.last) {
            // Bounded: spend the budget evenly, and always report the last unit — a phase that stops at
            // `[120/131]` reads as one that gave up there.
            (Some(total), _) => {
                let step = (total / PROGRESS_LINES_PER_PHASE).max(1);
                p.done.is_multiple_of(step) || p.done + 1 == total
            }
            (None, Some((_, last))) => p.done >= last + PROGRESS_UNBOUNDED_STEP,
            (None, None) => true,
        };
        if !entering && !due {
            return None;
        }
        self.last = Some((p.phase, p.done));
        Some(match p.total {
            Some(total) => format!("  [{}/{}] {verb}", p.done + 1, total),
            None => format!("  [{}] {verb}", p.done + 1),
        })
    }
}

/// A progress sink for the bulk ops: a line to stderr, so `--json` output on stdout stays clean and
/// `--quiet` silences it. Never cancels (the CLI has no interactive interrupt here). The phase names the
/// line, not the command that started it — a command is not one phase (a migration takes a pre-migration
/// backup and then walks the chain), so a verb fixed by the caller would show one counter apparently
/// restarting mid-run. A line per tick is not an option, and neither is silence: the phases that carry the
/// bytes — [`amenbo_core::progress::Phase::Blobs`] and [`amenbo_core::progress::Phase::Unpacking`] — tick
/// once per attachment, so a line each drowns the run, while dropping them leaves the longest stretch of a
/// multi-GB restore with nothing on the terminal at all. So the ticks are thinned ([`ProgressLines`]) to a
/// handful of lines per phase: enough to see it move, few enough to read.
fn progress_fn(flags: &Flags) -> impl FnMut(&amenbo_core::progress::Progress) -> std::ops::ControlFlow<()> + '_ {
    use amenbo_core::progress::Progress;
    let mut lines = ProgressLines::default();
    move |p: &Progress| {
        if !flags.json && !flags.quiet {
            if let Some(line) = lines.line(p) {
                eprintln!("{line}");
            }
        }
        std::ops::ControlFlow::Continue(())
    }
}

/// The CLI's half of the one execution site: carry this device's store forward before the command opens it,
/// whichever surface got here first. Everything about how is core's ([`amenbo_core::migrate::at_startup`] —
/// the lock the other surface waits on, the pre-migration backup, the rollback). What belongs here is only
/// what a terminal owes the human, on stderr so `--json` keeps stdout clean: before, what it will do and
/// what it will cost (`ensure_space` refuses a disk that cannot hold the backup with those same numbers in
/// the error, but a refusal is a bad first sight of them); after, where the backup went (the only way back —
/// there is no downgrade) and that older builds can no longer open this store. A store that is already
/// current is silent: it is the common case, and it has nothing to say.
fn migrate_at_startup(flags: &Flags) -> Result<(), CliError> {
    use amenbo_core::migrate::Pending;

    let mut announce = |p: &Pending| {
        if flags.quiet {
            return;
        }
        eprintln!(
            "Updating this device's store: format v{} → v{} ({} step(s)). Taking a pre-migration backup first (~{} MiB needed, ~{} MiB free).",
            p.from,
            p.to,
            p.steps,
            p.plan.required_bytes.div_ceil(1024 * 1024),
            p.plan.available_bytes.div_ceil(1024 * 1024),
        );
    };
    let mut progress = progress_fn(flags);
    let report = amenbo_core::migrate::at_startup(&mut announce, &mut progress).map_err(|e| CliError {
        code: "migrate_error",
        message: e.to_string(),
        hint: Some("The store was left as it was; nothing is half-migrated.".to_string()),
        exit: 1,
    })?;

    let Some(report) = report.filter(|r| r.migrated()) else { return Ok(()) };
    if !flags.quiet {
        eprintln!("✓ Store updated to format v{}.", report.run.to);
        if let Some(backup) = &report.backup {
            eprintln!("  The store as it was is kept at {} (the only way back — there is no downgrade).", backup.path);
        }
        // One rewind point, the newest. Say what went, so a deleted copy is never a silent one.
        if !report.superseded.is_empty() {
            eprintln!(
                "  Removed {} pre-migration backup(s) this one supersedes (nothing can go back past the newest).",
                report.superseded.len()
            );
        }
        eprintln!(
            "  Older amenbo builds can no longer open this store — update them (`{} update`, or reinstall from the latest installer; GUI and CLI ship together).",
            Paths::command_name()
        );
    }
    Ok(())
}

/// `hard-erase --json`: the erase report's own fields, flattened, plus the safety net it stands on — the
/// archive that can put the store back, and the earlier ones that archive superseded.
#[derive(serde::Serialize)]
struct HardEraseJson<'a> {
    #[serde(flatten)]
    erase: &'a amenbo_core::store::HardEraseReport,
    #[serde(flatten)]
    safety: &'a amenbo_core::archive::PreEraseReport,
}

/// `hard-erase`: physically erase content from the truth source (plaintext SQLite) — a comment in full (its
/// attachments' bytes with it), from either comment table, or one accepted decision's body. An ordinary delete leaves the freed
/// pages readable in the file, and editing a body in place does too, so this is the deliberate, gated exception
/// (see the `HardErase` command doc + `store::hard_erase`). Destructive: resolve targets, confirm (unless
/// `--yes`), take a safety backup, then erase + VACUUM. The safety backup still holds the erased content, so
/// we tell the operator to delete it after verifying. Exit 0 on success, 1 on an interactive abort.
fn hard_erase(store: &mut Store, flags: &Flags, sub: HardEraseCmd) -> Result<i32, CliError> {
    use amenbo_core::archive;
    use amenbo_core::store::HardEraseTarget;
    // Human-gated: AI cannot physically destroy store content (E guardrail).
    guard_ai_hard_erase(flags)?;

    // Resolve targets and describe exactly what will be erased (for the confirmation prompt).
    let (targets, what): (Vec<HardEraseTarget>, String) = match sub {
        HardEraseCmd::Comment { ids } => {
            let mut targets = Vec::with_capacity(ids.len());
            for id in &ids {
                targets
                    .push(HardEraseTarget::TaskComment { id: resolve_live_task_comment(store, id)? });
            }
            let what = format!(
                "physically erase {} task comment(s) — and any files attached to them — from the store",
                targets.len()
            );
            (targets, what)
        }
        HardEraseCmd::DecisionComment { ids } => {
            let mut targets = Vec::with_capacity(ids.len());
            for id in &ids {
                targets.push(HardEraseTarget::DecisionComment {
                    id: resolve_live_decision_comment(store, id)?,
                });
            }
            let what = format!(
                "physically erase {} decision comment(s) — and any files attached to them — from the store",
                targets.len()
            );
            (targets, what)
        }
        HardEraseCmd::Decision { id, body, body_file } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let new_body = read_body_input(body, body_file)?;
            let what = format!("redact the body of decision {} in the store", decision_label(did));
            (vec![HardEraseTarget::DecisionBody { id: did, new_body }], what)
        }
    };

    // Human gate (machine callers must pass --yes).
    if !confirm(flags, &format!("{what} — this is irreversible"))? {
        return Ok(1);
    }

    // Safety net: a verified backup archive before the destructive step, so a botched erase is recoverable
    // — through the one restore path there is (`amenbo restore`). It is written to an auto-named path next
    // to the store and carries the attachment bytes, which an erase destroys too. It still holds the erased
    // content, so delete it once the erase is verified.
    let Some(source) = archive::enumerate_store() else {
        return Err(CliError {
            code: "backup_error",
            message: "found no store to back up before erasing".to_string(),
            hint: Some(format!("Create or bind a store first (`{} init`).", Paths::command_name())),
            exit: 1,
        });
    };
    let safety_stamp = time::Timestamp::now().0.format("%Y%m%dT%H%M%SZ").to_string();
    let mut progress = progress_fn(flags);
    let safety = archive::pre_erase_backup(&source, &store.paths.base_dir, &safety_stamp, &mut progress)
        .map_err(|e| CliError {
            code: "backup_error", message: e.to_string(), hint: None, exit: 1,
        })?;
    human(flags, format!(
        "↩ Safety backup written to {} ({} attachment(s)) — it still contains the erased content, so delete it once you have verified the erase (`{} restore` puts it back)",
        safety.backup.path, safety.backup.blobs, Paths::command_name()
    ));
    // One rewind point per kind, the newest. Say what went, so a deleted copy is never a silent one.
    if !safety.superseded.is_empty() {
        human(flags, format!(
            "  Removed {} earlier safety backup(s) this one supersedes (each still held the content an earlier erase destroyed).",
            safety.superseded.len()
        ));
    }

    let report = store.hard_erase(&targets).map_err(CliError::from)?;
    if flags.json {
        // The erase report, plus the rewind point it stands on: a machine caller learns the archive it can
        // put the store back from, and the older ones that archive swept (never a silent delete).
        print_json(&HardEraseJson { erase: &report, safety: &safety });
    } else {
        human(flags, format!(
            "✓ Hard-erased {} task comment(s) + {} decision comment(s) + {} decision body(ies); {} row(s) removed and VACUUMed",
            report.task_comments_erased.len(), report.decision_comments_erased.len(),
            report.decisions_redacted.len(), report.rows_removed
        ));
        if report.blobs_reclaimed > 0 {
            human(flags, format!(
                "  {} attached file(s) reclaimed ({} bytes) — nothing else pointed at those bytes",
                report.blobs_reclaimed, report.bytes_reclaimed
            ));
        }
        human(flags, "  Verify the content is gone, then delete the safety backup next to the store.");
    }
    Ok(0)
}

/// A body-carrying argument, where the value `-` means "the body arrives on stdin" instead.
///
/// Bodies here are Markdown, and in practice they are thick with code spans — which a shell eats out of
/// a double-quoted `--text` argument by command substitution, silently, taking the word with it. `-` lets
/// the text reach amenbo without passing through word expansion at all (a heredoc piped in).
///
/// `-` is the spelling because it is the only one that works on every body option: omitting the flag is
/// already spoken for and means something different per command ("empty" on an add, "leave it alone" on
/// an edit), so `hard-erase decision`'s implicit-stdin shape ([`read_body_input`]) does not generalize.
/// A terminal on stdin is refused rather than waited on, so a `-` typed by hand never looks like a hang.
fn body_arg(v: String) -> Result<String, CliError> {
    if v != "-" {
        return Ok(v);
    }
    if std::io::stdin().is_terminal() {
        return Err(CliError {
            code: "invalid_value",
            message: "`-` says the body comes in on stdin, but stdin is a terminal".to_string(),
            hint: Some("Pipe the body in (`… | amenbo … -`), or pass the text itself.".to_string()),
            exit: 2,
        });
    }
    use std::io::Read;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| CliError {
        code: "io_error",
        message: format!("Cannot read the body from stdin: {e}"),
        hint: None,
        exit: 1,
    })?;
    Ok(s)
}

/// [`body_arg`] for an optional body — an absent flag stays absent (it is not a `-`).
fn body_arg_opt(v: Option<String>) -> Result<Option<String>, CliError> {
    v.map(body_arg).transpose()
}

/// The replacement body for `hard-erase decision`: `--body`, else `--body-file`, else stdin (for a
/// piped body). Refuses an interactive terminal with none of them given — a redaction must be
/// explicit about the new text, never an empty accident.
fn read_body_input(body: Option<String>, body_file: Option<String>) -> Result<String, CliError> {
    if let Some(b) = body {
        return Ok(b);
    }
    if let Some(f) = body_file {
        return std::fs::read_to_string(&f).map_err(|e| CliError {
            code: "io_error",
            message: format!("Cannot read --body-file {f}: {e}"),
            hint: None,
            exit: 1,
        });
    }
    if std::io::stdin().is_terminal() {
        return Err(CliError {
            code: "invalid_value",
            message: "no replacement body given".to_string(),
            hint: Some("Pass --body \"…\", --body-file <path>, or pipe the body on stdin.".to_string()),
            exit: 2,
        });
    }
    use std::io::Read;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| CliError {
        code: "io_error",
        message: format!("Cannot read body from stdin: {e}"),
        hint: None,
        exit: 1,
    })?;
    Ok(s)
}

/// Restore: destructively replace this device's store with the one the `.amenbo-backup` archive at `path`
/// carries, via core [`amenbo_core::archive::restore_into`] — all-or-nothing stage-and-swap, the archive's
/// store carried up the version chain in staging when it was taken by an older build, and the replaced truth
/// source set aside as `store.pre-restore-<stamp>.sqlite`. Destructive — confirms unless `--yes`.
///
/// Takes no [`Store`]: it writes the on-disk layout rather than the opened store, and it runs **ahead of**
/// the open so it still works on the store the open refuses — see the dispatch in [`run`].
fn run_restore(
    flags: &Flags,
    path: Option<String>,
) -> Result<i32, CliError> {
    use amenbo_core::archive;
    let Some(path) = path else {
        return Err(CliError {
            code: "missing_required_flag",
            message: "restore needs the archive path".to_string(),
            hint: Some(format!("Run `{} restore <path>.{}`.", Paths::command_name(), archive::ARCHIVE_EXT)),
            exit: 2,
        });
    };
    let archive = std::path::Path::new(&path);
    // Ask the release-stamp gate up front (`AMB-D-378`). Core refuses this restore anyway — the gate lives
    // with the code that migrates — but asking here keeps its message intact: it names the three ways
    // through, and reaching it through the wrapper below would bury them under the too-new-archive hint.
    amenbo_core::build_stamp::ensure_may_migrate().map_err(CliError::from)?;
    // Read the manifest before the destructive prompt: it is cheap (no extraction) and it is where an
    // archive this build cannot read is refused, so the user is not asked to consent to a restore that
    // was never going to run.
    let manifest = archive::read_manifest(archive).map_err(|e| CliError {
        code: "restore_error",
        message: e.to_string(),
        hint: Some(format!("Pass a .amenbo-backup archive produced by `{} backup`.", Paths::command_name())),
        exit: 1,
    })?;
    if !confirm(
        flags,
        &format!(
            "destructively replace this device's store from {} (taken {}; the current truth source is set aside as a timestamped backup)",
            path, manifest.created_at
        ),
    )? {
        return Ok(1);
    }
    let stamp = time::Timestamp::now().0.format("%Y%m%dT%H%M%SZ").to_string();
    let mut progress = progress_fn(flags);
    let report = archive::restore_into(archive, &stamp, &archive::restore_dest(), &mut progress)
        .map_err(|e| CliError {
            code: "restore_error",
            message: e.to_string(),
            hint: Some(format!("On a too-new archive, update amenbo first (`{} update`).", Paths::command_name())),
            exit: 1,
        })?;
    if flags.json {
        print_json(&report);
    } else {
        human(flags, format!("✓ Restore complete ({} attachment(s) written)", report.blobs));
        if let Some(prev) = &report.previous_saved_to {
            human(flags, format!("  Previous truth source set aside at {prev}"));
        }
        // The new aside is this store's rewind point, so the older ones were not kept.
        if !report.superseded.is_empty() {
            human(
                flags,
                format!(
                    "  Removed {} earlier set-aside store(s) this one supersedes",
                    report.superseded.len()
                ),
            );
        }
        // An archive taken by an older build is carried up the version chain on the way in. Say so: the
        // store the user gets back is not, byte for byte, the store they backed up.
        let m = &report.migration;
        if m.migrated() {
            human(
                flags,
                format!(
                    "  Archive brought forward from format v{} to v{} ({})",
                    m.from,
                    m.to,
                    m.applied.join(", ")
                ),
            );
        }
    }
    Ok(0)
}

// ───────────────────────── human renderers ─────────────────────────

fn render_status(s: &query::StatusResult) {
    println!("== {} ==", time::date_to_string(s.today_date));
    println!("overdue {} / today {} / in progress {} / within 7 days {} / no due date {} / completed today {}",
        s.counts.overdue, s.counts.due_today, s.counts.in_progress, s.counts.upcoming_7d, s.counts.no_due, s.counts.completed_today);
    if !s.overdue.is_empty() {
        println!("[Overdue]");
        for o in &s.overdue {
            println!("  {}  {} ({} day(s) overdue)", task_label(o.task.id), o.task.title, o.days_overdue);
        }
    }
    if let Some(dt) = &s.due_today {
        if !dt.is_empty() {
            println!("[Due today]");
            for t in dt {
                println!("  {}  {}", task_label(t.id), t.title);
            }
        }
    }
    // Unlike Overdue and Due today, whose counts appear on the summary line, suggestions are counted nowhere.
    // So always print the section, and say `(none)` when there is nothing to suggest — printing nothing would
    // be indistinguishable from the feature not existing.
    println!("[Next suggestions]");
    if s.next_suggested.is_empty() {
        println!("  (none)");
    } else {
        for n in &s.next_suggested {
            println!("  {}  {} — {}", task_label(n.id), n.title, n.reason);
        }
    }
}

fn render_discover(d: &query::DiscoverResult) {
    println!("== {} ==", time::date_to_string(d.today_date));
    if d.today.is_empty() {
        println!("No tasks for today.");
    } else {
        println!("[Today]");
        for t in &d.today {
            let check = if t.completed { "x" } else { " " };
            println!("  [{check}] {}  {}", task_label(t.id), t.title);
        }
    }
    for h in &d.hints {
        println!("- {h}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::collections::HashSet;

    /// Thin the progress ticks: feed in every tick, get back only the ones that earned a line.
    fn thinned(ticks: &[amenbo_core::progress::Progress]) -> Vec<String> {
        let mut lines = ProgressLines::default();
        ticks.iter().filter_map(|p| lines.line(p)).collect()
    }

    fn tick(phase: amenbo_core::progress::Phase, done: u64, total: Option<u64>) -> amenbo_core::progress::Progress {
        amenbo_core::progress::Progress { phase, done, total }
    }

    /// The premise-change fold (`AMB-D-366`): the `premise_change` key appears in a write's JSON envelope
    /// exactly when a premise shifted, so a `--json` caller reads it structurally and an unchanged
    /// reservation carries no noise key.
    #[test]
    fn attach_premise_change_only_adds_the_key_when_something_changed() {
        use amenbo_core::view::{PremiseChange, TaskRef};

        // Empty change: the key is absent.
        let mut v = json!({ "id": 1 });
        attach_premise_change(&mut v, &no_premise_change());
        assert!(v.get("premise_change").is_none());

        // A pinned-on blocker: the key carries it.
        let pc = PremiseChange {
            added_blockers: vec![TaskRef { id: 7, name: "後付け".to_string() }],
            ..no_premise_change()
        };
        attach_premise_change(&mut v, &pc);
        assert_eq!(v["premise_change"]["added_blockers"][0]["id"], 7);
    }

    /// The update config re-check (`AMB-D-359`): before a build is replaced, the new manifest's `required`
    /// settings are re-judged the way `enable` judges them (`AMB-D-351`/`AMB-D-356`). An enabled plugin
    /// whose new schema declares a `required` field this machine has no value for holds the update back;
    /// everything with nothing to break lets it through.
    #[test]
    fn an_update_that_would_leave_a_required_setting_unset_is_held_back_only_for_an_enabled_plugin() {
        use amenbo_core::plugin_manifest::{ConfigField, Manifest};
        use amenbo_core::plugin_trust::{self, Gate};

        fn manifest(config: serde_json::Value) -> Manifest {
            serde_json::from_value(serde_json::json!({
                "name": "watcher", "desc": "t", "author": "amenbo",
                "repo": "ShiroDoromoto/amenbo", "os": ["macos", "linux", "windows"],
                "category": "workflow", "url": "https://example.invalid/x.tar.gz",
                "checksum": "sha256:dead", "scope": "machine", "config": config,
            }))
            .unwrap()
        }
        fn field(key: &str, required: bool) -> ConfigField {
            ConfigField { key: key.to_string(), label: key.to_string(), secret: false, required }
        }

        let dir = amenbo_scratch::scratch("update-config-recheck");
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open_at(amenbo_core::config::Paths::at(dir)).unwrap();

        // Plant a machine-scoped install carrying one required setting, and give that setting a value.
        let installed = manifest(serde_json::json!([{ "key": "token", "label": "T", "required": true }]));
        let home = store.paths.plugin_dir("watcher");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            amenbo_core::plugin_installed::manifest_path(&store.paths, "watcher"),
            serde_json::to_vec(&installed).unwrap(),
        )
        .unwrap();
        std::fs::write(
            amenbo_core::plugin_installed::program_path(&store.paths, "watcher"),
            b"#!/bin/sh\n",
        )
        .unwrap();
        let token = field("token", true);
        amenbo_core::plugin_config::set(
            &mut store,
            &token,
            "watcher",
            "abc",
            amenbo_core::plugin_config::Scope::MachineDefault,
        )
        .unwrap();

        // A build whose new schema keeps the same required set is satisfied — nothing to fill.
        let same = manifest(serde_json::json!([{ "key": "token", "label": "T", "required": true }]));
        // A build whose new schema adds a *new* required field this machine has no value for.
        let grew = manifest(serde_json::json!([
            { "key": "token", "label": "T", "required": true },
            { "key": "channel", "label": "C", "required": true },
        ]));

        // Disabled: nothing fires, so nothing is held back — even the build that grew a required field.
        assert!(refuse_update_leaving_required_unset(&store, &grew).is_ok());

        // Enable it, then the two builds diverge: the satisfied one passes, the one that grew a required
        // field is held back and names it.
        plugin_trust::enable(&mut store, "watcher", Gate::Machine, &installed.config, |_| true).unwrap();
        assert!(refuse_update_leaving_required_unset(&store, &same).is_ok());
        let held = refuse_update_leaving_required_unset(&store, &grew).unwrap_err();
        assert_eq!(held.code(), "invalid_value");
        assert!(format!("{held:?}").contains("channel"), "the field to set is named: {held:?}");

        // A name that is not installed is enabled nowhere, so its update is never held back here.
        let absent = manifest(serde_json::json!([{ "key": "x", "label": "X", "required": true }]));
        let mut absent = absent;
        absent.name = "ghost".to_string();
        assert!(refuse_update_leaving_required_unset(&store, &absent).is_ok());
    }

    /// `attach open` hands its temp copy to another application and returns, so it can never delete what
    /// it wrote — only a later open can. The sweep is that later open: it reclaims what has aged past
    /// [`OPEN_TEMP_TTL`] and leaves anything recent, since a fresh copy may be the very file an
    /// application still has in front of the user.
    #[test]
    fn a_later_open_reclaims_the_temp_copies_the_earlier_ones_could_not() {
        let dir = amenbo_scratch::scratch("sweep-test");

        let stale = dir.join("amenbo-oldhash.pdf");
        let fresh = dir.join("amenbo-newhash.png");
        std::fs::write(&stale, b"opened days ago").unwrap();
        std::fs::write(&fresh, b"still on screen").unwrap();
        // Age the one file past the window; `elapsed()` reads mtime, so this is the whole of "old".
        let aged = std::time::SystemTime::now() - (OPEN_TEMP_TTL + std::time::Duration::from_secs(60));
        std::fs::File::options().write(true).open(&stale).unwrap().set_modified(aged).unwrap();

        sweep_open_temp(&dir);

        assert!(!stale.exists(), "a copy nothing can still be reading is reclaimed");
        assert!(fresh.exists(), "a fresh copy is left alone — an application may still be reading it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A per-attachment tick is not a unit of output: unpacking 131 files must not print 131 lines, but a
    /// handful — first and last among them. Lines that scroll past take the shape of the run with them.
    #[test]
    fn a_per_attachment_phase_is_thinned_to_a_handful_of_lines() {
        use amenbo_core::progress::Phase;
        let ticks: Vec<_> = (0..131).map(|i| tick(Phase::Unpacking, i, Some(131))).collect();
        let lines = thinned(&ticks);
        assert!(
            lines.len() <= PROGRESS_LINES_PER_PHASE as usize + 2,
            "131 ticks turned into {} lines: {lines:?}",
            lines.len()
        );
        assert_eq!(lines.first().unwrap(), "  [1/131] unpacking", "it must speak on the first tick");
        assert_eq!(lines.last().unwrap(), "  [131/131] unpacking", "it must report through to the last unit");
    }

    /// A phase that does not know its total (a streaming export never pre-counts its rows) still speaks: it
    /// pulses at a fixed stride and prints the count alone, never inventing a denominator like `[5/0]`.
    #[test]
    fn an_unbounded_phase_pulses_by_count_without_inventing_a_total() {
        use amenbo_core::progress::Phase;
        let ticks: Vec<_> = (0..2000).step_by(256).map(|i| tick(Phase::Exporting, i, None)).collect();
        let lines = thinned(&ticks);
        assert_eq!(lines.first().unwrap(), "  [1] exporting");
        assert!(lines.len() >= 2 && lines.len() < ticks.len(), "it pulses, but not on every tick: {lines:?}");
        assert!(lines.iter().all(|l| !l.contains('/')), "no denominator when the total is unknown: {lines:?}");
    }

    /// Entering a phase always earns a line, mid-stride or not — what is being done next is needed before how
    /// far along it is.
    #[test]
    fn entering_a_phase_always_earns_a_line() {
        use amenbo_core::progress::Phase;
        let lines = thinned(&[
            tick(Phase::Unpacking, 0, Some(3)),
            tick(Phase::Unpacking, 1, Some(3)),
            tick(Phase::Copying, 0, Some(1)),
            tick(Phase::Blobs, 0, Some(2)),
        ]);
        assert!(lines.contains(&"  [1/1] restoring".to_string()), "{lines:?}");
        assert!(lines.contains(&"  [1/2] attachments".to_string()), "{lines:?}");
    }

    /// Collect every leaf sub-command clap knows ("project add" and the like), skipping help and hidden
    /// commands. A hidden command (`githook-pre-commit`, the hook's own entry point) is not part of the
    /// surface an AI drives — it stands in for a hook line, and the AI-facing face is `lint` — so it is not
    /// expected in `agent --json`, the same as `help`.
    fn collect_leaves(cmd: &clap::Command, path: &str, out: &mut Vec<String>) {
        let subs: Vec<&clap::Command> = cmd
            .get_subcommands()
            .filter(|s| s.get_name() != "help" && !s.is_hide_set())
            .collect();
        if subs.is_empty() {
            out.push(path.to_string());
            return;
        }
        for s in subs {
            let child = format!("{path} {}", s.get_name());
            collect_leaves(s, child.trim(), out);
        }
    }

    /// Every sub-command must be registered in `agent --json`, or an AI never learns it exists. amenbo is a
    /// single local store, so there are no sharing, sync or key commands to account for.
    #[test]
    fn every_clap_leaf_is_in_agent() {
        let root = Cli::command();
        let mut leaves = Vec::new();
        for s in root.get_subcommands().filter(|s| s.get_name() != "help" && !s.is_hide_set()) {
            collect_leaves(s, s.get_name(), &mut leaves);
        }
        let agent: HashSet<String> = agent::command_names().into_iter().collect();
        let missing: Vec<&String> = leaves.iter().filter(|n| !agent.contains(*n)).collect();
        assert!(missing.is_empty(), "not registered in agent --json: {missing:?}");
    }

    /// Every command a capability points at must exist — this catches a typo'd or dropped command name — and
    /// no capability may list no commands at all.
    #[test]
    fn capabilities_reference_real_commands() {
        let spec = agent::build();
        let known: HashSet<String> = agent::command_names().into_iter().collect();
        let caps = spec["capabilities"].as_array().expect("capabilities is an array");
        assert!(!caps.is_empty(), "capabilities should not be empty");
        for c in caps {
            assert!(c["capability"].as_str().is_some_and(|s| !s.is_empty()), "capability text missing: {c}");
            let cmds = c["commands"].as_array().expect("capability.commands is an array");
            assert!(!cmds.is_empty(), "capability has no commands: {c}");
            for name in cmds {
                let n = name.as_str().unwrap_or("");
                assert!(known.contains(n), "capability references unknown command {n:?}: {c}");
            }
        }
    }

    /// Facet resolution: `--actor` is the only input, an unspecified facet stays unspecified, and a command
    /// that uses one gets `facet_required` rather than a default.
    #[test]
    fn decide_facet_reads_the_flag_alone_and_never_defaults() {
        // An explicit value is honoured whether or not the command uses a facet.
        assert_eq!(decide_facet(Some("ai"), true).ok(), Some(Some(ActorKind::Ai)));
        assert_eq!(decide_facet(Some("human"), true).ok(), Some(Some(ActorKind::Human)));
        assert_eq!(decide_facet(Some("ai"), false).ok(), Some(Some(ActorKind::Ai)));
        // Unspecified where the facet is used: facet_required, never a silent human.
        assert_eq!(decide_facet(None, true).err().map(|e| e.code), Some("facet_required"));
        assert_eq!(decide_facet(Some(""), true).err().map(|e| e.code), Some("facet_required"));
        // Unspecified where it is not used: it stays unspecified — there is no default to fall into.
        assert_eq!(decide_facet(None, false).ok(), Some(None));
        assert_eq!(decide_facet(Some(""), false).ok(), Some(None));
        // An invalid value is invalid_value either way.
        assert_eq!(decide_facet(Some("robot"), false).err().map(|e| e.code), Some("invalid_value"));
    }

    /// Amenbo's flags are told from the plugin's by their spelling, in the order they were written, and
    /// `--actor` keeps the value that follows it — the corrected line has to be one a person can paste,
    /// which means it carries the value too.
    #[test]
    fn amenbo_flags_are_picked_out_of_what_was_handed_to_the_plugin() {
        let split = |args: &[&str]| {
            let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            let (own, rest) = split_own_flags(&owned);
            (own.join(" "), rest.join(" "))
        };
        assert_eq!(split(&["start", "1", "--actor", "ai"]), ("--actor ai".into(), "start 1".into()));
        assert_eq!(split(&["start", "--actor=ai"]), ("--actor=ai".into(), "start".into()));
        assert_eq!(
            split(&["start", "--json", "--actor", "ai", "--yes"]),
            ("--json --actor ai --yes".into(), "start".into())
        );
        // A flag amenbo does not answer for is the plugin's, whatever it looks like.
        assert_eq!(split(&["start", "--branch", "main"]), (String::new(), "start --branch main".into()));
        // A bare `--actor` at the end takes no value with it, and the next flag is not eaten as one.
        assert_eq!(split(&["--actor", "--json"]), ("--actor --json".into(), String::new()));
    }

    /// The hint fires on one pairing: a facet that was typed, went to the plugin, and so never arrived.
    /// A plugin is entitled to a `--json` of its own, so a shared spelling alone is not a mistake — and
    /// no other failure is explained by where the flag was written.
    #[test]
    fn the_misplaced_flag_hint_fires_only_where_it_is_the_explanation() {
        let run = |args: &[&str]| {
            Some(Command::Plugin {
                sub: PluginCmd::Run {
                    name: "worktree".to_string(),
                    args: args.iter().map(|s| s.to_string()).collect(),
                },
            })
        };
        let hint = |cmd: &Option<Command>, err: CliError| {
            misplaced_flags_hint(cmd, err).hint.unwrap_or_default()
        };

        // The facet went to the plugin: the hint says so, and hands back the line to paste.
        let told = hint(&run(&["start", "1", "--actor", "ai"]), CliError::facet_required());
        assert!(told.contains("went to the plugin"), "{told}");
        assert!(told.contains("plugin run worktree start 1"), "the corrected line is complete: {told}");
        assert!(told.contains("--actor ai plugin run"), "the flag is hoisted in front: {told}");

        // No facet among them: the failure is that none was declared, not where one was written.
        let plain = hint(&run(&["start", "--json"]), CliError::facet_required());
        assert!(!plain.contains("went to the plugin"), "{plain}");

        // Another command's facet_required is not about a plugin's argv at all.
        let elsewhere = hint(&Some(Command::Version), CliError::facet_required());
        assert!(!elsewhere.contains("went to the plugin"), "{elsewhere}");

        // And another failure of the same command is left as it was — this explains one thing.
        let other = misplaced_flags_hint(
            &run(&["start", "--actor", "ai"]),
            CliError { code: "not_found", message: "x".into(), hint: None, exit: 1 },
        );
        assert!(other.hint.is_none(), "only the failure it explains is touched");
    }

    /// The facet is used by the writes that stamp it **and** by the reads that draw an AI's reach from it;
    /// false is the narrow set that touches neither. This line is what `--actor` is demanded by, so a read
    /// that surfaces store content landing on the false side would be an AI reading past its binding.
    #[test]
    fn uses_facet_covers_stamping_writes_and_reach_drawing_reads() {
        // Facts about this build, this machine's settings, and text handed in — no facet either way.
        assert!(!uses_facet(&Some(Command::Agent { command: None, full: false })));
        assert!(!uses_facet(&Some(Command::Version)));
        assert!(!uses_facet(&Some(Command::Whoami)));
        assert!(!uses_facet(&Some(Command::Bind { project: None, dir: None, force: false })));
        assert!(!uses_facet(&Some(Command::Lint { paths: Vec::new(), stdin: false })));
        assert!(!uses_facet(&Some(Command::GithookPreCommit)));
        assert!(!uses_facet(&Some(Command::Plugin { sub: PluginCmd::Validate { path: "p.yaml".to_string() } })));
        // Reads that surface store content draw the reach, so they use the facet too.
        assert!(uses_facet(&None)); // discover: this project's work
        assert!(uses_facet(&Some(Command::Status { scope: "today".to_string() })));
        assert!(uses_facet(&Some(Command::Task { sub: TaskCmd::List { project: None, filter: None, sort: "order".to_string(), limit: None, offset: None } })));
        assert!(uses_facet(&Some(Command::Task { sub: TaskCmd::Show { id: "x".to_string() } })));
        assert!(uses_facet(&Some(Command::Comment { sub: CommentCmd::List { task: "x".to_string(), limit: None, offset: None } })));
        assert!(uses_facet(&Some(Command::Doctor { fix: false })));
        // Writes stamp it.
        assert!(uses_facet(&Some(Command::Task { sub: TaskCmd::Status { id: "x".to_string(), status: "in_progress".to_string() } })));
        assert!(uses_facet(&Some(Command::Task { sub: TaskCmd::Done { id: "x".to_string() } })));
        assert!(uses_facet(&Some(Command::Comment { sub: CommentCmd::Add { task: "x".to_string(), text: "t".to_string() } })));
        assert!(uses_facet(&Some(Command::Doctor { fix: true })));
        // The rest of the plugin group moves per-project rows, so only `validate` is outside.
        assert!(uses_facet(&Some(Command::Plugin { sub: PluginCmd::List })));
        assert!(uses_facet(&Some(Command::Plugin { sub: PluginCmd::Enable { name: "p".to_string() } })));
    }

    /// The only faces allowed through without a binding are the ones that never read the store. Loosen this
    /// and a directory with no pointer falls into `Store::open()` quietly creating a new store — precisely
    /// what the exec guard exists to prevent.
    #[test]
    fn only_the_faces_that_never_open_the_store_run_without_a_pointer() {
        // The commands that place or remove the marker, and the ones that answer from facts about the build.
        assert!(!requires_pointer(&Some(Command::Init { name: None, language: None, force: false })));
        assert!(!requires_pointer(&Some(Command::Bind { project: None, dir: None, force: false })));
        assert!(!requires_pointer(&Some(Command::Version)));
        assert!(!requires_pointer(&Some(Command::Update { print: true, apply: false, rollback: false })));
        // Everything else opens the store and therefore needs a pointer. `agent` is the AI's entry point, so
        // it gets no exemption.
        assert!(requires_pointer(&None)); // discover
        assert!(requires_pointer(&Some(Command::Agent { command: None, full: false })));
        assert!(requires_pointer(&Some(Command::Whoami)));
        assert!(requires_pointer(&Some(Command::Status { scope: "today".to_string() })));
        assert!(requires_pointer(&Some(Command::Doctor { fix: false })));
    }

    /// The dispatch-cursor line (`AMB-D-380`) distinguishes the three shapes a store can be in, because a
    /// reader chasing a missing hook has to tell "the dispatcher never ran" from "it ran and delivered
    /// nothing you can see". The face is reported as who moved it, never as whose turn it is.
    #[test]
    fn the_dispatch_cursor_line_tells_never_delivered_from_delivered_by_someone() {
        use amenbo_core::plugin_drive::Face;

        let never = dispatch_cursor_line(0, None);
        assert!(never.contains("nothing has been delivered"), "{never}");

        let by_cli = dispatch_cursor_line(42, Some(Face::Cli));
        assert!(by_cli.contains("42") && by_cli.contains("last advanced by cli"), "{by_cli}");
        assert!(!by_cli.contains("next"), "the stamp is a record, not a turn order: {by_cli}");

        // Stood at an id with no face beside it: an older build delivered this span. Still a delivered
        // store, so it must not read as one nothing has run on.
        let unstamped = dispatch_cursor_line(42, None);
        assert!(unstamped.contains("42"), "{unstamped}");
        assert!(!unstamped.contains("nothing has been delivered"), "{unstamped}");
    }
}
