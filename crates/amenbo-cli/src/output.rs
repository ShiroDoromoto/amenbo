//! The two output layers (human-facing text / machine-readable `--json`), plus the shared error and
//! confirmation handling.

use std::io::{IsTerminal, Write};

use amenbo_core::config::Paths;
use amenbo_core::model::ActorKind;
use serde::Serialize;
use serde_json::json;

#[derive(Clone, Copy)]
pub struct Flags {
    pub json: bool,
    pub yes: bool,
    pub quiet: bool,
    /// The caller's `--no-color`. Read through [`Flags::color`], never on its own: it is one of the three
    /// things that turn escapes off, and a site that asks only this one gets the other two wrong.
    pub no_color: bool,
    /// The facet of this invocation (human / ai), from `--actor` and nowhere else. `None` is **not a
    /// default standing in for one**: it means this command declared no facet, which only a command that
    /// never uses one can do (`uses_facet` refuses the rest at the door). Read it through [`Flags::facet`]
    /// wherever the value is actually needed.
    pub actor: Option<amenbo_core::model::ActorKind>,
}

impl Flags {
    /// The facet to act as. Every caller is a path `uses_facet` declared as facet-using, so the door has
    /// already refused an undeclared one; this restates that same refusal where the value is taken, so a
    /// wrong line there fails loud instead of quietly stamping a facet nobody named.
    pub fn facet(&self) -> Result<amenbo_core::model::ActorKind, CliError> {
        self.actor.ok_or_else(CliError::facet_required)
    }

    /// May human output carry ANSI escapes? Three answers have to be no for it to be yes, and they are
    /// gathered here so no caller decides on one of them alone.
    ///
    /// - `--no-color`, the caller saying so outright;
    /// - `NO_COLOR` in the environment, the same thing said once for every tool the person runs;
    /// - stdout not being a terminal, which is what a redirect and a pipe both look like — escapes there
    ///   land in a file or in the next program's input, where nothing renders them.
    ///
    /// An escape is decoration and never content: everything a reader needs is in the characters, so this
    /// answering `false` costs nothing but the emphasis. `--json` is not consulted, because the machine
    /// face is built by [`print_json`] and never passes through here.
    pub fn color(&self) -> bool {
        !self.no_color && amenbo_core::env::no_color().is_none() && std::io::stdout().is_terminal()
    }
}

/// The **typed registry** of CLI-specific error codes (English, fixed — they are contract). Shaped like
/// core's [`amenbo_core::ErrorCode`]: `as_str()` is the **only** place the strings are defined, and
/// [`CliErrorCode::ALL`] is the single source of truth for the set. Core-originated codes (`e.code()`
/// via `From<Error>`) do not belong here; a parity test keeps the two sets disjoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CliErrorCode {
    ConfirmationRequired,
    NoPointer,
    InitPointerExists,
    InitAmbiguousOwners,
    ProjectDirBound,
    BindingNestedTree,
    NestedWorktree,
    PointerOtherStore,
    UnbindNoBinding,
    FacetRequired,
    AiGuardrail,
    /// The ledger can no longer say what changed since the cursor a carrier came back with, so the
    /// answer is this rather than the empty page that reads as "nothing changed" (`AMB-D-582`). Not a
    /// fault in the call: the way on is a fresh `sync snapshot` (`AMB-D-583`), which is why it is said
    /// in a code the caller can branch on.
    SyncGap,
    /// A road out of this store could not be walked — the snapshot could not be streamed.
    SyncError,
}

impl CliErrorCode {
    /// The stable strings the contract names (this match is the only place they are defined).
    /// It is exhaustive, so adding a variant breaks the build rather than silently omitting a code.
    pub const fn as_str(self) -> &'static str {
        match self {
            CliErrorCode::ConfirmationRequired => "confirmation_required",
            CliErrorCode::NoPointer => "no_pointer",
            CliErrorCode::InitPointerExists => "init_pointer_exists",
            CliErrorCode::InitAmbiguousOwners => "init_ambiguous_owners",
            CliErrorCode::ProjectDirBound => "project_dir_bound",
            CliErrorCode::BindingNestedTree => "binding_nested_tree",
            CliErrorCode::NestedWorktree => "nested_worktree",
            CliErrorCode::PointerOtherStore => "pointer_other_store",
            CliErrorCode::UnbindNoBinding => "unbind_no_binding",
            CliErrorCode::FacetRequired => "facet_required",
            CliErrorCode::AiGuardrail => "ai_guardrail",
            CliErrorCode::SyncGap => "sync_gap",
            CliErrorCode::SyncError => "sync_error",
        }
    }

    /// Every CLI-specific error code: the single source of truth for the parity test and the contract
    /// snapshot. Only the tests enumerate it in the binary today, but it is kept deliberately as the
    /// registry's truth source (the GUI's TS side is checked for parity against it too).
    #[allow(dead_code)]
    pub const ALL: &'static [CliErrorCode] = &[
        CliErrorCode::ConfirmationRequired,
        CliErrorCode::NoPointer,
        CliErrorCode::InitPointerExists,
        CliErrorCode::InitAmbiguousOwners,
        CliErrorCode::ProjectDirBound,
        CliErrorCode::BindingNestedTree,
        CliErrorCode::NestedWorktree,
        CliErrorCode::PointerOtherStore,
        CliErrorCode::UnbindNoBinding,
        CliErrorCode::FacetRequired,
        CliErrorCode::AiGuardrail,
        CliErrorCode::SyncGap,
        CliErrorCode::SyncError,
    ];
}

