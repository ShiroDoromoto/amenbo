//! `hooks` and `agent-hook`, and the two setups amenbo offers at startup — the lint hook, and
//! the line that has an AI tool run `agent` at every session start.

use std::io::IsTerminal;

use serde_json::json;

use amenbo_core::Store;
use amenbo_core::config::Paths;
use amenbo_core::model::ActorKind;

use crate::cli::*;
use crate::cmd::place::binding_project;
use crate::output::{human, print_json, set_setup_report, CliError, Flags};

/// `amenbo agent-hook snippet <tool>` — hand over the request that has one AI tool wired to run
/// `amenbo agent` at session start (`AMB-D-440`). It hands text over and writes nothing: the provider's
/// settings file stays the user's, here as everywhere in this feature.
///
/// **This command's stdout belongs to the text**, the way `plugin run`'s belongs to the plugin. What it
/// prints is meant to be consumed — piped to a clipboard, handed to the AI the reader works with — so a
/// courtesy line printed alongside it would ride into the request as though it were part of it. Where
/// the text is going, and that amenbo did not wire anything itself, is said on stderr; under `--json`
/// stdout is the document and both the request and the configuration ride inside it.
///
/// `--copy` is that pipe, for the hand that would rather not type it — and it prints the request on
/// stderr as it goes, because a copy nobody read is a text taken on trust: the reader is about to hand
/// it to something that edits their files, and the one moment to see what it asks for is this one.
pub(crate) fn agent_hook_snippet_cmd(flags: &Flags, tool: &str, copy: bool) -> Result<i32, CliError> {
    use amenbo_core::harness;

    // clap's parser for this argument is built from the catalog itself, so a name nobody lists was
    // already refused with the whole list — by the time it is here, it is a row.
    let harness = harness::find(tool).expect("the argument's parser accepts only the catalog's own ids");
    let cmd = Paths::command_name();
    let request = harness::request(harness, cmd);

    if copy {
        copy_to_clipboard(&request)?;
    }
    if flags.json {
        print_json(&json!({
            "ok": true,
            "action": "agent-hook.snippet",
            "tool": harness.id,
            "label": harness.label,
            "paste_into": harness.paste_into,
            "copied": copy,
            "request": request,
            // The payload on its own, for a caller standing in for the AI that carries the request out.
            // Reading it back out of the request's prose would make that caller the judge of prose it
            // does not own.
            "configuration": harness::configuration(harness, cmd),
        }));
        return Ok(0);
    }
    if !flags.quiet {
        eprintln!(
            "Give this to the AI you work with in this folder — it edits {}, and amenbo writes nothing.\n\
             Once that lands, this folder's AI runs `{cmd} agent --json` at the start of every session.",
            harness.paste_into
        );
        // On the clipboard route stdout stays empty, so this is the only place the text is shown — and
        // it is shown either way, since what a reader is about to hand over is what they should read.
        if copy {
            eprintln!("\n{request}\n");
        }
    }
    if !copy {
        println!("{request}");
    }
    Ok(0)
}

