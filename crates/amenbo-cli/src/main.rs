//! Entry point for the amenbo CLI: parse with clap, delegate to the core (amenbo-core) operations.
//! The CLI is a thin skin. Output comes in two layers: human-readable, and `--json`.
//!
//! What stays here is the way in: startup, the guards that judge where the command was typed and who
//! is typing it, and `run()`'s dispatch. What each command then does lives in [`cmd`], one module per
//! unit the dispatch already names.

// `commands()` in `agent.rs` is one huge `json!([...])` literal, and every entry added pushes it further
// past the default recursion limit (128). Raise the limit so the spec can stay a single array.
#![recursion_limit = "256"]

mod agent;
mod cli;
mod cmd;
mod doctor_text;
mod mcp;
mod output;
mod validate_text;

use std::sync::OnceLock;

use clap::{CommandFactory, FromArgMatches};
use serde_json::json;

use amenbo_core::agent as core_agent;
use amenbo_core::config::Paths;
use amenbo_core::model::ActorKind;
use amenbo_core::reach::Reach;
use amenbo_core::worktree;
use amenbo_core::{query, Store};

use cli::*;
use cmd::activity::activity_cmd;
use cmd::attach::attach;
use cmd::binding::{bind_cmd, init_cmd, sync_guide, unbind_cmd, whoami};
use cmd::comment::comment;
use cmd::config::config;
use cmd::data::{export, migrate_at_startup, run_backup, run_restore, sync_cmd};
use cmd::decision::decision;
use cmd::dimension::dimension;
use cmd::hard_erase::hard_erase;
use cmd::labels::project_label;
use cmd::lint::lint_cmd;
use cmd::outbox::{resume_dispatch, with_dispatch};
use cmd::place::{binding_project, bound_project, location_header, named_project_flag};
use cmd::plugin::{PluginsAtEntry, plugin_cmd, plugin_validate_cmd, plugins_for_agent};
use cmd::project::project;
use cmd::setup::{agent_hook_answer_cmd, agent_hook_setup, agent_hook_snippet_cmd, hooks_cmd, lint_hook_setup};
use cmd::status::{render_discover, render_status};
use cmd::task::task;
use cmd::tick::tick_cmd;
use cmd::update::{self_rollback_cmd, self_update_cmd, unstamped_line, update_cmd, version_unbound};
use mcp::mcp_cmd;
use output::{
    count_header, highlight, human, print_json, render_error, CliError, CliErrorCode, Flags,
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
    // What was handed to a plugin is taken out of the command line before the parser sees it — the words
    // after a plugin's name are not amenbo's to read (see `plugin_words`).
    let argv: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let handed = plugin_words(&argv);
    let amenbos = match &handed {
        Some((at, _)) => &argv[..=*at],
        None => &argv[..],
    };
    let mut parsed = match retargeted_cli()
        .try_get_matches_from(amenbos)
        .and_then(|m| Cli::from_arg_matches(&m))
    {
        Ok(c) => c,
        Err(e) => return handle_parse_error(e),
    };
    if let (Some((_, words)), Some(Command::Plugin { sub: PluginCmd::Run { args, .. } })) =
        (handed, &mut parsed.command)
    {
        *args = words;
    }
    // `plugin run` keeps no help flag of its own, so a `--help` sitting where the plugin's name goes
    // is the one nobody else will answer. It is answered here, ahead of the facet and the pointer,
    // because that is where a help request has always been answered — before the store is opened.
    if asks_for_own_help(&parsed.command) {
        return print_plugin_run_help();
    }
    if let Some(err) = flag_in_the_name_position(&parsed.command) {
        // No Flags yet, so render the error with a minimal set.
        let probe = Flags { json: parsed.json, yes: false, quiet: false, no_color: false, actor: None };
        return render_error(&probe, &err);
    }
    // facet (actor kind): `--actor` and nothing else (`AMB-D-408`). An operation that uses the facet —
    // stamping who acted, or drawing how far an AI reaches — must declare one, and gets `facet_required`
    // when it does not. An operation that uses none passes without one and never touches a facet again.
    // Nothing is inferred from the context of the call: an environment variable would propagate into
    // every process amenbo starts, and a human default would let an undeclared AI write as a person and
    // read past its binding.
    //
    // On the plugin face one of those two consumers is already served before the command line is read: the
    // reach came with the launch. So the door asks for a facet only where one is still used — the shape of
    // that is `facet_required` below.
    let plugin_face = amenbo_core::plugin_callback::window_declared();
    let actor = match decide_facet(parsed.actor.as_deref(), facet_required(&parsed.command, plugin_face)) {
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

/// Amenbo's own flags as they may stand **ahead of a plugin's name**, and whether each takes the word
/// after it. This is the whole set a reader is allowed to write there (`amenbo plugin run --json worktree
/// …`), so it is also the set [`plugin_words`] steps over on its way to the name.
const FLAGS_BEFORE_THE_NAME: &[(&str, bool)] = &[
    ("--json", false),
    ("--quiet", false),
    ("--no-color", false),
    ("--yes", false),
    ("-y", false),
    ("--actor", true),
    ("--project", true),
];

/// Does this word spell one of amenbo's flags, and does it take the next word? `--actor=ai` is one word
/// and takes nothing further.
fn flag_before_the_name(word: &str) -> Option<bool> {
    let (head, joined) = match word.split_once('=') {
        Some((head, _)) => (head, true),
        None => (word, false),
    };
    FLAGS_BEFORE_THE_NAME
        .iter()
        .find(|(flag, _)| *flag == head)
        .map(|(_, takes_value)| *takes_value && !joined)
}

/// Where the plugin's name stands in this command line, and the words handed to the plugin after it —
/// `None` when this invocation is not a `plugin run` with anything trailing the name.
///
/// **After the name every word is the plugin's** (`AMB-D-346`), and that line cannot be held by the
/// parser alone: amenbo's flags are global, so the parser answers for one wherever it appears — including
/// the first word after the name, which the plugin never then sees. A plugin author who puts `--json` or
/// `--yes` on their own face would find amenbo quietly eating it. So the split is made here, over the raw
/// command line, and the parser is handed only the words up to and including the name.
///
/// The name is the first word after `run` that is not one of amenbo's own (`amenbo plugin run --json
/// worktree …` is the documented place for those). Whatever lands there is the name position, a flag
/// included — that is where `plugin run --help` goes, and where a flag written one word too late is
/// reported from.
///
/// A word that is not valid UTF-8 anywhere in the line gives `None`: the parser owns that error, and it
/// says it better than a splitter could.
fn plugin_words(argv: &[std::ffi::OsString]) -> Option<(usize, Vec<String>)> {
    let at = name_position(argv)?;
    let handed = argv.get(at + 1..).filter(|words| !words.is_empty())?;
    let words: Vec<String> =
        handed.iter().map(|w| w.to_str().map(str::to_string)).collect::<Option<_>>()?;
    Some((at, words))
}

/// Walk the command line to the word standing where a plugin's name goes. Only the path `plugin run`
/// leads there, so anything else — another subcommand, a stray word, a flag amenbo does not answer for
/// before the path is complete — ends the walk with `None` and leaves the line to the parser.
fn name_position(argv: &[std::ffi::OsString]) -> Option<usize> {
    let mut i = 1;
    for step in ["plugin", "run"] {
        loop {
            let word = argv.get(i)?.to_str()?;
            match flag_before_the_name(word) {
                Some(takes_value) => i += if takes_value { 2 } else { 1 },
                None if word == step => {
                    i += 1;
                    break;
                }
                // Any other word means this line goes somewhere else entirely.
                None => return None,
            }
        }
    }
    // Amenbo's own flags reach one word further than the path does: written here they are still ahead of
    // the name, and still amenbo's.
    while let Some(takes_value) = flag_before_the_name(argv.get(i)?.to_str()?) {
        i += if takes_value { 2 } else { 1 };
    }
    // Whatever stands here is the name — a flag amenbo does not answer for included, since past `run`
    // there is nobody else left to read it.
    argv.get(i)?.to_str()?;
    Some(i)
}

/// How a person asks for help, in both spellings.
const HELP_FLAGS: &[&str] = &["--help", "-h"];

/// Is this a `plugin run` with a help flag standing where the plugin's name goes?
///
/// After the name every word is the plugin's (`AMB-D-346`), `--help` included — which is exactly the
/// one a plugin's author puts its usage behind, so amenbo answering it would hide the very text the
/// person asked for. `plugin run` therefore carries no help flag of its own and the word travels
/// through untouched. What is left is the form that names no plugin at all: there, the request is
/// amenbo's to answer, and the name is the position the flag lands in.
///
/// A plugin cannot be called `--help`: a catalog name is `[a-z0-9-]`, so nothing legitimate is shadowed.
fn asks_for_own_help(cmd: &Option<Command>) -> bool {
    matches!(
        cmd,
        Some(Command::Plugin { sub: PluginCmd::Run { name, .. } }) if HELP_FLAGS.contains(&name.as_str())
    )
}

/// A word starting with a hyphen where the plugin's name goes, and it is not a help flag.
///
/// The position takes hyphens at all so that `--help` has somewhere to land; nothing else written there
/// can be a plugin (`[a-z0-9-]`, never leading), so it is a flag of amenbo's put one word too late —
/// they all go ahead of the name. Saying that is the answer; hunting the catalog for a plugin nobody
/// could have installed is not.
fn flag_in_the_name_position(cmd: &Option<Command>) -> Option<CliError> {
    let Some(Command::Plugin { sub: PluginCmd::Run { name, args } }) = cmd else { return None };
    if !name.starts_with('-') {
        return None;
    }
    let corrected = [
        vec![Paths::command_name().to_string(), name.clone()],
        vec!["plugin".to_string(), "run".to_string()],
        args.clone(),
    ]
    .concat()
    .join(" ");
    Some(CliError {
        code: "invalid_value",
        message: format!("'{name}' is a flag, not a plugin's name"),
        hint: Some(format!(
            "amenbo's flags go before the plugin's name — after it every word is the plugin's:\n  {corrected}"
        )),
        exit: 2,
    })
}

/// Print `plugin run`'s own help, the way clap would have.
///
/// It is rendered off the same retargeted tree the parse ran against, so a dev build's help names the
/// command that build installs, exactly as every other help does. The tree is built first, which is what
/// gives a subcommand the full path to itself — without it the usage line opens at `run`, naming no
/// command anyone can type.
fn print_plugin_run_help() -> i32 {
    let mut cli = retargeted_cli();
    cli.build();
    let run = cli
        .find_subcommand_mut("plugin")
        .and_then(|plugin| plugin.find_subcommand_mut("run"))
        .expect("`plugin run` is in the command tree this arm was reached through");
    print!("{}", run.render_long_help());
    0
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
        // Hands over catalog text (`AMB-D-440`) — no store, and nothing of this folder's read either.
        // Its sibling `answer` is not here: it writes this project's row, so it declares a facet like
        // every other write.
        | Command::AgentHook { sub: AgentHookCmd::Snippet { .. } }
        // The MCP server opens no store of its own: it speaks a protocol on two streams and re-runs this
        // executable for every tool call (`AMB-D-665`). The facet is the child's to declare, in the folder
        // the child works — and a host launching a server is in no position to pass one anyway.
        | Command::Mcp { .. }
        // A runner fires the hooks a facet's own writes already queued: it creates nothing, assigns
        // nothing, and was handed the store to work (`AMB-T-2175`). So was a plugin calling amenbo back,
        // whose window comes from the gate it fired through rather than from a facet (`AMB-D-406`).
        | Command::PluginRunner { .. }
        // The hourly tick is woken by the OS scheduler, which is neither a person nor their AI and has no
        // facet to declare (`AMB-D-706`). It reads no store content on anyone's behalf either: what it
        // carries out is amenbo's own errand, whose reach is the device rather than a bound project.
        | Command::Tick
        // `validate` reads a manifest file the author names and touches no store at all — unlike the rest
        // of the group, which moves this machine's plugin state and the plugin's own per-project rows.
        | Command::Plugin { sub: PluginCmd::Validate { .. } } => false,
        // Everything else — every write, and every read that surfaces store content.
        _ => true,
    }
}

/// Does this command **stamp** the facet — into created_by / assign / activity? The write half of
/// [`uses_facet`]'s two consumers, asked on its own by [`facet_required`].
///
/// False is every read, and with them the faces that change something while naming no author: the machine's
/// own settings and plugin state (config / bind / hooks / the whole plugin group, whose per-project rows
/// carry no facet), and the maintenance ops that move the truth source wholesale rather than record an act in
/// it (backup / restore / hard-erase / export). Everything else defaults to true (**fail-closed**), and a
/// variant misjudged here still cannot stamp a facet nobody named: [`Flags::facet`] answers `facet_required`
/// where the value is taken.
fn stamps_facet(cmd: &Option<Command>) -> bool {
    let Some(c) = cmd else { return false }; // no args = discover (a read)
    match c {
        // Pure reads / discovery / local settings / transport (record no facet).
        Command::Agent { .. }
        | Command::Version
        | Command::Update { .. } // installs a build; nothing of it lands in the store
        | Command::Whoami
        | Command::Status { .. }
        | Command::Activity { .. }
        | Command::Search { .. }
        | Command::Validate { .. }
        | Command::Backup { .. } // reads the truth source into a snapshot file; records no facet or activity
        | Command::Restore { .. } // replaces the truth source from a snapshot; maintenance op, records no facet or activity
        | Command::HardErase { .. } // physically erases append-only content; maintenance op, records no facet or activity
        | Command::Export { .. }
        // The carrier's road out: both faces read (a version, a snapshot) and neither records an act —
        // which is what lets a plugin call them with no facet at all (`facet_required`).
        | Command::Sync { .. }
        | Command::Lint { .. } // reads the text it is handed; no store, so nothing to stamp a facet onto
        | Command::GithookPreCommit // the hook's face of `lint`; reads the staged diff, no store
        | Command::GithookCommitMsg { .. } // the hook's face of `lint <file>`; reads the message file, no store
        // A runner fires the hooks a facet's own writes already queued; it creates nothing and assigns
        // nothing, so there is no author for it to stamp (`AMB-T-2175`).
        | Command::PluginRunner { .. }
        // The tick carries out amenbo's own errands and fires the hooks they queued; it creates nothing and
        // assigns nothing, so there is no author for it to stamp.
        | Command::Tick
        // The MCP server writes nothing itself; what its tool calls run is a child that stamps its own.
        | Command::Mcp { .. }
        // `validate` reads a manifest file the author names and touches no store at all; the rest of the
        // group moves this machine's plugin state and the plugin's own per-project rows (settings, the
        // enable gate). Those are local settings like `config`: they carry no author and leave no activity
        // to stamp a facet onto.
        | Command::Plugin { .. }
        | Command::Hooks { .. }
        // The AI-harness consent, like the lint's: a per-project row that records an answer, with no
        // author to stamp and no activity behind it.
        | Command::AgentHook { .. }
        | Command::Config { .. } // settings live in the user layer and leave no activity behind
        | Command::Bind { .. } => false, // only writes the `.amenbo` pointer (no facet recorded)
        // Sub-command groups that are reads.
        Command::Doctor { fix } => *fix, // only --fix writes
        // Anything that is not a read (list/show) counts as a write.
        Command::Project { sub } => !matches!(sub, ProjectCmd::List { .. } | ProjectCmd::Show { .. }),
        Command::Dimension { sub } => !matches!(sub, DimensionCmd::List { .. } | DimensionCmd::Show { .. }),
        Command::Task { sub } => match sub {
            TaskCmd::List { .. } | TaskCmd::Show { .. } => false,
            // The commit group nests: `task commit list` is a read, add/rm stamp a facet.
            TaskCmd::Commit { sub } => !matches!(sub, TaskCommitCmd::List { .. }),
            _ => true,
        },
        Command::Comment { sub } => !matches!(sub, CommentCmd::List { .. }),
        Command::Decision { sub } => !matches!(
            sub,
            DecisionCmd::List { .. } | DecisionCmd::Show { .. } | DecisionCmd::Comment { sub: DecisionCommentCmd::List { .. } }
        ),
        // `attach` group: only `rm` writes; ls/show/open/save are reads. (Adds happen under task/decision
        // attach.)
        Command::Attach { sub } => matches!(sub, AttachCmd::Rm { .. }),
        // The rest (Init …) stamp created_by and friends, so default to true (fail-closed).
        _ => true,
    }
}

/// Must this invocation declare a facet at the door? [`uses_facet`] names the facet's two consumers; this
/// asks which of them is left for the command line to answer.
///
/// On the **plugin face** — amenbo launched this process as a plugin and handed it a window
/// ([`window_declared`](amenbo_core::plugin_callback::window_declared)) — the reach is that window
/// (`AMB-D-406`), and `run` opens the store through it whichever facet is named. The reach-drawing consumer
/// is therefore already served, and a read there uses no facet at all: the read-back an author is shown
/// (`amenbo task show <id> --json`, no facet) is a call in which nothing is decided by one. A write is
/// untouched — it still stamps who acted, and the payload the plugin was handed already names that actor.
///
/// The two are intersected, so this face never asks for *more* than the ordinary one however a variant falls
/// in [`stamps_facet`].
fn facet_required(cmd: &Option<Command>, plugin_face: bool) -> bool {
    uses_facet(cmd) && (!plugin_face || stamps_facet(cmd))
}

/// The command tree clap parses with, worded for the CLI **this build installs**
/// ([`Paths::command_name`]).
///
/// `cli.rs` is authored with the production spelling throughout — the derive takes literals only, so
/// there is nowhere to interpolate a name into, and a doc comment full of `{}` would be unreadable
/// besides. The swap happens here instead, on the way out, which is the same shape the agent spec
/// takes ([`core_agent::retarget_prose`], which words the rule). Without it a dev build's `--help` opens
/// with a usage line naming a command that is not installed, and hands out examples nobody there can
/// run.
///
/// On the production channel every string is already its own spelling and nothing changes — but the
/// walk still happens, so a break in it shows up wherever the tests run rather than only on a channel
/// nothing tests.
fn retargeted_cli() -> clap::Command {
    let named = Cli::command().name(Paths::command_name());
    reword_help(named, &core_agent::retarget_prose)
}

/// Puts `reword` through every help string clap holds: one command's about and long about, each of
/// its arguments' help and long help, and then the same for each subcommand, all the way down.
///
/// The rule is the caller's so the walk can be tested for what it is — whether it *reaches* every
/// string. On the channel the tests run, the real rule rewrites nothing, so a walk that missed a
/// whole branch would look exactly like one that worked.
fn reword_help(mut cmd: clap::Command, reword: &impl Fn(&str) -> String) -> clap::Command {
    if let Some(about) = cmd.get_about().map(ToString::to_string) {
        cmd = cmd.about(reword(&about));
    }
    if let Some(long) = cmd.get_long_about().map(ToString::to_string) {
        cmd = cmd.long_about(reword(&long));
    }
    cmd.mut_args(|mut arg| {
        if let Some(help) = arg.get_help().map(ToString::to_string) {
            arg = arg.help(reword(&help));
        }
        if let Some(long) = arg.get_long_help().map(ToString::to_string) {
            arg = arg.long_help(reword(&long));
        }
        arg
    })
    .mut_subcommands(|sub| reword_help(sub, reword))
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
            // The tick is started by the OS scheduler, from whatever directory that scheduler stands in, so
            // there never is a pointer for it to find and "run init first" would be advice to nobody. What
            // this guard protects against is met a different way for it: rather than take a missing pointer
            // as leave to raise a store, `run` looks for the device's store and, finding none, does nothing
            // at all.
            | Some(Command::Tick)
    )
}

