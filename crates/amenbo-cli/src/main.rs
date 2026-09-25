//! Entry point for the Amenbo CLI: parse with clap, delegate to the core (amenbo-core) operations.
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
use cmd::data::{export, migrate_at_startup, run_backup, run_restore};
use cmd::decision::decision;
use cmd::dimension::dimension;
use cmd::hard_erase::hard_erase;
use cmd::labels::project_label;
use cmd::lint::lint_cmd;
use cmd::outbox::{resume_dispatch, resume_the_viewer, with_dispatch};
use cmd::place::{binding_project, location_header, named_project_flag};
use cmd::project::project;
use cmd::setup::{
    agent_hook_answer_cmd, agent_hook_setup, agent_hook_snippet_cmd, hooks_cmd, lint_hook_setup, tick_cmd,
    tick_reconcile,
};
use cmd::status::{render_discover, render_status};
use cmd::task::task;
use cmd::tick::tick_run_cmd;
use cmd::update::{self_rollback_cmd, self_update_cmd, unstamped_line, update_cmd, version_unbound};
use mcp::mcp_cmd;
use output::{
    count_header, highlight, human, print_json, render_error, CliError, Flags,
};

/// The effective project id picked by an explicit override (`--project`). It is a process-wide setting
/// decided once from argv, hence `OnceLock` (one CLI run is one process). Unset means the binding
/// (`.amenbo`) decides.
static PROJECT_OVERRIDE: OnceLock<i64> = OnceLock::new();

fn main() {
    // Both of these run before anything else, and before any thread exists: one hands SIGPIPE back to the
    // kernel, the other disowns an inherited `TMPDIR` whose directory the OS already took away
    // (`AMB-T-3461`).
    restore_sigpipe();
    amenbo_core::tmpdir::forget_if_gone();
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
    let (parsed, typed) = match retargeted_cli()
        .try_get_matches_from(std::env::args_os())
        .and_then(|m| Cli::from_arg_matches(&m).map(|c| (c, named(&m))))
    {
        Ok(c) => c,
        Err(e) => return handle_parse_error(e),
    };
    // Before anything is read or written, so a refusal here has changed nothing.
    if let Err(err) = in_a_step(&typed, amenbo_core::env::automation_step().is_some()) {
        let probe = Flags { json: parsed.json, yes: false, quiet: false, no_color: false, actor: None };
        return render_error(&probe, &err);
    }
    // facet (actor kind): `--actor` and nothing else (`AMB-D-408`). An operation that uses the facet —
    // stamping who acted, or drawing how far an AI reaches — must declare one, and is refused
    // when it does not. An operation that uses none passes without one and never touches a facet again.
    // Nothing is inferred from the context of the call: an environment variable would propagate into
    // every process Amenbo starts, and a human default would let an undeclared AI write as a person and
    // read past its binding.
    let actor = match decide_facet(parsed.actor.as_deref(), uses_facet(&parsed.command)) {
        Ok(a) => a,
        Err(err) => {
            // No Flags yet, so render the error with a minimal set.
            let probe = Flags { json: parsed.json, yes: false, quiet: false, no_color: false, actor: None };
            return render_error(&probe, &err);
        }
    };
    let flags = Flags {
        json: parsed.json,
        yes: parsed.yes,
        quiet: parsed.quiet,
        no_color: parsed.no_color,
        actor,
    };
    // Whether the refs this run writes are links. It is a fact about the terminal this process was
    // started in, so it is settled once, here, ahead of anything that could name a record.
    cmd::labels::settle_link_rendering();
    match run(parsed, &flags) {
        Ok(code) => code,
        Err(err) => render_error(&flags, &err),
    }
}

/// **The command a line names, as the registry spells it** (`task status`, `notify target-list`) —
/// the subcommands clap matched, joined. A line naming none is `amenbo` itself.
fn named(matches: &clap::ArgMatches) -> String {
    let mut words = Vec::new();
    let mut at = matches;
    while let Some((name, under)) = at.subcommand() {
        words.push(name);
        at = under;
    }
    match words.is_empty() {
        true => core_agent::Cmd::Amenbo.name().to_string(),
        false => words.join(" "),
    }
}

/// **The commands no hand types, and so no table holds** — each is hidden, and each is launched by
/// something that runs inside a step's terminal as readily as anywhere: git runs the hooks on every
/// commit a step makes, Amenbo launches the sender and the carrier after a write, the scheduler runs
/// the tick, and the window names its pane. Refusing them there would fail a step's commit on its own
/// hook. The parser test holds every command to being in the table or here.
const LAUNCHED_NOT_TYPED: &[&str] =
    &["githook-pre-commit", "githook-commit-msg", "notify-sender", "viewer-carrier", "tick run", "talk name"];