/// `amenbo agent-hook answer <yes|no>` — write down what a person answered about starting this folder's
/// AI on amenbo (`AMB-D-440`).
///
/// **The answer only exists if someone is asked, and amenbo asks no one here.** The question is put on a
/// terminal a person is watching; on the `--json` face it is carried as a report for the AI reading it,
/// which is what puts the question to the human. Without a way back in, that face's question could never
/// be closed and the report would stand for ever — so this is the door: the AI asks, the person answers,
/// and this records the answer as theirs.
///
/// **It records, and that is all it does.** Where the lint's `hooks install` consents *by* writing the
/// hooks, nothing here reads or writes a settings file: a `yes` is an answer, not a wiring, and the edit
/// is still the person's to make. So a `yes` is followed by the line that hands over the text, rather
/// than by anything having changed on disk.
///
/// The row is the project's, not the folder's, and replaces whatever it said before — a person may say
/// yes today and no tomorrow. What carries over is whether the one re-ask has been spent
/// ([`amenbo_core::harness::Consent::asked_again`]): that is a memory of what amenbo has already put to
/// them, which an answer to the question is no reason to hand back.
pub(crate) fn agent_hook_answer_cmd(store: &Store, flags: &Flags, yes: bool) -> Result<i32, CliError> {
    use amenbo_core::harness::Consent;

    let cmd = Paths::command_name();
    let Some(project) = binding_project(store) else {
        return Err(CliError {
            code: "not_found",
            message: "this folder is not bound to a project, so there is nowhere to record the answer"
                .to_string(),
            hint: Some(format!("bind it first: `{cmd} bind --project <name or id>`")),
            exit: 1,
        });
    };
    // Keep the re-ask's memory: it says what amenbo has already asked, which is a different fact from
    // what was answered.
    let spent = store.harness_consent(project).unwrap_or(None).is_some_and(|had| had.asked_again);
    let answer = Consent { allowed: yes, asked_again: spent };
    store.set_harness_consent(project, answer).map_err(CliError::from)?;

    if flags.json {
        print_json(&json!({
            "ok": true,
            "action": "agent-hook.answer",
            "allowed": answer.allowed,
            // What is left to do after a yes: amenbo writes no settings file, so the wiring is still owed.
            "next": yes.then(|| format!("{cmd} agent-hook snippet <tool>")),
        }));
        return Ok(0);
    }
    if yes {
        human(flags, "Recorded: yes — this project may have its AI started on amenbo.");
        human(flags, format!("One step is left: `{cmd} agent-hook snippet <tool>` prints the text to give your AI."));
    } else {
        human(flags, "Recorded: no — we will not ask about this again.");
        human(flags, format!("Nothing is forbidden by it: `{cmd} agent-hook snippet <tool>` hands the text over whenever you want it."));
    }
    Ok(0)
}

