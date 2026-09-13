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
//! **What is made here is tidied here.** A home outlives the run because the pane does, so it is
//! taken away when the pane is ([`crate::pane_home::forget`]) and what an ended run left behind is
//! cleared by the next one ([`crate::pane_home::sweep`]) — the same shape `crate::pty::sweep` clears the drop boxes with. Both doors
//! answer for every root, including the rows whose way back is down: a home an earlier build made is
//! swept whether or not a pane would be given one today.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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
    /// What each home shares with the reader's own, by symbolic link.
    ///
    /// **Measured rather than documented**: each of these was watched being needed
    /// (`AMB-T-4633`, `AMB-T-4677`). A provider adding a file to its directory in some later version
    /// is a file that will not be shared until this list says so, which is the cost `AMB-D-869`
    /// accepted for not reading its internals.
    shared: &'static [&'static str],
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
        points: &[],
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
        // The third thing the pane stops without — it exits on a folder it has not been told to
        // trust, and its TUI asks about the folder every time (`AMB-T-4677`) — and the one the
        // provider replaces rather than writes into, so it is pointed at (`AMB-D-878`).
        points: &[("GEMINI_CLI_TRUSTED_FOLDERS_PATH", "trustedFolders.json")],
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

/// The home the AI in this frame runs in, made and linked the first time it is asked for, with the
/// variables the pane is started with: the one it is told its home by, and one for each file it is
/// pointed at where the reader keeps it ([`Kind::points`]). Nothing at all for a pane running
/// anything else.
///
/// **Keyed by the frame rather than by the session**, because what comes back is the place: a pane
/// resumed in the next run is the same frame with a new process in it, and the home it is pointed at
/// has to be the one its conversation is in. Frame ids are counted up and never reused
/// (`app/src/talk/layout.ts`), so one home is one pane for as long as both exist.
///
/// A frame id that is not a plain number is refused rather than made a directory for: it arrives from
/// the window, and what a name would do here is write outside the directory this module answers for.
pub fn for_pane(frame: &str, agent: Option<&str>) -> Option<(Vec<(&'static str, PathBuf)>, PathBuf)> {
    let kind = kind_of(agent)?;
    if frame.is_empty() || !frame.bytes().all(|byte| byte.is_ascii_digit()) {
        log::warn!("no {} home for frame {frame:?}: a frame is a number", kind.agent);
        return None;
    }
    let home = homes_root(kind)?.join(frame);
    let theirs = amenbo_core::env::home_dir().map(|dir| dir.join(kind.theirs));
    // A home that is already there is the one that comes back, links and all; only the first ask
    // makes one. The variables are read off the row either way — they are what the pane is started
    // with, not something the making leaves behind.
    let home = if home.is_dir() { home } else { make(kind, &home, theirs.as_deref())? };
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

/// Take one pane's home away, once the pane is gone (`crate::frames`).
///
/// **It answers only for directories it made.** What is handed in is a pane's resume handle, and the
/// handles of the other five providers are session ids rather than paths — one of those names
/// nothing under this root and is let go without a filesystem call.
pub fn forget(handle: &Path) {
    for kind in KINDS {
        let Some(root) = homes_root(kind) else { continue };
        forget_in(&root, handle);
    }
}

/// Clear the homes of panes that are no longer in the arrangement. Call it once, off the launch path.
///
/// A pane that is closed while the app is up takes its home with it ([`forget`]); a run that ends
/// without that happening — a quit with panes open, a crash — leaves the directory behind, and the
/// row that named it goes at the same time. So the reckoning is done from the other end: the rows the
/// store kept are the homes there is still a way back into, and everything else under the root is a
/// pane nothing can return to.
///
/// **Rows and not age settle it.** Only this build writes under this root — a dev channel keeps its
/// app-data elsewhere, and only one Amenbo of a channel runs at a time (`crate::single_instance`) —
/// so there is no other run's home here to be wrong about.
pub fn sweep() {
    let Ok(store) = crate::commands::open_store_read() else { return };
    let kept = match store.saved_layout() {
        Ok(layout) => layout.map(|kept| kept.panes.iter().map(|pane| pane.id.clone()).collect()),
        // Nothing is taken away on a store that would not answer: the rows are the whole of what says
        // which homes are still somebody's, and sweeping without them would empty the roots.
        Err(e) => {
            log::warn!("pane homes are not swept: {e}");
            return;
        }
    };
    let kept = kept.unwrap_or_default();
    for kind in KINDS {
        let Some(root) = homes_root(kind) else { continue };
        sweep_in(&root, &kept);
    }
}

/// Where this build's per-pane homes live for one provider, or nothing on a machine whose app-data
/// cannot be resolved.
fn homes_root(kind: &Kind) -> Option<PathBuf> {
    Some(amenbo_core::config::Paths::resolve().ok()?.base_dir.join(kind.homes))
}

/// Make one home and link the reader's own halves into it, answering with it — and with nothing where
/// the directory itself could not be made.
///
/// **A link that fails is a warning and not a refusal.** What it costs is one of the things in
/// [`Kind::shared`]: a pane that logs in again, or comes up without the reader's skills. That is
/// worth saying in the log and is not worth refusing to open a terminal over — and on Windows, where
/// a symbolic link needs a privilege this process deliberately does not ask for (`crate::launch`),
/// it is the ordinary outcome rather than the odd one. **What that costs is not the same on both
/// rows**: Codex opens without its links and Gemini stops on the first two of its, so the Windows
/// answer for this row is `AMB-T-4687`'s to give.
fn make(kind: &Kind, home: &Path, theirs: Option<&Path>) -> Option<PathBuf> {
    let into = home.join(kind.inside);
    if let Err(e) = std::fs::create_dir_all(&into) {
        log::warn!("no {} home at {}: {e}", kind.agent, into.display());
        return None;
    }
    if let Some(theirs) = theirs {
        for name in kind.shared {
            let from = theirs.join(name);
            // What the reader does not have is not linked to: a link to nothing is a path the
            // provider would read as a file it cannot open, rather than as a file that is not there.
            if !from.exists() {
                continue;
            }
            if let Err(e) = link(&from, &into.join(name)) {
                log::warn!("{name} is not shared into {}: {e}", into.display());
            }
        }
    }
    Some(home.to_path_buf())
}

/// One entry of the reader's own directory, reached from inside a pane's home.
#[cfg(unix)]
fn link(from: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, at)
}

/// The same, on the operating system that spells a link to a directory differently from a link to a
/// file and refuses both to a process holding no privilege for them.
#[cfg(windows)]
fn link(from: &Path, at: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::os::windows::fs::symlink_dir(from, at)
    } else {
        std::os::windows::fs::symlink_file(from, at)
    }
}

