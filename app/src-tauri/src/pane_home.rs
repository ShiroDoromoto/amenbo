//! A directory of its own for the AI in each pane, so two panes on one folder are two conversations
//! (`AMB-D-869`, `AMB-D-875`).
//!
//! **Two of the six are not resumed by a name.** The others take a session id on the launch line;
//! these take a word meaning "the last one", and "last" is counted per working folder — so two
//! panes opened on one repository come back to the same conversation, and one of the two people
//! reading them is looking at somebody else's (`AMB-T-4630`). Pointing each pane at a home of its
//! own is what parts them: the only sessions in a home are that pane's, so "the last one" is
//! structurally the right one and no id has to be issued or read (`AMB-T-4633`, `AMB-T-4677`).
//!
//! **Which rows these are is the catalog's answer, not this module's.** A row is given a home while
//! its way back is the place it runs in ([`amenbo_core::harness::Resume::comes_back_by_a_place`]),
//! and stops being given one the moment that way back is taken down — which is what keeps a home
//! from being made, and paid for, for a pane that can no longer come back to it (`AMB-T-4678`).
//!
//! **A home is made empty, and what the person configured is linked into it.** A bare home is a
//! provider that has to be logged into again, on a model nobody chose, with none of the reader's own
//! skills — and for Codex it weighs 29MB a pane, against 2.3–4.3MB with the links in (`AMB-T-4633`).
//! Gemini's is heavier than that: without them it does not open at all, stopping on the auth method
//! it cannot read and then on a folder it has not been told to trust (`AMB-T-4677`). The link is the
//! point rather than a copy: settings the person edits by hand go on working, and so does an
//! authentication that is refreshed.
//!
//! **What a pane settles reaches back out.** Codex writes a folder's `trust_level` into the
//! `config.toml` it is pointed at, and Gemini writes a folder it has been trusted with into
//! `trustedFolders.json`, so trusting a folder in a pane reaches the reader's own file — as does
//! anything else settled there. That is the price of the file a pane reads being the reader's own,
//! reached by a link or by a path alike, and it is `AMB-D-869`'s and `AMB-D-875`'s to have paid;
//! what is owed here is that the lists below are not widened without the same question being asked
//! again.
//!
//! **One file is pointed at rather than shared.** Gemini replaces `trustedFolders.json` — a
//! temporary file and a rename — and a link does not survive that on Windows, where a hard one is
//! the only kind this process can make (`AMB-D-878`). A pane told where the reader keeps the file
//! reads the one file there has ever been, so there is nothing for the replacing to break. The
//! answer is the same on every operating system: a symbolic link would have done on macOS, and one
//! row that says this once is worth more than a branch that buys nothing.
//!
//! **One file is copied in rather than shared, and only where a link will not do.** Codex replaces
//! `config.toml` the same way and offers no variable to name it with, so on Windows a pane is given
//! a copy of the reader's, taken again on every open. What that keeps is the person's model and
//! servers reaching the pane; what it gives up is the pane's own writes coming back. macOS and Linux
//! keep the link, which survives the replacing and gives up neither (`AMB-D-878`).
//!
//! **What is made here outlives the pane it was made for.** A home is where the conversation is:
//! these two come back by the place they ran in and not by an id, so a home taken away is a
//! conversation there is no way back into. A pane that closes leaves its home standing, and what is
//! let go of is the watch on it ([`crate::pane_home::forget`]). What bounds the pile is the total
//! size the homes come to and not whether a pane is still open: over the budget, the ones least
//! recently opened are taken away until what is left is inside it ([`crate::pane_home::rotate`],
//! `AMB-D-898`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// One provider's per-pane homes — where they are made, what is shared into them, and the variable
/// the provider is told about its own by.
///
/// It is a table for the reason the launch catalog is one (`AMB-D-791`): the two rows differ in
/// every particular and in no step, so a third provider that comes back by a place is a row here
/// rather than a branch anywhere.
struct Kind {
    /// The catalog id of the provider this row is for ([`amenbo_core::harness::LAUNCHES`]).
    agent: &'static str,
    /// The variable the provider reads its own directory from.
    ///
    /// It is a floor and not a setting, the way everything else a pane is started with is
    /// (`crate::launch`): a profile that exports the same variable is read after this is set and
    /// wins, and a reader who has one has pointed all of their panes at one place on purpose.
    env: &'static str,
    /// What the provider's own directory is called under the reader's home — the one the links point
    /// into.
    theirs: &'static str,
    /// The directory, under Amenbo's app-data, this provider's per-pane homes are made in. It is
    /// app-data and not the temporary directory because a home is how a pane comes back, and it has
    /// to outlive the run that made it.
    homes: &'static str,
    /// Where inside a pane's home the shared entries sit, and empty where that is the home itself.
    ///
    /// **The variable does not mean the same thing on both rows.** `CODEX_HOME` names the directory
    /// Codex keeps its own things in; `GEMINI_CLI_HOME` names a home to build `.gemini` inside of —
    /// it is read by the provider's `homedir()` and everything else is put together on top of it
    /// (`AMB-T-4677`). So the path a link is made at is the home and this, and the difference is a
    /// word in a row rather than a branch.
    inside: &'static str,
    /// What each home shares with the reader's own. How one is reached is [`link`]'s to say — the
    /// spelling differs by operating system, and on one of them by whether the name is a directory.
    ///
    /// **Measured rather than documented**: each of these was watched being needed
    /// (`AMB-T-4633`, `AMB-T-4677`). A provider adding a file to its directory in some later version
    /// is a file that will not be shared until this list says so, which is the cost `AMB-D-869`
    /// accepted for not reading its internals.
    shared: &'static [&'static str],
    /// Which of [`Kind::shared`] the provider **makes when somebody logs in**, rather than one the
    /// reader already has (`AMB-D-880`).
    ///
    /// These are linked to whether or not the reader has one yet, which the rest are not: there is
    /// nothing to read either way, and what the link buys is the login landing in the reader's own
    /// directory rather than in a home that goes when the pane does. A reader who has never logged
    /// in would otherwise be asked again by every pane they open, for as long as they use Amenbo
    /// (`AMB-T-4709`).
    made: &'static [&'static str],
    /// What each home is told the path of, in the reader's own directory, instead of being given a
    /// link to it — the variable, and the name under [`Kind::theirs`].
    ///
    /// It is for the files the provider **replaces** rather than writes into: a temporary file and
    /// a rename leave a link pointing at what used to be there. A path names the file however often
    /// it is rewritten, so nothing has to survive the rewriting — and writes still reach the
    /// reader's own file, which is [`Kind::shared`]'s price and not a further one.
    ///
    /// Unlike a shared name, a file the reader has none of is still pointed at: the provider makes
    /// the file where it is told to, and a variable left out would send it back to the home.
    points: &'static [(&'static str, &'static str)],
    /// Which of [`Kind::shared`] the provider **replaces** rather than writes into — a temporary
    /// file and a rename.
    ///
    /// A symbolic link is resolved afresh every time it is read and comes through that; the hard
    /// link Windows is given is a second name for the file that was there, and is left on it
    /// (`AMB-D-878`). So on Windows these are carried in as copies instead, on every open rather
    /// than on the open that made the home.
    copied: &'static [&'static str],
    /// Which keys of a copied file the pane is meant to settle, and which are therefore written back
    /// into the reader's own ([`crate::pane_settled`]).
    ///
    /// **It is what keeps a copy from being one direction only.** The rest of the copy is the pane's
    /// and goes with it: a folder's `trust_level` and a server an `mcp add` wrote there were settled
    /// for the pane rather than for the reader, and carrying the file back whole would carry both —
    /// which is the cost `AMB-D-878` weighed and the reason it took the copy over a link it would
    /// have had to rebuild.
    ///
    /// Every name in [`Kind::copied`] is answered for here, because a file copied in with nothing
    /// named is a file whose writes the reader loses without being told.
    carried_back: &'static [(&'static str, &'static [&'static str])],
}

