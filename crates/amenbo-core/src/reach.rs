//! Reach — how far the binding (`.amenbo`) lets an AI facet see.
//!
//! The `.amenbo` pointer is not decoration that merely says which store to open: it exists to **contain an
//! AI running in that folder to that project**. If a row outside the binding is readable, its content
//! enters the session's context, and from there it bleeds into summaries, memory, commit messages and
//! handoffs.
//!
//! The type has exactly two values:
//! - [`Reach::All`] — everything on this machine. **The default for a human** (the overview is the human's
//!   place to stand), and what the GUI runs with.
//! - [`Reach::Project`] — one bound project. This is where the **AI facet** (`--actor ai` /
//!   `AMENBO_ACTOR=ai`) lands.
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
//! exists, we say only that it cannot be reached from here. And amenbo closes only its own surfaces — an AI
//! with a shell can still read files directly, and we do not pretend otherwise.

use crate::error::{Error, Result};

/// How far this operation reaches. The default is [`Reach::All`] (humans, the GUI, library use); only the
/// AI facet is closed to the bound project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reach {
    /// Everything on this machine, across projects.
    #[default]
    All,
    /// This one project and nothing else.
    Project(i64),
}

impl Reach {
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
            Some(pid) => Ok(Reach::Project(pid)),
            None => Err(unbound()),
        }
    }

    /// The bound project — `Some` only when the reach is closed.
    pub fn project(self) -> Option<i64> {
        match self {
            Reach::All => None,
            Reach::Project(id) => Some(id),
        }
    }

    /// Does this reach cover that project? An entity that belongs to no project (an unplaced task) is out of
    /// a closed reach.
    pub fn allows(self, project_id: Option<i64>) -> bool {
        match self {
            Reach::All => true,
            Reach::Project(p) => project_id == Some(p),
        }
    }

    /// Folds the reach into a listing's scope slot (`project_id`). When the reach is closed, an unspecified
    /// slot is filled with the bound project, and naming a project outside the reach is an error — never a
    /// silent empty result.
    pub fn narrow(self, requested: Option<i64>) -> Result<Option<i64>> {
        match (self, requested) {
            (Reach::All, r) => Ok(r),
            (Reach::Project(p), None) => Ok(Some(p)),
            (Reach::Project(p), Some(r)) if r == p => Ok(Some(p)),
            (Reach::Project(p), Some(r)) => Err(out_of_reach(&crate::idref::project(r), p)),
        }
    }

    /// The **vocabulary that names a project** (`--project`, the `project:` filter) belongs to humans. Under
    /// a closed reach it is an error even when it names the bound project itself — an AI does not get to
    /// choose which project it works in. We neither ignore it silently nor silently fall back to the
    /// binding. (`what` is the name of the vocabulary in question.)
    pub fn refuse_project_choice(self, what: &str) -> Result<()> {
        match self {
            Reach::All => Ok(()),
            Reach::Project(p) => {
                let bound = crate::idref::project(p);
                Err(Error::out_of_reach(
                    format!(
                        "{what} is for humans — an AI does not pick a project: it works in the one its \
                         folder's .amenbo names ({bound}), and only there. Drop {what}; the binding \
                         already scopes this command."
                    ),
                    format!(
                        "{what} は人間のためのものです — AI はプロジェクトを選びません。フォルダの .amenbo が \
                         指すプロジェクト（{bound}）の中だけで働きます。{what} を外してください——束縛が \
                         既にこのコマンドの範囲を決めています。"
                    ),
                ))
            }
        }
    }

    /// Is the entity named by an id within the reach? (`what` is the display ref, e.g. `AMB-T-<n>`.)
    pub fn check(self, what: &str, project_id: Option<i64>) -> Result<()> {
        match self {
            Reach::All => Ok(()),
            Reach::Project(p) if project_id == Some(p) => Ok(()),
            Reach::Project(p) => Err(out_of_reach(what, p)),
        }
    }
}

/// The out-of-reach wording, in both languages. It says "you cannot reach that from here", not "it does not
/// exist".
fn out_of_reach(what: &str, bound: i64) -> Error {
    let bound = crate::idref::project(bound);
    Error::out_of_reach(
        format!(
            "{what} is outside project {bound}, the project this folder is bound to — an AI reaches \
             only the project its .amenbo names. Ask a human to run this, or work in the \
             folder bound to that project."
        ),
        format!(
            "{what} は、このフォルダが束縛しているプロジェクト {bound} の外です — AI が到達できるのは \
             .amenbo が指すプロジェクトだけです。人間に実行してもらうか、そのプロジェクトに \
             束縛されたフォルダで作業してください。"
        ),
    )
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
        format!(
            "このフォルダはどのプロジェクトにも束縛されていません — AI が到達できるのは .amenbo が指す \
             プロジェクトだけなので、束縛の無いフォルダからは何にも到達できません。人間に束縛して \
             もらう（`{cmd} bind --project <name or id>`）か、既に束縛されたフォルダで作業してください。"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ai_draws_its_reach_from_the_binding_and_an_unbound_folder_reaches_nothing() {
        assert_eq!(Reach::for_ai(Some(3)).unwrap(), Reach::Project(3));
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
        let r = Reach::Project(3);
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
        let r = Reach::Project(3);
        assert!(r.check("#1", Some(3)).is_ok());
        assert_eq!(r.check("#2", Some(4)).unwrap_err().code(), "out_of_reach");
        // An unplaced task (belonging to no project) is out of a closed reach as well.
        assert_eq!(r.check("#3", None).unwrap_err().code(), "out_of_reach");
    }
}