/// A CLI-level error (core-originated or CLI-specific). `code` comes from [`amenbo_core::ErrorCode`]
/// for the former and [`CliErrorCode`] for the latter — never a raw literal.
pub struct CliError {
    pub code: &'static str,
    pub message: String,
    pub hint: Option<String>,
    pub exit: i32,
}

/// The way out of [`CliError::no_pointer`] written for a caller reaching this folder over MCP, where
/// the two commands that place a pointer are refused (`AMB-D-666`) and there is no terminal to type
/// them in anyway.
///
/// Two things are said, and the order is the point. The folders the server *does* serve come first,
/// because the likeliest mistake is a caller that named the wrong one of a set it was given, and that
/// mistake is corrected in the next call without troubling anybody. Only then the road that opens this
/// folder, which is a person's: they add it to a project in Amenbo's own window. Naming the host's
/// settings instead would be advice for a different mistake — the set is chosen there, but nothing in
/// it reaches a folder Amenbo has never been told about.
fn hint_for_a_caller_with_no_terminal(dirs: &str) -> String {
    let listed = dirs
        .lines()
        .filter(|dir| !dir.trim().is_empty())
        .map(|dir| format!("\n  • {dir}"))
        .collect::<String>();
    format!(
        "The commands that would set a project up here — init and bind — are not served over MCP, so neither is a road you can take.\nThe folders this server works in:{listed}\nIf you meant one of those, name it in the next call. To work in *this* folder, ask the person to add it to a project in Amenbo's own window (the project's folders) — that is the only thing that opens it, and it takes no terminal."
    )
}

impl CliError {
    pub fn confirmation_required(what: &str) -> CliError {
        CliError {
            code: CliErrorCode::ConfirmationRequired.as_str(),
            message: format!("{what} is a destructive operation and requires confirmation."),
            hint: Some("To run non-interactively, pass --yes / -y.".to_string()),
            exit: 1,
        }
    }

    /// Execution guard: an operating command was run in a bare directory with no pointer (`.amenbo`)
    /// and no `AMENBO_HOME`. Rather than silently creating a store, point the user at `amenbo init`. The
    /// commands that create or name the pointer (init / join / bind) are the only exceptions.
    /// What we hand back is not an explanation of the machinery but a **fork in the road** — the two
    /// things they can do (init a new project / bind to an existing one) — plus, when
    /// `candidate_projects` is non-empty, the projects that actually exist on this machine. Enumerating
    /// them is a plain SQLite probe: it never opens a store, so it cannot forward-migrate one as a side
    /// effect.
    ///
    /// Both halves of that fork are a person's to walk, and over MCP there is no person at the other
    /// end: `init` and `bind` are refused there (`AMB-D-666`), so a caller handed this hint would be
    /// sent down two roads that are both closed. When the run came from a server
    /// ([`amenbo_core::env::mcp_dirs`])
    /// the hint is written for that reader instead — the folders it can actually reach, and the one
    /// thing that opens this one, which is a person adding it to a project in Amenbo's own window.
    pub fn no_pointer(candidate_projects: &[String]) -> CliError {
        let cmd = Paths::command_name();
        let hint = match amenbo_core::env::mcp_dirs() {
            Some(dirs) => hint_for_a_caller_with_no_terminal(&dirs),
            None => {
                let mut hint = format!(
                    "Pick one:\n  • Start a new project here: {cmd} init --name <you>\n  • Link an existing project: {cmd} bind --project <name or id>",
                );
                if !candidate_projects.is_empty() {
                    hint.push_str("\nExisting projects:");
                    for p in candidate_projects {
                        hint.push_str(&format!("\n  • {cmd} bind --project {p}"));
                    }
                }
                hint
            }
        };
        CliError {
            code: CliErrorCode::NoPointer.as_str(),
            message: "This folder is not linked to Amenbo (no .amenbo found).".to_string(),
            hint: Some(hint),
            exit: 1,
        }
    }

    /// init guard: this folder (or an ancestor) is already bound to another project by a `.amenbo`
    /// pointer. Creating a new project would overwrite that pointer unasked and the folder's real
    /// project would vanish from the CWD's view (a clobber), so we refuse by default — the same
    /// "respect what is already there" rule AGENTS.md follows. Re-point it with `bind`; start over for
    /// real with `--force`. We name the project (or `(no project)` when the pointer is unset).
    pub fn init_pointer_exists(dir: &str, project_id: &str, has_data: bool) -> CliError {
        let data_note = if has_data { " (it has data)" } else { "" };
        CliError {
            code: CliErrorCode::InitPointerExists.as_str(),
            message: format!(
                "This folder ({dir}) is already linked to Amenbo (project {project_id}){data_note}. init was refused because it would create a new project and overwrite the pointer."
            ),
            hint: Some({
                let cmd = Paths::command_name();
                format!(
                    "To use the existing project in this folder, run `{cmd} bind --project {project_id}`. If only the pointer is broken, recover with `{cmd} bind --project <name or id>`. To really create a brand-new project and overwrite, run `{cmd} init --force`."
                )
            }),
            exit: 1,
        }
    }