impl Kind {
    /// The shared names this operating system reaches through a link — everything, less what it
    /// copies in instead.
    fn linked(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.shared.iter().copied().filter(|name| !self.copied_here().contains(name))
    }

    /// The names this operating system carries in as copies. Nothing at all where a link survives
    /// the file it names being replaced, which is everywhere but Windows (`AMB-D-878`).
    fn copied_here(&self) -> &'static [&'static str] {
        if cfg!(windows) {
            self.copied
        } else {
            &[]
        }
    }

    /// What this operating system carries back out of those copies. Nothing where nothing was
    /// copied: a link already reaches the reader's own file and there is no second copy to read.
    fn carried_back_here(&self) -> &'static [(&'static str, &'static [&'static str])] {
        if cfg!(windows) {
            self.carried_back
        } else {
            &[]
        }
    }
}

/// Every provider that comes back by the place it was started in, in the catalog's order.
const KINDS: &[Kind] = &[
    Kind {
        agent: "codex-cli",
        env: "CODEX_HOME",
        theirs: ".codex",
        homes: "codex-homes",
        inside: "",
        // Take `auth.json` out and the pane asks to log in; take `config.toml` out and it comes up
        // on a model nobody chose; take `skills`, `prompts` and `plugins` out and the reader's own
        // are gone from the pane; take `cache` and `models_cache.json` out and the home is 29MB
        // instead of 2.3 (`AMB-T-4633`).
        shared: &["auth.json", "config.toml", "skills", "prompts", "cache", "plugins", "models_cache.json"],
        // The one a login makes. Watched being written in place through a link that had nothing
        // behind it, leaving the link and the reader's own new file (`AMB-D-880`). The rest of the
        // row is configuration and cache: a reader who has none of those is not asked for them
        // again and again.
        made: &["auth.json"],
        points: &[],
        // Codex replaces `config.toml` — on an `mcp add`, and on a plain `codex exec`, which adds
        // the folder's `trust_level` (`AMB-T-4696`). What is wanted of the file is the reader's
        // model and servers reaching the pane, and a copy carries that however often it is
        // rewritten (`AMB-D-878`).
        copied: &["config.toml"],
        // What `/model` settles, and the whole of what it settles: the two were watched changing
        // together on a press and nothing else in the file moved with them (`AMB-T-4835`). The
        // second is the picker's own second step, so a model carried back without it would leave the
        // reader on a pairing they never chose.
        carried_back: &[("config.toml", &["model", "model_reasoning_effort"])],
    },
    Kind {
        agent: "gemini-cli",
        env: "GEMINI_CLI_HOME",
        theirs: ".gemini",
        homes: "gemini-homes",
        inside: ".gemini",
        // Without `settings.json` the pane is a stopped one (`AMB-T-4677`): it exits on the auth
        // method it cannot read. The other is for the machines with no keychain to keep a token in:
        // there the provider writes one into the home it was pointed at, which is this one and not
        // the reader's, so an authentication would be asked for again per pane.
        shared: &["settings.json", "gemini-credentials.json"],
        // The credentials, and not `settings.json` beside them: that one is the reader's own
        // configuration rather than something a login writes, and a pane with none of it does not
        // open at all (`AMB-T-4677`) — which is a thing to say rather than a file to invent.
        made: &["gemini-credentials.json"],
        // The third thing the pane stops without — it exits on a folder it has not been told to
        // trust, and its TUI asks about the folder every time (`AMB-T-4677`) — and the one the
        // provider replaces rather than writes into, so it is pointed at (`AMB-D-878`).
        points: &[("GEMINI_CLI_TRUSTED_FOLDERS_PATH", "trustedFolders.json")],
        // Gemini's own replaced file is the one above: it is named by a variable rather than
        // carried in, because the provider offers one to name it with.
        copied: &[],
        // Nothing is copied in, so there is nothing to carry back out.
        carried_back: &[],
    },
];

/// The row for this provider, or nothing where it is not one that comes back by a place — either
/// because it never was, or because its way back is down and a home would be a price paid for
/// nothing (`AMB-T-4678`).
fn kind_of(agent: Option<&str>) -> Option<&'static Kind> {
    let agent = agent?;
    let kind = KINDS.iter().find(|kind| kind.agent == agent)?;
    amenbo_core::harness::find_launch(agent)?
        .resume
        .as_ref()
        .filter(|resume| resume.comes_back_by_a_place())
        .map(|_| kind)
}

/// Whether this is a frame id as the window draws them — eight, four, four, four and twelve hex
/// digits, in lower case (`AMB-D-897`).
///
/// **It is asked as a shape and not as an identity.** What it keeps out is a name with a path in it,
/// which is the one thing a frame id must not be able to do here; whether the id names a pane this
/// run has is the arrangement's answer, not a directory's. The three ids an older build counted are
/// gone by the time this is asked — the chain drew each of them afresh and moved its home with it
/// (`amenbo_core::store_engine::migrate`).
fn is_drawn_id(frame: &str) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let lower_hex = |part: &str| {
        part.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    };
    let mut parts = frame.split('-');
    GROUPS.iter().all(|len| parts.next().is_some_and(|part| part.len() == *len && lower_hex(part)))
        && parts.next().is_none()
}

