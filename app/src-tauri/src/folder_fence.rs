//! How far a name may reach: the folders a project is bound to, and the one path under them that a
//! caller's segments are allowed to mean.
//!
//! **The fence is the project's folders, not a session's.** The face's rows belong to the project:
//! the tree does not move when the pane beside it is switched (`AMB-T-3602`). So the root a caller
//! may name is a folder the project is bound to, checked against the store rather than taken on the
//! caller's word, and everything under it is judged the way [`crate::fileproto`] judges a path —
//! segment by segment as text, the folders then resolved against the real filesystem, and the last
//! name left as text for the open to refuse a link at ([`crate::folder_fence::under`], [`crate::folder_fence::open_no_follow`],
//! which every
//! door onto a file shares).

use std::path::{Component, Path, PathBuf};

use amenbo_core::binding::canonical_dir;

use crate::error::CmdError;

/// The path `segments` name inside the folder `roots[base]`, and which of `roots` it belongs to —
/// the fence every door onto a project's folders is built on.
///
/// **The folders above the last name are resolved; the last name is only ever text.** Each segment is
/// checked on its own as a single ordinary name, the ones above the last are then resolved against the
/// real filesystem — a folder that is a link leading out of the project is caught there and nowhere
/// else — and the last name is joined on without being resolved at all. That is what lets a name which
/// does not exist yet be spoken for (a file about to be written), and it is why a link at the end is
/// refused where the file is opened rather than here: [`open_no_follow`] refuses it in the same call
/// that opens it, leaving no window between asking and acting.
///
/// **Which folder it belongs to is the deepest one holding it**, not the one the caller named. A
/// project can be bound to a folder inside another one — this repository binds itself and its plugins
/// — and a file in the inner folder is the inner folder's: that is whose git state it is read from and
/// whose watch reports it (`AMB-D-782`). Taking the first match instead would hand the answer to the
/// order the folders happen to be held in, which is their path names and not anybody's ranking
/// (`AMB-D-531`).
///
/// Whether the answer may be a directory is the caller's to say, since one door hands out bytes and
/// another lists names.
///
/// **Canonical here is the reader's spelling** ([`canonical_dir`], `AMB-D-703`), not
/// `std::fs::canonicalize`'s. On Windows that call answers in the verbatim `\\?\C:\…` form, and a
/// path in that form is not a path every Win32 entry point takes: `SHOpenWithDialog` rejects it
/// outright with `E_INVALIDARG` and draws nothing (`AMB-T-3651` measured it on a real machine).
///
/// **That spelling is not guaranteed past 260 characters**, which is where [`canonical_dir`] stops
/// taking the verbatim front off — a folder bound at 582 characters is answered for in the verbatim
/// form and so is everything under it (`AMB-T-3749` measured it). A door handing a path to the shell
/// therefore spells it with [`plain`] rather than trusting what it was given; below 260 that changes
/// nothing, because what arrives here is already plain.
pub fn under(
    roots: &[PathBuf],
    base: usize,
    segments: impl IntoIterator<Item = impl AsRef<str>>,
) -> Option<(usize, PathBuf)> {
    let mut names = Vec::new();
    for segment in segments {
        // One ordinary name and nothing else. `..`, `.`, an embedded separator, a root and a drive
        // letter all come back as some other kind of component, or as more than one — none of which
        // is a file name.
        let mut parts = Path::new(segment.as_ref()).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(name)), None) => names.push(name.to_os_string()),
            _ => return None,
        }
    }

    let last = names.pop();
    let mut walked = canonical_dir(roots.get(base)?).ok()?;
    walked.extend(names);
    // The filesystem's own answer for the folders, links followed. A link inside the folder that
    // points out of it is only caught here, which is why the text check above is not the end of it.
    let walked = canonical_dir(&walked).ok()?;
    let path = match last {
        Some(name) => walked.join(name),
        None => walked,
    };

    // Of two nested folders, one spelling is a prefix of the other, so the longer one is the deeper.
    // Both sides are levelled first: on Windows one folder has two spellings, and which one a path
    // comes back in depends on how long it is (see `plain`).
    let compared = plain(&path);
    let owner = roots
        .iter()
        .enumerate()
        .filter(|(_, root)| compared.starts_with(plain(root)))
        .max_by_key(|(_, root)| root.as_os_str().len())?
        .0;
    Some((owner, path))
}