    /// init guard, ambiguous case: in a folder with no `.amenbo` pointer but with a managed block, the
    /// reverse lookup in the bindings registry named **several live projects** as owners of this folder.
    /// There is no way to tell whose lost pointer we should restore, so instead of silently picking one
    /// we stop and list the candidates (the plural sibling of `init_pointer_exists`). One live owner is
    /// recovered outright and zero means init just proceeds, so we only land here when it is genuinely
    /// ambiguous.
    pub fn init_ambiguous_owners(dir: &str, project_ids: &[String]) -> CliError {
        let cmd = Paths::command_name();
        let mut hint = String::from("Multiple projects claim this folder. Pick the one to recover:");
        for pid in project_ids {
            hint.push_str(&format!("\n  • {cmd} bind --project {pid}"));
        }
        hint.push_str(&format!("\nOr run `{cmd} init --force` to create a brand-new project here."));
        CliError {
            code: CliErrorCode::InitAmbiguousOwners.as_str(),
            message: format!(
                "This folder ({dir}) has no .amenbo pointer, but {} existing projects claim it, so init could not tell which lost pointer to recover.",
                project_ids.len()
            ),
            hint: Some(hint),
            exit: 1,
        }
    }

    /// `project add --dir` guard: the folder given is already linked to a project. Linking it to a
    /// brand-new one would overwrite that pointer unasked, and the folder's real project would drop out
    /// of view there — the same clobber `init` refuses. `project add` has no `--force`: the two things
    /// someone could mean are already commands of their own, so the hint names them.
    pub fn project_dir_bound(dir: &str, project_id: &str) -> CliError {
        let cmd = Paths::command_name();
        CliError {
            code: CliErrorCode::ProjectDirBound.as_str(),
            message: format!(
                "This folder ({dir}) is already linked to Amenbo (project {project_id}), so no new project was created."
            ),
            hint: Some(format!(
                "Pass a folder nothing is linked to, or re-point this one at another existing project with `{cmd} bind --project <name or id> --dir {dir}`."
            )),
            exit: 1,
        }
    }

    /// A folder recorded for this project is gone — moved, renamed, or restored somewhere else. Amenbo
    /// does not quietly go and work in whatever is left, so the read stops here (`binding_stale`, core's
    /// own code, since this is core's own refusal — what is added is the way out).
    ///
    /// The way out is a re-point and not a fresh `bind`, which is why the hint lines up the vanished
    /// bindings by **id**: binding a folder again records a new row, and a task filed at the old one is
    /// left naming a row nobody points at (`AMB-D-648`). `--rebind` keeps the id, so the folder is the
    /// one thing that changed. Listing them is also the only way a human learns the ids at all — a
    /// number nobody has seen is not one they can pass.
    pub fn binding_stale(project_id: i64, gone: &[amenbo_core::binding::BoundFolder]) -> CliError {
        let cmd = Paths::command_name();
        let first = gone.first().map(|b| b.dir.as_str()).unwrap_or_default();
        let mut hint = String::from(
            "The folder moved? Re-point that binding from its new home, so whatever points at it follows:",
        );
        for b in gone {
            hint.push_str(&format!("\n  • {cmd} bind --project {project_id} --rebind {}   ({})", b.id, b.dir));
        }
        hint.push_str(&format!(
            "\nBinding it again with `{cmd} bind --project {project_id}` instead records a new binding, leaving anything filed at the old one naming nothing."
        ));
        CliError {
            code: amenbo_core::ErrorCode::BindingStale.as_str(),
            message: format!("the linked project directory was not found: {first}"),
            hint: Some(hint),
            exit: 1,
        }
    }

    /// `bind --rebind <id>` named a binding that is not there — a mistyped number, or one already
    /// retired by an unbind (the ids are `AUTOINCREMENT`, so a retired number is never handed out
    /// again). Nothing is written: re-pointing is an operation on a row that exists, and inventing one
    /// for the number given would hand back an id the human never chose.
    pub fn binding_unknown(id: i64, project_id: i64, known: &[amenbo_core::binding::BoundFolder]) -> CliError {
        let cmd = Paths::command_name();
        let mine: Vec<&amenbo_core::binding::BoundFolder> =
            known.iter().filter(|b| b.project_id == project_id).collect();
        let hint = if mine.is_empty() {
            format!("This project has no folder bound, so there is none to re-point. Link one with `{cmd} bind --project {project_id}`.")
        } else {
            let mut h = String::from("The bindings this project has, and where each one points:");
            for b in mine {
                let mark = if b.exists() { "" } else { "   (missing)" };
                h.push_str(&format!("\n  • {cmd} bind --project {project_id} --rebind {}   {}{mark}", b.id, b.dir));
            }
            h
        };
        CliError {
            code: amenbo_core::ErrorCode::NotFound.as_str(),
            message: format!("no binding has id {id}, so there was nothing to re-point."),
            hint: Some(hint),
            exit: 1,
        }
    }

