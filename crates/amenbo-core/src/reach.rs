//! Reach — how far the binding (`.amenbo`) lets an AI facet see.
//!
//! The `.amenbo` pointer is not decoration that merely says which store to open: it exists to **contain an
//! AI running in that folder to that project**. If a row outside the binding is readable, its content
//! enters the session's context, and from there it bleeds into summaries, memory, commit messages and
//! handoffs.
//!
//! The type has exactly two values:
//! - [`Reach::All`] — everything on this machine. **The default for a human** (the overview is the human's
//!   place to stand), what the GUI runs with, and what a plugin declaring `scope: machine` is launched with
//!   (`AMB-D-601`: its gate is the device's, and the gate is the window).
//! - [`Reach::Project`] — one project and nothing else. This is where the **AI facet** (`--actor ai`) lands,
//!   and also where a `scope: project` plugin calling Amenbo back lands (`AMB-D-406`) — two ways in that
//!   reach exactly as far as each other. What closed it is carried along ([`Closed`]) for one reason: a
//!   refusal has to name something the reader can act on, and a plugin's author cannot act on a binding
//!   they never made.
//!
//! "An AI in an unbound folder" is not a third value but an **error** ([`Reach::for_ai`]). An empty reach
//! lets no operation through at all, so refusing at the door is both more honest than carrying an empty
//! value around and closes every surface at once — not just read and write, but diagnostics and export too.
//!
//! Enforcement lives at the **doors**; conditions are not sprinkled across individual SQL statements:
//! - Listings (`task list` / `decision list` / `activity` / `status`) fold the reach into the scope slot
//!   (`project_id`) with [`Reach::narrow`]. A listing that names a project outside the reach is an error —
//!   it must not degrade into an empty result.
//! - Reads that name an id (reference resolution, e.g. `task show`) check the entity's project with
//!   [`Reach::check`]. Write paths always resolve their references too, so every mutation that names a ref
//!   is closed here.
//! - Mutations that never resolve a reference — the paths that take a comment / attachment / dimension-value
//!   id directly, and the paths that create new entities — are checked at the single write door
//!   (`Store::write_one`, via `store::write_reach`).
//! - An operation whose subject is the **whole device** without naming anything in it (`export` /
//!   `backup` / `restore`) has no project for either of those to catch, so it asks
//!   [`Reach::refuse_whole_device`] outright — and gets a different answer for each way a reach was closed
//!   (`AMB-D-224` ruled all three through for the AI facet; a plugin's window is neither the user taking
//!   their own data out nor the agent recovering their device).
//!
//! Doors alone would spring a leak the day someone adds a surface that queries the engine
//! (`store_engine::read`) directly: forgetting to declare a scope still compiles, and quietly returns
//! everything. So **an engine read that returns content must take the reach as an argument**. When the reach
//! is closed the SQL narrows to the bound project, and a read that names a project is refused by
//! [`Reach::check`] / [`Reach::narrow`]. A read that forgets does not compile — containment does not rest on
//! the author remembering.
//!
//! Engine reads that are **already keyed by id** (`comment_list` / `decision_card_row` /
//! `attachments_for_target`, …) take no such argument: those ids arrive from a door (`store::read`'s
//! `reachable_*`) that has already checked which project they belong to. Giving a reach to `task_project` /
//! `decision_project` — the very functions that answer that question — would only be circular.
//!
//! Out of reach is **[`crate::error::Error::out_of_reach`], never not_found**: we do not deny that a thing
//! exists, we say only that it cannot be reached from here. And Amenbo closes only its own surfaces — an AI
//! with a shell can still read files directly, and we do not pretend otherwise.

use crate::error::{Error, Result};