/// **Refuse what the terminal a run opened for a step may not type** (`AMB-D-968`), for every command
/// alike, from the one table that says ([`core_agent::Cmd::in_a_step`]) — the same table the list a
/// step is taught is made from, so what it is told and what it is let do cannot drift apart.
///
/// **A name the table does not hold is refused.** The table is an allow-list, and a command that
/// reached here without being written into it is on the side that is refused rather than the one that
/// quietly lets it through. Outside a step nothing is asked.
fn in_a_step(typed: &str, inside: bool) -> Result<(), CliError> {
    use core_agent::{Cmd, InAStep};
    if !inside || LAUNCHED_NOT_TYPED.contains(&typed) {
        return Ok(());
    }
    match Cmd::ALL.iter().find(|cmd| cmd.name() == typed).map(|cmd| cmd.in_a_step()) {
        Some(InAStep::HandsBack | InAStep::Reaches) => Ok(()),
        Some(InAStep::MovesTheTask) => Err(CliError::automation_task_is_the_runs(typed)),
        Some(InAStep::OutsideARun) | None => Err(CliError::automation_outside_only(typed)),
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


/// The flag that takes a value; the rest stand alone. It is also the only one whose misplacement
/// **explains** a failure, which is why it is what the hint triggers on.
const FACET_FLAG: &str = "--actor";



/// Amenbo's own flags as they may stand **ahead of the command**, and whether each takes the word after
/// it. It is what a reader of a line has to step over to reach the word that names the command, which is
/// the question the MCP face asks of the words a caller sent ([`crate::mcp`]).
const FLAGS_BEFORE_THE_NAME: &[(&str, bool)] = &[
    ("--json", false),
    ("--quiet", false),
    ("--no-color", false),
    ("--yes", false),
    ("-y", false),
    ("--actor", true),
    ("--project", true),
];

/// Does this word spell one of Amenbo's flags, and does it take the next word? `--actor=ai` is one word
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
/// them (bind / lint / the git hooks), and the entry points Amenbo starts itself with a store already
/// named (the notification sender, the Viewer carrier). Those never reach a facet, so there is nothing for
/// an undeclared one to go wrong in. Everything else defaults to true (**fail-closed**: a
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
        // One timer for the machine, answered on the device: it reads and writes a config key and no
        // store content at all, which is `config`'s class rather than the lint hook's.
        | Command::Tick { .. }
        // A device's skins: files beside the store and one name in the config. Nothing of a
        // project's content is read or written, which is `config`'s class.
        | Command::Skin { .. }
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
        // A sender posts what a facet's own writes already earned: it creates nothing, assigns nothing,
        // and was handed the store to post through (`AMB-D-885`).
        | Command::NotifySender { .. }
        // A carrier is handed the store to carry and takes the turn its launcher's write earned; it
        // creates nothing and assigns nothing, so there is no facet for it to declare (`AMB-D-884`).
        | Command::ViewerCarrier { .. }
        // The surface layer speaks to the pane on screen and writes to no store (`AMB-D-749`), so there
        // is no author to stamp and no reach to draw — which is why it takes no `--actor` at all.
        | Command::Talk { .. } => false,
        // Everything else — every write, and every read that surfaces store content.
        _ => true,
    }
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
            // The scheduler's own face, started from whatever directory that scheduler stands in, so there
            // never is a pointer for it to find and "run init first" would be advice to nobody. What this
            // guard protects against is met a different way for it: rather than take a missing pointer as
            // leave to raise a store, `run` looks for the device's store and, finding none, does nothing at
            // all. Its siblings are not here — a person typing `tick install` stands somewhere on purpose.
            | Some(Command::Tick { sub: TickCmd::Run })
            // The surface layer opens no store: a statement goes to the pane that is on screen now, and
            // the folder a terminal happens to stand in decides nothing about it. Requiring a pointer
            // would silence the vocabulary in exactly the checkouts an agent is put to work in.
            | Some(Command::Talk { .. })
    )
}