/// Put `text` on this machine's clipboard, through whatever tool this platform hands it over with.
///
/// On a Linux desktop that is Wayland's or X11's tool and nothing on the machine reliably says which, so
/// the answer is whichever one is installed and runs — tried in that order. A machine with none of them
/// (a container, an ssh session) is not a failure to work around: it is refused, naming the pipe, which
/// is the same text through a route that exists everywhere.
fn copy_to_clipboard(text: &str) -> Result<(), CliError> {
    use std::io::Write;

    #[cfg(target_os = "macos")]
    let tools: &[(&str, &[&str])] = &[("pbcopy", &[])];
    #[cfg(target_os = "windows")]
    let tools: &[(&str, &[&str])] = &[("clip", &[])];
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let tools: &[(&str, &[&str])] =
        &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"]), ("xsel", &["--clipboard", "--input"])];

    for (program, args) in tools {
        let spawned = amenbo_core::sys::command(program)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .spawn();
        let Ok(mut child) = spawned else { continue };
        // A clipboard tool that took the text and then failed is not one to fall through from: it was
        // there, and reporting the pipe would be answering a question nobody asked.
        let written = child.stdin.take().is_some_and(|mut pipe| pipe.write_all(text.as_bytes()).is_ok());
        let ended = child.wait().map_err(|e| CliError {
            code: "io_error",
            message: format!("could not hand the text to {program}: {e}"),
            hint: None,
            exit: 1,
        })?;
        if written && ended.success() {
            return Ok(());
        }
        return Err(CliError {
            code: "io_error",
            message: format!("{program} did not take the text"),
            hint: Some(format!("pipe it instead: `{} agent-hook snippet <tool> | {program}`", Paths::command_name())),
            exit: 1,
        });
    }
    let pipe = tools.first().map(|(program, _)| *program).unwrap_or("pbcopy");
    Err(CliError {
        code: "io_error",
        message: "no clipboard tool on this machine".to_string(),
        hint: Some(format!(
            "print it and pipe it yourself: `{} agent-hook snippet <tool> | {pipe}`",
            Paths::command_name()
        )),
        exit: 1,
    })
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
pub(crate) fn hooks_cmd(store: &mut Store, flags: &Flags, sub: HooksCmd) -> Result<i32, CliError> {
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
pub(crate) fn lint_hook_setup(store: &mut Store, flags: &Flags) -> bool {
    use amenbo_core::hooks;

    let Some(project) = binding_project(store) else { return false };
    let Ok(cwd) = std::env::current_dir() else { return false };
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
    // Whether a question was actually put here — the only branch that returns an answer is the one that
    // asked for it, which is what the next setup needs to know to hold its own question back.
    answered.is_some()
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
        set_setup_report(
            "unwired",
            json!(notice
                .unwired
                .iter()
                .map(|slot| json!({
                    "hook": slot.name(),
                    "fix": format!("{cmd} hooks install"),
                }))
                .collect::<Vec<_>>()),
        );
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

/// The session-start hook's two duties, run before the command the user came for (`AMB-D-440`): the one
/// question, where it can be answered, and the standing report of what is not wired.
///
/// **amenbo writes nothing here.** Every other setup path in this file ends in amenbo writing a file; this
/// one ends in text a person pastes, which is why the report cannot be ended by an answer — only by the
/// paste landing ([`amenbo_core::harness::setup_notice`]).
///
/// `lint_asked` holds the one-question-at-a-time rule: two prompts in one run, over two different things,
/// is how a reader ends up answering neither on purpose. The lint's question goes first because it is the
/// older one and it has a `no` that closes it for good; this one is put on the next run, having recorded
/// nothing, which is exactly what the unanswered state is for.
pub(crate) fn agent_hook_setup(store: &Store, flags: &Flags, lint_asked: bool) {
    use amenbo_core::harness::{self, Consent, ConsentAction, ConsentContext};

    let Some(project) = binding_project(store) else { return };
    let Ok(cwd) = std::env::current_dir() else { return };
    let cmd = Paths::command_name();
    let found = harness::probe(&cwd, cmd);
    let recorded = store.harness_consent(project).unwrap_or(None);
    let can_ask = !flags.json
        && flags.actor != Some(ActorKind::Ai)
        && std::io::stdin().is_terminal()
        && !lint_asked;
    let wired = found.iter().any(harness::Wiring::wired);

    let action = harness::reconcile(&ConsentContext { consent: recorded, wired, can_ask });
    let answered = match action {
        ConsentAction::Nothing => None,
        // Wired before anyone asked: the hand that did it answered, so write that down rather than put a
        // question whose answer is already on disk.
        ConsentAction::AdoptWired => Some(Consent::answered(true)),
        ConsentAction::Ask => offer_agent_hook(&found, cmd, false),
        ConsentAction::AskAgain => offer_agent_hook(&found, cmd, true),
    };
    if let Some(answer) = answered {
        // Best-effort, like the lint's note: the row decides only whether amenbo offers again, and failing
        // the command the user actually ran over it would undo nothing.
        let _ = store.set_harness_consent(project, answer);
    }
    // A run that just put the question has said all of this at length, and repeating it underneath as a
    // warning reads as nagging about something that was handed over a line ago. The report is a standing
    // signal, so the next run carries it — nothing on disk changed here either way.
    if !matches!(action, ConsentAction::Ask | ConsentAction::AskAgain) {
        report_unwired_harnesses(flags, &found, answered.or(recorded), cmd);
    }
}

/// Put the question about the session-start hook, and hand over what a yes asked for. `again` words the
/// one re-ask, whose occasion is not a fresh reader but a wiring that has gone missing.
///
/// A yes is answered with the text, or with the line that prints it: what a reader can do with a yes is
/// hand that text to an AI of theirs, so an offer that recorded consent and said nothing else would have
/// taken an answer and given nothing back. A no says how to come back, because it closes the question and
/// not the door.
fn offer_agent_hook(
    found: &[amenbo_core::harness::Wiring],
    cmd: &str,
    again: bool,
) -> Option<amenbo_core::harness::Consent> {
    use amenbo_core::harness::Consent;

    let named: Vec<&amenbo_core::harness::Wiring> =
        found.iter().filter(|one| one.traced && !one.wired()).collect();
    let mut prompt = String::new();
    if again {
        // The occasion is a standing yes with nothing wired, and amenbo cannot tell an edit that was never
        // asked for from one a clone did not carry — so the wording says the only thing it knows, which is
        // that the setting is not there. Claiming either story would be wrong half the time. It leads, and
        // what the text does still follows it: the first asking was a terminal the reader no longer has.
        prompt.push_str(
            "You said yes to this before, and the setting is still not in place — a clone may not have carried it.\n",
        );
    }
    // What a yes buys, in the reader's terms: the text, the hand that makes the edit — theirs, not
    // amenbo's — and what their AI does differently afterwards. Named where the folder settles on one
    // tool, and "your tool" where it does not, so the sentence is whole either way.
    let tool = match named.as_slice() {
        [one] => one.label,
        _ => "your tool",
    };
    prompt.push_str(&format!(
        "We hand you a text. Give it to your AI to edit {tool}'s settings, and your AI reads how to work with amenbo at the start of every session and records the work as tasks without being asked.\n",
    ));
    prompt.push_str(if again {
        "Want the text again? We will not ask a third time."
    } else {
        "We ask this once. Want the text?"
    });

    let yes = ask_yes_no(&prompt)?;
    match (yes, named.as_slice()) {
        // One tool, and the answer is yes: the text itself, which is the whole of what was asked for.
        (true, [one]) => match amenbo_core::harness::find(one.id) {
            Some(harness) => eprintln!(
                "\nGive this to your AI — it edits {}:\n\n{}\n",
                harness.paste_into,
                amenbo_core::harness::request(harness, cmd)
            ),
            None => eprintln!("Get it with: {cmd} agent-hook snippet {}", one.id),
        },
        // No tool named, or several: which one is the reader's to say, and the command lists them.
        (true, _) => eprintln!("Get the text for your tool: {cmd} agent-hook snippet <tool>"),
        (false, _) => eprintln!(
            "We will not ask again — `{cmd} agent-hook snippet <tool>` prints the text whenever you want it."
        ),
    }
    Some(if again { Consent::answered_again(yes) } else { Consent::answered(yes) })
}

/// Report that this folder's AI is not being started on amenbo, on every response until it is — the
/// standing signal, where [`offer_agent_hook`] is a one-time question. Under `--json` it lands as a field
/// on the answer the caller already parses, which is the one surface an AI is sure to read: it can then
/// hand the human the text, which is the only way this setup ever finishes, since amenbo will not write
/// the file itself.
///
/// **The two faces do not report the same set, because they are told which provider by different things**
/// (`AMB-D-440`: the trace and the self-declaration). A person is shown only what amenbo can point at — a
/// provider whose own directory is in this folder, unwired — because a standing warning about a tool there
/// is no sign of is a line they cannot act on, arriving on every command they run. The `--json` face
/// carries the catalog as well, since the reader there is the harness itself and knows which one it is even
/// where the folder shows nothing. The one-time question is put in either case: it is asked once per
/// project, and being asked once is how the feature is ever discovered.
///
/// **So the two are silenced by different things too**, and each asks its own
/// ([`amenbo_core::harness::setup_incomplete`] here, [`amenbo_core::harness::setup_notice`] for the
/// person). A folder wired for one tool and traced by no other has nothing left to tell a person — while
/// the next AI opened in it is another tool entirely, still unwired, and the report is where it finds that
/// out.
///
/// **A run the MCP server started is told none of it** (`AMB-D-683`). What the report asks for is a
/// session-start hook, and a hook is a shell command a provider runs when it opens a folder: the caller
/// on the other side of MCP opens no folder and has no shell, so it is being asked for something it
/// cannot do — on every call, the report being a standing one. The duty the hook would carry is already
/// done there by the server's own `agent` tool (`AMB-D-667`).
///
/// It is read off the road the call arrived by ([`amenbo_core::env::mcp_dirs`], set by `amenbo mcp` on
/// the children a tool call re-runs) and not off the folder. The folder is what the GUI has to go on,
/// having no caller to ask (`AMB-D-680`) — and reading it here would silence the *shell* AI working in
/// that same folder, which is the one reader that can act on the report.
///
/// A warning either way: the command the user ran succeeds regardless, and text goes to stderr so stdout
/// stays pipeable.
fn report_unwired_harnesses(
    flags: &Flags,
    found: &[amenbo_core::harness::Wiring],
    consent: Option<amenbo_core::harness::Consent>,
    cmd: &str,
) {
    use amenbo_core::harness;

    if amenbo_core::env::mcp_dirs().is_some() {
        return;
    }
    if flags.json {
        if !harness::setup_incomplete(found, consent) {
            return;
        }
        set_setup_report(
            "agent_hook",
            json!({
                // What amenbo can point at: the shortlist a face built for a person is held to, carried
                // here too so a reader can tell the providers this folder shows from the rest of the row.
                "unwired": found.iter().filter(|one| one.traced && !one.wired()).map(|one| json!({
                    "tool": one.id,
                    "label": one.label,
                    "fix": format!("{cmd} agent-hook snippet {}", one.id),
                })).collect::<Vec<_>>(),
                "any_wired": found.iter().any(harness::Wiring::wired),
                // Every row of the catalog with what this folder says about it, because a harness that left
                // no trace in the folder is still the one reading this and can name itself (`AMB-D-440`).
                // Its own row is the answer it came for — whether *it* is wired — which no set amenbo
                // picked out by trace can give it.
                "tools": found.iter().map(|one| json!({
                    "tool": one.id,
                    "label": one.label,
                    "wired": one.wired(),
                    "wired_at": one.wired_at,
                    "traced": one.traced,
                    // Only where there is something to fix: on a wired row it would read as an edit still
                    // owed.
                    "fix": (!one.wired()).then(|| format!("{cmd} agent-hook snippet {}", one.id)),
                })).collect::<Vec<_>>(),
                // While nobody has answered, the reader here is the one who can put the question to a
                // person — amenbo cannot, on this face. Naming the way back is what lets that answer land;
                // once there is one on record, there is no question left to carry.
                "record_answer": consent.is_none().then(|| format!("{cmd} agent-hook answer <yes|no>")),
            }),
        );
        return;
    }
    let Some(notice) = harness::setup_notice(found, consent) else { return };
    if !flags.quiet {
        match notice.unwired.as_slice() {
            // Nothing to point at: the folder shows no provider of its own, so there is no line here a
            // person could act on. The question already offered them the feature, and the `--json` face
            // above still carries it for the reader that can name itself.
            [] => {}
            [one] => {
                eprintln!("⚠ One step is left before your AI keeps this work in amenbo: {}'s settings here do not open a session with `{cmd} agent`.", one.label);
                eprintln!("  The text to give your AI: {cmd} agent-hook snippet {}", one.id);
            }
            several => {
                let labels = several.iter().map(|one| one.label).collect::<Vec<_>>().join(", ");
                eprintln!("⚠ One step is left before your AI keeps this work in amenbo: the settings of {labels} here do not open a session with `{cmd} agent`.");
                eprintln!("  The text to give your AI, one tool at a time: {cmd} agent-hook snippet <tool>");
            }
        }
    }
}