/// The one spelling of a path that both halves of Windows agree on — what two paths are compared
/// by, and what is handed to anything outside this process.
///
/// **Windows answers in whichever of two spellings a path's length falls into.** [`canonical_dir`]
/// takes the verbatim front off what the system hands back, but only up to 260 characters — past
/// that it stays on, because that is the only spelling a path that long works in. Two things follow
/// from the same fact:
///
/// - **Comparing.** A folder bound at 249 characters comes back plain and a file 289 characters deep
///   inside it comes back verbatim, and asking whether the second starts with the first is asking
///   whether a path that begins with the verbatim front begins with a drive letter: it does not. The
///   fence would then turn away a file the tree is drawing and the filesystem opens without
///   complaint, and the shape that does it is the most ordinary there is — a folder of ordinary
///   length with one long branch (`AMB-T-3749` measured it; the reading in `AMB-D-771` had it the
///   other way round).
/// - **Handing over.** A folder bound *past* 260 characters comes back verbatim itself, and so does
///   every path [`under`] answers with beneath it. `SHOpenWithDialog` refuses that form with
///   `E_INVALIDARG` and draws nothing at all (`AMB-T-3651`), so a root long enough kills every door
///   that goes out to the shell, while the fence itself is perfectly happy.
///
/// **Below 260 this is the identity**, since what [`canonical_dir`] handed back was already plain —
/// which is why spelling a path here changes nothing for an ordinary folder.
///
/// What it does not do is ask whether the plain spelling names the same file. [`canonical_dir`] does
/// ask, and keeps the verbatim front on for a name Win32 reserves; here there is nothing to be
/// gained by keeping it, because the form the shell refuses is not a form a door can hand over.
#[cfg(windows)]
pub fn plain(path: &Path) -> std::borrow::Cow<'_, Path> {
    match without_verbatim(&path.as_os_str().to_string_lossy()) {
        Some(text) => std::borrow::Cow::Owned(PathBuf::from(text)),
        None => std::borrow::Cow::Borrowed(path),
    }
}

/// Everywhere else a path has one spelling, and a name that really holds those characters is a
/// name somebody wrote.
#[cfg(not(windows))]
pub fn plain(path: &Path) -> std::borrow::Cow<'_, Path> {
    std::borrow::Cow::Borrowed(path)
}

/// The plain spelling of a verbatim Windows path, or nothing when it is not one.
///
/// **The network form is not the disk form with something else in front.** A verbatim path to a
/// share spells it `\\?\UNC\server\share`, so taking the front off and stopping there leaves
/// `UNC\server\share` — which is not the folder, and no longer resembles the `\\server\share` it
/// has to be compared with. That case is read first for exactly that reason.
///
/// Written against a string rather than a path so that the one part of this with two cases in it
/// can be read on every machine — which is also why it is compiled into a test build anywhere and
/// into an ordinary build only where something calls it.
#[cfg(any(windows, test))]
fn without_verbatim(text: &str) -> Option<String> {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return Some(format!(r"\\{rest}"));
    }
    text.strip_prefix(r"\\?\").map(str::to_string)
}