/// Which folder would this invocation use Amenbo in — the one [`refuse_a_nested_worktree`] judges — or
/// `None` when the command is outside the guard's reach. Usually the CWD, but `bind --dir <path>` and
/// `project add --dir <path>` place their pointer elsewhere, and the hazard belongs to the folder that
/// receives the pointer rather than to the one the command was typed in. A `--dir` that names no directory
/// is left to the command itself to report.
///
/// Out of reach are the commands that place no pointer and read no store (`version` / `update` / `lint` /
/// `agent-hook`),
/// and `unbind` — the way *out*. Refusing that one would strand a pointer an older build wrote, leaving a
/// text editor as the only way to remove it, and its single store write forgets this folder's registration:
/// that cleans the binding up rather than driving the backlog with it. So is the notification sender: it is
/// handed the store to post through and inherits only its launcher's directory, so the walk this guard makes
/// would answer about a folder it never consulted — and refusing it would drop a message over where a
/// command was typed.
fn nested_guard_target(cmd: &Option<Command>) -> Option<std::path::PathBuf> {
    match cmd {
        Some(Command::Version)
        | Some(Command::Update { .. })
        | Some(Command::Lint { .. })
        | Some(Command::GithookPreCommit)
        | Some(Command::GithookCommitMsg { .. })
        | Some(Command::AgentHook { .. })
        | Some(Command::NotifySender { .. })
        | Some(Command::ViewerCarrier { .. })
        // The MCP server is launched by a host, from whatever directory that host happened to be in, and
        // it opens no store there. The folder that decides anything is `--dir`, and the child that runs in
        // it meets this guard itself — one answer, given where it is owed.
        | Some(Command::Mcp { .. })
        // The scheduler's face, for the reason the sender's is: the store it works is this device's, and
        // the folder its launcher happened to stand in decides nothing about it. Refusing it would stall a
        // delivery over where a scheduler was configured.
        | Some(Command::Tick { sub: TickCmd::Run })
        // The surface layer, for the reason it needs no pointer: it reaches the pane it was launched in
        // and no store at all, so a checkout that is no place to drive the backlog is still a place to
        // say what is happening in it — which is where an agent works.
        | Some(Command::Talk { .. })
        | Some(Command::Unbind { .. }) => None,
        Some(Command::Bind { dir: Some(d), .. })
        | Some(Command::Project { sub: ProjectCmd::Add { dir: d, .. } }) => {
            let p = std::path::PathBuf::from(d);
            p.is_dir().then(|| amenbo_core::binding::canonical_dir(&p).unwrap_or(p))
        }
        _ => std::env::current_dir().ok(),
    }
}

/// Refuse to use Amenbo in a git worktree cut inside an Amenbo-managed folder, the sibling of the pointer
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
/// directory: `version`, `update`, `lint`, the git hooks, `agent-hook`, the notification sender, the Viewer
/// carrier, and `mcp` (whose child meets this guard in the folder its call named).
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
            | Some(Command::NotifySender { .. })
            | Some(Command::ViewerCarrier { .. })
            | Some(Command::Mcp { .. })
            | Some(Command::Tick { sub: TickCmd::Run })
            | Some(Command::Talk { .. })
            | Some(Command::Unbind { .. })
            | Some(Command::Bind { .. })
            | Some(Command::Init { .. })
            | Some(Command::Project { sub: ProjectCmd::Add { .. } })
    );
    (!out_of_reach).then(|| std::env::current_dir().ok()).flatten()
}

/// Is there a `.amenbo` pointer in the current directory (or above it)? An explicit AMENBO_HOME is the
/// caller's business to allow for; this is nothing but the pointer search.
fn pointer_present() -> bool {
    std::env::current_dir()
        .ok()
        .map(|cwd| amenbo_core::binding::find_upward(&cwd).is_some())
        .unwrap_or(false)
}

/// Is the bound folder — the one holding the `.amenbo` this invocation resolved — inside a git checkout?
/// This is what gates the agent spec's git advice — the `worktree` cycle, and the steps of `commit` that
/// only git makes possible: what git reaches, that advice reaches, and no further. It
/// asks about that folder rather than the CWD because the binding is what says where the work lives, and a
/// caller with no pointer at all has no such folder, so the answer is no.
fn bound_dir_is_under_git() -> bool {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| amenbo_core::binding::find_upward(&cwd))
        .is_some_and(|(dir, _)| worktree::under_git(&dir))
}

/// Can this invocation reach a store at all — is one named, by a pointer or by `AMENBO_HOME`? If not,
/// `Store::open()` would quietly create a new one, so "may we open?" is asked in exactly one place: both the
/// exec guard and the faces that answer without opening (version/update) consult this.
///
/// `AMENBO_PROJECT_DIR` used to count here too, back when it named where the pointer search began and the
/// `.amenbo` it found chose a store (`AMB-D-155` retired that: a pointer names a project, never a store).
/// Once it stopped naming anywhere, a run that set it was let past this guard without anything being named
/// at all — so it is gone rather than kept as a word that only widens the door.
fn store_reachable() -> bool {
    amenbo_core::env::home().is_some() || pointer_present()
}