/// Which folder would this invocation use amenbo in — the one [`refuse_a_nested_worktree`] judges — or
/// `None` when the command is outside the guard's reach. Usually the CWD, but `bind --dir <path>` and
/// `project add --dir <path>` place their pointer elsewhere, and the hazard belongs to the folder that
/// receives the pointer rather than to the one the command was typed in. A `--dir` that names no directory
/// is left to the command itself to report.
///
/// Out of reach are the commands that place no pointer and read no store (`version` / `update` / `lint` /
/// `agent-hook`),
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
        | Some(Command::AgentHook { .. })
        | Some(Command::PluginRunner { .. })
        | Some(Command::Plugin { sub: PluginCmd::Validate { .. } })
        // The MCP server is launched by a host, from whatever directory that host happened to be in, and
        // it opens no store there. The folder that decides anything is `--dir`, and the child that runs in
        // it meets this guard itself — one answer, given where it is owed.
        | Some(Command::Mcp { .. })
        // The tick, for the reason `plugin-runner` is: the store it works is this device's, and the folder
        // its launcher happened to stand in decides nothing about it. Refusing it would stall a delivery
        // over where a scheduler was configured.
        | Some(Command::Tick)
        | Some(Command::Unbind { .. }) => None,
        Some(Command::Bind { dir: Some(d), .. })
        | Some(Command::Project { sub: ProjectCmd::Add { dir: d, .. } }) => {
            let p = std::path::PathBuf::from(d);
            p.is_dir().then(|| amenbo_core::binding::canonical_dir(&p).unwrap_or(p))
        }
        _ => std::env::current_dir().ok(),
    }
}