    /// Nested-binding guard: a new binding was requested in a **subdirectory** of a tree an ancestor's
    /// `.amenbo` already manages. A pointer there shadows the ancestor's binding (Amenbo run in that
    /// subdirectory would resolve to the subdirectory's store, not the parent's) and scatters
    /// `.amenbo`/AGENTS.md/CLAUDE.md through the source tree. Same "respect the existing tree" rule as
    /// `init`'s clobber guard. `--force` is the way through when binding a subdirectory separately is
    /// what you actually meant — which is `bind`'s alone, so `forceable` says whether the caller has one
    /// to offer: `project add` does not, and a hint naming a flag that command has never had would send
    /// the reader to look for it.
    pub fn binding_nested_tree(ancestor_dir: &str, forceable: bool) -> CliError {
        let cmd = Paths::command_name();
        CliError {
            code: CliErrorCode::BindingNestedTree.as_str(),
            message: format!(
                "This folder is already inside an Amenbo-managed tree (bound at {ancestor_dir}). Binding a subdirectory would shadow that pointer."
            ),
            hint: Some(if forceable {
                "Run bind from the managed root instead, or pass --force to intentionally bind this subdirectory.".to_string()
            } else {
                format!("Pass a folder outside that tree, or bind this subdirectory on purpose with `{cmd} bind --project <name or id> --dir <path> --force`.")
            }),
            exit: 1,
        }
    }

    /// Nested-worktree guard: the CWD sits in a git worktree cut **inside** an Amenbo-managed folder, so it
    /// inherited that folder's binding by the upward walk. The worktree is throwaway; the store it would
    /// write to is not — it lives in app-data, outside the worktree, and outlives its deletion. Refusing
    /// beats warning: a warning that can be ignored is prose that merely moved house. The way out is to run
    /// Amenbo in the project folder, never to `bind` this checkout — restoring the binding here is the
    /// accident itself, so it is neither offered nor a way through ([`amenbo_core::worktree`]).
    pub fn nested_worktree(worktree_root: &str, bound_dir: &str) -> CliError {
        CliError {
            code: CliErrorCode::NestedWorktree.as_str(),
            message: format!(
                "This is a git worktree ({worktree_root}) cut inside an Amenbo-managed folder, so it inherited the binding of {bound_dir} from above. Deleting the worktree would not undo what Amenbo wrote: the store lives outside it."
            ),
            hint: Some(format!(
                "Operate Amenbo in the project folder itself ({bound_dir}). Cut worktrees outside it, where there is no binding to inherit."
            )),
            exit: 1,
        }
    }

    /// Pointer-store guard: the `.amenbo` this invocation would resolve was written by a build of
    /// another channel (`AMB-D-685`). Its `project_id` is a primary key in *that* store's numbering, so
    /// reading it here lands on whatever this store happens to keep at the same key — and the slug
    /// cross-check cannot catch it, a dev store being seeded by copying another one, ids, slugs and all.
    /// Refusing beats warning for the reason [`CliError::nested_worktree`] gives: what a warning cannot
    /// undo is the write that follows it.
    ///
    /// The way out is to claim the folder for this store (`bind`), or to run the build it already
    /// belongs to. Both are named, because which one is right is the user's to know: a repository
    /// opened with the wrong build wants the other binary, a folder handed over for good wants `bind`.
    pub fn pointer_other_store(dir: &str, recorded: &str, running: &str) -> CliError {
        CliError {
            code: CliErrorCode::PointerOtherStore.as_str(),
            message: format!(
                "This folder's .amenbo ({dir}) belongs to {recorded}, and this is {running}. The project it names is {recorded}'s, so reading it here would land on a different project."
            ),
            hint: Some(format!(
                "Run {recorded} here instead, or hand the folder to {running} with `{} bind --project <name or ID>`.",
                Paths::command_name()
            )),
            exit: 1,
        }
    }

    /// unbind guard: the folder being unbound has no `.amenbo` pointer — it was never bound in the first
    /// place. If an ancestor is bound, we do not quietly unbind it and take the whole tree down with it;
    /// we say to run unbind there instead (unbind only ever releases **that** folder's binding).
    pub fn unbind_no_binding(dir: &str, ancestor: Option<&str>) -> CliError {
        let cmd = Paths::command_name();
        let hint = match ancestor {
            Some(a) => format!(
                "This folder inherits a binding from an ancestor ({a}). To unbind that, run `{cmd} unbind` there (or `{cmd} unbind --dir {a}`)."
            ),
            None => format!("Nothing to unbind here. Link a folder with `{cmd} init` or `{cmd} bind`."),
        };
        CliError {
            code: CliErrorCode::UnbindNoBinding.as_str(),
            message: format!("This folder ({dir}) has no .amenbo pointer to unbind."),
            hint: Some(hint),
            exit: 1,
        }
    }

    /// No facet, and nothing to default it to. This operation **uses** the facet — it stamps who acted,
    /// or draws how far an AI reaches — and `--actor` was not given (`AMB-D-408`). Quietly defaulting to
    /// human would file an AI session's status/comment/done under the human facet, and let an AI that
    /// declared nothing read past the project its folder is bound to. Neither is worth a default, so the
    /// facet is demanded instead. The context of the call (`--json`, a TTY) does not enter into it: what
    /// decides is only whether the operation uses a facet at all.
    pub fn facet_required() -> CliError {
        CliError {
            code: CliErrorCode::FacetRequired.as_str(),
            message: "facet is unspecified. This operation uses the facet (it stamps who acted, or draws how far an AI reaches), and it is never defaulted."
                .to_string(),
            hint: Some("Declare the facet: pass --actor ai (AI agents) or --actor human.".to_string()),
            exit: 2,
        }
    }