/// The home the AI in this frame runs in, made and linked the first time it is asked for, with the
/// variables the pane is started with: the one it is told its home by, and one for each file it is
/// pointed at where the reader keeps it ([`Kind::points`]). Nothing at all for a pane running
/// anything else.
///
/// **Keyed by the frame rather than by the session**, because what comes back is the place: a pane
/// resumed in the next run is the same frame with a new process in it, and the home it is pointed at
/// has to be the one its conversation is in. A frame id is drawn once and never handed out again
/// (`app/src/talk/layout.ts`), so one home is one pane for as long as both exist.
///
/// A frame id that is not one of those is refused rather than made a directory for: it arrives from
/// the window, and what a name would do here is write outside the directory this module answers for.
pub fn for_pane(frame: &str, agent: Option<&str>) -> Option<(Vec<(&'static str, PathBuf)>, PathBuf)> {
    let kind = kind_of(agent)?;
    if !is_drawn_id(frame) {
        log::warn!("no {} home for frame {frame:?}: a frame is a UUID", kind.agent);
        return None;
    }
    let home = homes_root(kind)?.join(frame);
    let theirs = amenbo_core::env::home_dir().map(|dir| dir.join(kind.theirs));
    // A home that is already there is the one that comes back; only the first ask makes one. The
    // variables are read off the row either way — they are what the pane is started with, not
    // something the making leaves behind.
    let home = if home.is_dir() { home } else { make(kind, &home)? };
    // The sharing is done on every open rather than on the open that made the home, because a name
    // that was linked can stop being one while the pane runs: Gemini's own keychain unlinks the
    // credentials file as it logs out, which takes this pane's link and leaves the reader's file
    // where it was (`AMB-T-4710`). Left to the making, that pane would go on running with a home
    // that shares nothing and no sign of it, until somebody closed the pane and opened another.
    //
    // The copies are here for a neighbouring reason: what they are for is the reader's file as it is
    // now, and a copy taken on the open that made the home would be as old as the pane.
    if let Some(theirs) = theirs.as_deref() {
        let into = home.join(kind.inside);
        share(kind, &into, theirs);
        copy_in(kind.copied_here(), &into, theirs);
        // Straight after the copies, because what the watch holds is the values they were taken
        // with: what the file says other than that afterwards, the pane put there.
        crate::pane_settled::watch(&home, &into, theirs, kind.carried_back_here());
    }
    Some((vars(kind, home.clone(), theirs.as_deref()), home))
}

/// The variables a pane with this home is started with, in the order [`Kind`] gives them.
///
/// **A reader's own directory that cannot be resolved leaves only the home.** There is no path to
/// point the pane at, and a variable set to a guess would send the provider somewhere nobody keeps
/// anything.
fn vars(kind: &Kind, home: PathBuf, theirs: Option<&Path>) -> Vec<(&'static str, PathBuf)> {
    let mut vars = vec![(kind.env, home)];
    if let Some(theirs) = theirs {
        vars.extend(kind.points.iter().map(|(var, name)| (*var, theirs.join(name))));
    }
    vars
}

/// Let go of what this run holds for one pane's home, once the pane is gone (`crate::frames`), and
/// answer whether the handle was a home made here.
///
/// **The home itself stays** (`AMB-D-898`). It is where the conversation is, and a pane being closed
/// is not a reason to take a conversation away; what goes with the pane is the watch that was
/// carrying back what it settled ([`crate::pane_settled`]).
///
/// **It answers only for homes it made.** What is handed in is a pane's resume handle, and the
/// handles of the other five providers are session ids rather than paths — the watch is kept under
/// the home it is on, so one of those is let go of without anything being done to it.
///
/// **The answer is the caller's cue to weigh the pile** ([`rotate`]). A home stops being spoken for
/// the moment its pane goes, which is the one thing that can bring the roots inside the budget
/// without a byte having changed.
pub fn forget(handle: &Path) -> bool {
    crate::pane_settled::forget(handle);
    made_here(handle)
}

/// Whether this path is one of the homes made under this build's roots — its parent is one of them.
///
/// It is asked as a place and not as a directory that is there: a handle is answered for whether or
/// not the home it names has since been weighed out.
fn made_here(handle: &Path) -> bool {
    KINDS.iter().filter_map(homes_root).any(|root| handle.parent() == Some(root.as_path()))
}

/// The most the per-pane homes may come to, all told (`AMB-D-898`).
///
/// **It is a bound on the disk and not on how many conversations are kept**, because the disk is
/// what there is to protect. One home was measured at 5–8MB (2.3–4.3MB of it empty, 1–3MB of it what
/// was said), so this is 130–200 of them; the same 150 homes counted instead would have been
/// anywhere between 750MB and 1.2GB.
const BUDGET: u64 = 1024 * 1024 * 1024;

/// Weigh the homes and take the least recently opened away until what is left is inside the budget.
/// Asked when the run starts and when a pane's home stops being spoken for ([`forget`]).
///
/// **A pane the arrangement still has is weighed and never taken.** Its conversation is the one
/// somebody is in, and a home removed from under a running provider is that pane's way back gone
/// while its reader watches. So the open panes are what the budget is spent on first, and what is
/// taken is only ever drawn from the rest.
///
/// **Nothing is taken on a store that would not answer.** The rows are the whole of what says which
/// homes are somebody's; weighing without them would put every open pane's home up for removal.
///
/// **The table `AMB-D-897` keeps is not touched.** A task that names a pane whose home has been
/// weighed out goes on naming it: the row is what says who made the task, and whether the
/// conversation behind it can still be opened is the directory's answer rather than the row's.
pub fn rotate() {
    // One weighing at a time. A pane closing and the run starting can ask at once, and two readings
    // of the same root would each take the homes the other had already decided to take — the second
    // one working from a total that was true before the first started.
    static WEIGHING: Mutex<()> = Mutex::new(());
    let Ok(_weighing) = WEIGHING.lock() else { return };

    let Ok(store) = crate::commands::open_store_read() else { return };
    let spoken_for: BTreeSet<String> = match store.saved_layout() {
        Ok(layout) => layout.map(|kept| kept.panes.iter().map(|pane| pane.id.clone()).collect()),
        Err(e) => {
            log::warn!("pane homes are not weighed: {e}");
            return;
        }
    }
    .unwrap_or_default();
    let roots: Vec<PathBuf> = KINDS.iter().filter_map(homes_root).collect();
    rotate_in(&roots, &spoken_for, BUDGET);
}

/// Where this build's per-pane homes live for one provider, or nothing on a machine whose app-data
/// cannot be resolved.
fn homes_root(kind: &Kind) -> Option<PathBuf> {
    Some(amenbo_core::config::Paths::resolve().ok()?.base_dir.join(kind.homes))
}

/// Make one home, answering with it — and with nothing where the directory itself could not be made.
///
/// What goes **into** it is [`share`]'s and [`copy_in`]'s, and both are asked on every open rather
/// than only on this one: a home outlives the run that made it, and what it holds does not.
fn make(kind: &Kind, home: &Path) -> Option<PathBuf> {
    let into = home.join(kind.inside);
    if let Err(e) = std::fs::create_dir_all(&into) {
        log::warn!("no {} home at {}: {e}", kind.agent, into.display());
        return None;
    }
    Some(home.to_path_buf())
}

/// Link the reader's own halves into a pane's home — every name that is not already reached from
/// there.
///
/// **It is asked on every open and not only on the one that made the home** (`AMB-T-4710`). A name
/// linked into a home can stop being one while the pane runs, and the provider is the one that takes
/// it: Gemini's own keychain unlinks the credentials file when the last of them is deleted, which is
/// what a logout does. What goes is this pane's link; the reader's file stays where it is. Left
/// until the next home was made, that pane would keep running against a home that shares nothing,
/// taking none of the reader's later changes and saying nothing about it.
///
/// **A name already reached from the home is left exactly as it is.** What is asked of it is
/// [`std::fs::symlink_metadata`] and not whether it can be opened: a link to a name a login has yet
/// to make ([`Kind::made`]) answers nothing on the far side and is still the link that login will go
/// through, and relinking it every open would be work for nothing at best.
///
/// **A link that fails is a warning and not a refusal.** What it costs is one of the things in
/// [`Kind::shared`]: a pane that logs in again, or comes up without the reader's skills. That is
/// worth saying in the log and is not worth refusing to open a terminal over. **What it costs is not
/// the same on both rows**: Codex opens without its links and Gemini does not open at all, stopping
/// on the first two of its. Which is why [`link`] is spelled the way it is on Windows, where until
/// `AMB-D-878` this was the ordinary outcome rather than the odd one.
fn share(kind: &Kind, into: &Path, theirs: &Path) {
    for name in kind.linked() {
        let at = into.join(name);
        if at.symlink_metadata().is_ok() {
            continue;
        }
        let from = theirs.join(name);
        // What the reader does not have is not linked to: a link to nothing is a path the
        // provider would read as a file it cannot open, rather than as a file that is not
        // there. The exception is a name a login makes (`Kind::made`, `AMB-D-880`) — that one
        // is linked to where the file will be, so that the login lands in the reader's own
        // directory instead of in this home.
        if !from.exists() {
            if !kind.made.contains(&name) {
                continue;
            }
            // A reader who has never run this provider has no directory either, and a write
            // through the link needs one to land in.
            if let Some(parent) = from.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    log::warn!("{name} is not shared into {}: {e}", into.display());
                    continue;
                }
            }
        }
        if let Err(e) = link(&from, &at) {
            log::warn!("{name} is not shared into {}: {e}", into.display());
        }
    }
}