/// Say where the plugins went, once on this surface (`AMB-D-884` / [`amenbo_core::handover`]).
///
/// What a person is owed differs by what they had: a device that carried connections is told its
/// notifications are in Amenbo's own settings now, and one that only ever had `worktree` is told the
/// command it types instead. Marked told only after it has actually been written, so a run that dies
/// mid-sentence still owes it.
///
/// A store that cannot be written is not a reason to fail a command: the account stays owed and the
/// person is told again next time, which is the harmless end of being wrong here.
fn announce_the_handover(store: &Store) {
    let Ok(Some(said)) = store.handover_waiting(amenbo_core::handover::surface::CLI) else { return };
    if said.plugins.is_empty() {
        return;
    }
    let cmd = Paths::command_name();
    eprintln!(
        "✓ The plugins are part of Amenbo now ({}) — there is nothing to install and nothing to enable.",
        said.plugins.join(", "),
    );
    if said.carried_notifications() {
        eprintln!(
            "  Your mail and Slack settings were carried over: {} connection(s) on this device's shelf, {} project(s) reporting through them (`{cmd} notify`).",
            said.targets, said.projects,
        );
    }
    if said.viewer {
        eprintln!("  The Viewer is in this device's own settings, with the keys it was paired on (`{cmd} viewer`).");
    }
    if said.plugins.iter().any(|p| p == "worktree") {
        eprintln!("  Cutting a task its own worktree is `{cmd} worktree start <id>`.");
    }
    let _ = store.handover_told(amenbo_core::handover::surface::CLI);
}

/// Nudge a Linux user off an older *system-wide* install. The retired `.deb`/`.rpm` left the GUI and CLI
/// under `/usr/bin` (root-owned); the per-user build cannot retire those, and by policy it advises
/// rather than auto-strips them. On stderr (so `--json` stdout stays clean), no-op off Linux or once the
/// packages are gone — mirroring the "newer version available" advisory it sits beside. The package name
/// stays lowercase `amenbo` — it is what those retired packages were built under, not what the
/// product is called today. `apt` covers Debian/Ubuntu, `dpkg -r` / `rpm -e` the rest.
fn advise_linux_system_orphan() {
    if amenbo_core::self_update::linux_system_orphan_present() {
        eprintln!(
            "⚠ An older system-wide Amenbo is still installed under /usr/bin. Remove it with your \
             package manager: `sudo apt remove amenbo` (or `dpkg -r amenbo` / `rpm -e amenbo`)."
        );
    }
}