/// Refuse to use amenbo in a git worktree cut inside an amenbo-managed folder, the sibling of the pointer
/// guard: that one asks whether a binding exists, this one whether the checkout is a place to use one. Such
/// a worktree inherits the project's `.amenbo` through the upward walk while the store it writes to sits in
/// app-data and outlives the checkout, so a throwaway environment would drive the real backlog.
///
/// `bind`, `init` and `project add` are held to it too, though they carry no pointer to inherit — they
/// *write* one, and the asymmetry is theirs all the same: `init --force` and `project add` raise a project
/// in the real store, which no `git worktree remove` takes back, and `bind --force` upserts a managed block
/// into CLAUDE.md/AGENTS.md, which in most repositories are tracked. `--force` on either of the first two
/// means "overwrite the pointer already there" and says nothing about this hazard, so it buys no passage
/// here. What is refused is only a worktree nested inside a managed tree; parking one beside the project is
/// the way to have a bound one.
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

/// Refuse to read a `.amenbo` that another store's build wrote (`AMB-D-685`) — the third of the guards
/// that ask what this directory is before anything is dispatched. The pointer's `project_id` is a
/// primary key in the store that wrote it, so a build of another channel reading it lands on whatever
/// its own store keeps at that key; a dev store is seeded by copying another one, so the slug
/// cross-check agrees all the way and says nothing.
///
/// Only the pointer is read ([`amenbo_core::binding::foreign_pointer`]) and no store is opened, so the
/// answer is the same wherever this sits in the dispatch — which is why it can sit at the top, ahead of
/// the commands that answer without a store at all.
///
/// The commands that **write** a pointer are outside it, and that is the whole way out: `bind` and
/// `project add --dir` claim the folder for this store, `init` raises a project in it. Refusing those
/// would leave a text editor as the only way to release a folder — the same reason `unbind` (the other
/// way out) is outside the nested-worktree guard. So are the faces that decide nothing by this
/// directory: `version`, `update`, `lint`, the git hooks, `agent-hook`, `plugin validate`,
/// `plugin-runner`, and `mcp` (whose child meets this guard in the folder its call named).
fn refuse_a_pointer_from_another_store(cmd: &Option<Command>) -> Result<(), CliError> {
    match pointer_store_guard_target(cmd).and_then(|dir| amenbo_core::binding::foreign_pointer(&dir)) {
        Some(foreign) => Err(CliError::pointer_other_store(
            &foreign.dir.to_string_lossy(),
            &foreign.recorded,
            foreign.running,
        )),
        None => Ok(()),
    }
}