/// Open one file for reading **without following a link at the last name**.
///
/// The fence resolves the folders above a name and leaves the name itself as text ([`under`]), which
/// is what lets a file that is not there yet be named — and what leaves the last hop to be refused
/// here. A `docs/CLAUDE.md` that is really a link to `~/dotfiles/CLAUDE.md` is inside the folder by its
/// spelling and outside it by its bytes; a door that followed it would read, and once there is a way
/// to save, write, somebody's machine-wide configuration through a panel fenced to one project
/// (`AMB-D-782`).
///
/// Asking whether a name is a link and then opening it would leave a window between the two answers.
/// The flag closes it: the kernel refuses in the same call, with `ELOOP`.
#[cfg(unix)]
pub fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

/// Open one file for reading, with a link at the last name opened rather than followed.
///
/// Why the last name is the one to judge is the Unix half's to say. What differs here is that Windows
/// has no flag which refuses: `FILE_FLAG_OPEN_REPARSE_POINT` hands back the link itself rather than
/// what it points at, so nothing outside the folder is ever read through it — but saying no is then
/// ours to do, once the handle is in hand.
#[cfg(windows)]
pub fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if file.metadata()?.file_type().is_symlink() {
        return Err(std::io::Error::other("a link is not followed here"));
    }
    Ok(file)
}

/// Open one file **for writing**, emptied, and without following a link at the last name.
///
/// The reading half above says why the last name is the one to judge. What is different here is
/// what following one would cost: reading through a link hands out a file from outside the project,
/// and writing through it *replaces* that file — which is the hole `AMB-D-782` closed, measured
/// actually happening (`AMB-T-3739`).
///
/// `create` is for the name that has never existed: a save writes its bytes into a file of its own
/// beside the one it is replacing ([`crate::folder::SAVING`]), and that one is made here.
///
/// **Emptied by this call rather than by the caller**, so that the file handed back is one whose
/// whole content is what gets written into it. Truncating through the open options instead would
/// empty a link's target on Windows before anything had asked whether it was a link.
#[cfg(unix)]
pub fn write_no_follow(path: &Path, create: bool) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(create)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.set_len(0)?;
    Ok(file)
}

/// The same, on the operating system with no flag that refuses: the handle comes back naming the
/// link itself rather than what it points at, and saying no is then ours to do — which is done
/// before the file is emptied, so a link is never the thing that gets emptied.
#[cfg(windows)]
pub fn write_no_follow(path: &Path, create: bool) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(create)
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if file.metadata()?.file_type().is_symlink() {
        return Err(std::io::Error::other("a link is not written through here"));
    }
    file.set_len(0)?;
    Ok(file)
}

/// The folders this project is bound to, in the order they are held — their path names, which is a
/// spelling and not a ranking (`AMB-D-531`) — each in the reader's spelling.
///
/// A folder that is not there is left out rather than carried as a root nothing can resolve under: an
/// unmounted disk and a deleted folder hold no file. What the panel does about a folder that has gone
/// is a question for the panel, which is told by the store and not by this fence.
pub fn roots_of(project_id: i64) -> Result<Vec<PathBuf>, CmdError> {
    let store = crate::commands::open_store_read()?;
    Ok(store
        .bindings()
        .dirs_for_project(project_id)
        .into_iter()
        .filter_map(|dir| canonical_dir(dir).ok())
        .collect())
}

/// The folders this call may reach, and which of them the caller named.
///
/// The caller names a root, and a webview is not trusted to name one: the registry is asked whether
/// this project claims that folder. The rest of the list travels with it because a folder bound inside
/// another one owns what is in it ([`under`]).
pub fn rooted(project_id: i64, root: &str) -> Result<(Vec<PathBuf>, usize), CmdError> {
    let asked = canonical_dir(root).map_err(|_| gone())?;
    let roots = roots_of(project_id)?;
    let base = roots.iter().position(|dir| *dir == asked).ok_or_else(gone)?;
    Ok((roots, base))
}

/// The one folder [`rooted`] proved, for a caller that watches a folder rather than resolves a path
/// under it.
pub fn root_of(project_id: i64, root: &str) -> Result<PathBuf, CmdError> {
    let (mut roots, base) = rooted(project_id, root)?;
    Ok(roots.swap_remove(base))
}

