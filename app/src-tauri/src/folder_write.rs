//! Changing what a folder holds — making a name, renaming one, moving and copying (`AMB-D-782`).
//!
//! [`crate::folder`] answers what is in a folder; this is the door that alters it, and it is a
//! separate one for the reason every other door here is: what it has to get right is its own.
//! Everything below is fenced by [`crate::folder::under`] exactly as reading is, and nothing here
//! resolves the last name of a path — a link at the end is a name, not a way out of the project.
//!
//! **Three things shape all of it:**
//!
//! 1. **Nothing here is atomic against another writer.** `fs::rename` replaces what is already
//!    there without a word, on all three operating systems (Rust's std passes Windows
//!    `MOVEFILE_REPLACE_EXISTING`), so "is that name taken" is a question asked first and answered
//!    out of date the instant it is answered. Closing the window means a different call on each
//!    platform — `renameat2(RENAME_NOREPLACE)`, `renamex_np(RENAME_EXCL)`, `MoveFileEx` without the
//!    flag — and the macOS one answers `ENOTSUP` over SMB (`AMB-T-3749`), so the asking-first path
//!    has to exist regardless. What is between the two answers is a person's own machine, a file
//!    manager and an agent, not a hostile writer.
//! 2. **A name is judged by the machine that has to hold it.** What every operating system refuses
//!    is refused here; what only Windows refuses is refused only there. A reader on macOS may write
//!    `a:b.md`, and there is no reason a panel should be the thing that stops them.
//! 3. **A carry that stops says where it got to.** Moving three rows and failing on the second
//!    leaves one moved, one where it was and one untried, and that is the answer — not an error
//!    that says nothing about the other two (`AMB-D-782`). Crossing a disk is where it happens: the
//!    bytes are written again, and there is no way to make that one step.

use std::path::{Path, PathBuf};

use amenbo_core::binding::canonical_dir;

use crate::dto::{DropEffectDto, FolderCarriedDto, FolderStopDto, FolderStoppedDto};
use crate::error::CmdError;
use crate::folder::{gone, rooted, under};

/// The most bytes one name may take. Every filesystem worth naming stops at 255, and the ones that
/// stop earlier answer for themselves when the name is used.
const NAME_CAP: usize = 255;

/// Make one name inside a project's folder: an empty file, or a folder.
///
/// **"Not if it is already there" is the operating system's answer, not one asked for first.**
/// `create_new` and `create_dir` both refuse an existing name in the same call that would have made
/// it, which is the one shape of this question with no window in it — and it is the filesystem that
/// decides whether `Alpha.md` is already there when `alpha.md` is, which is exactly who should.
#[tauri::command]
pub fn folder_make(
    project_id: i64,
    root: String,
    path: Vec<String>,
    dir: bool,
) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let name = path.last().ok_or_else(gone)?;
    if !nameable(name) {
        return Err(unnameable(name));
    }
    let (_, target) = under(&roots, base, &path).ok_or_else(gone)?;

    if dir {
        std::fs::create_dir(&target).map_err(|e| made(e, name))
    } else {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map(|_| ())
            .map_err(|e| made(e, name))
    }
}

/// Give one name a different one, in the folder it is already in.
///
/// **Changing only the letter case is a rename like any other**, and asking whether the new name is
/// taken must not be what stops it. So the question put here is whether the folder already holds
/// that name *spelled that way* — which a case-insensitive filesystem answers no to for
/// `alpha.md` → `Alpha.md`, and yes to when something else really is called `Alpha.md`. Asking
/// whether the path exists instead would refuse the rename on the very machines where it is most
/// ordinary: the same Mac can mount a volume that tells the two apart, so no rule written here
/// could answer for both (`AMB-T-3739` made both exist at once).
#[tauri::command]
pub fn folder_rename(
    project_id: i64,
    root: String,
    path: Vec<String>,
    name: String,
) -> Result<(), CmdError> {
    if !nameable(&name) {
        return Err(unnameable(&name));
    }
    let (roots, base) = rooted(project_id, &root)?;
    let (_, from) = under(&roots, base, &path).ok_or_else(gone)?;

    let mut renamed: Vec<String> = path.clone();
    *renamed.last_mut().ok_or_else(gone)? = name.clone();
    let (_, to) = under(&roots, base, &renamed).ok_or_else(gone)?;

    rename_one(&from, &to, &name)
}