    /// Guardrail violation: an AI (`--actor ai`) attempted an operation the machine-local policy forbids.
    /// This prevents accidents by an honest actor; it is not a security boundary.
    pub fn ai_guardrail(message: impl Into<String>) -> CliError {
        CliError {
            code: CliErrorCode::AiGuardrail.as_str(),
            message: message.into(),
            hint: Some("Have a human run it, or allow it via local policy.".to_string()),
            exit: 1,
        }
    }
}

impl From<amenbo_core::Error> for CliError {
    fn from(e: amenbo_core::Error) -> CliError {
        use amenbo_core::{Error as E, ErrorCode};
        let cmd = Paths::command_name();
        let hint = match &e {
            E::AmbiguousId { candidates, .. } => {
                Some(format!("Candidates: {}", candidates.join(", ")))
            }
            E::NotFound(_) => Some(format!("Run `{cmd} agent --json` to see how to operate.")),
            // The folder is gone, and re-linking is the wrong reflex: `bind --project <id>` records a
            // new binding, so anything filed at the old one is left naming nothing (`AMB-D-648`). The
            // command that answers a folder that moved keeps the id. Where the vanished bindings are in
            // hand, `CliError::binding_stale` lists them by id instead of this fallback.
            E::BindingStale(_) => Some(format!(
                "The folder moved? Re-point that binding from its new home with `{cmd} bind --project <id> --rebind <binding-id>` — it keeps its id. Run `{cmd} bind` in a folder bound to that project to see the vanished bindings by id."
            )),
            E::AlreadyReserved(_) => Some(format!(
                "Another session reserved it first. Pick the next task (`{cmd} agent --json`), or hand it back with `{cmd} task status <id> todo` if the reservation is stale."
            )),
            // The counterpart to `already_reserved`. That one means someone else holds it (→ move on to
            // the next task); this one means a premise you declared is unmet (→ resolve the premise).
            // The two point in opposite directions, so keep them apart.
            E::NotReady(_) => Some(format!(
                "A declared premise is unmet, and there is no --force. Resolve it: finish the blocker (`{cmd} task done <blocker>`) or drop the edge (`{cmd} task undepend <id> --on <blocker>`); settle the premise (`{cmd} decision accept AMB-D-N`) or unlink it (`{cmd} decision link AMB-D-N <id> --unlink`); finish creating the task (`{cmd} task finish-creating <id>`)."
            )),
            // The refusal states the shape and stops there, which leaves the caller holding the very
            // value that failed: `git log --oneline` prints the short form, so the value nearest to hand
            // is the one the door will never take. Amenbo does not expand it — it never runs git
            // (`AMB-D-281`) — so what it can do is name the one command that does.
            // The refusal names the axes but not the way to answer them, and the way is two steps: the
            // values live on the axis, and putting one on the task is its own command. A caller who has
            // just been told "you carry no value on X" is one step from `dimension show X`.
            E::Invalid(m) if m.code() == Some(ErrorCode::InvalidTaskRequiredDimension) => Some(format!(
                "This project requires a value on that axis before a creation can be finished. `{cmd} dimension show <axis>` lists what it offers, then `{cmd} dimension set <AMB-T-n> <axis> <value>` puts one on the task."
            )),
            // The decision side of the same door. The way out is the same two steps, on the ref the
            // other kind is named by — and `dimension set` takes either, so only the ref changes.
            E::Invalid(m) if m.code() == Some(ErrorCode::InvalidDecisionRequiredDimension) => Some(format!(
                "This project requires a value on that axis before a decision can be settled. `{cmd} dimension show <axis>` lists what it offers, then `{cmd} dimension set <AMB-D-n> <axis> <value>` puts one on the decision."
            )),
            // Lowering the flag is the other way out, and it is the one nobody thinks of while holding a
            // task they only wanted to reclassify.
            E::Invalid(m) if m.code() == Some(ErrorCode::InvalidDimensionRequiredUnset) => Some(format!(
                "Move the task to another value with `{cmd} dimension set <AMB-T-n> <axis> <value>`, or stop the axis demanding one with `{cmd} dimension update <axis> --required false`."
            )),
            E::Invalid(m) if m.code() == Some(ErrorCode::InvalidCommitSha) => Some(
                "`git log --oneline` prints the short form. Expand it with `git rev-parse <short sha>` and pass what that returns — Amenbo never runs git itself."
                    .to_string(),
            ),
            _ => None,
        };
        CliError {
            code: e.code(),
            // The CLI surface is English-only; the GUI (`e.to_string()` in the Tauri commands) uses the localized Display.
            message: e.message_en(),
            hint,
            exit: 1,
        }
    }
}

/// The unfinished-setup report to hang on every `--json` response, filled in per run from argv and the
/// filesystem. Empty means there is nothing to report, which is the ordinary case and stays free: a run
/// that never calls [`set_setup_report`] serialises exactly what it always did.
///
/// It is a map rather than one value because more than one setup can be unfinished at a time — the lint
/// hooks and the session-start hook are independent, each reported by the code that reads it — and every
/// one of them has to arrive on the same response, since the field is read by a caller who parses it once.
static SETUP_REPORT: std::sync::Mutex<Option<serde_json::Map<String, serde_json::Value>>> =
    std::sync::Mutex::new(None);