/// Carry the reader's own copy of each of these into a pane's home, replacing what an earlier open
/// left there.
///
/// **What is standing there is taken away before anything is written.** It may be the hard link an
/// earlier build left at that name, and a copy onto that is a copy onto the reader's own file —
/// opened for truncation and then read from, which empties it rather than shares it. Removing the
/// name first costs nothing in the ordinary case and is the whole of the difference in that one.
///
/// **A reader with none of their own leaves the pane with none either.** The copy an earlier open
/// took is not what the file is now; it is what it was before the person deleted it.
fn copy_in(names: &[&str], into: &Path, theirs: &Path) {
    for name in names {
        let at = into.join(name);
        if at.exists() {
            if let Err(e) = std::fs::remove_file(&at) {
                log::warn!("{name} is not carried into {}: {e}", into.display());
                continue;
            }
        }
        let from = theirs.join(name);
        if !from.exists() {
            continue;
        }
        if let Err(e) = std::fs::copy(&from, &at) {
            log::warn!("{name} is not carried into {}: {e}", into.display());
        }
    }
}

/// One entry of the reader's own directory, reached from inside a pane's home.
#[cfg(unix)]
fn link(from: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, at)
}

/// The same, on the operating system where a symbolic link is the one kind a standard user cannot
/// make.
///
/// `symlink_dir` and `symlink_file` need a privilege this process deliberately does not ask for
/// (`crate::launch`), so with developer mode off they made not one link of the seven and the pane
/// came up on an empty home — a Codex asking to be logged into again, a Gemini that would not open
/// (`AMB-T-4643`). A junction and a hard link were measured being made, and followed, by that same
/// standard user, so a directory is reached by the one and a file by the other (`AMB-D-878`).
///
/// **A hard link is a second name for the reader's file rather than a pointer to it.** Editing
/// through either name reaches the other, which is what the sharing is for; replacing the file does
/// not, and the two entries a provider replaces rather than edits are `AMB-T-4706`'s and
/// `AMB-T-4707`'s to take out of this list. It also has to be made on the drive the file is already
/// on — the reader's home and Amenbo's app-data both sit under `%USERPROFILE%`.
///
/// **A hard link needs a file to be a second name for**, which is where the two operating systems
/// part on [`Kind::made`]: Unix links to where the login will put the file, and here the file is
/// brought into being first, empty. Empty is not what nothing reads as — `codex login status` says
/// `EOF while parsing a value` against a nought-byte `auth.json` where it would have said `Not
/// logged in` — and it is still the shape taken, because the one alternative measured, `{}`, is
/// answered with `Logged in using ChatGPT` by a reader who is not (`AMB-D-880`).
#[cfg(windows)]
fn link(from: &Path, at: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        junction::create(from, at)
    } else {
        if !from.exists() {
            std::fs::File::create(from)?;
        }
        std::fs::hard_link(from, at)
    }
}

/// [`rotate`] against named roots, a named set of panes and a named budget, so what it weighs and
/// what it takes can be asked of it.
///
/// **What is over the budget is taken from the oldest end until it is not.** The order is the time
/// each home was last written in, which is when its pane was last open with anything being said in
/// it — a home nobody has been back to in a month is the one whose conversation is least likely to
/// be wanted, and it is the reading a directory answers without a row having to be kept anywhere.
fn rotate_in(roots: &[PathBuf], spoken_for: &BTreeSet<String>, budget: u64) {
    let mut taking: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total: u64 = 0;
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        for entry in entries.filter_map(Result::ok) {
            // The type readdir already answered, which says nothing of what a link points at: a
            // home is a directory made here, and a name that is anything else is not weighed.
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let at = entry.path();
            let (bytes, opened) = weigh(&at);
            total += bytes;
            let named = at.file_name().map(|name| name.to_string_lossy().into_owned());
            // Counted either way, and only ever taken from the rest: what an open pane's home costs
            // is as real as any other, and it is the budget's to carry rather than to be spared.
            if named.is_some_and(|frame| spoken_for.contains(&frame)) {
                continue;
            }
            taking.push((opened, bytes, at));
        }
    }
    if total <= budget {
        return;
    }
    taking.sort();
    for (_, bytes, at) in taking {
        if total <= budget {
            return;
        }
        if let Err(e) = std::fs::remove_dir_all(&at) {
            log::warn!("the pane home at {} stayed: {e}", at.display());
            continue;
        }
        total -= bytes;
    }
}

