//! A directory of its own for the Codex in each pane, so two panes on one folder are two sessions
//! (`AMB-D-869`).
//!
//! **Codex is the one provider that is not resumed by a name.** The other five take a session id on
//! the launch line; Codex takes `resume --last`, and "last" is counted per working folder — so two
//! panes opened on one repository come back to the same conversation, and one of the two people
//! reading them is looking at somebody else's (`AMB-T-4630`). Pointing each pane at a `CODEX_HOME` of
//! its own is what parts them: the only sessions in a home are that pane's, so "the last one" is
//! structurally the right one and no id has to be issued or read (`AMB-T-4633`).
//!
//! **A home is made empty, and what the person configured is linked into it.** A bare home is a
//! Codex that has to be logged into again, on a model nobody chose, with none of the reader's own
//! skills — and it weighs 29MB a pane, against 2.3–4.3MB with the links in (`SHARED` below,
//! `AMB-T-4633`). The link is the point rather than a copy: settings the person edits by hand go on
//! working, and so does an authentication that is refreshed.
//!
//! **The same link carries writes back out.** Codex writes a folder's `trust_level` into the
//! `config.toml` it is pointed at, so trusting a folder in a pane reaches the reader's own file — as
//! does anything else Codex settles there. That is the price of sharing the file, and it is
//! `AMB-D-869`'s to have paid; what is owed here is that the list below is not widened without the
//! same question being asked again.
//!
//! **What is made here is tidied here.** A home outlives the run because the pane does, so it is
//! taken away when the pane is ([`crate::codex_home::forget`]) and what an ended run left behind is cleared by the next
//! one ([`crate::codex_home::sweep`]) — the same shape `crate::pty::sweep` clears the drop boxes with.
//!
//! **No pane is pointed at one of these at the moment** (`AMB-T-4678`). A Codex session was watched
//! not being recorded in a home of this shape, so `resume --last` had nothing to come back into
//! (`AMB-T-4666`) — the way back came off the catalog row, and [`crate::codex_home::for_pane`] came
//! off the launch path with it (`crate::pty::pty_open`). **This module is not dead code**:
//! `AMB-T-4679` points at it again once a home is known to record what a pane said in it, and until
//! then the two tidying doors are still called, for the homes runs before this one left behind.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The variable Codex reads its own directory from.
///
/// Nothing sets it while the way back is down (`AMB-T-4678`); it is held for `AMB-T-4679`.
///
/// It is a floor and not a setting, the way everything else a pane is started with is
/// (`crate::launch`): a profile that exports `CODEX_HOME` is read after this is set and wins, and a
/// reader who has one has pointed all of their Codexes at one place on purpose.
#[allow(dead_code)]
pub const ENV: &str = "CODEX_HOME";

/// The catalog id of the one provider any of this is about
/// ([`amenbo_core::harness::LAUNCHES`]).
const CODEX: &str = "codex-cli";

/// What Codex's own directory is called under the reader's home — the one the links point into.
const THEIRS: &str = ".codex";

/// The directory, under Amenbo's app-data, the per-pane homes are made in. It is app-data and not
/// the temporary directory because a home is how a pane comes back, and it has to outlive the run
/// that made it.
const HOMES: &str = "codex-homes";

/// What each home shares with the reader's own, by symbolic link.
///
/// **Measured rather than documented** (`AMB-T-4633`): each of these was watched being needed. Take
/// `auth.json` out and the pane asks to log in; take `config.toml` out and it comes up on a model
/// nobody chose; take `skills`, `prompts` and `plugins` out and the reader's own are gone from the
/// pane; take `cache` and `models_cache.json` out and the home is 29MB instead of 2.3.
///
/// Codex adding a file to its directory in some later version is a file that will not be shared
/// until this list says so, which is the cost `AMB-D-869` accepted for not reading its internals.
const SHARED: [&str; 7] =
    ["auth.json", "config.toml", "skills", "prompts", "cache", "plugins", "models_cache.json"];