/// What closed a reach — which decides nothing about how far it reaches, and everything about what a
/// reader turned away by it can do next.
///
/// The two are told apart because their way out is not the same. A binding is in the reader's hands: a
/// human can run the command, or the work can move to the folder bound to that project. A window is not:
/// it was fixed by the runner that launched this process, before the plugin's own code ran, and no
/// argument the plugin passes widens it. Naming the binding at a plugin would send its author looking for
/// an `.amenbo` that decided nothing here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    /// The folder's `.amenbo`, which is where the AI facet draws its reach from.
    Binding,
    /// The window a plugin was launched with (`AMB-D-406`) — the gate it fires through, read back.
    Window,
}

/// How far this operation reaches. The default is [`Reach::All`] (humans, the GUI, library use); the AI
/// facet and a plugin's window are what close it to one project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reach {
    /// Everything on this machine, across projects.
    #[default]
    All,
    /// This one project and nothing else, and what closed it to that.
    Project { id: i64, closed_by: Closed },
}

impl Reach {
    /// A reach closed by the folder's binding — the AI facet's.
    pub fn binding(project: i64) -> Reach {
        Reach::Project { id: project, closed_by: Closed::Binding }
    }

    /// A reach closed by the window a plugin was launched with (`AMB-D-406`).
    pub fn window(project: i64) -> Reach {
        Reach::Project { id: project, closed_by: Closed::Window }
    }

    /// Derives an AI facet's reach **from the binding and nothing else**. With no binding the reach is
    /// **empty** — an AI does not get to pick a project, so there is nothing to contain it to. That is an
    /// error, not a fall back to `All`: quietly showing everything would turn the binding back into
    /// decoration.
    ///
    /// The exceptions are the operation that creates a binding (`amenbo init`) and the one that does not
    /// reflect any store content (`unbind`) — both are handled before the store is opened, so they never
    /// arrive here.
    pub fn for_ai(binding: Option<i64>) -> Result<Reach> {
        match binding {
            Some(pid) => Ok(Reach::binding(pid)),
            None => Err(unbound()),
        }
    }

    /// The bound project — `Some` only when the reach is closed.
    pub fn project(self) -> Option<i64> {
        match self {
            Reach::All => None,
            Reach::Project { id, .. } => Some(id),
        }
    }

    /// Does this reach cover that project? An entity that belongs to no project (an unplaced task) is out of
    /// a closed reach.
    pub fn allows(self, project_id: Option<i64>) -> bool {
        match self {
            Reach::All => true,
            Reach::Project { id, .. } => project_id == Some(id),
        }
    }

    /// Folds the reach into a listing's scope slot (`project_id`). When the reach is closed, an unspecified
    /// slot is filled with the bound project, and naming a project outside the reach is an error — never a
    /// silent empty result.
    pub fn narrow(self, requested: Option<i64>) -> Result<Option<i64>> {
        match (self, requested) {
            (Reach::All, r) => Ok(r),
            (Reach::Project { id, .. }, None) => Ok(Some(id)),
            (Reach::Project { id, .. }, Some(r)) if r == id => Ok(Some(id)),
            (Reach::Project { id, closed_by }, Some(r)) => {
                Err(out_of_reach(&crate::idref::project(r), id, closed_by))
            }
        }
    }

    /// The **vocabulary that names a project** (`--project`, the `project:` filter) belongs to humans. Under
    /// a closed reach it is an error even when it names the bound project itself — an AI does not get to
    /// choose which project it works in. We neither ignore it silently nor silently fall back to the
    /// binding. (`what` is the name of the vocabulary in question.)
    pub fn refuse_project_choice(self, what: &str) -> Result<()> {
        match self {
            Reach::All => Ok(()),
            Reach::Project { id, closed_by } => {
                let bound = crate::idref::project(id);
                Err(Error::out_of_reach(match closed_by {
                    Closed::Binding => format!(
                        "{what} is for humans — an AI does not pick a project: it works in the one its \
                         folder's .amenbo names ({bound}), and only there. Drop {what}; the binding \
                         already scopes this command."
                    ),
                    Closed::Window => format!(
                        "{what} is for humans — a plugin does not pick a project: it reads through the \
                         window it was launched with ({bound}), and naming one does not widen it. Drop \
                         {what}; the window already scopes this command."
                    ),
                }))
            }
        }
    }