/// The rename itself, once the fence has answered for both ends.
///
/// The question is put to the folder's own listing rather than to the path, and the difference is
/// the whole of what makes a case-only rename possible: `Alpha.md` is a name `alpha.md`'s folder
/// does not hold, even on a filesystem that would hand you the same file if you asked for it.
fn rename_one(from: &Path, to: &Path, name: &str) -> Result<(), CmdError> {
    let parent = from.parent().ok_or_else(gone)?;
    if to != from && holds_name(parent, name) {
        return Err(taken(name));
    }
    std::fs::rename(from, to)
        .map_err(|e| CmdError::coded("folder_rename", e.to_string(), why(name, &e)))
}

/// Move rows into another folder — another of the project's folders included (`AMB-D-782`).
///
/// Whether the folders are two of the project's or one is not what makes this risky: crossing a
/// **disk** is, and that happens inside one folder as readily as between two.
#[tauri::command]
pub fn folder_move(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
    to_root: String,
    to: Vec<String>,
) -> Result<FolderCarriedDto, CmdError> {
    carry(project_id, &root, &paths, &to_root, &to, move_one)
}

/// Copy rows into another folder. The same fence, the same answer when it stops part way.
///
/// A link is copied as a link rather than as what it points at: following it would write a copy of
/// something outside the project into it, which is the hole `AMB-D-782` closed on the reading side.
///
/// ⚠ **A copy carries more than the bytes on macOS.** `fs::copy` is `fcopyfile` there, which brings
/// the extended attributes along; Linux and Windows have not been measured for it (`AMB-T-3739`).
#[tauri::command]
pub fn folder_copy(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
    to_root: String,
    to: Vec<String>,
) -> Result<FolderCarriedDto, CmdError> {
    carry(project_id, &root, &paths, &to_root, &to, copy_one)
}

/// Bring rows in from outside — the files a person dragged onto the window (`AMB-D-775`).
///
/// **Only the far end is fenced, and only the far end can be.** What was dropped is whatever the
/// reader was holding, and the operating system is what granted it: a path under no bound folder is
/// the ordinary case here, not an escape from one. So `paths` arrive as the host gave them, whole,
/// and it is the folder they land in that is proved against the project's own — the same
/// [`landing`] a move or a copy is aimed at, and the same answer when the carry stops part way.
///
/// **A plain drop copies.** A move takes the file away from wherever it was, which on a drop from
/// the desktop is a place Amenbo does not answer for; taking it is done only where the operating
/// system says the reader asked for it in so many words (`crate::dropped`). `default` is the face's
/// to read, and this is the face reading it.
#[tauri::command]
pub fn folder_import(
    project_id: i64,
    paths: Vec<String>,
    to_root: String,
    to: Vec<String>,
    effect: DropEffectDto,
) -> Result<FolderCarriedDto, CmdError> {
    let (roots, base) = rooted(project_id, &to_root)?;
    let into = landing(&roots, base, &to)?;
    let from: Vec<PathBuf> = paths.iter().map(|path| levelled(Path::new(path))).collect();
    let one = if matches!(effect, DropEffectDto::Move) { move_one } else { copy_one };
    Ok(carried(&from, &into, one))
}

/// A dropped path in the spelling the landing is in, so the two can be compared.
///
/// Everything but the last name is resolved, and the last name is left alone — the same shape
/// [`crate::folder::under`] leaves a fenced path in, for the same reason: a link at the end of a
/// path is a name to carry, not a way through. What it buys is the one comparison that matters:
/// "is the folder being carried into inside the folder being carried" is asked of two paths, and on
/// macOS the one the operating system hands over goes through `/var` where the one the fence
/// resolved says `/private/var`. Unresolvable is left as it came — a path to nothing fails at the
/// carry, which is where it should.
fn levelled(path: &Path) -> PathBuf {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return path.to_path_buf();
    };
    canonical_dir(parent).map_or_else(|_| path.to_path_buf(), |dir| dir.join(name))
}