/// What one home comes to on the disk, and the last time anything in it was written.
///
/// **The reader's own is not walked into and not weighed.** Every shared name is a link, and what a
/// link costs is the entry rather than what it points at; walking one would weigh the reader's
/// skills into a pane's home, and would read their file's time as this pane's last word. It is
/// [`std::fs::symlink_metadata`] throughout, which is also what makes the junctions Windows is given
/// entries rather than directories (`AMB-D-878`).
///
/// **A home nothing was said in answers with the time it was made**, which is the floor the walk
/// starts from: a pane opened once and left is older than one that was talked to yesterday, and both
/// have a time.
fn weigh(home: &Path) -> (u64, SystemTime) {
    let mut bytes = 0;
    let mut newest = home.symlink_metadata().and_then(|meta| meta.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut walking = vec![home.to_path_buf()];
    while let Some(dir) = walking.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let at = entry.path();
            let Ok(meta) = at.symlink_metadata() else { continue };
            if let Ok(written) = meta.modified() {
                newest = newest.max(written);
            }
            if meta.is_dir() {
                walking.push(at);
            } else {
                bytes += meta.len();
            }
        }
    }
    (bytes, newest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row all of the making is asked of, and the row whose way back is down.
    const GEMINI: &str = "gemini-cli";
    const CODEX: &str = "codex-cli";

    /// What one open does to a home: make it where it is not there, and share the reader's own into
    /// it — [`for_pane`]'s pair, without the parts of it that read the device.
    ///
    /// The tests call this rather than [`make`] because the two are what an open is, and because the
    /// half that is asked **every** time is the half half of them are about (`AMB-T-4710`).
    fn opened(kind: &Kind, home: &Path, theirs: Option<&Path>) -> Option<PathBuf> {
        let home = if home.is_dir() { home.to_path_buf() } else { make(kind, home)? };
        if let Some(theirs) = theirs {
            share(kind, &home.join(kind.inside), theirs);
        }
        Some(home)
    }

    /// The row for a provider, whether or not it is one being given homes today — the table itself,
    /// which the tests below make homes against directly.
    fn kind(agent: &str) -> &'static Kind {
        KINDS.iter().find(|kind| kind.agent == agent).expect(agent)
    }

    /// The reader's own directory for one provider, with everything it shares in it — files as
    /// files, and the ones that are directories as directories, since a link to one is spelled
    /// differently on Windows.
    fn theirs(kind: &Kind) -> PathBuf {
        let dir = amenbo_scratch::scratch("pane-theirs").join(kind.agent);
        for name in kind.shared.iter().chain(kind.points.iter().map(|(_, name)| name)) {
            let at = dir.join(name);
            if name.contains('.') {
                std::fs::create_dir_all(dir.as_path()).unwrap();
                std::fs::write(&at, name).unwrap();
            } else {
                std::fs::create_dir_all(&at).unwrap();
                std::fs::write(at.join("one"), name).unwrap();
            }
        }
        dir
    }

    /// Assert that one entry of a home is the reader's own rather than a copy of it — which is what
    /// the sharing is for, and the one thing every way of spelling it has in common.
    ///
    /// The spelling is not common to them: a symbolic link on Unix, and on Windows a junction where
    /// the shared name is a directory and a hard link where it is a file (`AMB-D-878`). A hard link
    /// is not a link to anything and does not read back as one, so asking after the kind of the
    /// entry would be asking a different question on each. What is asked instead is the reach —
    /// something written at the reader's end is read at the pane's.
    fn shares(at: &Path, theirs: &Path) {
        let (written, read) = if theirs.is_dir() {
            (theirs.join("later"), at.join("later"))
        } else {
            (theirs.to_path_buf(), at.to_path_buf())
        };
        std::fs::write(&written, "written at the reader's end").unwrap();
        assert_eq!(
            std::fs::read_to_string(&read).unwrap_or_default(),
            "written at the reader's end",
            "{} does not reach {}",
            at.display(),
            theirs.display()
        );
    }

    /// Every provider these homes are made for is one the catalog has a row for — so a rename there
    /// is a test failure rather than a pane that silently stops getting a home.
    #[test]
    fn the_providers_this_is_for_are_catalogued() {
        for kind in KINDS {
            assert!(amenbo_core::harness::find_launch(kind.agent).is_some(), "{}", kind.agent);
        }
    }

    /// A home is empty of the provider's own state and full of the reader's: each linked name is
    /// reached through it, and reading one reaches the reader's own file.
    #[test]
    fn a_home_shares_what_the_reader_configured() {
        for kind in KINDS {
            let theirs = theirs(kind);
            let home = amenbo_scratch::scratch("pane-home").join(kind.agent).join("7");

            assert_eq!(opened(kind, &home, Some(&theirs)).as_deref(), Some(home.as_path()));
            for name in kind.linked() {
                let at = home.join(kind.inside).join(name);
                // Reading through it reaches the reader's own — the entry itself where it is a
                // file, and what is inside it where the shared name is a directory.
                let read = if name.contains('.') { at.clone() } else { at.join("one") };
                assert_eq!(std::fs::read_to_string(&read).unwrap(), name, "{}: {name}", kind.agent);
                shares(&at, &theirs.join(name));
            }
        }
    }

    /// Gemini's variable names a home to build `.gemini` inside of rather than the directory itself,
    /// so what is shared sits a level in — and what the pane is handed is still the home
    /// (`AMB-T-4677`).
    #[test]
    fn the_gemini_home_holds_the_providers_own_directory() {
        let kind = kind(GEMINI);
        let theirs = theirs(kind);
        let home = amenbo_scratch::scratch("pane-home-gemini").join("7");

        opened(kind, &home, Some(&theirs)).unwrap();

        shares(&home.join(".gemini/settings.json"), &theirs.join("settings.json"));
        assert!(home.join("settings.json").symlink_metadata().is_err(), "shared at the home itself");
    }

    /// What the reader does not have is left out rather than linked to: a link to a file that is not
    /// there reads as one that cannot be opened, which is a different thing to say.
    #[test]
    fn what_the_reader_has_none_of_is_not_linked_to() {
        let kind = kind(GEMINI);
        let theirs = theirs(kind);
        std::fs::remove_file(theirs.join("settings.json")).unwrap();
        let home = amenbo_scratch::scratch("pane-home-missing").join("7");

        opened(kind, &home, Some(&theirs)).unwrap();

        assert!(home.join(".gemini/settings.json").symlink_metadata().is_err());
        assert!(home.join(".gemini/gemini-credentials.json").symlink_metadata().is_ok());
    }

    /// The one thing the reader has none of that **is** linked to: the name a login makes.
    ///
    /// This is the test the whole of `AMB-T-4709` is: a reader who has never logged in was asked
    /// again by every pane they opened, because the credentials the first pane wrote went into that
    /// pane's home and left with it. What is read here is the reach in the direction the login goes
    /// — something written at the pane's end arriving in the reader's own directory — which is the
    /// opposite direction to the sharing everywhere else and the only one that matters here.
    #[test]
    fn the_name_a_login_makes_is_linked_to_before_there_is_anything_behind_it() {
        for kind in KINDS {
            let theirs = theirs(kind);
            let home = amenbo_scratch::scratch("pane-home-login").join(kind.agent).join("7");
            for name in kind.made {
                std::fs::remove_file(theirs.join(name)).unwrap();
            }

            opened(kind, &home, Some(&theirs)).unwrap();

            for name in kind.made {
                let at = home.join(kind.inside).join(name);
                assert!(
                    at.symlink_metadata().is_ok(),
                    "{}: {name} is not there for a login to write through",
                    kind.agent
                );
                // Nothing to read yet, the same as a reader who has not logged in.
                assert!(std::fs::read(&at).is_err() || std::fs::read(&at).unwrap().is_empty());
                // And the login lands in the reader's own directory rather than in the home.
                std::fs::write(&at, "what the login wrote").unwrap();
                assert_eq!(
                    std::fs::read_to_string(theirs.join(name)).unwrap(),
                    "what the login wrote",
                    "{}: {name} stayed in the pane's home",
                    kind.agent
                );
            }
        }
    }

    /// A link the provider took away is put back on the next open, and one that is still standing is
    /// left exactly as it was.
    ///
    /// **The taking away is the provider's own doing and not a mishap** (`AMB-T-4710`): Gemini's
    /// keychain unlinks the credentials file as the last of them is deleted, which is what a logout
    /// does. What goes is this pane's name for the reader's file; the reader's file stays. Shared
    /// only on the open that made the home, that pane would go on running against a home that
    /// reaches nothing — taking none of the reader's later changes, and saying nothing about it
    /// until somebody thought to close the pane.
    #[test]
    fn a_link_the_provider_took_away_comes_back_on_the_next_open() {
        let kind = kind(GEMINI);
        let theirs = theirs(kind);
        let home = amenbo_scratch::scratch("pane-home-logged-out").join("7");
        opened(kind, &home, Some(&theirs)).unwrap();
        let into = home.join(kind.inside);
        let settings = into.join("settings.json");
        let credentials = into.join("gemini-credentials.json");

        // The logout: the provider takes its own name for the file away, and nothing else moves.
        std::fs::remove_file(&credentials).unwrap();
        assert!(credentials.symlink_metadata().is_err());

        opened(kind, &home, Some(&theirs)).unwrap();

        assert!(credentials.symlink_metadata().is_ok(), "the pane came back sharing nothing");
        // Reached rather than merely there: what would be useless is a name standing over a copy.
        shares(&credentials, &theirs.join("gemini-credentials.json"));
        // And the one that was never taken away still reaches the reader's own.
        shares(&settings, &theirs.join("settings.json"));
    }

    /// A reader who has never run the provider has no directory either, and the login has to land
    /// in one — so it is made rather than the link being given up on.
    #[test]
    fn a_reader_who_has_never_run_the_provider_is_given_the_directory_the_login_needs() {
        let kind = kind(CODEX);
        let theirs = amenbo_scratch::scratch("pane-theirs-never").join(kind.agent);
        let _ = std::fs::remove_dir_all(&theirs);
        let home = amenbo_scratch::scratch("pane-home-never").join("7");

        opened(kind, &home, Some(&theirs)).unwrap();

        assert!(theirs.is_dir(), "the reader's own directory was not made");
        let at = home.join(kind.inside).join("auth.json");
        std::fs::write(&at, "what the login wrote").unwrap();
        assert_eq!(std::fs::read_to_string(theirs.join("auth.json")).unwrap(), "what the login wrote");
    }

    /// Every name a login makes is one this provider shares, and none of them is one carried in as
    /// a copy: a copy is taken from the reader's file on every open, so a login written into the
    /// pane's copy would be thrown away by the next one.
    #[test]
    fn the_names_a_login_makes_are_shared_names_and_never_copied_ones() {
        for kind in KINDS {
            for name in kind.made {
                assert!(kind.shared.contains(name), "{}: {name}", kind.agent);
                assert!(!kind.copied.contains(name), "{}: {name}", kind.agent);
                // Files, not directories: what is made on Windows against a name with nothing
                // behind it is an empty file, and a junction is the spelling for the other kind.
                assert!(name.contains('.'), "{}: {name} is not a file", kind.agent);
            }
        }
    }

    /// The file the provider replaces is not linked into the home at all: the pane is told where the
    /// reader keeps it, and reads the one file there has ever been (`AMB-D-878`).
    #[test]
    fn the_file_the_provider_replaces_is_pointed_at_rather_than_linked() {
        let kind = kind(GEMINI);
        let theirs = theirs(kind);
        let home = amenbo_scratch::scratch("pane-home-pointed").join("7");

        opened(kind, &home, Some(&theirs)).unwrap();

        assert!(home.join(".gemini/trustedFolders.json").symlink_metadata().is_err());
        assert_eq!(
            vars(kind, home.clone(), Some(&theirs)),
            vec![
                (kind.env, home),
                ("GEMINI_CLI_TRUSTED_FOLDERS_PATH", theirs.join("trustedFolders.json")),
            ]
        );
    }

    /// A file that is pointed at is pointed at whether or not the reader has one yet — the provider
    /// makes it where it is told to, and a variable left out would send it into the pane's home.
    #[test]
    fn a_file_the_reader_has_none_of_is_still_pointed_at() {
        let kind = kind(GEMINI);
        let theirs = theirs(kind);
        std::fs::remove_file(theirs.join("trustedFolders.json")).unwrap();
        let home = amenbo_scratch::scratch("pane-home-pointed-missing").join("7");

        assert_eq!(
            vars(kind, home.clone(), Some(&theirs)).get(1),
            Some(&("GEMINI_CLI_TRUSTED_FOLDERS_PATH", theirs.join("trustedFolders.json")))
        );
    }

    /// The file the provider replaces and offers no variable for is carried in as a copy, and only
    /// where a link would not survive the replacing (`AMB-D-878`).
    #[test]
    fn the_file_that_is_copied_in_is_left_linked_where_a_link_lasts() {
        let kind = kind(CODEX);

        assert_eq!(kind.copied, ["config.toml"]);
        let linked: Vec<_> = kind.linked().collect();
        if cfg!(windows) {
            assert_eq!(kind.copied_here(), kind.copied);
            assert!(!linked.contains(&"config.toml"), "{linked:?}");
        } else {
            assert!(kind.copied_here().is_empty());
            assert!(linked.contains(&"config.toml"), "{linked:?}");
        }
    }

    /// Every file carried in as a copy has its keys carried back out, so a copy is never one
    /// direction only ([`crate::pane_settled`]).
    ///
    /// **This is what the face rests on.** The row under a pane says the press moves the reader's
    /// own default, and it says it on every operating system: a name copied in with nothing carried
    /// back would make that sentence false on Windows alone, which is the hardest kind of wrong to
    /// find.
    #[test]
    fn everything_copied_in_is_carried_back_out() {
        for kind in KINDS {
            for name in kind.copied {
                let carried = kind.carried_back.iter().find(|(file, _)| file == name);
                let (_, keys) = carried.unwrap_or_else(|| panic!("{}: {name}", kind.agent));
                assert!(!keys.is_empty(), "{}: {name}", kind.agent);
            }
            // And nothing is carried back out of a file that was never copied in: there would be no
            // second copy to read it from.
            for (file, _) in kind.carried_back {
                assert!(kind.copied.contains(file), "{}: {file}", kind.agent);
            }
        }
    }

    /// The key the catalog names as the one a `/model` press keeps is the file this row copies, so
    /// the two cannot drift apart: a `keeps` respelled on one side would leave the face naming a
    /// file the pane no longer carries back.
    #[test]
    fn the_file_the_model_press_keeps_is_the_one_the_pane_carries_back() {
        let keeps = amenbo_core::harness::find_launch(CODEX).unwrap().switch.keeps.unwrap();
        let kind = kind(CODEX);

        assert_eq!(keeps, format!("~/{}/{}", kind.theirs, kind.carried_back[0].0));
    }

    /// A copy is taken again on every open, so a pane that was made runs ago is still given what the
    /// person has chosen since.
    #[test]
    fn a_copy_is_what_the_readers_file_is_now() {
        let theirs = amenbo_scratch::scratch("pane-copy-theirs");
        let into = amenbo_scratch::scratch("pane-copy-home");
        std::fs::write(theirs.join("config.toml"), "the model they chose first").unwrap();
        copy_in(&["config.toml"], &into, &theirs);
        std::fs::write(theirs.join("config.toml"), "the model they chose since").unwrap();

        copy_in(&["config.toml"], &into, &theirs);

        assert_eq!(
            std::fs::read_to_string(into.join("config.toml")).unwrap(),
            "the model they chose since"
        );
    }

    /// What is standing at the name is taken away before the copy is written. An earlier build left
    /// a hard link there, and writing onto that is writing into the reader's own file — which is
    /// emptied rather than shared.
    #[test]
    fn a_copy_does_not_reach_back_through_a_link_an_earlier_build_made() {
        let theirs = amenbo_scratch::scratch("pane-copy-linked-theirs");
        let into = amenbo_scratch::scratch("pane-copy-linked-home");
        let own = theirs.join("config.toml");
        std::fs::write(&own, "the model they chose").unwrap();
        std::fs::hard_link(&own, into.join("config.toml")).unwrap();

        copy_in(&["config.toml"], &into, &theirs);

        assert_eq!(std::fs::read_to_string(&own).unwrap(), "the model they chose");
        // And what stands there afterwards is the pane's own file rather than a second name for the
        // reader's: what the pane settles in it stays in the pane.
        std::fs::write(into.join("config.toml"), "what the pane settled").unwrap();
        assert_eq!(std::fs::read_to_string(&own).unwrap(), "the model they chose");
    }

    /// A file the reader has deleted leaves the pane with none either, rather than with the copy an
    /// earlier open took of it.
    #[test]
    fn a_copy_goes_when_the_reader_has_none_of_their_own() {
        let theirs = amenbo_scratch::scratch("pane-copy-gone-theirs");
        let into = amenbo_scratch::scratch("pane-copy-gone-home");
        std::fs::write(theirs.join("config.toml"), "the model they chose").unwrap();
        copy_in(&["config.toml"], &into, &theirs);
        std::fs::remove_file(theirs.join("config.toml")).unwrap();

        copy_in(&["config.toml"], &into, &theirs);

        assert!(!into.join("config.toml").exists());
    }

    /// A machine whose home directory cannot be resolved leaves the pane with its own home and
    /// nothing else: there is no path to name.
    #[test]
    fn a_pane_with_no_readers_directory_is_told_its_home_alone() {
        let kind = kind(GEMINI);
        let home = amenbo_scratch::scratch("pane-home-unpointed").join("7");

        assert_eq!(vars(kind, home.clone(), None), vec![(kind.env, home)]);
    }

    /// A home is made once. Asked for again, the same directory comes back with the reader's own
    /// halves still reaching where they did — a second making would have to decide what to do with a
    /// pane's conversation, and there is nothing right to decide.
    #[test]
    fn a_home_that_is_already_there_is_the_one_that_comes_back() {
        let kind = kind(GEMINI);
        let theirs = theirs(kind);
        let home = amenbo_scratch::scratch("pane-home-again").join("7");
        opened(kind, &home, Some(&theirs)).unwrap();
        let talk = home.join(".gemini/chat.jsonl");
        std::fs::write(&talk, "a conversation").unwrap();

        opened(kind, &home, Some(&theirs)).unwrap();

        assert_eq!(std::fs::read_to_string(&talk).unwrap(), "a conversation");
        shares(&home.join(".gemini/settings.json"), &theirs.join("settings.json"));
    }

    /// A pane that is gone leaves its home where it is, conversation and all (`AMB-D-898`). These two
    /// come back by the place they ran in, so a home taken away when the pane closed would be the
    /// conversation taken away with it — which is what every other provider here does not do.
    ///
    /// **Both rows**, because both are given homes and neither is the reader's to lose.
    #[test]
    fn a_pane_that_is_gone_leaves_its_home_standing() {
        for kind in KINDS {
            let theirs = theirs(kind);
            let root = amenbo_scratch::scratch("pane-homes-gone").join(kind.agent);
            let home = root.join("7");
            opened(kind, &home, Some(&theirs)).unwrap();
            let talk = home.join(kind.inside).join("what-was-said");
            std::fs::write(&talk, "a conversation").unwrap();

            forget(&home);

            assert!(home.is_dir(), "{}", kind.agent);
            assert_eq!(
                std::fs::read_to_string(&talk).unwrap(),
                "a conversation",
                "{}: the conversation is still there",
                kind.agent
            );
        }
    }

    /// Homes made in this order, each with a hundred bytes said in it — oldest first, so what the
    /// weighing reads off the directories is the order they were last written in.
    fn three_homes(root: &Path, kind: &Kind) {
        for frame in ["oldest", "middle", "newest"] {
            let home = root.join(frame);
            opened(kind, &home, None).unwrap();
            std::fs::write(home.join(kind.inside).join("what-was-said"), [b'x'; 100]).unwrap();
            // The order is the whole of what this test is about, and two writes inside one tick of
            // the filesystem's clock would come back in either order.
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Over the budget, homes go from the least recently opened end, and the taking stops the moment
    /// what is left is inside it — the rest of the conversations stay (`AMB-D-898`).
    #[test]
    fn what_is_over_the_budget_goes_from_the_oldest_end() {
        let kind = kind(GEMINI);
        let root = amenbo_scratch::scratch("pane-homes-weighed");
        three_homes(&root, kind);

        // Three hundred bytes of conversation against a budget that holds two of them.
        rotate_in(std::slice::from_ref(&root), &BTreeSet::new(), 250);

        assert!(!root.join("oldest").exists(), "the least recently opened one went");
        assert!(root.join("middle").is_dir(), "and the taking stopped there");
        assert!(root.join("newest").is_dir());
    }

    /// A home the arrangement still has a pane for is never the one taken, however old it is: its
    /// conversation is the one somebody is in, and a provider whose home went while it ran is a pane
    /// that cannot come back while its reader watches.
    #[test]
    fn a_pane_that_is_still_open_keeps_its_home() {
        let kind = kind(GEMINI);
        let root = amenbo_scratch::scratch("pane-homes-open");
        three_homes(&root, kind);

        rotate_in(std::slice::from_ref(&root), &BTreeSet::from(["oldest".to_string()]), 250);

        assert!(root.join("oldest").is_dir(), "the open pane's home stayed");
        assert!(!root.join("middle").exists(), "and what was taken was the oldest of the rest");
        assert!(root.join("newest").is_dir());
    }

    /// Inside the budget nothing is taken at all — a weighing is not a reason to lose a conversation.
    #[test]
    fn nothing_is_taken_while_the_homes_are_inside_the_budget() {
        let kind = kind(GEMINI);
        let root = amenbo_scratch::scratch("pane-homes-inside");
        three_homes(&root, kind);

        rotate_in(std::slice::from_ref(&root), &BTreeSet::new(), 1_000);

        for frame in ["oldest", "middle", "newest"] {
            assert!(root.join(frame).is_dir(), "{frame}");
        }
    }

    /// What a weighed-out home takes with it is its own entries, never what they are a second name
    /// for — the reader's own directory is left exactly as it was.
    ///
    /// **Both rows, because the shared names that are directories are only on one of them.** On
    /// Windows those are junctions, and a removal that walked into one would empty the reader's own
    /// skills and prompts rather than the pane's way to them.
    #[test]
    fn what_goes_is_the_home_and_never_the_readers_own() {
        for kind in KINDS {
            let theirs = theirs(kind);
            let root = amenbo_scratch::scratch("pane-homes-taken").join(kind.agent);
            let home = root.join("7");
            opened(kind, &home, Some(&theirs)).unwrap();

            // A budget nothing fits inside, so the one home there is, is the one taken.
            rotate_in(std::slice::from_ref(&root), &BTreeSet::new(), 0);

            assert!(!home.exists(), "{}", kind.agent);
            for name in kind.shared {
                let at = theirs.join(name);
                assert!(at.exists(), "{}: the reader's own {name} is untouched", kind.agent);
                if at.is_dir() {
                    assert!(at.join("one").exists(), "{}: and so is what is in it", kind.agent);
                }
            }
        }
    }

    /// A shared name weighs what the entry weighs and not what it points at. The reader's own
    /// directory is not a pane's to be charged for — it is one directory reached from every home
    /// there is, and walking into it would put the same bytes on all of them.
    #[test]
    fn the_readers_own_is_not_weighed_into_a_home() {
        let kind = kind(CODEX);
        let theirs = theirs(kind);
        let home = amenbo_scratch::scratch("pane-homes-weight").join("7");
        opened(kind, &home, Some(&theirs)).unwrap();
        std::fs::write(theirs.join("skills").join("heavy"), [b'x'; 100_000]).unwrap();

        let (bytes, _) = weigh(&home);

        assert!(bytes < 100_000, "the home weighs {bytes}, which is the reader's own weighed in");
    }

    /// A pane is given a home while the catalog says it comes back by the place it runs in, and not
    /// otherwise. Both rows are on it (`AMB-T-4679`); what the asking is for is the day one of them
    /// comes down again, the way Codex did (`AMB-T-4678`) — keeping a home for a pane that cannot
    /// come back would pay `AMB-D-869`'s price, writes reaching the reader's own directory, for
    /// nothing.
    #[test]
    fn a_home_is_made_for_the_rows_that_come_back_by_one() {
        for agent in [GEMINI, CODEX] {
            assert!(kind_of(Some(agent)).is_some(), "{agent}");
        }
    }

    /// Only a provider that comes back by a place gets one. The others are resumed by a session id
    /// and have no use for a directory, and a pane at a plain prompt has nothing to resume at all.
    #[test]
    fn nothing_but_a_row_that_comes_back_by_a_place_is_given_a_home() {
        let frame = "7b3f0c1e-2d4a-4c88-9a51-6e0d2f83b114";
        assert!(for_pane(frame, None).is_none());
        assert!(for_pane(frame, Some("claude-code")).is_none());
    }

    /// A frame id is one the window drew. Anything else is refused rather than made a directory for
    /// — a name with a path in it would write outside the root this module answers for, and the
    /// numbers an older build counted are gone by the time anything asks here.
    #[test]
    fn a_frame_that_is_not_a_drawn_id_is_refused() {
        for frame in [
            "../elsewhere",
            "",
            "1",
            // A drawn id with a path hung off it, and one a separator is hiding inside.
            "7b3f0c1e-2d4a-4c88-9a51-6e0d2f83b114/../elsewhere",
            "7b3f0c1e-2d4a-4c88-9a51-6e0d2f83b11/4",
            // The right shape in the wrong alphabet: a case-folding filesystem would let two ids
            // that differ only in case name one directory.
            "7B3F0C1E-2D4A-4C88-9A51-6E0D2F83B114",
            // And the shape itself, missed in each direction.
            "7b3f0c1e2d4a4c889a516e0d2f83b114",
            "7b3f0c1e-2d4a-4c88-9a51-6e0d2f83b114-0000",
        ] {
            assert!(!is_drawn_id(frame), "{frame:?}");
            assert!(for_pane(frame, Some(GEMINI)).is_none(), "{frame:?}");
        }
        assert!(is_drawn_id("7b3f0c1e-2d4a-4c88-9a51-6e0d2f83b114"));
    }
}