/// The home the Codex in this frame runs in, made and linked the first time it is asked for — and
/// nothing at all for a pane running anything else.
///
/// **Keyed by the frame rather than by the session**, because what comes back is the place: a pane
/// resumed in the next run is the same frame with a new process in it, and the home it is pointed at
/// has to be the one its conversation is in. Frame ids are counted up and never reused
/// (`app/src/talk/layout.ts`), so one home is one pane for as long as both exist.
///
/// A frame id that is not a plain number is refused rather than made a directory for: it arrives from
/// the window, and what a name would do here is write outside the directory this module answers for.
///
/// **Nobody calls this while the way back is down** (`AMB-T-4678`, the module's own note). It is
/// kept whole rather than taken out, because `AMB-T-4679` is the row that calls it again.
#[allow(dead_code)]
pub fn for_pane(frame: &str, agent: Option<&str>) -> Option<PathBuf> {
    if agent != Some(CODEX) {
        return None;
    }
    if frame.is_empty() || !frame.bytes().all(|byte| byte.is_ascii_digit()) {
        log::warn!("no codex home for frame {frame:?}: a frame is a number");
        return None;
    }
    let home = homes_root()?.join(frame);
    if home.is_dir() {
        return Some(home);
    }
    let theirs = amenbo_core::env::home_dir().map(|dir| dir.join(THEIRS));
    make(&home, theirs.as_deref())
}

/// Take one pane's home away, once the pane is gone (`crate::frames`).
///
/// **It answers only for directories it made.** What is handed in is a pane's resume handle, and the
/// handles of the other five providers are session ids rather than paths — one of those names
/// nothing under this root and is let go without a filesystem call.
pub fn forget(handle: &Path) {
    let Some(root) = homes_root() else { return };
    forget_in(&root, handle);
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
    let Some(root) = homes_root() else { return };
    let Ok(store) = crate::commands::open_store_read() else { return };
    let kept = match store.saved_layout() {
        Ok(layout) => layout.map(|kept| kept.panes.iter().map(|pane| pane.id.clone()).collect()),
        // Nothing is taken away on a store that would not answer: the rows are the whole of what says
        // which homes are still somebody's, and sweeping without them would empty the root.
        Err(e) => {
            log::warn!("codex homes are not swept: {e}");
            return;
        }
    };
    sweep_in(&root, &kept.unwrap_or_default());
}

/// Where this build's per-pane homes live, or nothing on a machine whose app-data cannot be resolved.
fn homes_root() -> Option<PathBuf> {
    Some(amenbo_core::config::Paths::resolve().ok()?.base_dir.join(HOMES))
}