/// Which folder [`refuse_a_pointer_from_another_store`] asks about — always the one this command would
/// resolve a pointer from, which is where it was typed, or `None` when the command is outside the guard.
/// Unlike [`nested_guard_target`] a `--dir` never enters into it: every command that names one is a
/// command that writes the pointer, and those are exactly the ones outside.
fn pointer_store_guard_target(cmd: &Option<Command>) -> Option<std::path::PathBuf> {
    let out_of_reach = matches!(
        cmd,
        Some(Command::Version)
            | Some(Command::Update { .. })
            | Some(Command::Lint { .. })
            | Some(Command::GithookPreCommit)
            | Some(Command::GithookCommitMsg { .. })
            | Some(Command::AgentHook { .. })
            | Some(Command::PluginRunner { .. })
            | Some(Command::Plugin { sub: PluginCmd::Validate { .. } })
            | Some(Command::Mcp { .. })
            | Some(Command::Tick)
            | Some(Command::Unbind { .. })
            | Some(Command::Bind { .. })
            | Some(Command::Init { .. })
            | Some(Command::Project { sub: ProjectCmd::Add { .. } })
    );
    (!out_of_reach).then(|| std::env::current_dir().ok()).flatten()
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
        // And whether the pointer this directory offers is even ours to read (`AMB-D-685`). A plugin is
        // outside it for the same reason: the store it works was named to it, so the folder its
        // launcher happened to stand in decides nothing here.
        refuse_a_pointer_from_another_store(&cli.command)?;
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
        // `agent-hook snippet` reads the catalog and this build's own name, and that is all it needs
        // (`AMB-D-440`): store-free like `lint`, so the answer is the same in a bound folder, a fresh
        // clone, and a checkout nobody has bound at all — which is where somebody wiring their tool for
        // the first time is standing.
        Some(Command::AgentHook { sub: AgentHookCmd::Snippet { tool, copy } }) => {
            return agent_hook_snippet_cmd(flags, tool, *copy)
        }
        // The MCP server, ahead of every guard that asks about *this* directory: a host launches it from
        // wherever it happens to stand, and the only folders that decide anything here are the ones
        // `--dir` names (`AMB-D-679`). It opens no store — each tool call re-runs this executable in the
        // folder that call named, and the guards answer there, to the child, in the words a person
        // typing there would read.
        Some(Command::Mcp { dir }) => return mcp_cmd(dir),
        Some(Command::Version) if !store_reachable() => {
            advise_linux_system_orphan();
            return version_unbound(flags);
        }
        Some(Command::Update { print, apply, rollback }) if !store_reachable() => {
            advise_linux_system_orphan();
            return if *rollback {
                self_rollback_cmd(flags)
            } else if *apply {
                self_update_cmd(flags)
            } else {
                update_cmd(flags, *print)
            };
        }
        _ => {}
    }

    // Exec guard (strict). Reaching here means the command neither places the marker (init) nor removes it
    // (unbind), and is not one of the faces that answer without opening a store (version/update, handled
    // above). In a bare directory — no pointer (.amenbo), no AMENBO_HOME/AMENBO_PROJECT_DIR — do not quietly
    // create the single store; tell the user to run init. bind is the exception (it is what places a
    // pointer). `agent` still requires a pointer: the AI's entry point is stopped by location, on purpose.
    // `--project <name or id>` is no side door around this: it names which project a command works in, and
    // still needs a location that reaches a store (this guard runs whether or not it was passed).
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
    // The tick's own half of that guard, which it is outside of. It resolves no folder, so a missing pointer
    // says nothing about whether there is anything to do — but it must not be the thing that brings a store
    // into being either, and `Store::open` below would do exactly that on a device where amenbo has never
    // been used. Reach for this device's store file directly: no store, nothing owed, nothing to say.
    if matches!(cli.command, Some(Command::Tick))
        && !amenbo_core::config::Paths::resolve().map_err(CliError::from)?.store_file.exists()
    {
        return Ok(0);
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
        // The one reach check that cannot wait for the store to be opened, because this command replaces
        // the store rather than reading it. A window was opened to observe one project (`AMB-D-406`), and
        // what this would do is overwrite every project on the device with the contents of a file — the
        // furthest thing from observing. `AMB-D-224` allowed it to the AI facet as the recovery an agent
        // runs on their user's own device; nothing launched a plugin to do that.
        if let Some(window) = plugin_window {
            window.refuse_whole_device("restore").map_err(CliError::from)?;
        }
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
    //
    // Neither does a flush, and for the opposite reason: that command *is* the delivery, and the kick
    // does the same drive with the other launcher — handing every queue to a runner process before the
    // command that was asked to work them, and to say what moved, ever reaches one. What it left to
    // report was then somebody else's work by definition, which is nothing (`AMB-T-2507`). Nothing is
    // lost by standing down for it: the flush drives both layers, unconditionally, and waits.
    //
    // Nor does the tick, which is the flush's reason word for word: working the queues to their end, in this
    // process, is half of what the scheduler started it for, and a kick that handed every queue to a runner
    // first would leave it with somebody else's work to report — which is nothing.
    if plugin_window.is_none()
        && !matches!(
            cli.command,
            Some(Command::Plugin { sub: PluginCmd::Flush }) | Some(Command::Tick)
        )
    {
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

    // The two setups amenbo offers, in the order their questions are put. `hooks` is outside both: its argv
    // already answered the lint's question, and the harness path can record consent, which `hooks status`
    // promises not to do. So is the tick: a scheduler is nobody to put a question to, and what it does must
    // not depend on the directory it was started in.
    if !matches!(cli.command, Some(Command::Hooks { .. }) | Some(Command::Tick)) {
        let lint_asked = lint_hook_setup(&mut store, flags);
        agent_hook_setup(&store, flags, lint_asked);
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
            // Asked once, and the whole of what plugins put in this document: the shelf they are named
            // on, and the lines their authors hung on amenbo's own steps. Asking twice would walk the
            // disk twice and let the two halves disagree.
            let PluginsAtEntry { list, tools, empty_because } =
                plugins_for_agent(&store, bound_project(&store));
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
                // What the user installed and switched on here (`AMB-D-437`) — in the author's own words
                // when amenbo is that author, and as the callable line alone when it is not
                // (`AMB-D-575`/`AMB-D-576`). A key of its own, never folded into `cycles`: those are
                // amenbo's own working practice and amenbo answers for every line of them, while these
                // are a third party's — kept on a separate shelf so a reader can always tell whose words
                // they are reading. Runtime like the fields above, and for the same reason: what is
                // installed and open is the store's answer, not the static spec's.
                map.insert("plugins".to_string(), list);
                // Only when there is nothing to name: a reader with a list in hand has no use for a
                // sentence about lists that are empty, and the key's presence is itself the answer.
                if let Some(why) = empty_because {
                    map.insert(
                        "pluginsEmptyBecause".to_string(),
                        serde_json::Value::String(why.to_string()),
                    );
                }
            }
            // Where git is not in play, drop the `worktree` cycle outright rather than letting it arrive
            // with a "if you use git" caveat: a caveat still spends the reader's context, and what the
            // spec advises here is not a thing they can do. The steps that branch to it lose the branch
            // with it, which is why this goes through core rather than lifting the key out here. The
            // question is about the **bound folder** — the pointer's, not wherever the caller stands —
            // since that is the checkout the work happens in. Core stays a static builder; this is the
            // same runtime seam the fields above use.
            if !bound_dir_is_under_git() {
                amenbo_core::agent::drop_cycle(&mut spec, amenbo_core::agent::Cyc::Worktree);
            }
            // And where a plugin's author named a step of amenbo's own cycle, hang the line to type on
            // that step (`AMB-D-571`): the advice and the tool for it are otherwise in one document
            // with no way to reach each other, which leaves an AI told to cut a worktree with no hand.
            // After the cycle drop above, so a step this run does not carry gets nothing hung on it.
            amenbo_core::agent::attach_tools(&mut spec, &tools);
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
                    "release_build": amenbo_core::build_stamp::is_release_build(),
                    "format_version": vs.format_version,
                    "max_supported_format": vs.max_supported_format,
                    "latest_version": vs.latest_version,
                    "update_available": vs.update_available,
                }));
            } else {
                let suffix = if channel == "amenbo" { String::new() } else { format!(" ({channel})") };
                human(flags, format!("amenbo {}{}", agent::VERSION, suffix));
                human(flags, format!("format: store v{} (this build opens up to v{})", vs.format_version, vs.max_supported_format));
                if let Some(line) = unstamped_line() {
                    human(flags, line);
                }
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
                self_update_cmd(flags)
            } else {
                update_cmd(flags, print)
            };
        }
        Command::Whoami => return whoami(&store, flags),
        Command::Bind { project, dir, force, rebind } => return bind_cmd(&store, flags, project, dir, force, rebind),
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
        Command::Search { words, project, filter, kind, face, sort, limit, offset } => {
            let project_id =
                project.map(|p| store.resolve_project_ref(&p)).transpose().map_err(CliError::from)?;
            let result = store
                .search(query::SearchParams {
                    // The words arrive as separate arguments (a shell has already split them), and the read
                    // splits on whitespace — so they are handed over joined rather than re-quoted.
                    text: words.join(" "),
                    project_id,
                    filter_expr: filter,
                    kind: kind.as_deref().map(query::SearchKind::parse).transpose().map_err(CliError::from)?,
                    face: face.as_deref().map(query::HitFace::parse).transpose().map_err(CliError::from)?,
                    sort: query::SearchSort::parse(&sort).map_err(CliError::from)?,
                    limit,
                    offset,
                })
                .map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, count_header(result.count, result.total_matched, "hit"));
                // Asked once for the whole listing rather than per row: whether escapes render is a
                // property of where the output is going, and it cannot change between two hits.
                let color = flags.color();
                for h in &result.hits {
                    // Where it landed, then what it is: the ref reads first because it is what the reader
                    // opens next. A comment says which one, since the ref alone names only the record.
                    let face = format!("{:?}", h.face).to_lowercase();
                    let at = h.comment.as_deref().map(|c| format!(" · {c}")).unwrap_or_default();
                    human(flags, format!("  [{face}] {} {}{at}", h.r#ref, h.title));
                    // Where the record stands, on a line of its own before the excerpt (`AMB-D-567`): a
                    // hit on a task that is over is a different answer from one still to be taken, and
                    // the ref and the title say neither. Each side speaks its own vocabulary — a task
                    // states its status, its priority and what it is filed under, a decision its status,
                    // which is all it has — so the same line reads as the record's own without naming
                    // which of the two it is. Nothing is printed when the read that fills this in came
                    // back empty: the words really are written there, and a blank line claiming a state
                    // would be worse than the row saying nothing about one.
                    if let Some(standing) = &h.standing {
                        let pri = standing.priority.as_deref().map(|p| format!(" [{p}]")).unwrap_or_default();
                        // Written the way the filter takes it (`axis=value`), like `task show`'s own
                        // line, so what is read here pastes straight into `--filter "dim:…"`.
                        let filed = match standing.labels.is_empty() {
                            true => String::new(),
                            false => format!(
                                " · {}",
                                standing.labels.iter()
                                    .map(|l| format!("{}={}", l.axis, l.value))
                                    .collect::<Vec<_>>().join(", ")
                            ),
                        };
                        human(flags, format!("      {}{pri}{filed}", standing.status));
                    }
                    // The excerpt says where the words are; this says where in the excerpt they are, which
                    // is the question a two-word search leaves open (`AMB-D-566`).
                    human(flags, format!("      {}", highlight(&h.snippet, &h.matches, color)));
                }
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
        Command::Sync { sub } => return sync_cmd(&store, flags, sub),
        Command::Backup { path } => return run_backup(&store, flags, path),
        Command::HardErase { sub } => return hard_erase(&mut store, flags, sub),
        Command::Restore { .. } => {
            unreachable!("handled before open")
        }
        Command::Tick => return tick_cmd(&store, flags),
        Command::Hooks { sub } => return hooks_cmd(&mut store, flags, sub),
        // The recording face is the one that needs this folder: the answer is kept against the project
        // it is bound to, so unlike `snippet` it opens the store like any other write.
        Command::AgentHook { sub: AgentHookCmd::Answer { answer } } => {
            return agent_hook_answer_cmd(&store, flags, answer == "yes")
        }
        Command::AgentHook { .. } => {
            unreachable!("`agent-hook snippet` is handled before open")
        }
        Command::Mcp { .. } => {
            unreachable!("the MCP server is handled before open")
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::collections::HashSet;

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

    /// The nested-worktree guard judges the folder that will **receive** the pointer, and `project add`
    /// places one just as `bind --dir` does — so it is asked about `--dir`, not about where the command
    /// was typed. A `--dir` naming nothing is left to the command itself to report, which is the shape
    /// that keeps this guard from answering "no hazard" for a path it never looked at.
    #[test]
    fn the_nested_guard_judges_the_folder_project_add_would_link() {
        use clap::Parser;
        let dir = amenbo_scratch::scratch("guard-target-project-add");
        let parse = |args: &[&str]| Cli::try_parse_from(args).expect("parses").command;

        let target = nested_guard_target(&parse(&[
            "amenbo", "project", "add", "--name", "P", "--dir", &dir.to_string_lossy(),
        ]));
        assert_eq!(
            target.map(|p| amenbo_core::binding::canonical_dir(&p).unwrap_or(p)),
            Some(amenbo_core::binding::canonical_dir(&dir).unwrap_or(dir.clone())),
            "the folder the pointer lands in is what the guard judges",
        );

        let missing = dir.join("never-made");
        assert_eq!(
            nested_guard_target(&parse(&[
                "amenbo", "project", "add", "--name", "P", "--dir", &missing.to_string_lossy(),
            ])),
            None,
            "a --dir that names no directory is `project add`'s to report, not this guard's",
        );
    }

    /// The pointer-store guard holds every command that would **read** this folder's pointer, and lets
    /// through the three that write one — otherwise a folder another store claimed could only be
    /// released with a text editor (`AMB-D-685`).
    #[test]
    fn the_pointer_store_guard_lets_through_the_commands_that_would_release_the_folder() {
        use clap::Parser;
        let parse = |args: &[&str]| Cli::try_parse_from(args).expect("parses").command;
        let asks = |args: &[&str]| pointer_store_guard_target(&parse(args)).is_some();

        for read in [
            vec!["amenbo", "status"],
            vec!["amenbo", "task", "list"],
            vec!["amenbo", "agent"],
            vec!["amenbo", "doctor"],
        ] {
            assert!(asks(&read), "a command that reads the pointer is held to the guard: {read:?}");
        }
        for way_out in [
            vec!["amenbo", "bind", "--project", "7"],
            vec!["amenbo", "init", "--name", "Alice"],
            vec!["amenbo", "unbind"],
            vec!["amenbo", "project", "add", "--name", "P", "--dir", "/tmp"],
        ] {
            assert!(!asks(&way_out), "the way out of a claimed folder is not refused: {way_out:?}");
        }
        for store_free in [
            vec!["amenbo", "version"],
            vec!["amenbo", "lint", "--stdin"],
            vec!["amenbo", "agent-hook", "snippet", "cursor"],
            vec!["amenbo", "mcp", "--dir", "/tmp"],
        ] {
            assert!(!asks(&store_free), "a face that decides nothing by this folder: {store_free:?}");
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

    /// Every help string clap holds must be reachable by the reword — the whole of `--help`, not the
    /// branches someone remembered. A rewriter that marks what it touched is used instead of the real
    /// rule, because the real one rewrites nothing on the channel the tests run: an untouched string
    /// would then be indistinguishable from a correctly-left-alone one.
    #[test]
    fn rewording_reaches_every_help_string() {
        let reworded = reword_help(Cli::command(), &|text: &str| format!("«{text}"));
        let (mut seen, mut unreached) = (0usize, Vec::new());
        fn walk(cmd: &clap::Command, path: &str, seen: &mut usize, out: &mut Vec<String>) {
            let mut check = |s: Option<&clap::builder::StyledStr>, what: &str| {
                if let Some(text) = s {
                    *seen += 1;
                    if !text.to_string().starts_with('«') {
                        out.push(format!("{path}: {what}"));
                    }
                }
            };
            check(cmd.get_about(), "about");
            check(cmd.get_long_about(), "long about");
            for arg in cmd.get_arguments() {
                check(arg.get_help(), &format!("{} help", arg.get_id()));
                check(arg.get_long_help(), &format!("{} long help", arg.get_id()));
            }
            for sub in cmd.get_subcommands().filter(|s| s.get_name() != "help") {
                walk(sub, &format!("{path} {}", sub.get_name()), seen, out);
            }
        }
        walk(&reworded, "amenbo", &mut seen, &mut unreached);
        assert!(unreached.is_empty(), "the reword did not reach: {unreached:?}");
        assert!(seen > 250, "the walk found almost no help strings ({seen}) — it stopped reaching them");
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

    /// The boundary the parser cannot hold: from the plugin's name onward every word is the plugin's,
    /// amenbo's own spellings included, while the same flags written ahead of the name stay amenbo's.
    /// A line that goes anywhere else is left alone for the parser to read as it always has.
    #[test]
    fn the_words_after_a_plugins_name_are_taken_off_the_command_line() {
        let split = |line: &str| {
            let argv: Vec<std::ffi::OsString> =
                line.split_whitespace().map(std::ffi::OsString::from).collect();
            plugin_words(&argv).map(|(at, words)| (argv[at].to_str().unwrap().to_string(), words.join(" ")))
        };
        let handed = |line: &str| split(line).map(|(_, words)| words);

        // The word right after the name — the one position the parser answered for itself.
        assert_eq!(split("amenbo plugin run worktree --actor ai"), Some(("worktree".into(), "--actor ai".into())));
        assert_eq!(handed("amenbo plugin run worktree --json"), Some("--json".into()));
        assert_eq!(handed("amenbo plugin run worktree -y start"), Some("-y start".into()));
        // Ahead of the name they are amenbo's, and the name is still found past them.
        assert_eq!(split("amenbo plugin run --json worktree start"), Some(("worktree".into(), "start".into())));
        assert_eq!(split("amenbo --actor ai plugin run worktree start"), Some(("worktree".into(), "start".into())));
        assert_eq!(split("amenbo plugin run --actor=ai worktree start"), Some(("worktree".into(), "start".into())));
        // Nothing trailing the name: there is nothing to take off, so the line is left whole.
        assert_eq!(handed("amenbo plugin run worktree"), None);
        assert_eq!(handed("amenbo plugin run"), None);
        // The name position holds whatever was written there, a flag included — that is where amenbo's
        // own help lands, and where a misplaced flag is reported from.
        assert_eq!(handed("amenbo plugin run --help"), None);
        assert_eq!(split("amenbo plugin run --jsn usage"), Some(("--jsn".into(), "usage".into())));
        // Another command entirely, and a word standing where the path should be.
        assert_eq!(handed("amenbo plugin log usage"), None);
        assert_eq!(handed("amenbo task list --json"), None);
        assert_eq!(handed("amenbo --help plugin run worktree start"), None);
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
        assert!(!uses_facet(&Some(Command::Bind { project: None, dir: None, force: false, rebind: None })));
        assert!(!uses_facet(&Some(Command::Lint { paths: Vec::new(), stdin: false })));
        assert!(!uses_facet(&Some(Command::GithookPreCommit)));
        assert!(!uses_facet(&Some(Command::AgentHook {
            sub: AgentHookCmd::Snippet { tool: "claude-code".to_string(), copy: false }
        })));
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

    /// `stamps_facet` is the write half alone: it must not claim a read, because on the plugin face this is
    /// the whole of what `--actor` is still demanded by.
    #[test]
    fn stamps_facet_is_the_write_half_alone() {
        assert!(!stamps_facet(&None)); // discover
        assert!(!stamps_facet(&Some(Command::Task { sub: TaskCmd::Show { id: "x".to_string() } })));
        assert!(!stamps_facet(&Some(Command::Task { sub: TaskCmd::List { project: None, filter: None, sort: "order".to_string(), limit: None, offset: None } })));
        assert!(!stamps_facet(&Some(Command::Comment { sub: CommentCmd::List { task: "x".to_string(), limit: None, offset: None } })));
        assert!(!stamps_facet(&Some(Command::Status { scope: "today".to_string() })));
        assert!(!stamps_facet(&Some(Command::Doctor { fix: false })));
        // Changing something while naming no author: this machine's settings and its plugin state.
        assert!(!stamps_facet(&Some(Command::Plugin { sub: PluginCmd::Enable { name: "p".to_string() } })));
        // Writes name an author.
        assert!(stamps_facet(&Some(Command::Comment { sub: CommentCmd::Add { task: "x".to_string(), text: "t".to_string() } })));
        assert!(stamps_facet(&Some(Command::Task { sub: TaskCmd::Status { id: "x".to_string(), status: "in_progress".to_string() } })));
        assert!(stamps_facet(&Some(Command::Task { sub: TaskCmd::Done { id: "x".to_string() } })));
        assert!(stamps_facet(&Some(Command::Doctor { fix: true })));
    }

    /// The door's own line (`AMB-T-2460`): a plugin reads back with no facet, because the window it was
    /// launched with is what draws the reach — while a write from the same plugin still declares who acted.
    /// Off the plugin face nothing moves, and no command is asked for a facet on the plugin face that the
    /// ordinary face lets through.
    #[test]
    fn the_plugin_face_asks_for_a_facet_only_where_one_is_still_used() {
        let read = Some(Command::Task { sub: TaskCmd::Show { id: "x".to_string() } });
        let discover = None;
        let write = Some(Command::Comment { sub: CommentCmd::Add { task: "x".to_string(), text: "t".to_string() } });
        // The read-back the author's documentation shows, and the bare discover beside it.
        assert!(facet_required(&read, false), "off the plugin face a read draws the reach from the facet");
        assert!(!facet_required(&read, true));
        assert!(facet_required(&discover, false));
        assert!(!facet_required(&discover, true));
        // A write stamps who acted on either face.
        assert!(facet_required(&write, false));
        assert!(facet_required(&write, true));
        // Never stricter than the ordinary face.
        for cmd in [
            None,
            Some(Command::Version),
            Some(Command::Agent { command: None, full: false }),
            Some(Command::Update { print: false, apply: false, rollback: false }),
            Some(Command::Bind { project: None, dir: None, force: false, rebind: None }),
            Some(Command::Plugin { sub: PluginCmd::List }),
            Some(Command::Doctor { fix: false }),
            Some(Command::Doctor { fix: true }),
            Some(Command::Task { sub: TaskCmd::Done { id: "x".to_string() } }),
        ] {
            assert!(
                !facet_required(&cmd, true) || facet_required(&cmd, false),
                "the plugin face asks for a facet where the ordinary face does not: {cmd:?}"
            );
        }
    }

    /// The only faces allowed through without a binding are the ones that never read the store. Loosen this
    /// and a directory with no pointer falls into `Store::open()` quietly creating a new store — precisely
    /// what the exec guard exists to prevent.
    #[test]
    fn only_the_faces_that_never_open_the_store_run_without_a_pointer() {
        // The commands that place or remove the marker, and the ones that answer from facts about the build.
        assert!(!requires_pointer(&Some(Command::Init { name: None, language: None, force: false })));
        assert!(!requires_pointer(&Some(Command::Bind { project: None, dir: None, force: false, rebind: None })));
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
}