/// Declare one thing whose setup is unfinished, under its own key, so [`print_json`] carries it. Each
/// reporter decides its own key once, before any output; a second call under the same key replaces it.
pub fn set_setup_report(key: &str, report: serde_json::Value) {
    if let Ok(mut held) = SETUP_REPORT.lock() {
        held.get_or_insert_with(serde_json::Map::new).insert(key.to_string(), report);
    }
}

/// The report as it stands, or `None` when nothing is unfinished.
fn setup_report() -> Option<serde_json::Value> {
    let held = SETUP_REPORT.lock().ok()?;
    let map = held.as_ref()?;
    (!map.is_empty()).then(|| serde_json::Value::Object(map.clone()))
}

/// Pretty-print a JSON value to stdout, carrying the unfinished-setup report when there is one. The report
/// rides here rather than at each of the forty call sites because its whole point is to reach an AI on
/// every response, and a field the caller has to remember to add is one they forget; it is grafted on only
/// when the payload is a JSON object and only when something is actually unfinished, so the ordinary
/// response is untouched and a payload that is not an object cannot be corrupted into one.
pub fn print_json<T: Serialize>(value: &T) {
    let s = match (setup_report(), serde_json::to_value(value)) {
        (Some(report), Ok(mut v)) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("setup_incomplete".to_string(), report);
            }
            serde_json::to_string_pretty(&v)
        }
        _ => serde_json::to_string_pretty(value),
    };
    println!("{}", s.unwrap_or_else(|_| "{}".to_string()));
}

/// One human-facing line (suppressed under `--json` / `--quiet`).
pub fn human(flags: &Flags, line: impl AsRef<str>) {
    if !flags.json && !flags.quiet {
        println!("{}", line.as_ref());
    }
}

/// An excerpt with the places the words landed lit up, for a terminal that will render escapes.
///
/// **The ranges are taken as given and never re-derived here.** Which characters a term matches is the
/// index's answer, folded once (`AMB-D-450`) and reported by the core as positions in this very string
/// (`AMB-D-566`); a face matching again for itself would be a second definition of the same word. So this
/// only ever slices: the ranges arrive sorted, disjoint and counted in **characters**, and the walk turns
/// them into an alternation of plain and bright runs.
///
/// Bold is the emphasis, not a colour: the excerpt is one line among several on the row, a hue would ask
/// the reader to know what it means, and weight reads the same on every theme and to a reader who sees no
/// colour at all. `color = false` returns the excerpt untouched — the characters are the whole of the
/// answer, and nothing is lost with the escapes.
///
/// A range reaching past the end is clamped rather than trusted, and one whose start is behind where the
/// walk already stands is dropped: this is display code, and a bad pair should cost the emphasis, never a
/// character of the text or a panic on a character boundary.
pub fn highlight(snippet: &str, matches: &[amenbo_core::store_engine::search::MatchRange], color: bool) -> String {
    const BOLD: &str = "\u{1b}[1m";
    const PLAIN: &str = "\u{1b}[0m";
    if !color || matches.is_empty() {
        return snippet.to_string();
    }
    let chars: Vec<char> = snippet.chars().collect();
    let mut out = String::with_capacity(snippet.len());
    let mut at = 0usize;
    for m in matches {
        let start = m.start.max(at).min(chars.len());
        let end = m.end.min(chars.len());
        if end <= start {
            continue;
        }
        out.extend(&chars[at..start]);
        out.push_str(BOLD);
        out.extend(&chars[start..end]);
        out.push_str(PLAIN);
        at = end;
    }
    out.extend(&chars[at..]);
    out
}

/// Print an error to stderr and return the exit code.
pub fn render_error(flags: &Flags, err: &CliError) -> i32 {
    if flags.json {
        let mut obj = json!({ "error": { "code": err.code, "message": err.message } });
        if let Some(h) = &err.hint {
            obj["error"]["hint"] = json!(h);
        }
        eprintln!("{}", serde_json::to_string_pretty(&obj).unwrap());
    } else {
        eprintln!("Error: {}", err.message);
        if let Some(h) = &err.hint {
            eprintln!("Hint: {h}");
        }
    }
    err.exit
}

/// Print the envelope every write command shares.
pub fn write_envelope(
    flags: &Flags,
    action: &str,
    resource_key: &str,
    resource: serde_json::Value,
    changed: Option<Vec<String>>,
    noop: bool,
    human_line: impl AsRef<str>,
) {
    if flags.json {
        // State the facet acted on, so a mis-set one (meant ai, acted as human) is visible right in the
        // output. Every write declares one, so the key is always there; it is written from the declaration
        // rather than filled in, so there is no facet here that nobody named.
        let mut obj = json!({ "ok": true, "action": action, "noop": noop });
        if let Some(facet) = flags.actor {
            obj["acted_facet"] = json!(facet.as_str());
        }
        if let Some(c) = changed {
            obj["changed"] = json!(c);
        }
        obj[resource_key] = resource;
        print_json(&obj);
    } else {
        // Writes on the ai facet carry the effective facet in the human line too (so a mix-up shows); the interactive human gets no marker.
        if flags.actor == Some(ActorKind::Ai) {
            human(flags, format!("{} · recorded as ai", human_line.as_ref()));
        } else {
            human(flags, human_line);
        }
    }
}