/// The one refusal made about a file in this folder. Which rule turned a caller away — outside the
/// folder, not bound, not there at all — is itself an answer about the filesystem, so all of them
/// say the same thing (the reasoning is [`crate::fileproto`]'s). [`crate::open_with`] is the third
/// door onto the same files and makes the same refusal, which is why this is not private to here.
pub fn gone() -> CmdError {
    CmdError::from(amenbo_core::Error::not_found(
        "no such file in this project's folder",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with something in it, and a sibling holding a secret that must stay out of reach.
    fn folders() -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("work");
        std::fs::create_dir_all(root.join("notes")).expect("the folder");
        std::fs::create_dir_all(root.join("node_modules")).expect("the machine's folder");
        std::fs::write(root.join("notes/a.md"), b"hello").expect("a file");
        std::fs::write(root.join("node_modules/x.js"), b"built").expect("the machine's file");
        std::fs::write(dir.path().join("secret.txt"), b"no").expect("the secret");
        (dir, vec![canonical_dir(&root).expect("the folder is there")])
    }

    /// The fence, which is the whole of what this module has to get right: every spelling of
    /// "somewhere else" is refused, including the ones that would land back inside after a detour.
    #[test]
    fn nothing_outside_the_folder_can_be_named() {
        let (dir, roots) = folders();
        assert!(under(&roots, 0, ["notes", "a.md"]).is_some());
        for segments in [
            vec![".."],
            vec!["notes", "..", "..", "secret.txt"],
            vec!["notes/a.md"],
            vec!["/etc/passwd"],
            vec!["."],
        ] {
            assert!(under(&roots, 0, &segments).is_none(), "reached out with {segments:?}");
        }
        drop(dir);
    }

    /// A name that is not there yet is still a name this fence can answer for: the folders above it
    /// are resolved, and the name itself is only read as text. Saving to a file that does not exist
    /// is the whole reason (`AMB-D-782`).
    #[test]
    fn a_name_that_is_not_there_yet_can_be_spoken_for() {
        let (dir, roots) = folders();
        let (owner, path) = under(&roots, 0, ["notes", "not-written-yet.md"]).expect("a name");
        assert_eq!(owner, 0);
        assert!(!path.exists());
        assert_eq!(path.parent(), Some(canonical_dir(roots[0].join("notes")).unwrap().as_path()));
        // The folders above it are resolved, so a folder that is not there answers nothing at all.
        assert!(under(&roots, 0, ["nowhere", "a.md"]).is_none());
        drop(dir);
    }

    /// Unlike the door that hands out bytes, this one answers for a folder too — it is what a tree
    /// is opened with — and for the root itself, named by no segments at all.
    #[test]
    fn a_folder_is_an_answer_here() {
        let (dir, roots) = folders();
        assert_eq!(
            under(&roots, 0, ["notes"]),
            Some((0, canonical_dir(roots[0].join("notes")).unwrap())),
        );
        assert_eq!(under(&roots, 0, Vec::<String>::new()), Some((0, roots[0].clone())));
        drop(dir);
    }

    /// A file in a folder bound inside another bound folder is the inner one's, whichever of the two
    /// the caller named. Which folder a file belongs to is what its git state is read from and what
    /// its watch reports, so the answer cannot be left to the order the folders are held in.
    #[test]
    fn the_deepest_folder_holding_a_file_is_the_one_it_belongs_to() {
        let (dir, mut roots) = folders();
        let inner = roots[0].join("plugin");
        std::fs::create_dir_all(&inner).expect("the inner folder");
        std::fs::write(inner.join("b.md"), b"mine").expect("a file");
        roots.push(canonical_dir(&inner).expect("the inner folder is there"));

        let (owner, _) = under(&roots, 0, ["plugin", "b.md"]).expect("named through the outer");
        assert_eq!(owner, 1, "the inner folder owns what is in it");
        let (owner, _) = under(&roots, 1, ["b.md"]).expect("named through the inner");
        assert_eq!(owner, 1);
        // And what is outside the inner one is still the outer one's.
        let (owner, _) = under(&roots, 0, ["notes", "a.md"]).expect("a file beside it");
        assert_eq!(owner, 0);
        drop(dir);
    }

    /// What the fence hands back is handed on to the shell, so it comes back in the spelling the
    /// shell takes and not in Win32's internal one (`AMB-D-703`). A verbatim `\\?\C:\…` path is
    /// what `std::fs::canonicalize` answers with on Windows, and `SHOpenWithDialog` refuses one
    /// with `E_INVALIDARG` and draws nothing at all — measured on a real machine in `AMB-T-3651`.
    #[test]
    fn what_the_fence_hands_back_is_spelled_the_way_the_shell_takes_it() {
        let (dir, roots) = folders();
        let (_, file) = under(&roots, 0, ["notes", "a.md"]).expect("a file inside the folder");
        assert!(
            !file.to_string_lossy().starts_with(r"\\?\"),
            "no verbatim prefix leaves the fence: {}",
            file.display(),
        );
        drop(dir);
    }

    /// A folder that is a link out of the project is caught by the fence, because the folders above
    /// the last name are resolved. Judging the text alone would pass it: nothing in the spelling
    /// says where it goes.
    #[cfg(unix)]
    #[test]
    fn a_folder_that_links_out_of_the_project_is_refused() {
        let (dir, roots) = folders();
        let outside = dir.path().join("elsewhere");
        std::fs::create_dir_all(&outside).expect("the folder");
        std::fs::write(outside.join("secret.txt"), b"no").expect("the secret");
        std::os::unix::fs::symlink(&outside, roots[0].join("escape")).expect("the link");
        assert!(under(&roots, 0, ["escape", "secret.txt"]).is_none());
        drop(dir);
    }

    /// Past 260 characters the verbatim front is not a spelling Windows is offering as one of two —
    /// it is the only one `canonicalize` will answer in, and `dunce` leaves it on for that reason
    /// (`is_safe_to_strip_unc` refuses anything longer). A door out to the shell has to take it off
    /// anyway: `SHOpenWithDialog` refuses the verbatim form outright and draws nothing at all
    /// (`AMB-T-3651`), so keeping it means every door under a folder bound that long is dead while
    /// the fence itself reports no trouble.
    ///
    /// So this is deliberately not `dunce::simplified`: length is not a reason to keep it here.
    #[test]
    fn a_path_too_long_for_dunce_is_still_spelled_plainly_for_the_shell() {
        let root = format!(r"\\?\C:\{}", "folder-with-a-long-name\\".repeat(12));
        assert!(root.len() > 260, "the shape this is about is one dunce refuses: {}", root.len());
        assert_eq!(
            without_verbatim(&root).as_deref(),
            Some(root.trim_start_matches(r"\\?\")),
            "the front comes off however long the path is",
        );
    }

    /// The two spellings Windows keeps for one folder are levelled to one before they are compared —
    /// the network one is not the disk one, so taking the front off a share has to leave a path
    /// that still names the server.
    #[test]
    fn one_folder_is_compared_by_one_spelling() {
        assert_eq!(without_verbatim(r"\\?\C:\work\repo").as_deref(), Some(r"C:\work\repo"));
        assert_eq!(
            without_verbatim(r"\\?\UNC\server\share\repo").as_deref(),
            Some(r"\\server\share\repo"),
        );
        // Already plain, in either shape: nothing to level.
        assert_eq!(without_verbatim(r"C:\work\repo"), None);
        assert_eq!(without_verbatim(r"\\server\share\repo"), None);
        assert_eq!(without_verbatim("/work/repo"), None);
    }

}