/// [`forget`] against a named root, so what it will and will not take away can be asked of it.
fn forget_in(root: &Path, handle: &Path) {
    if handle.parent() != Some(root) {
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(handle) {
        log::warn!("the pane home at {} stayed: {e}", handle.display());
    }
}

/// [`sweep`] against a named root and a named set of frames, so it can be asked what it clears.
fn sweep_in(root: &Path, kept: &BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let named = path.file_name().map(|name| name.to_string_lossy().into_owned());
        if named.is_some_and(|frame| kept.contains(&frame)) {
            continue;
        }
        let _ = std::fs::remove_dir_all(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row all of the making is asked of, and the row whose way back is down.
    const GEMINI: &str = "gemini-cli";
    const CODEX: &str = "codex-cli";

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

    /// Every provider these homes are made for is one the catalog has a row for — so a rename there
    /// is a test failure rather than a pane that silently stops getting a home.
    #[test]
    fn the_providers_this_is_for_are_catalogued() {
        for kind in KINDS {
            assert!(amenbo_core::harness::find_launch(kind.agent).is_some(), "{}", kind.agent);
        }
    }

    /// A home is empty of the provider's own state and full of the reader's: each shared name is
    /// reached through it, and reading one reaches the reader's own file.
    #[test]
    fn a_home_shares_what_the_reader_configured() {
        for kind in KINDS {
            let theirs = theirs(kind);
            let home = amenbo_scratch::scratch("pane-home").join(kind.agent).join("7");

            assert_eq!(make(kind, &home, Some(&theirs)).as_deref(), Some(home.as_path()));
            for name in kind.shared {
                let at = home.join(kind.inside).join(name);
                let kind_of_file = at.symlink_metadata().unwrap().file_type();
                assert!(kind_of_file.is_symlink(), "{}: {name} is a link", kind.agent);
                assert_eq!(std::fs::read_link(&at).unwrap(), theirs.join(name));
                // And reading through the link reaches the reader's own — the entry itself where it
                // is a file, and what is inside it where the shared name is a directory.
                let read = if name.contains('.') { at } else { at.join("one") };
                assert_eq!(std::fs::read_to_string(&read).unwrap(), *name, "{}: {name}", kind.agent);
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

        make(kind, &home, Some(&theirs)).unwrap();

        assert!(home.join(".gemini/settings.json").symlink_metadata().unwrap().file_type().is_symlink());
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

        make(kind, &home, Some(&theirs)).unwrap();

        assert!(home.join(".gemini/settings.json").symlink_metadata().is_err());
        assert!(home.join(".gemini/gemini-credentials.json").symlink_metadata().is_ok());
    }

    /// The file the provider replaces is not linked into the home at all: the pane is told where the
    /// reader keeps it, and reads the one file there has ever been (`AMB-D-878`).
    #[test]
    fn the_file_the_provider_replaces_is_pointed_at_rather_than_linked() {
        let kind = kind(GEMINI);
        let theirs = theirs(kind);
        let home = amenbo_scratch::scratch("pane-home-pointed").join("7");

        make(kind, &home, Some(&theirs)).unwrap();

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
        make(kind, &home, Some(&theirs)).unwrap();
        let talk = home.join(".gemini/chat.jsonl");
        std::fs::write(&talk, "a conversation").unwrap();

        make(kind, &home, Some(&theirs)).unwrap();

        assert_eq!(std::fs::read_to_string(&talk).unwrap(), "a conversation");
        assert_eq!(
            std::fs::read_link(home.join(".gemini/settings.json")).unwrap(),
            theirs.join("settings.json")
        );
    }

    /// A home goes when its pane does, and the reader's own directory is not followed on the way out
    /// — what is removed is the links, never what they point at.
    #[test]
    fn a_pane_that_is_gone_takes_its_home_and_nothing_else() {
        let kind = kind(GEMINI);
        let theirs = theirs(kind);
        let root = amenbo_scratch::scratch("pane-homes-gone");
        let home = root.join("7");
        make(kind, &home, Some(&theirs)).unwrap();

        forget_in(&root, &home);

        assert!(!home.exists());
        assert!(theirs.join("settings.json").exists(), "the reader's own is untouched");
    }

    /// A handle that names nothing under the root is let go of rather than acted on — which is every
    /// one of the providers whose handle is a session id.
    #[test]
    fn a_handle_that_is_not_a_home_is_left_alone() {
        let root = amenbo_scratch::scratch("pane-homes-elsewhere-root");
        let elsewhere = amenbo_scratch::scratch("pane-homes-elsewhere");
        std::fs::write(elsewhere.join("keep"), "not ours").unwrap();

        forget_in(&root, Path::new("0f9c-a session id"));
        forget_in(&root, &elsewhere);

        assert!(elsewhere.join("keep").exists());
    }

    /// The sweep keeps a home for every pane the store still has a row for, and clears the rest — the
    /// panes a run that ended badly never got to close.
    #[test]
    fn the_sweep_keeps_the_panes_that_came_back() {
        let kind = kind(GEMINI);
        let root = amenbo_scratch::scratch("pane-homes-swept");
        for frame in ["1", "2", "3"] {
            make(kind, &root.join(frame), None).unwrap();
        }

        sweep_in(&root, &BTreeSet::from(["1".to_string(), "3".to_string()]));

        assert!(root.join("1").is_dir());
        assert!(!root.join("2").exists());
        assert!(root.join("3").is_dir());
    }

    /// An arrangement with no panes in it leaves no homes: every one of them is a pane there is no
    /// row for.
    #[test]
    fn the_sweep_of_an_empty_arrangement_clears_the_root() {
        let root = amenbo_scratch::scratch("pane-homes-empty");
        make(kind(GEMINI), &root.join("1"), None).unwrap();

        sweep_in(&root, &BTreeSet::new());

        assert!(!root.join("1").exists());
        assert!(root.is_dir(), "the root itself stands");
    }

    /// A pane is given a home while the catalog says it comes back by the place it runs in, and not
    /// otherwise. Codex is the row that is not, for now (`AMB-T-4678`): keeping its home while it
    /// cannot come back would pay `AMB-D-869`'s price — writes reaching the reader's own directory —
    /// for a way back the pane no longer has.
    #[test]
    fn a_home_is_made_for_the_rows_that_come_back_by_one() {
        assert!(kind_of(Some(GEMINI)).is_some());
        assert!(kind_of(Some(CODEX)).is_none(), "a home for a row whose way back is down");
    }

    /// Only a provider that comes back by a place gets one. The others are resumed by a session id
    /// and have no use for a directory, and a pane at a plain prompt has nothing to resume at all.
    #[test]
    fn nothing_but_a_row_that_comes_back_by_a_place_is_given_a_home() {
        assert!(for_pane("1", None).is_none());
        assert!(for_pane("1", Some("claude-code")).is_none());
    }

    /// A frame id is a number counted up by the window. Anything else is refused rather than made a
    /// directory for — a name with a path in it would write outside the root this module answers for.
    #[test]
    fn a_frame_that_is_not_a_number_is_refused() {
        assert!(for_pane("../elsewhere", Some(GEMINI)).is_none());
        assert!(for_pane("", Some(GEMINI)).is_none());
    }
}