/// Confirm a destructive operation: `--yes` allows it outright, `--json` without it is an error, and an
/// interactive run asks y/N.
pub fn confirm(flags: &Flags, what: &str) -> Result<bool, CliError> {
    if flags.yes {
        return Ok(true);
    }
    if flags.json || !std::io::stdin().is_terminal() {
        return Err(CliError::confirmation_required(what));
    }
    print!("About to: {what}. Proceed? [y/N]: ");
    std::io::stdout().flush().ok();
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).ok();
    let yes = matches!(buf.trim(), "y" | "Y" | "yes");
    if !yes {
        human(flags, "Aborted.");
    }
    Ok(yes)
}

/// The rough length at which a body counts as "long prose" (tunable). Counted in chars, so one CJK
/// character counts as one.
const LONG_BODY_CHARS: usize = 600;

/// Inspect a body and return the readability hints that apply (fixed strings, printed to stderr). Kept
/// conservative, to avoid false positives.
pub fn body_hints(body: &str) -> Vec<&'static str> {
    let trimmed = body.trim();
    let mut hints = Vec::new();
    if trimmed.chars().count() > LONG_BODY_CHARS && !trimmed.contains('\n') {
        hints.push(
            "本文が長く無構造です。結論を先頭に置き、箇条書き/表(GFM)で構造化すると走査しやすくなります。",
        );
    }
    if has_mermaid(trimmed) && !has_text_outside_fences(trimmed) {
        hints.push(
            "mermaid 図が本文の全てです。CLI では図は生ソース表示なので、要点をテキスト一行添えてください（図を唯一の伝達路にしない）。",
        );
    }
    hints
}

/// If the line opens a fence (``` / ~~~), return that fence character.
fn fence_char(trimmed_line: &str) -> Option<char> {
    if trimmed_line.starts_with("```") {
        Some('`')
    } else if trimmed_line.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

/// Whether the body contains a `` ```mermaid `` (or `~~~mermaid`) code fence.
fn has_mermaid(body: &str) -> bool {
    body.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("```mermaid") || t.starts_with("~~~mermaid")
    })
}

/// Whether any non-empty text line lives outside a code fence (i.e. the body says something besides the diagram).
fn has_text_outside_fences(body: &str) -> bool {
    let mut fence: Option<char> = None;
    for line in body.lines() {
        let t = line.trim();
        match fence {
            Some(f) => {
                if fence_char(t) == Some(f) {
                    fence = None; // closed by a terminator of the same fence character
                }
            }
            None => match fence_char(t) {
                Some(f) => fence = Some(f),
                None if !t.is_empty() => return true,
                None => {}
            },
        }
    }
    false
}

/// Print the body hints to stderr (call after a successful write). Never touches stdout, `--json` or not.
pub fn warn_body(body: &str) {
    for h in body_hints(body) {
        eprintln!("hint: {h}");
    }
}