/// What both carries do, differing only in what one row costs.
///
/// The rows are taken in the order they were given and the first failure ends it: what came before
/// arrived, what comes after was never touched, and the answer says which is which. It is an answer
/// rather than a refusal because a refusal would be the same word for "none of them moved" and "two
/// of the three did".
fn carry(
    project_id: i64,
    root: &str,
    paths: &[Vec<String>],
    to_root: &str,
    to: &[String],
    one: fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<FolderCarriedDto, CmdError> {
    let (roots, base) = rooted(project_id, root)?;
    let asked = canonical_dir(to_root).map_err(|_| gone())?;
    let to_base = roots.iter().position(|dir| *dir == asked).ok_or_else(gone)?;
    let into = landing(&roots, to_base, to)?;

    let mut from = Vec::with_capacity(paths.len());
    for path in paths {
        from.push(under(&roots, base, path).ok_or_else(gone)?.1);
    }
    Ok(carried(&from, &into, one))
}

/// The folder a carry is aimed at, proved against the project's own before anything is written.
///
/// It is asked for as a folder and not merely as a path: the rows are joined onto it by name, so a
/// file at the far end would silently become a parent of names it cannot hold.
fn landing(roots: &[PathBuf], base: usize, to: &[String]) -> Result<PathBuf, CmdError> {
    let (_, into) = under(roots, base, to).ok_or_else(gone)?;
    if !into.is_dir() {
        return Err(gone());
    }
    Ok(into)
}

/// The carry itself, once both ends are paths: the rows in the order they were given, stopping on
/// the first that will not go.
///
/// **The near end is a path and nothing more**, which is what lets the same loop answer for rows
/// out of the project's own folders and for rows dropped in from the desktop — where they came from
/// is the fence's question and it has already been asked.
fn carried(
    from: &[PathBuf],
    into: &Path,
    one: fn(&Path, &Path) -> std::io::Result<()>,
) -> FolderCarriedDto {
    let mut arrived = Vec::new();
    for path in from {
        // A path with no last name is not a row: a drop can hand over a whole volume, and there is
        // no name to give what would be carried in.
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            let name = path.to_string_lossy().into_owned();
            let why = "this has no name to carry it in under".to_string();
            let stopped = FolderStoppedDto { name, code: Some(FolderStopDto::Nameless), why };
            return FolderCarriedDto { arrived, stopped: Some(stopped) };
        };
        let target = into.join(&name);

        // A folder cannot be carried into itself: the copy would be writing into what it is reading,
        // and the move would be asking the kernel to make a folder its own child.
        //
        // The three Amenbo decides carry a code as well as a sentence, and the machine's carry the
        // sentence alone: what a refusal made here means is the same every time and can be put in the
        // reader's language, where what a filesystem says is its own words in its own (`crate::dto`).
        let stopped = if into.starts_with(path) {
            Some((Some(FolderStopDto::Inside), "a folder cannot be moved inside itself".to_string()))
        } else if holds(&target) {
            Some((Some(FolderStopDto::Taken), format!("{name} is already there")))
        } else {
            one(path, &target).err().map(|e| (None, e.to_string()))
        };

        if let Some((code, why)) = stopped {
            return FolderCarriedDto { arrived, stopped: Some(FolderStoppedDto { name, code, why }) };
        }
        arrived.push(name);
    }
    FolderCarriedDto { arrived, stopped: None }
}

/// Move one row, and copy it instead where the kernel says the two ends are not the same disk.
///
/// The source is removed only once the copy is through, so a carry that fails half way leaves what
/// it was carrying where it was — the copy at the far end is the part that is untidy, and it is the
/// part a person can see and delete (`AMB-D-782`).
fn move_one(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Err(e) if crosses_disks(&e) => {
            copy_one(from, to)?;
            remove(from)
        }
        other => other,
    }
}