fn run(cli: Cli, flags: &Flags) -> Result<i32, CliError> {
    // Whether this checkout is a place to use Amenbo at all is asked before any dispatch: `init` raises a
    // project in the real store and returns below without ever reaching the guards further down, so a
    // refusal that came later would arrive after the damage it exists to prevent.
    refuse_a_nested_worktree(&cli.command)?;
    // And whether the pointer this directory offers is even ours to read (`AMB-D-685`).
    refuse_a_pointer_from_another_store(&cli.command)?;
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
        // The surface layer (`AMB-D-749`), on the same store-free footing as `lint` and ahead of the exec
        // guard for a sharper reason: it must answer the same in a bound folder, a worktree and a bare
        // directory, because the pane it speaks to is the same pane in all three. Opening a store to say
        // something about a terminal would be work for an answer that does not depend on it.
        Some(Command::Talk { sub }) => return cmd::talk::talk_cmd(flags, sub.as_ref()),
        // The hook's own entry points, same store-free footing as `lint`: `pre-commit` lints the staged
        // diff (no paths), `commit-msg` lints the message file git hands over.
        Some(Command::GithookPreCommit) => return lint_cmd(flags, Vec::new(), false),
        Some(Command::GithookCommitMsg { path }) => return lint_cmd(flags, vec![path.clone()], false),
        // A notification sender: Amenbo launched this process to post one drive's worth of messages through
        // the store it was handed (`AMB-D-885`). It opens the store it was handed, so it sits ahead of every
        // guard that asks about *this* directory — its own is whatever its launcher happened to be in, and
        // it was never asked to answer for it.
        Some(Command::NotifySender { store }) => {
            amenbo_core::notify_dispatch::send_process(store.into());
            return Ok(0);
        }
        // A Viewer carrier, on the same footing: Amenbo launched this process to take one turn of the send
        // over the store it was handed (`AMB-D-884`).
        Some(Command::ViewerCarrier { store }) => {
            amenbo_core::viewer::send::carry_process(store.into());
            return Ok(0);
        }
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
    // above). In a bare directory — no pointer (.amenbo), no AMENBO_HOME — do not quietly create the single
    // store; tell the user to run init. bind is the exception (it is what places a pointer). `agent` still requires a pointer: the AI's entry point is stopped by location, on purpose.
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
    // The scheduler's own half of that guard, which it is outside of. It resolves no folder, so a missing
    // pointer says nothing about whether there is anything to do — but it must not be the thing that brings
    // a store into being either, and `Store::open` below would do exactly that on a device where Amenbo has
    // never been used. Reach for this device's store file directly: no store, nothing owed, nothing to say.
    if matches!(cli.command, Some(Command::Tick { sub: TickCmd::Run }))
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
        return run_restore(flags, path.clone());
    }
    // Migration runs here — before the store is opened, before the command matters. There is no saying
    // whether the CLI or the GUI comes up first on this device, so both enter through the same door
    // (`migrate::at_startup`), and whichever arrives second waits there while the other runs. A store already
    // at the current version (the normal case) leaves immediately without even taking the lock.
    migrate_at_startup(flags)?;

    let store = Store::open().map_err(CliError::from)?;

    // The store-internal integrity check (`doctor`), **asked for here** rather than taken off the open:
    // no open computes it (`AMB-D-857`), because the fold is O(total) and this is one of only two
    // surfaces that display the answer. Taken **before the reach is narrowed below**, so the tally is the
    // device's — which is what the reporting further down rests on. What is done with it is decided
    // there, once the reach is fixed.
    let startup_check = match store.config.startup_integrity_check {
        true => Some(store.compute_startup_health().map_err(CliError::from)?),
        false => None,
    };

    // Clone detection: warn that this store may have been copied to another machine.
    if store.forked {
        eprintln!(
            "⚠ This store may have been copied to a different machine (hardware identity changed)."
        );
    }

    // The startup kick (`AMB-D-399`), ahead of the command and of the network the update check does: a
    // delivery a previous run left standing is picked up now, whatever this invocation was called to do.
    //
    // The tick stands down from it: that command *is* the carrying, and it posts in the process it was
    // woken in rather than handing the messages to a sender it would not outlive. A kick ahead of it would
    // start that sender first and leave the tick with somebody else's work to report, which is nothing.
    if !matches!(cli.command, Some(Command::Tick { sub: TickCmd::Run })) {
        resume_dispatch(&store);
    }

    // The Viewer's half of that same kick (`AMB-D-884`). A carrier that died between reading the backlog
    // out and placing it leaves a queue, and only a write sets one off — so without this the rows wait for
    // whenever somebody next writes, which on a device being read from is never.
    //
    // **The `viewer` group stands down from it**, the way the flush and the tick stand down above and for
    // the same reason: those roads are the person attending to this by hand, and a carrier started behind
    // their back would answer the press they came to make with "somebody else has the turn".
    if !matches!(cli.command, Some(Command::Viewer { .. })) {
        resume_the_viewer(&store);
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
            // The address is the manifest's, and a manifest that named none for this machine leaves
            // the line without one rather than with a page guessed at here (`AMB-D-849`). The
            // command to run is the half that is true either way.
            match rel.update_url() {
                Some(url) => eprintln!(
                    "⚠ A newer Amenbo ({}) is available (this build is {}). Run `{} update` to install it, or see {url}",
                    rel.version,
                    agent::VERSION,
                    Paths::command_name(),
                ),
                None => eprintln!(
                    "⚠ A newer Amenbo ({}) is available (this build is {}). Run `{} update` to install it.",
                    rel.version,
                    agent::VERSION,
                    Paths::command_name(),
                ),
            }
        }
    }

    // A one-time nudge for a Linux user who migrated off the old `.deb`/`.rpm` but still has the retired
    // `/usr/bin` copy. Self-clearing and no-op off Linux, so it sits harmlessly beside the version advisory.
    advise_linux_system_orphan();

    // Where the plugins went (`AMB-D-884`). A person who installed one and finds it gone is owed the
    // sentence that says it is part of Amenbo now, and the migration that took them in left the account
    // for both surfaces to read. Said once per surface — the app owes its own — and on stderr beside the
    // other advisories. A `--json` caller is a machine and is told nothing, so the person's turn is still
    // theirs when they next type something.
    if !flags.json && !flags.quiet {
        announce_the_handover(&store);
    }

    // If this invocation is inside a bound folder, whatever that folder still carries on disk — an outdated
    // managed block, a legacy `.amenbo` — is brought up to the current form here: resolving is what repairs
    // it (`binding::resolve_upward`). Run for every actor, once: it is the AI that reads stale guidance, but
    // the only chance to fix it is "Amenbo ran in that folder", and who ran it is beside the point. Outside a
    // binding (an `AMENBO_HOME` sandbox, say) nothing happens.
    if let Ok(cwd) = std::env::current_dir() {
        amenbo_core::binding::resolve_upward(&store, &cwd);
    }

    // Decide this surface's reach once, here — from the facet and the binding. An AI (`--actor ai`) is confined to the project
    // the `.amenbo` points at; a human sees the whole device
    // (the overview is the human's place). Reach is drawn from the binding alone — `--project` never widens
    // it: the resolution below is checked against exactly this reach, and naming an outside project is
    // rejected as out_of_reach. With no binding, an AI's reach is empty, so refuse here. A CWD with neither a
    // pointer nor `AMENBO_HOME` was already stopped by the exec guard above, which leaves the cases "opened
    // via env" and "the pointer names no project, or names one that is gone" — in each, confinement has
    // nothing to bite on, and falling back to All would reduce the binding to decoration. init (which creates
    // the binding) and migrate/unbind (which do not surface store content) are handled before this point and
    // never arrive here; that is the shape of the exceptions.
    let mut store = match flags.actor {
        Some(ActorKind::Ai) => {
            let reach = Reach::for_ai(binding_project(&store)).map_err(CliError::from)?;
            store.with_reach(reach)
        }
        // A human sees the device, which is the store's own reach — nothing to narrow. So is a command
        // that declared no facet, and that arm is not a human default in disguise: `uses_facet` let
        // through only the commands that surface no store content (version / agent / whoami / config /
        // bind …), so the reach they keep is never consulted.
        Some(ActorKind::Human) | None => store,
    };

    // Read-only integrity check at startup. Problems are reported as warnings and never repaired
    // automatically (repair is the explicit `amenbo doctor --fix`); the check itself has no side effects.
    // Turn it off with `amenbo config set startup_integrity_check false`.
    //
    // Count only after the reach is fixed. The tally taken above (`startup_check`) knows nothing of reach —
    // it looks at the whole device — so reading it out inside a closed reach would tell an AI the number of
    // issues that `doctor` will not show it, and send it looking for something it cannot see. Only when we
    // know something is there do we count again, within the reach.
    if startup_check.as_ref().is_some_and(|h| h.has_warnings()) {
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

    // The two setups Amenbo offers, in the order their questions are put. `hooks` is outside both: its argv
    // already answered the lint's question, and the harness path can record consent, which `hooks status`
    // promises not to do. So is the tick group: `tick`'s own argv already answers, and the face the
    // scheduler calls is nobody to put a question to.
    if !matches!(cli.command, Some(Command::Hooks { .. }) | Some(Command::Tick { .. })) {
        let lint_asked = lint_hook_setup(&mut store, flags);
        agent_hook_setup(&store, flags, lint_asked);
    }
    // The hourly tick settles its own two states here, for the same reason and with the same exception:
    // `tick`'s argv already says what this would say, and `tick status` promises to only read.
    if !matches!(cli.command, Some(Command::Tick { .. })) {
        tick_reconcile(&mut store);
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
            // Leave the mark in the pane this was run in, before answering: whether the first word
            // reached the AI working here is settled by the fact that it ran this command, and every
            // route through `agent` is that fact (`AMB-D-805`). Outside a pane, and where the mark
            // cannot be written, this does nothing and says nothing — `agent` is a read of this build
            // and stays one.
            amenbo_core::session::briefed();
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
            // Inside a step of a run the entry is the step's own: the whole entry is about working a
            // mailbox, and the step's work came with the text it was started on. `--full` still answers
            // whole, for whoever asked for every command on purpose.
            if !full && amenbo_core::env::automation_step().is_some() {
                print_json(&agent::build_step());
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
                // The same rule where it runs through a cycle rather than around one. `commit` is
                // written for whoever is about to send text out of this store, which is everybody,
                // and two of its steps — anchoring a commit's SHA, wiring git's hook slots — are for
                // the half that has git. Dropping the cycle whole would take the lint advice with
                // them, and that advice holds over a file or piped text with no git anywhere near it.
                amenbo_core::agent::drop_git_only_steps(&mut spec);
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
                    "release_build": amenbo_core::build_stamp::is_release_build(),
                    // The update endpoint compiled into this build. It is injected at build time and has
                    // no default, so what a binary answers here is the one proof that the value reached
                    // the artifact — the release workflow's per-OS gate reads exactly this field, and it
                    // has to, because a build cannot check its own injection while it is still building
                    // (`AMB-D-849`).
                    "latest_json_url": amenbo_core::update_check::LATEST_JSON_URL,
                    "format_version": vs.format_version,
                    "max_supported_format": vs.max_supported_format,
                    "latest_version": vs.latest_version,
                    "update_available": vs.update_available,
                }));
            } else {
                let suffix = if channel == "amenbo" { String::new() } else { format!(" ({channel})") };
                human(flags, format!("Amenbo {}{}", agent::VERSION, suffix));
                human(flags, format!("format: store v{} (this build opens up to v{})", vs.format_version, vs.max_supported_format));
                if let Some(line) = unstamped_line() {
                    human(flags, line);
                }
                if let Some(latest) = vs.latest_version.as_deref() {
                    human(flags, format!("latest published: {latest}"));
                }
                if vs.update_available {
                    human(flags, format!(
                        "update available — a newer Amenbo ({}) is out. Run `{} update` to get the installer.",
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
        Command::Talk { .. } => {
            unreachable!("handled before open")
        }
        Command::GithookPreCommit | Command::GithookCommitMsg { .. } => {
            unreachable!("handled before open")
        }
        Command::NotifySender { .. } | Command::ViewerCarrier { .. } => {
            unreachable!("handled before open")
        }
        // Notifications: the device's shelf and what this project reports through it (`AMB-D-885`). The
        // writes ride the dispatch seam like every other, so a target raised here is on the shelf before
        // the next write goes looking for it.
        Command::Notify { sub } => {
            return cmd::outbox::with_dispatch(&mut store, |store| {
                cmd::notify::notify(store, flags, sub)
            })
        }
        // The Viewer: the server this store is read from on a phone, and which phone may read it
        // (`AMB-D-884`). Every one of these is the device's — one account, one key, one read code — so
        // none of them asks which project it is about.
        Command::Viewer { sub } => return cmd::viewer::viewer(&mut store, flags, sub),
        // A task's own checkout, cut and folded (`AMB-D-881`). Only git moves; the store is read to see
        // whether this is the repository the task is worked in.
        Command::Worktree { sub } => return cmd::worktree::worktree(&store, flags, sub),
        Command::Config { sub } => return config(&mut store, flags, sub),
        Command::Skin { sub } => return crate::cmd::skin::skin(&mut store, flags, sub),
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
                    // the ref and the title say neither. Each side speaks its own vocabulary — both
                    // state their status and what they are filed under, and a task its priority on top
                    // of that — so the same line reads as the record's own without naming which of the
                    // two it is. Nothing is printed when the read that fills this in came
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
                        // While a decision's writing is unfinished the status has nothing to tell
                        // apart — it is `decided` from the moment it is saved (`AMB-D-918`) — so the
                        // row says the draft in its place, the same substitution `decision list` makes.
                        let state = if standing.draft { "draft" } else { standing.status.as_str() };
                        human(flags, format!("      {state}{pri}{filed}"));
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

                // Sweep attachment rows whose target is gone. **Before the blob sweep**: an orphan still
                // holds its `blob_hash` in the GC root set, so the bytes are not collectible until the row
                // is, and running these the other way round would leave the file for the next `--fix`.
                match store.sweep_orphan_attachments() {
                    Ok(0) => human(flags, "doctor --fix: No cleanup targets (orphaned attachments)."),
                    Ok(n) => human(flags, format!("\u{2713} Swept {n} orphaned attachment row(s).")),
                    Err(e) => human(flags, format!("\u{26a0} Could not sweep orphaned attachments: {e}")),
                }

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
        // Automations: the library of prompts, and the pictures built out of them. Only the building
        // side, and nothing here refuses an unfinished automation — the launch check is where a person
        // is let down by one.
        Command::Automation { sub } => {
            return with_dispatch(&mut store, |s| cmd::automation::automation(s, flags, sub))
        }
        Command::Comment { sub } => return with_dispatch(&mut store, |s| comment(s, flags, sub)),
        Command::Decision { sub } => return with_dispatch(&mut store, |s| decision(s, flags, sub)),
        Command::Attach { sub } => return attach(&mut store, flags, sub),
        Command::Export { out } => return export(&store, flags, out),
        Command::Backup { path } => return run_backup(flags, path),
        Command::HardErase { sub } => return hard_erase(&mut store, flags, sub),
        Command::Restore { .. } => {
            unreachable!("handled before open")
        }
        Command::Hooks { sub } => return hooks_cmd(&mut store, flags, sub),
        Command::Tick { sub: TickCmd::Run } => return tick_run_cmd(&store, flags),
        Command::Tick { sub } => return tick_cmd(&mut store, flags, sub),
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
    /// **Every command a line can name is one the step table holds, or one no hand types** — so none is
    /// refused inside a step only for being spelled differently there, and the table and the parser
    /// cannot drift apart.
    #[test]
    fn every_command_the_parser_names_is_in_the_step_table() {
        fn leaves(cmd: &clap::Command, above: &[&str], out: &mut Vec<String>) {
            for sub in cmd.get_subcommands().filter(|sub| sub.get_name() != "help") {
                let mut path = above.to_vec();
                path.push(sub.get_name());
                match sub.has_subcommands() {
                    true => leaves(sub, &path, out),
                    false => out.push(path.join(" ")),
                }
            }
        }
        let mut named = Vec::new();
        leaves(&Cli::command(), &[], &mut named);
        let missing: Vec<&String> = named
            .iter()
            .filter(|name| !core_agent::Cmd::ALL.iter().any(|cmd| cmd.name() == name.as_str()))
            .filter(|name| !LAUNCHED_NOT_TYPED.contains(&name.as_str()))
            .collect();
        assert!(missing.is_empty(), "named by the parser and missing from the table: {missing:?}");
        for launched in LAUNCHED_NOT_TYPED {
            assert!(named.iter().any(|name| name == launched), "{launched} is no command the parser names");
        }
    }

    /// **Inside a step, the table decides; outside one, nothing is asked** — and what moves the task
    /// is refused with the sentence that says the run moves it.
    #[test]
    fn inside_a_step_the_table_decides_what_is_typed() {
        for reaches in ["automation step-done", "task show", "comment add", "task add", "amenbo", "githook-pre-commit"] {
            assert!(in_a_step(reaches, true).is_ok(), "{reaches}");
        }
        for (refused, says) in [
            ("task done", "moves its task's status"),
            ("task status", "moves its task's status"),
            ("task assign", "moves its task's status"),
            ("automation start", "is typed from outside one"),
            ("decision add", "is typed from outside one"),
            ("no such command", "is typed from outside one"),
        ] {
            let err = in_a_step(refused, true).expect_err(refused);
            assert_eq!(err.code, "automation_outside_only", "{refused}");
            assert!(err.message.contains(says), "{refused}: {}", err.message);
        }
        assert!(in_a_step("task done", false).is_ok(), "outside a step nothing is asked");
    }

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

    /// The surface layer decides nothing by the folder it was typed in: it reaches the pane that named it
    /// in the environment, and no store. So it declares no facet, and is outside both guards
    /// that ask what this directory is — including the nested-worktree one, since a throwaway checkout is
    /// exactly where an agent works and saying what it is doing there is not driving a backlog with it.
    #[test]
    fn the_surface_layer_is_judged_by_the_pane_that_named_it_and_not_by_this_folder() {
        let talk = Some(Command::Talk { sub: None });
        assert!(!uses_facet(&talk), "there is no store content to draw a reach over");
        assert!(
            nested_guard_target(&talk).is_none(),
            "a worktree is a place to say what is happening in it",
        );
        assert!(
            pointer_store_guard_target(&talk).is_none(),
            "and whose pointer sits above it decides nothing about the pane",
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

    /// Every sub-command must be registered in `agent --json`, or an AI never learns it exists. Amenbo is a
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
        // Reads that surface store content draw the reach, so they use the facet too.
        assert!(uses_facet(&None)); // discover: this project's work
        assert!(uses_facet(&Some(Command::Status { scope: "today".to_string() })));
        assert!(uses_facet(&Some(Command::Task { sub: TaskCmd::List { project: None, filter: None, sort: "order".to_string(), limit: None, offset: None } })));
        assert!(uses_facet(&Some(Command::Task { sub: TaskCmd::Show { id: "x".to_string() } })));
        assert!(uses_facet(&Some(Command::Comment { sub: CommentCmd::List { task: "x".to_string(), limit: None, offset: None } })));
        assert!(uses_facet(&Some(Command::Doctor { fix: false })));
        // Writes stamp it.
        assert!(uses_facet(&Some(Command::Task { sub: TaskCmd::Status { id: "x".to_string(), status: "in_progress".to_string() } })));
        assert!(uses_facet(&Some(Command::Task { sub: TaskCmd::Done { id: "x".to_string(), report: None } })));
        assert!(uses_facet(&Some(Command::Comment { sub: CommentCmd::Add { task: "x".to_string(), text: "t".to_string() } })));
        assert!(uses_facet(&Some(Command::Doctor { fix: true })));
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
        // And the surface layer, which opens no store at all: it speaks to the pane it was launched in,
        // and an agent is put to work in checkouts nobody bound (`AMB-D-749`).
        assert!(!requires_pointer(&Some(Command::Talk { sub: None })));
        // Everything else opens the store and therefore needs a pointer. `agent` is the AI's entry point, so
        // it gets no exemption.
        assert!(requires_pointer(&None)); // discover
        assert!(requires_pointer(&Some(Command::Agent { command: None, full: false })));
        assert!(requires_pointer(&Some(Command::Whoami)));
        assert!(requires_pointer(&Some(Command::Status { scope: "today".to_string() })));
        assert!(requires_pointer(&Some(Command::Doctor { fix: false })));
    }
}