    /// Refuse an operation whose subject is **the whole device** to a reader holding only a window
    /// (`AMB-D-406`) — reading every project out (`export`, `backup`) or writing over all of them at once
    /// (`restore`). A window is the gate a plugin fires through, so what it may touch is what it may
    /// observe, and these are the calls that step past that without ever naming a project for
    /// [`narrow`](Self::narrow) or [`check`](Self::check) to catch. (`what` is the operation.)
    ///
    /// **A binding is let through, and the asymmetry is a ruling rather than an oversight.**
    /// `AMB-D-224` weighed these same three for the AI facet and allowed them: taking your own data out
    /// is the user's right, their AI acts for them, and disaster recovery is work an agent is there to
    /// run. What that decision narrowed was the door, not the contents — with no destination the export
    /// writes a file instead of streaming the device into the session. A plugin holding one project's
    /// window is not the user: it migrates nowhere, recovers nothing, and its window was fixed by the
    /// runner before its own code ran.
    ///
    /// A plugin whose author declared `scope: machine` holds no window to step past — it was launched
    /// reaching the device (`AMB-D-601`), and enabling it was the consent for exactly that — so it lands in
    /// the first arm with the human and the binding. That is the layer being taken at its word, not a hole:
    /// this refusal has always asked *how far does this reader reach*, never *what kind of reader is it*.
    pub fn refuse_whole_device(self, what: &str) -> Result<()> {
        match self {
            Reach::All | Reach::Project { closed_by: Closed::Binding, .. } => Ok(()),
            Reach::Project { id, closed_by: Closed::Window } => {
                let bound = crate::idref::project(id);
                Err(Error::out_of_reach(format!(
                    "{what} acts on this whole device, and a plugin reaches only through the window it \
                     fires in ({bound}) — a window no argument widens. Nothing outside it was yours to \
                     read or to replace: the ids in the payload name what you were launched for."
                )))
            }
        }
    }

    /// Is the entity named by an id within the reach? (`what` is the display ref, e.g. `AMB-T-<n>`.)
    pub fn check(self, what: &str, project_id: Option<i64>) -> Result<()> {
        match self {
            Reach::All => Ok(()),
            Reach::Project { id, .. } if project_id == Some(id) => Ok(()),
            Reach::Project { id, closed_by } => Err(out_of_reach(what, id, closed_by)),
        }
    }
}

/// The out-of-reach wording. It says "you cannot reach that from here", not "it does not exist" — and it
/// says it in the terms of whatever closed the reach, since the reader's way out is not the same on both
/// (see [`Closed`]).
fn out_of_reach(what: &str, bound: i64, closed_by: Closed) -> Error {
    let bound = crate::idref::project(bound);
    Error::out_of_reach(match closed_by {
        Closed::Binding => format!(
            "{what} is outside project {bound}, the project this folder is bound to — an AI reaches \
             only the project its .amenbo names. Ask a human to run this, or work in the \
             folder bound to that project."
        ),
        Closed::Window => format!(
            "{what} is outside project {bound}, the project this plugin was launched to observe — a \
             plugin reads only through the window it fires in, which no argument widens. The ids in \
             the payload it was handed are inside that project."
        ),
    })
}