/// The count header on any listing. When paging returns only part of the matches (count < total_matched),
/// name the total too: `3 task(s)`, or `3 of 42 task(s)` on a page.
pub fn count_header(count: usize, total_matched: usize, noun: &str) -> String {
    if count < total_matched {
        format!("{count} of {total_matched} {noun}(s)")
    } else {
        format!("{count} {noun}(s)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::ErrorCode;
    use std::collections::BTreeSet;

    fn set<I: IntoIterator<Item = &'static str>>(it: I) -> BTreeSet<&'static str> {
        it.into_iter().collect()
    }

    /// The way out written for a caller with no terminal names the folders that *are* served and the
    /// person who can link this one — and neither of the two commands that place a pointer, since both
    /// are refused over MCP and advising them sends the reader down a closed road (`AMB-T-3156`).
    #[test]
    fn the_no_terminal_hint_offers_the_set_and_a_person_rather_than_two_closed_roads() {
        let hint = hint_for_a_caller_with_no_terminal("/work/shop\n/work/greenhouse\n");
        assert!(hint.contains("/work/shop"), "the folders it could have named: {hint}");
        assert!(hint.contains("/work/greenhouse"), "all of them: {hint}");
        assert!(hint.contains("Amenbo's own window"), "and the road a person walks: {hint}");
        for closed in ["init --name", "bind --project"] {
            assert!(!hint.contains(closed), "`{closed}` cannot be reached from here: {hint}");
        }
    }

    #[test]
    fn cli_error_code_registry_is_the_full_fixed_set() {
        // Contract snapshot: pins the whole set of CLI-specific codes, so adding, dropping or renaming one is a deliberate checkpoint.
        let expected = set([
            "confirmation_required",
            "no_pointer",
            "init_pointer_exists",
            "init_ambiguous_owners",
            "project_dir_bound",
            "binding_nested_tree",
            "nested_worktree",
            "pointer_other_store",
            "unbind_no_binding",
            "facet_required",
            "ai_guardrail",
            "sync_gap",
            "sync_error",
        ]);
        let actual = set(CliErrorCode::ALL.iter().map(|c| c.as_str()));
        assert_eq!(actual, expected, "the full set of CLI error codes does not match the contract");
        assert_eq!(
            CliErrorCode::ALL.len(),
            actual.len(),
            "duplicate CLI error code strings"
        );
        for c in CliErrorCode::ALL {
            let s = c.as_str();
            assert!(!s.is_empty(), "an empty code");
            assert!(
                s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "a code is lowercase snake_case: {s}"
            );
        }
    }

    #[test]
    fn core_and_cli_code_sets_do_not_collide() {
        // The code strings of the two production registries (core / cli) never overlap — the same code is
        // never defined twice with different meanings — which is what makes "one source of truth" hold across crates.
        let core = set(ErrorCode::ALL.iter().map(|c| c.as_str()));
        let cli = set(CliErrorCode::ALL.iter().map(|c| c.as_str()));
        let overlap: Vec<_> = core.intersection(&cli).collect();
        assert!(overlap.is_empty(), "core and cli define colliding codes: {overlap:?}");
    }

    #[test]
    fn retired_account_binding_codes_are_absent() {
        // There is no account dimension (binding/vault), so the codes that came from the "one folder holds
        // at most one account" collision checks are absent from the registry.
        let cli = set(CliErrorCode::ALL.iter().map(|c| c.as_str()));
        assert!(!cli.contains("folder_already_bound"));
        assert!(!cli.contains("binding_persona_conflict"));
    }

    // ── Body readability hints ───────────────────────────────

    #[test]
    fn body_hints_long_unstructured_prose_warns() {
        // Over 600 chars with no newline is a monolithic wall of prose, so hint at structure.
        let body = "あ".repeat(700);
        let hints = body_hints(&body);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].contains("構造化"));
    }

    #[test]
    fn body_hints_long_but_structured_is_silent() {
        // Past 600 chars but with newlines it is not unstructured, so stay quiet (no false positive).
        let body = format!("# 見出し\n\n{}", "本文。".repeat(300));
        assert!(body.chars().count() > LONG_BODY_CHARS);
        assert!(body_hints(&body).is_empty());
    }

    #[test]
    fn body_hints_short_body_is_silent() {
        // A short body stays quiet even when unstructured (boundary: 600 or fewer).
        assert!(body_hints(&"x".repeat(LONG_BODY_CHARS)).is_empty());
        assert!(body_hints("結論。理由。次の一手。").is_empty());
    }

    #[test]
    fn body_hints_boundary_601_chars_warns() {
        // Boundary: exactly 601 chars (no newline) fires.
        assert!(body_hints(&"x".repeat(LONG_BODY_CHARS + 1)).len() == 1);
    }

    #[test]
    fn body_hints_mermaid_only_warns() {
        // Diagram only (no non-empty text outside the fence), so hint at adding the gist.
        let body = "```mermaid\ngraph TD; A-->B;\n```";
        let hints = body_hints(body);
        assert_eq!(hints.len(), 1);
        assert!(hints[0].contains("mermaid"));
    }

    #[test]
    fn body_hints_mermaid_with_summary_is_silent() {
        // A summary line around the diagram keeps it quiet.
        let body = "要点: A から B へ遷移する。\n\n```mermaid\ngraph TD; A-->B;\n```";
        assert!(body_hints(body).is_empty());
    }

    #[test]
    fn body_hints_no_body_is_silent() {
        assert!(body_hints("").is_empty());
        assert!(body_hints("   \n  ").is_empty());
    }

    fn range(start: usize, end: usize) -> amenbo_core::store_engine::search::MatchRange {
        amenbo_core::store_engine::search::MatchRange { start, end }
    }

    /// The ranges arrive as character positions, and the excerpt they point into is a person's prose —
    /// so the walk has to count characters, not bytes. A search worth highlighting is routinely one
    /// where they differ.
    #[test]
    fn highlight_counts_characters_and_not_bytes() {
        let out = highlight("全文検索の索引を張る", &[range(2, 4)], true);
        assert_eq!(out, "全文\u{1b}[1m検索\u{1b}[0mの索引を張る");
    }

    /// Two words, two runs, and the text between and around them intact. The ranges are the core's,
    /// already sorted and disjoint, so the walk only alternates.
    #[test]
    fn highlight_lights_every_range_and_keeps_the_text_between() {
        let out = highlight("alpha beta gamma", &[range(0, 5), range(11, 16)], true);
        assert_eq!(out, "\u{1b}[1malpha\u{1b}[0m beta \u{1b}[1mgamma\u{1b}[0m");
    }

    /// Off, the excerpt comes back exactly as it went in — not "the same text with the escapes
    /// stripped", which is a different promise and one an off-by-one could break.
    #[test]
    fn highlight_off_hands_the_excerpt_straight_back() {
        let text = "全文検索の索引を張る";
        assert_eq!(highlight(text, &[range(2, 4)], false), text);
        assert_eq!(highlight(text, &[], true), text, "a face with none of the words is the routine case");
    }

    /// This is display code: a range that does not fit the excerpt costs the emphasis and nothing else.
    /// Never a panic on a character boundary, and never a character of the text.
    #[test]
    fn highlight_survives_a_range_that_does_not_fit() {
        assert_eq!(highlight("abc", &[range(1, 99)], true), "a\u{1b}[1mbc\u{1b}[0m", "past the end, clamped");
        assert_eq!(highlight("abc", &[range(9, 99)], true), "abc", "wholly past the end, dropped");
        assert_eq!(highlight("abc", &[range(2, 1)], true), "abc", "inverted, dropped");
        assert_eq!(
            highlight("abcdef", &[range(2, 4), range(1, 3)], true),
            "ab\u{1b}[1mcd\u{1b}[0mef",
            "a second range behind where the walk stands is dropped, and no text is lost with it"
        );
    }
}