/// Copy one row, whatever it is: a folder with everything under it, a link as a link, a file as its
/// bytes.
fn copy_one(from: &Path, to: &Path) -> std::io::Result<()> {
    let kind = from.symlink_metadata()?.file_type();
    if kind.is_symlink() {
        return copy_link(from, to);
    }
    if !kind.is_dir() {
        return std::fs::copy(from, to).map(|_| ());
    }
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy_one(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// Copy a link as a link — the same text pointing at the same place, whether or not that place is
/// inside the project. Reading through it and writing what it found would be the copy that carries
/// somebody's machine-wide file into a folder (`AMB-D-782`).
#[cfg(unix)]
fn copy_link(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(std::fs::read_link(from)?, to)
}

/// The same, on the operating system that has two calls for it and hands neither to an ordinary
/// account: a machine with developer mode off answers with a refusal, which is what the row stops
/// on and says.
#[cfg(windows)]
fn copy_link(from: &Path, to: &Path) -> std::io::Result<()> {
    let target = std::fs::read_link(from)?;
    if from.is_dir() {
        std::os::windows::fs::symlink_dir(target, to)
    } else {
        std::os::windows::fs::symlink_file(target, to)
    }
}

/// Take one row away, whatever it is. A link is taken away as itself, never followed.
fn remove(path: &Path) -> std::io::Result<()> {
    if path.symlink_metadata()?.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Whether the kernel is saying the two ends are not on the same filesystem — the one failure a
/// move answers by copying. The number is read rather than `ErrorKind`'s name for it, which was
/// stabilised after the version this crate says it builds under.
#[cfg(unix)]
fn crosses_disks(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(libc::EXDEV)
}

#[cfg(windows)]
fn crosses_disks(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_NOT_SAME_DEVICE as i32)
}

/// Whether the folder holds this name already — a dangling link included, since a name pointing at
/// nothing is still a name in use.
fn holds(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Whether `dir` already holds a name spelled exactly this way.
///
/// **Not the same question as whether the path is there**, and the two part company on a
/// case-insensitive filesystem: it answers yes for `Alpha.md` when what it holds is `alpha.md`,
/// which would make renaming a file to its own name in different letters impossible. The listing is
/// what the filesystem really holds, so a machine that tells the two apart says so by holding both,
/// and one that does not says so by holding one (`AMB-T-3739` mounted a volume of each on the same
/// Mac — no rule written here could have answered for both).
fn holds_name(dir: &Path, name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .any(|entry| entry.file_name() == std::ffi::OsStr::new(name))
}

/// Whether the machine this is running on will take `name` as the name of one file.
///
/// The floor is what no filesystem takes: nothing, a separator, the two names every directory
/// already has, a NUL, or more bytes than a name may be. `under` refuses most of these too, since a
/// name with a separator in it is not one name — this says so for a name that has not been made a
/// path yet, and says the rest.
fn nameable(name: &str) -> bool {
    if name.is_empty() || name.len() > NAME_CAP || name == "." || name == ".." {
        return false;
    }
    if name.contains(['/', '\0']) || name.contains(std::path::MAIN_SEPARATOR) {
        return false;
    }
    windows_takes(name)
}

/// What only Windows refuses, refused only on Windows: the characters its shell reads as syntax, a
/// trailing dot or space (which it drops without telling anybody, so the name made is not the name
/// asked for), and the names its device files have held since DOS — with or without an extension.
#[cfg(windows)]
fn windows_takes(name: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if name.contains(['<', '>', ':', '"', '\\', '|', '?', '*']) {
        return false;
    }
    if name.chars().any(|c| c.is_control()) {
        return false;
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    !RESERVED.contains(&stem.as_str())
}

/// Everywhere else, the floor is the whole of it: a name with a colon in it is a name on macOS, and
/// a panel is not the place to decide it should not be (`AMB-D-782`).
#[cfg(not(windows))]
fn windows_takes(_name: &str) -> bool {
    true
}

/// The refusal for a name this machine will not hold. It names the name: what is wrong with it is
/// the one thing the reader has to see to write a different one.
fn unnameable(name: &str) -> CmdError {
    CmdError::coded(
        "folder_name",
        format!("this machine will not take {name} as a name"),
        fields(name),
    )
}

/// The refusal for a name that is already in the folder.
fn taken(name: &str) -> CmdError {
    CmdError::coded("folder_taken", format!("{name} is already there"), fields(name))
}

/// What the making of a name failed with — the one refusal worth its own code being the name that
/// was already there, which is what `create_new` and `create_dir` answer with.
fn made(e: std::io::Error, name: &str) -> CmdError {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        return taken(name);
    }
    CmdError::coded("folder_make", e.to_string(), why(name, &e))
}

/// The one value every refusal here interpolates: the name it is about.
fn fields(name: &str) -> serde_json::Value {
    serde_json::json!({ "name": name })
}

/// The same, plus what the machine said — for the two refusals whose reason is the machine's own and
/// not this module's. The sentence a reader is shown is written where the rest of the prose is
/// (`app/src/core/i18n`), so the reason has to travel as a value rather than only inside the English
/// line, which no template can reach into.
fn why(name: &str, e: &std::io::Error) -> serde_json::Value {
    serde_json::json!({ "name": name, "reason": e.to_string() })
}

/// Only what a caller could ask for. Where the answer is the filesystem's, it is asked in a test
/// with a real folder rather than described here.
#[cfg(test)]
mod tests {
    use super::*;

    /// The floor is every machine's, and what is over it is the machine's own: a colon is a name on
    /// macOS and Linux and syntax on Windows, and this is compiled for one of them at a time.
    #[test]
    fn a_name_is_judged_by_the_machine_that_has_to_hold_it() {
        assert!(nameable("notes.md"));
        assert!(nameable(".env"));
        assert!(!nameable(""));
        assert!(!nameable("."));
        assert!(!nameable(".."));
        assert!(!nameable("a/b.md"));
        assert!(!nameable("a\0b"));
        assert!(!nameable(&"x".repeat(NAME_CAP + 1)));
        assert_eq!(nameable("a:b.md"), !cfg!(windows));
        assert_eq!(nameable("CON"), !cfg!(windows));
        assert_eq!(nameable("con.txt"), !cfg!(windows));
        assert_eq!(nameable("trailing."), !cfg!(windows));
    }

    /// A name in use is a name in use even when what is at the end of it is nothing: a link nobody
    /// can follow still occupies the name, and a rename onto it would take it away.
    #[test]
    fn a_name_pointing_at_nothing_is_still_in_use() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let name = dir.path().join("dangling");
        assert!(!holds(&name));
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("never-made"), &name).expect("a link");
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(dir.path().join("never-made"), &name).expect("a link");
        assert!(holds(&name), "the name is taken, whatever is at the end of it");
    }

    /// A folder is copied with everything under it, and a link inside it is copied as a link — what
    /// it points at is not read, let alone written into the copy.
    #[test]
    fn a_folder_is_copied_whole_and_a_link_stays_a_link() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("src");
        std::fs::create_dir_all(from.join("deep")).expect("a folder");
        std::fs::write(from.join("deep/note.md"), b"mine").expect("a file");
        let outside = dir.path().join("outside.md");
        std::fs::write(&outside, b"not this project's").expect("a file");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, from.join("link.md")).expect("a link");

        let to = dir.path().join("copy");
        copy_one(&from, &to).expect("the copy");
        assert_eq!(std::fs::read(to.join("deep/note.md")).expect("the file"), b"mine");
        #[cfg(unix)]
        {
            let kind = to.join("link.md").symlink_metadata().expect("the link").file_type();
            assert!(kind.is_symlink(), "a link is copied as a link, not as what it points at");
            assert_eq!(std::fs::read_link(to.join("link.md")).expect("the link"), outside);
        }
        // And what was carried is still where it was: a copy takes nothing away.
        assert!(from.join("deep/note.md").is_file());
    }

    /// A name is free when the folder does not hold it *spelled that way* — which is what lets a
    /// file be renamed to itself in different letters on a filesystem that would answer "already
    /// there" to the path.
    #[test]
    fn only_the_letters_change_and_the_rename_goes_through() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("alpha.md");
        std::fs::write(&from, b"mine").expect("a file");

        let to = dir.path().join("Alpha.md");
        rename_one(&from, &to, "Alpha.md").expect("the letters are the only change");
        let held: Vec<String> = std::fs::read_dir(dir.path())
            .expect("the folder")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(held, ["Alpha.md"], "one file, under the name asked for: {held:?}");
    }

    /// A name something else already has is refused, and refused before the rename runs — `rename`
    /// itself replaces what is there without a word, on every one of the three.
    #[test]
    fn a_name_something_else_has_is_refused() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("alpha.md");
        let taken = dir.path().join("beta.md");
        std::fs::write(&from, b"mine").expect("a file");
        std::fs::write(&taken, b"somebody else's").expect("a file");

        let refused = rename_one(&from, &taken, "beta.md").expect_err("the name is taken");
        assert_eq!(refused.code, "folder_taken");
        assert_eq!(std::fs::read(&taken).expect("the file"), b"somebody else's");
        assert!(holds(&from), "and what was being renamed is still there");
    }

    /// A carry that cannot be made leaves what it was carrying where it was. The disk-crossing case
    /// cannot be asked for with one temp folder; what can is the shape of the answer, which is the
    /// same one: the source is taken away only after the far end is written.
    #[test]
    fn a_move_that_fails_leaves_the_source_alone() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("note.md");
        std::fs::write(&from, b"mine").expect("a file");

        move_one(&from, &dir.path().join("never-made/note.md")).expect_err("nowhere to move it to");
        assert_eq!(std::fs::read(&from).expect("the file"), b"mine");
    }

    /// A carry stops on the first row that will not go, and says where it got to: what came before
    /// is in the folder, what it stopped on is named, and what came after was never touched.
    #[test]
    fn a_carry_that_stops_says_where_it_got_to() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let into = dir.path().join("into");
        std::fs::create_dir(&into).expect("a folder");
        std::fs::write(into.join("beta.md"), b"already here").expect("a file");
        let rows: Vec<PathBuf> = ["alpha.md", "beta.md", "gamma.md"]
            .iter()
            .map(|name| {
                let at = dir.path().join(name);
                std::fs::write(&at, b"mine").expect("a file");
                at
            })
            .collect();

        let answer = carried(&rows, &into, copy_one);
        assert_eq!(answer.arrived, ["alpha.md"]);
        let stopped = answer.stopped.expect("it stopped on the name that was taken");
        assert_eq!(stopped.name, "beta.md");
        assert_eq!(
            std::fs::read(into.join("beta.md")).expect("the file"),
            b"already here",
            "and what was already there is untouched",
        );
        assert!(!holds(&into.join("gamma.md")), "the rest were never tried");
    }

    /// What a drop hands over is a whole path from outside the project, and that is the ordinary
    /// case: the fence is on the folder it lands in, and the name it lands under is the path's own.
    #[test]
    fn a_row_from_outside_is_carried_in_under_its_own_name() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let outside = dir.path().join("desktop/note.md");
        std::fs::create_dir_all(outside.parent().expect("the folder")).expect("a folder");
        std::fs::write(&outside, b"dropped").expect("a file");
        let into = dir.path().join("into");
        std::fs::create_dir(&into).expect("a folder");

        let answer = carried(std::slice::from_ref(&outside), &into, copy_one);
        assert_eq!(answer.arrived, ["note.md"]);
        assert_eq!(std::fs::read(into.join("note.md")).expect("the file"), b"dropped");
        assert!(holds(&outside), "a copy takes nothing away from where it was dragged from");
    }

    /// A folder cannot be carried into itself, and the two paths have to be in the same spelling for
    /// that to be seen: on macOS a temp folder is `/var/...` as it is handed over and `/private/var/...`
    /// once the fence has resolved it, which is two paths neither of which contains the other.
    #[test]
    fn a_dropped_path_is_levelled_against_the_landing() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("carried");
        std::fs::create_dir(&from).expect("a folder");
        let into = from.join("inside");
        std::fs::create_dir(&into).expect("a folder");

        let asked = levelled(&from);
        assert_eq!(asked, canonical_dir(&from).expect("the folder"));
        let answer = carried(
            std::slice::from_ref(&asked),
            &canonical_dir(&into).expect("the folder"),
            copy_one,
        );
        assert!(answer.arrived.is_empty());
        assert_eq!(answer.stopped.expect("it is inside itself").name, "carried");
    }

    /// Moving on one disk is a rename, and a rename takes the source away. The other half — the
    /// copy a move falls back to across disks — cannot be asked for with one temp folder.
    #[test]
    fn a_move_leaves_nothing_where_it_was() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("note.md");
        std::fs::write(&from, b"mine").expect("a file");
        let to = dir.path().join("moved.md");

        move_one(&from, &to).expect("the move");
        assert!(!holds(&from));
        assert_eq!(std::fs::read(&to).expect("the file"), b"mine");
    }
}