/// Make one home and link the reader's own halves into it, answering with it — and with nothing where
/// the directory itself could not be made.
///
/// **A link that fails is a warning and not a refusal.** What it costs is one of the things in
/// [`SHARED`]: a pane that logs in again, or comes up without the reader's skills. That is worth
/// saying in the log and is not worth refusing to open a terminal over — and on Windows, where a
/// symbolic link needs a privilege this process deliberately does not ask for (`crate::launch`), it
/// is the ordinary outcome rather than the odd one.
fn make(home: &Path, theirs: Option<&Path>) -> Option<PathBuf> {
    if let Err(e) = std::fs::create_dir_all(home) {
        log::warn!("no codex home at {}: {e}", home.display());
        return None;
    }
    if let Some(theirs) = theirs {
        for name in SHARED {
            let from = theirs.join(name);
            // What the reader does not have is not linked to: a link to nothing is a path Codex would
            // read as a file it cannot open, rather than as a file that is not there.
            if !from.exists() {
                continue;
            }
            if let Err(e) = link(&from, &home.join(name)) {
                log::warn!("{name} is not shared into {}: {e}", home.display());
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
        log::warn!("the codex home at {} stayed: {e}", handle.display());
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

    /// The reader's own directory, with everything in [`SHARED`] in it — files as files, and the four
    /// that are directories as directories, since a link to one is spelled differently on Windows.
    fn theirs() -> PathBuf {
        let dir = amenbo_scratch::scratch("codex-theirs");
        for name in SHARED {
            let at = dir.join(name);
            if name.contains('.') {
                std::fs::write(&at, name).unwrap();
            } else {
                std::fs::create_dir_all(&at).unwrap();
                std::fs::write(at.join("one"), name).unwrap();
            }
        }
        dir
    }

    /// The provider all of this is about is one the catalog has a row for — so a rename there is a
    /// test failure rather than a pane that silently stops getting a home.
    #[test]
    fn the_provider_this_is_for_is_catalogued() {
        assert!(amenbo_core::harness::find_launch(CODEX).is_some());
    }

    /// A home is empty of Codex's own state and full of the reader's: each of the seven is reached
    /// through it, and reading one reaches the reader's own file.
    #[test]
    fn a_home_shares_what_the_reader_configured() {
        let theirs = theirs();
        let home = amenbo_scratch::scratch("codex-home").join("7");

        assert_eq!(make(&home, Some(&theirs)).as_deref(), Some(home.as_path()));
        for name in SHARED {
            let at = home.join(name);
            assert!(at.symlink_metadata().unwrap().file_type().is_symlink(), "{name} is a link");
            assert_eq!(std::fs::read_link(&at).unwrap(), theirs.join(name));
        }
        assert_eq!(std::fs::read_to_string(home.join("auth.json")).unwrap(), "auth.json");
        assert_eq!(std::fs::read_to_string(home.join("skills/one")).unwrap(), "skills");
    }

    /// What the reader does not have is left out rather than linked to: a link to a file that is not
    /// there reads as one that cannot be opened, which is a different thing to say.
    #[test]
    fn what_the_reader_has_none_of_is_not_linked_to() {
        let theirs = theirs();
        std::fs::remove_file(theirs.join("auth.json")).unwrap();
        std::fs::remove_dir_all(theirs.join("plugins")).unwrap();
        let home = amenbo_scratch::scratch("codex-home").join("7");

        make(&home, Some(&theirs)).unwrap();

        assert!(home.join("auth.json").symlink_metadata().is_err());
        assert!(home.join("plugins").symlink_metadata().is_err());
        assert!(home.join("config.toml").symlink_metadata().is_ok());
    }

    /// A home is made once. Asked for again, the same directory comes back with the reader's own
    /// halves still reaching where they did — a second making would have to decide what to do with a
    /// pane's conversation, and there is nothing right to decide.
    #[test]
    fn a_home_that_is_already_there_is_the_one_that_comes_back() {
        let theirs = theirs();
        let home = amenbo_scratch::scratch("codex-home").join("7");
        make(&home, Some(&theirs)).unwrap();
        std::fs::write(home.join("sessions.jsonl"), "a conversation").unwrap();

        make(&home, Some(&theirs)).unwrap();

        assert_eq!(std::fs::read_to_string(home.join("sessions.jsonl")).unwrap(), "a conversation");
        assert_eq!(std::fs::read_link(home.join("config.toml")).unwrap(), theirs.join("config.toml"));
    }

    /// A home goes when its pane does, and the reader's own directory is not followed on the way out
    /// — what is removed is the links, never what they point at.
    #[test]
    fn a_pane_that_is_gone_takes_its_home_and_nothing_else() {
        let theirs = theirs();
        let root = amenbo_scratch::scratch("codex-homes");
        let home = root.join("7");
        make(&home, Some(&theirs)).unwrap();

        forget_in(&root, &home);

        assert!(!home.exists());
        assert!(theirs.join("skills/one").exists(), "the reader's own is untouched");
    }

    /// A handle that names nothing under the root is let go of rather than acted on — which is every
    /// one of the other five providers, whose handle is a session id.
    #[test]
    fn a_handle_that_is_not_a_home_is_left_alone() {
        let root = amenbo_scratch::scratch("codex-homes");
        let elsewhere = amenbo_scratch::scratch("codex-elsewhere");
        std::fs::write(elsewhere.join("keep"), "not ours").unwrap();

        forget_in(&root, Path::new("0f9c-a session id"));
        forget_in(&root, &elsewhere);

        assert!(elsewhere.join("keep").exists());
    }

    /// The sweep keeps a home for every pane the store still has a row for, and clears the rest — the
    /// panes a run that ended badly never got to close.
    #[test]
    fn the_sweep_keeps_the_panes_that_came_back() {
        let root = amenbo_scratch::scratch("codex-homes");
        for frame in ["1", "2", "3"] {
            make(&root.join(frame), None).unwrap();
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
        let root = amenbo_scratch::scratch("codex-homes");
        make(&root.join("1"), None).unwrap();

        sweep_in(&root, &BTreeSet::new());

        assert!(!root.join("1").exists());
        assert!(root.is_dir(), "the root itself stands");
    }

    /// Only a pane running Codex gets one. The other five are resumed by a session id and have no use
    /// for a directory, and a pane at a plain prompt has nothing to resume at all.
    #[test]
    fn nothing_but_codex_is_given_a_home() {
        assert!(for_pane("1", None).is_none());
        assert!(for_pane("1", Some("claude-code")).is_none());
    }

    /// A frame id is a number counted up by the window. Anything else is refused rather than made a
    /// directory for — a name with a path in it would write outside the root this module answers for.
    #[test]
    fn a_frame_that_is_not_a_number_is_refused() {
        assert!(for_pane("../elsewhere", Some(CODEX)).is_none());
        assert!(for_pane("", Some(CODEX)).is_none());
    }
}