/// The wording for an AI running in an unbound folder, in both languages. It says "this folder is bound to
/// nothing" rather than "you may not look", and points at the ways out: have a human bind it, or work in a
/// folder that already is.
fn unbound() -> Error {
    let cmd = crate::config::Paths::command_name();
    Error::out_of_reach(
        format!(
            "this folder is not bound to any project — an AI reaches only the project its .amenbo names, \
             so an unbound folder reaches nothing. Ask a human to bind it (`{cmd} bind --project \
             <name or id>`), or work in a folder that is already bound."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ai_draws_its_reach_from_the_binding_and_an_unbound_folder_reaches_nothing() {
        assert_eq!(Reach::for_ai(Some(3)).unwrap(), Reach::binding(3));
        // No binding does not fall back to All — we do not paper over a state where containment cannot hold
        // by quietly showing everything.
        assert_eq!(Reach::for_ai(None).unwrap_err().code(), "out_of_reach");
    }

    #[test]
    fn all_reaches_everything_and_narrows_nothing() {
        assert!(Reach::All.allows(Some(7)));
        assert!(Reach::All.allows(None));
        assert_eq!(Reach::All.narrow(None).unwrap(), None);
        assert_eq!(Reach::All.narrow(Some(7)).unwrap(), Some(7));
        assert!(Reach::All.check("#1", None).is_ok());
    }

    #[test]
    fn a_bound_reach_fills_the_scope_slot_and_refuses_another_project() {
        let r = Reach::binding(3);
        // An unspecified slot is filled with the bound project — a listing never quietly returns everything.
        assert_eq!(r.narrow(None).unwrap(), Some(3));
        // Naming the same project is allowed through.
        assert_eq!(r.narrow(Some(3)).unwrap(), Some(3));
        // Another project is an error, not an empty result — existence is not denied.
        let e = r.narrow(Some(4)).unwrap_err();
        assert_eq!(e.code(), "out_of_reach");
    }

    #[test]
    fn a_bound_reach_refuses_an_entity_outside_it_including_an_unplaced_one() {
        let r = Reach::binding(3);
        assert!(r.check("#1", Some(3)).is_ok());
        assert_eq!(r.check("#2", Some(4)).unwrap_err().code(), "out_of_reach");
        // An unplaced task (belonging to no project) is out of a closed reach as well.
        assert_eq!(r.check("#3", None).unwrap_err().code(), "out_of_reach");
    }

    /// A window reaches exactly as far as a binding does — and says something else when it turns a reader
    /// away, because what the reader can do about it is not the same. A plugin's author never made an
    /// `.amenbo` here and cannot run this as a human: naming either would send them at something that
    /// decided nothing.
    #[test]
    fn a_window_reaches_as_far_as_a_binding_and_is_refused_in_its_own_terms() {
        let binding = Reach::binding(3);
        let window = Reach::window(3);
        assert_eq!(binding.project(), window.project());
        assert!(window.check("#1", Some(3)).is_ok());
        assert!(window.allows(Some(3)) && !window.allows(Some(4)) && !window.allows(None));
        assert_eq!(window.narrow(None).unwrap(), Some(3));

        let refused = window.check("AMB-T-2", Some(4)).unwrap_err();
        assert_eq!(refused.code(), "out_of_reach");
        let said = refused.to_string();
        assert!(said.contains("plugin") && said.contains("AMB-P-3"), "got: {said}");
        assert!(!said.contains(".amenbo") && !said.contains("human"), "got: {said}");

        // The vocabulary that names a project is refused on both, and points at the one that closed it.
        let named = window.refuse_project_choice("--project").unwrap_err().to_string();
        assert!(named.contains("window") && !named.contains(".amenbo"), "got: {named}");
        assert!(
            binding.refuse_project_choice("--project").unwrap_err().to_string().contains(".amenbo")
        );
    }

    /// Taking the whole device as the subject is the one place the two closed reaches part company. A
    /// window is refused it — observing one project is the whole of what a plugin was launched for. A
    /// binding keeps it, because `AMB-D-224` ruled these through for the AI facet: the user's own way out
    /// of the tool, and the recovery their agent is there to run.
    #[test]
    fn only_a_window_is_refused_the_whole_device() {
        for what in ["export", "backup", "restore"] {
            assert!(Reach::All.refuse_whole_device(what).is_ok());
            assert!(Reach::binding(3).refuse_whole_device(what).is_ok());

            let refused = Reach::window(3).refuse_whole_device(what).unwrap_err();
            assert_eq!(refused.code(), "out_of_reach");
            let said = refused.to_string();
            assert!(said.contains(what) && said.contains("plugin"), "got: {said}");
            assert!(said.contains("AMB-P-3"), "it names the window it was closed to: {said}");
        }
    }
}
