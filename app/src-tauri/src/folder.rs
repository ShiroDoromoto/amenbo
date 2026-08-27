//! What a folder holds — the question [`crate::fileproto`] refuses, answered here.
//!
//! That door hands out the bytes of one file and says so plainly: a directory is not listed there,
//! because listing is not what a door that streams bytes is for. The file face asks the other half
//! of the question — what is in this folder, and what does this file say — and it asks over the
//! command seam, where an answer can be a list.
//!
//! **The fence is the project's folders, not a session's.** The face's rows belong to the project:
//! the tree does not move when the pane beside it is switched (`AMB-T-3602`).
//! So the root a caller may name is a folder the project is bound to, checked against the store
//! rather than taken on the caller's word, and everything under it is judged the way `fileproto`
//! judges a path — segment by segment as text, the folders then resolved against the real filesystem,
//! and the last name left as text for the open to refuse a link at ([`crate::folder::under`],
//! [`crate::folder::open_no_follow`], which both doors share).
//!
//! **What a file is, is read off its bytes.** A name says nothing reliable: the extension table this
//! replaces could not answer for 19% of this repository's files (`AMB-T-3547`). A NUL byte in the
//! head is what separates text from everything else, and a picture is recognised by the bytes it
//! starts with — so a `.md` that is really a PNG draws as one, and a text file with no extension at
//! all still reads.

use std::path::{Component, Path, PathBuf};

use amenbo_core::binding::canonical_dir;

use crate::dto::{
    FolderEntryDto, FolderFileDto, FolderImageDto, FolderLineEndingDto, FolderOversizeDto,
};
use crate::error::CmdError;

/// The floor of the pruning: folders whose contents are the machine's rather than the person's,
/// pruned whether or not anything says to. A tree that lists them buries what somebody wrote under
/// what a build wrote, and the walk the watch is laid over would be nearly all build output —
/// a build touches thousands of files in seconds (`AMB-T-3566`).
///
/// Above this floor the folder speaks for itself: what its `.gitignore` calls noise is noise here
/// too ([`walker`]). A floor is still needed under that, because `.git` is not in anybody's ignore
/// file, and a folder that is not a repository has no ignore file at all.
const PRUNED: [&str; 4] = [".git", "node_modules", "target", "dist"];

/// The front of the name a save writes its bytes into before it puts them in place
/// (`crate::folder_save`).
///
/// **It is left out of both walks below.** The window it exists in is about a tenth of a
/// millisecond and the watch waits 400 ms for quiet before it looks, so it is rarely seen
/// (`AMB-T-3739`) — but a burst arriving around it draws a row in the tree under a name nobody
/// wrote, which reads as the app being broken rather than as the app working. The row would go on
/// standing there too: what takes it away is the next walk, and nothing is bound to happen next.
///
/// **It carries no part of the file's own name.** A name may be 255 bytes and no more, so one built
/// out of another name plus a front of its own is longer than the filesystem will take for exactly
/// the files whose names are longest — a refusal that would have nothing to do with what the reader
/// was trying to save.
pub const SAVING: &str = ".amenbo-saving-";

/// How much of a file is read to decide whether it is text (`AMB-T-3547`).
const HEAD: usize = 8000;

/// The most text a panel is handed. A file longer than this is drawn as far as this goes and said
/// to be cut — the face reads, it does not page.
///
/// The old quarter of a megabyte was a number for looking, not for working: four files in this
/// repository alone are over it, and a cut one written back drops its tail without saying so
/// (`AMB-D-783`). What is cut is what `truncated` is for — a face that lets a file be edited reads
/// it and refuses to save.
const TEXT_CAP: usize = 5 * 1024 * 1024;

/// The largest picture the panel draws, in bytes. Past it the reader is told there is a picture and
/// not made to wait for it.
///
/// This is the cap on what the **host** holds. The bytes no longer cross the command seam — the
/// webview fetches them from [`crate::fileproto`] — but that door still reads the file whole into
/// this process to answer a request with no range on it, so the number guards the same thing it
/// always did.
const IMAGE_CAP: u64 = 5 * 1024 * 1024;

/// The largest picture a webview is asked to draw, in pixels — the second cap, and not a
/// restatement of the first (`AMB-D-783`).
///
/// **The two guard different things and neither subsumes the other.** Bytes stand for what this
/// process holds; pixels stand for what the webview decodes, and the relation between them is the
/// compression ratio, which an author chooses. A 4.83 MB PNG of sixteen hundred megapixels passes
/// the byte cap and freezes the window for twenty-two seconds; a 14 MB JPEG of nine hundred
/// megapixels passes this one and is decoded almost for free (`AMB-T-3769` measured both).
///
/// A hundred megapixels is roughly ten thousand square. Of the 27,659 pictures on the machine this
/// was measured against, the largest was 64 megapixels — so nothing anybody actually has is refused
/// by it, and the worst case it still admits costs about 430 MB and under a second.
const PIXEL_CAP: u64 = 100_000_000;

/// How much of a JPEG is read before it is asked how large it is.
///
/// Every other form answers within thirty bytes, so [`HEAD`] is all they need. JPEG writes its
/// frame header behind whatever came first, and what commonly comes first is an EXIF thumbnail and
/// a colour profile: of the 12,545 JPEGs measured in `AMB-T-3769`, 8 KB answered for 78.9% and
/// 64 KB for 99.3%. It is one extra read of a file already known to be under the byte cap.
const JPEG_HEAD: usize = 64 * 1024;

/// How many names the walk behind the watch will look at before it stops. A folder someone points
/// the app at can be anything, and a set of watches is not worth an unbounded walk.
const VISIT_CAP: usize = 20_000;

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
/// beside the one it is replacing ([`SAVING`]), and that one is made here.
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

/// What both walks below agree on: the floor is pruned outright, and a dotfile is not noise.
///
/// Hidden files are **not** skipped, which is where this parts company with the ignore crate's
/// default: a dotfile is a file somebody wrote, and `.amenbo` and `.env` are exactly the ones a
/// reader goes looking for after an agent has been at work.
///
/// The one name left out on top of the floor is this app's own half-written save ([`SAVING`]),
/// which is not a file anybody wrote and is gone again before a reader could act on it.
fn floor(root: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder.hidden(false).require_git(false).filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        !PRUNED.contains(&name.as_ref()) && !name.starts_with(SAVING)
    });
    builder
}

/// The walk the tree is drawn from: the floor, and nothing the repository has to say (`AMB-D-786`).
///
/// **`.gitignore` says what git does not record, not what a person may not look at.** The two were
/// the same walk until now, and the argument against that is in this module's own reasoning: the
/// dotfiles named above as the ones worth showing — `.amenbo`, `.env` — are the very files a
/// repository ignores, this one included.
///
/// What is left out is still left out. A build directory is the floor's business, and the floor is
/// what keeps a tree from burying what somebody wrote under what a build wrote.
fn shown(root: &Path) -> ignore::WalkBuilder {
    let mut builder = floor(root);
    builder
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false);
    builder
}

/// The walk the watch is laid over: the floor, plus the repository's own answer (`AMB-T-3604`).
///
/// `.gitignore`, the global one and the parents' are all read, so a build directory this project
/// happens to call `.next` or `__pycache__` drops out without anybody listing it here. A folder
/// that is no repository loses nothing — there is simply nothing to read, and the floor is all that
/// applies.
///
/// **Here the ignore file is doing work a tree does not need done.** A build rewrites thousands of
/// files a second, and a watch laid over every folder it writes in would wake the face without
/// pause — in exactly the folders people work in (`AMB-D-786`).
fn walker(root: &Path) -> ignore::WalkBuilder {
    let mut builder = floor(root);
    builder
        .ignore(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true);
    builder
}

/// The names directly inside the folder a walk is rooted at, each with whether it is a folder.
fn level(builder: &mut ignore::WalkBuilder) -> Vec<(String, bool)> {
    builder
        .max_depth(Some(1))
        .build()
        .filter_map(Result::ok)
        // The first entry of a walk is the folder it started in.
        .filter(|entry| entry.depth() > 0)
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.file_type().is_some_and(|t| t.is_dir()),
            )
        })
        .collect()
}

/// Whether a path a watch woke us with names something the walk above would never have shown.
///
/// The floor is the same [`PRUNED`] table, read off a path instead of walked: where one watch
/// covers a whole tree ([`crate::folder_watch`]) the kernel reports the build output nobody asked
/// to see, and it is dropped here rather than by declining to watch it. Only the part below `root`
/// is judged — a folder somebody registered is theirs whatever the folders above it are called.
///
/// **This runs on every event a build fires**, thousands a second, so it reads names and nothing
/// else: no filesystem call, no allocation. 50 µs each was the difference between 0.1% of events
/// missed and 69% of them on Windows (`AMB-T-3753`).
///
/// What slips past is not a wrong answer, only an extra walk: the scan behind it compares what
/// would be drawn and finds it unchanged. `cargo clean` is that case — it renames `target` to
/// `target<six letters>` before removing it, and 0.24% of a build's events arrive under the new
/// name (`AMB-T-3752`). The folder's own `.gitignore` is left out for the same reason: reading it
/// per event costs more than the walk it would save.
pub fn pruned(root: &Path, path: &Path) -> bool {
    let Ok(below) = path.strip_prefix(root) else {
        // Not under the root at all. Nothing here can say what it is, so it is not thrown away.
        return false;
    };
    below.components().any(|part| {
        matches!(part, Component::Normal(name) if PRUNED.iter().any(|floor| name == *floor))
    })
}

/// The folders under `root` that a reader would call theirs — the list a watch is installed over
/// (`crate::folder_watch`).
///
/// **The files are walked past and not carried.** Nothing reads them any more: the tree asks for
/// one level at a time as it is opened, and what has changed in the folder is git's answer rather
/// than a list of the newest names (`AMB-D-785`). What that leaves out is the `modified` of every
/// file in the tree — one `stat` per name, which is 26-44% of what this walk cost.
///
/// The walk is capped rather than trusted to end: a folder somebody points the app at can be
/// anything, and a set of watches is not worth an unbounded one. `capped` is true when the cap is
/// what stopped it, which is the one thing the caller cannot work out from the answer.
pub struct Scan {
    /// Every folder walked, `root` included — one watch each.
    pub dirs: Vec<PathBuf>,
    /// Whether the walk stopped at the cap rather than at the end of the tree.
    pub capped: bool,
}

pub fn scan(root: &Path) -> Scan {
    let mut dirs = Vec::new();
    let mut visited = 0usize;
    let mut capped = false;

    for entry in walker(root).build() {
        let Ok(entry) = entry else { continue };
        visited += 1;
        if visited > VISIT_CAP {
            capped = true;
            break;
        }
        // The files are still walked — they are what the cap counts, and a folder is only reached
        // by walking what is beside it — but nothing is kept of them.
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            dirs.push(entry.path().to_path_buf());
        }
    }

    Scan { dirs, capped }
}

/// The names directly inside one folder — folders first, then files, each run in the order a person
/// reads them. Nothing recurses: a folded tree opens one level at a time (`AMB-T-3602`).
#[tauri::command]
pub fn folder_entries(
    project_id: i64,
    root: String,
    path: Vec<String>,
) -> Result<Vec<FolderEntryDto>, CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, dir) = under(&roots, base, &path).ok_or_else(gone)?;
    // Read off the name itself and not off what it leads to: a folder that is a link is not walked,
    // whatever is on the other side of it (`AMB-D-782`).
    if !std::fs::symlink_metadata(&dir).is_ok_and(|meta| meta.is_dir()) {
        return Err(gone());
    }
    // The level is walked twice, and the difference between the two walks is the mark: what the
    // repository ignores is drawn, and drawn as ignored (`AMB-D-786`). Asking the ignore rules
    // directly instead would be a second reading of them — global file, parents, `.git/info/exclude`
    // and all — and the one that could drift from the walk the watch is actually laid over.
    let kept: std::collections::HashSet<String> =
        level(&mut walker(&dir)).into_iter().map(|(name, _)| name).collect();
    let mut rows: Vec<FolderEntryDto> = level(&mut shown(&dir))
        .into_iter()
        .map(|(name, is_dir)| FolderEntryDto {
            ignored: !kept.contains(&name),
            name,
            is_dir,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(rows)
}

/// Open one file the way the machine would open it — the reader's own applications, not ours.
///
/// The face has an editor of its own, and this is still worth having: what it opens a file in is
/// whatever the person already opens that kind of file with. The OS decides what that is, and Amenbo
/// does not keep an opinion about it.
///
/// The path goes out through [`plain`] because this is a door out of the process: past 260
/// characters the fence answers in Windows's internal spelling, and what is on the other side of
/// this call is the shell (`AMB-T-3749`).
#[tauri::command]
pub fn folder_open_file(project_id: i64, root: String, path: Vec<String>) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    tauri_plugin_opener::open_path(plain(&file).as_ref(), None::<&str>)
        .map_err(|e| CmdError::coded("folder.open", e.to_string(), serde_json::Value::Null))
}

/// Show one file where it lives, in the machine's file manager.
///
/// It is the other half of opening: what a person wants of a file is as often "where is this" as
/// "what is in it", and a panel that could only read would leave them hunting for a path they can
/// already see.
///
/// Spelled with [`plain`] for the same reason as [`folder_open_file`]: the file manager is outside
/// this process, and the plugin's own levelling stops at 260 characters where ours does not
/// (`dunce::simplified`, which it calls, keeps the verbatim front on past that).
#[tauri::command]
pub fn folder_reveal_file(project_id: i64, root: String, path: Vec<String>) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    tauri_plugin_opener::reveal_item_in_dir(plain(&file).as_ref())
        .map_err(|e| CmdError::coded("folder.reveal", e.to_string(), serde_json::Value::Null))
}

/// The encodings a file may be reopened in, in the order to offer them.
///
/// It is asked for rather than written on the panel's own side because the list is one fact with
/// one owner: which encodings this can write back ([`crate::encoding::writable_names`]). A copy
/// kept over there would go on offering an encoding the day it stopped being written.
#[tauri::command]
pub fn folder_encodings() -> Vec<String> {
    crate::encoding::writable_names().into_iter().map(str::to_owned).collect()
}

/// What one file has to show: its text, or that it is a picture and of what type, or why the
/// picture is not drawn, or none of those.
///
/// The head is read once and answers both questions — whether there is a NUL in it, and what the
/// first bytes say the file is — so a name is never consulted about either. Only what that head
/// settles on is then read further: the text up to its cap, or a JPEG's front far enough to reach
/// the frame header. A file that is neither is never read past the head at all.
///
/// **A picture is never read whole here.** Its bytes reach the webview through
/// [`crate::fileproto`], which the caller can address because it named this file by the same
/// project, folder and path (`AMB-D-783`).
///
/// `encoding` is the reader putting the guess right. Left out, the bytes are guessed at as usual;
/// named, that encoding is what they are decoded as and nothing is guessed (`AMB-D-773`). A name
/// this cannot write back is refused rather than honoured — offering to open a file in an encoding
/// that could never be saved would be handing back a file to look at and not to keep — and a name
/// on a file that is not text is simply not reached, the encoding question never being asked of a
/// picture.
#[tauri::command]
pub fn folder_read(
    project_id: i64,
    root: String,
    path: Vec<String>,
    encoding: Option<String>,
) -> Result<FolderFileDto, CmdError> {
    let asked = match encoding.as_deref() {
        None => None,
        Some(name) => Some(crate::encoding::writable(name).ok_or_else(|| {
            CmdError::coded(
                "folder.encoding",
                format!("not an encoding this writes back: {name}"),
                serde_json::Value::Null,
            )
        })?),
    };
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    // The name's own answer, not the one it leads to: a link is not a file to read here.
    let meta = std::fs::symlink_metadata(&file).map_err(|_| gone())?;
    if !meta.is_file() {
        return Err(gone());
    }
    let size = meta.len();
    let head = read_head(&file, HEAD).map_err(|_| gone())?;

    // The one judgement, made on bytes: text is what has no NUL in its head. Which encoding that
    // text is in is a separate question and never this one's — a page of Shift_JIS is text to the
    // person who wrote it — and it is `crate::encoding`'s to answer.
    if !head.contains(&0) {
        let bytes = if size > HEAD as u64 {
            read_head(&file, TEXT_CAP).map_err(|_| gone())?
        } else {
            head
        };
        let truncated = (bytes.len() as u64) < size;
        // The reader's own language is the guess's only hint, and it is fetched here rather than
        // held because only a file that is not UTF-8 is ever guessed at — one in 645 of them.
        let read = match asked {
            Some(encoding) => crate::encoding::read_as(&bytes, truncated, encoding),
            None => crate::encoding::read(&bytes, truncated, language_tld()),
        };
        return Ok(FolderFileDto {
            truncated,
            text: Some(read.text),
            image: None,
            oversize: None,
            encoding: Some(read.encoding.name().to_string()),
            bom: read.bom,
            line_ending: line_ending(read.line_ending),
            clean: read.clean,
        });
    }

    let Some(mime) = picture(&head) else {
        return Ok(FolderFileDto {
            text: None,
            truncated: false,
            image: None,
            oversize: None,
            encoding: None,
            bom: false,
            line_ending: FolderLineEndingDto::Lf,
            clean: false,
        });
    };

    // A JPEG is the one form whose size is not already in hand (`JPEG_HEAD`), and reading further
    // is worth nothing where the read cannot succeed: a file over the byte cap is refused whatever
    // its size turns out to be.
    let front = match mime {
        "image/jpeg" if size <= IMAGE_CAP && size > HEAD as u64 => {
            read_head(&file, JPEG_HEAD).unwrap_or(head)
        }
        _ => head,
    };
    let pixels = measure(mime, &front);

    if carriable(size, pixels) {
        // The bytes are not read here: what the panel is handed is the type, and it asks
        // fileproto for the picture itself at the path it named this call with.
        return Ok(FolderFileDto {
            text: None,
            truncated: false,
            image: Some(FolderImageDto { mime: mime.to_string() }),
            oversize: None,
            encoding: None,
            bom: false,
            line_ending: FolderLineEndingDto::Lf,
            clean: false,
        });
    }
    Ok(FolderFileDto {
        text: None,
        truncated: false,
        image: None,
        encoding: None,
        bom: false,
        line_ending: FolderLineEndingDto::Lf,
        clean: false,
        oversize: Some(FolderOversizeDto {
            bytes: size,
            width: pixels.map(|(width, _)| width),
            height: pixels.map(|(_, height)| height),
        }),
    })
}

/// The type the bytes say they are, for the pictures a webview can draw. Sniffed rather than looked
/// up by name, which is the same rule the text judgement above follows.
fn picture(head: &[u8]) -> Option<&'static str> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const GIF: &[u8] = b"GIF8";
    if head.starts_with(PNG) {
        return Some("image/png");
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if head.starts_with(GIF) {
        return Some("image/gif");
    }
    // RIFF containers name their form in the four bytes after the length: WEBP is one of several.
    if head.starts_with(b"RIFF") && head.len() >= 12 && &head[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Whether a picture this large is one the panel draws — both caps, asked as one question
/// (`AMB-D-783`).
///
/// **A size nobody could read is not a refusal.** What cannot be measured is nearly always a JPEG
/// behind a thick profile, and a JPEG under the byte cap is cheap however many pixels it holds; the
/// forms that are not cheap — PNG, GIF, WebP — all answer within thirty bytes. So "unmeasured"
/// never stands in for "dangerous", and the byte cap is left to guard those alone.
fn carriable(bytes: u64, pixels: Option<(u32, u32)>) -> bool {
    bytes <= IMAGE_CAP
        && match pixels {
            Some((width, height)) => u64::from(width) * u64::from(height) <= PIXEL_CAP,
            None => true,
        }
}

/// How large a picture says it is, in pixels — or nothing at all, where the bytes in hand do not
/// say (`AMB-D-783`).
///
/// **This rides on a read that has already happened.** Every one of these forms writes its size
/// near the front, so the head the type was sniffed from is usually the same bytes the size is read
/// out of; the whole of it costs single-digit nanoseconds (`AMB-T-3769`). Nothing is decoded and no
/// image library is involved — a decoder is exactly the cost this measurement exists to avoid
/// paying.
///
/// The four forms are the four [`picture`] answers for, and it stays that way on purpose: a form
/// this cannot measure is a form the panel does not draw either.
fn measure(mime: &str, head: &[u8]) -> Option<(u32, u32)> {
    match mime {
        "image/png" => png_pixels(head),
        "image/gif" => gif_pixels(head),
        "image/webp" => webp_pixels(head),
        "image/jpeg" => jpeg_pixels(head),
        _ => None,
    }
}

/// PNG writes it in the IHDR chunk, which the format requires to be the first one — so it is two
/// words at a fixed offset, and the chunk's own name is checked rather than assumed.
fn png_pixels(head: &[u8]) -> Option<(u32, u32)> {
    if head.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be32(head, 16)?, be32(head, 20)?))
}

/// GIF writes it in the screen descriptor, immediately behind the six-byte signature.
fn gif_pixels(head: &[u8]) -> Option<(u32, u32)> {
    Some((le16(head, 6)?, le16(head, 8)?))
}

/// WebP is a RIFF container whose first chunk says which of three forms this is, and each of the
/// three writes its size somewhere else, in its own way.
fn webp_pixels(head: &[u8]) -> Option<(u32, u32)> {
    match head.get(12..16)? {
        // Lossy: the VP8 key frame header, behind a three-byte frame tag and the three-byte start
        // code — which is checked, because without it the offsets below are being read out of
        // whatever else the file happens to be. Fourteen bits each; the top two are a scale.
        b"VP8 " => {
            if head.get(23..26)? != [0x9D, 0x01, 0x2A] {
                return None;
            }
            Some((le16(head, 26)? & 0x3FFF, le16(head, 28)? & 0x3FFF))
        }
        // Lossless: fourteen bits each, packed into one little-endian word behind a signature byte,
        // and each written one short of the real number.
        b"VP8L" => {
            if *head.get(20)? != 0x2F {
                return None;
            }
            let packed = le32(head, 21)?;
            Some(((packed & 0x3FFF) + 1, ((packed >> 14) & 0x3FFF) + 1))
        }
        // Extended: the canvas rather than a frame, behind four bytes of feature flags — three
        // bytes each, and again one short.
        b"VP8X" => Some((le24(head, 24)? + 1, le24(head, 27)? + 1)),
        _ => None,
    }
}

/// JPEG writes it in a start-of-frame segment, and the only way to that segment is to walk the ones
/// in front of it — which is why this form alone is handed more of the file ([`JPEG_HEAD`]).
///
/// The walk stops at the scan: past that marker the file is entropy-coded data, not segments, and a
/// frame header that has not appeared by then is not going to.
fn jpeg_pixels(head: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    loop {
        // A marker is 0xFF and then the marker byte, and any number of 0xFF may pad the gap.
        if *head.get(at)? != 0xFF {
            return None;
        }
        while *head.get(at)? == 0xFF {
            at += 1;
        }
        let marker = *head.get(at)?;
        at += 1;
        // Restarts and the one-byte extension carry no length at all, so there is nothing to skip.
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            continue;
        }
        if marker == 0xDA {
            return None;
        }
        let length = be16(head, at)? as usize;
        // A length counts its own two bytes, so anything under that would walk backwards forever.
        if length < 2 {
            return None;
        }
        // Every start-of-frame writes the two sizes in the same place, behind the length and the
        // sample precision. The three markers excepted are in the range but are not frames: a
        // Huffman table, an arithmetic-coding table, and a reserved extension.
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return Some((be16(head, at + 5)?, be16(head, at + 3)?));
        }
        at += length;
    }
}

/// The four ways these headers spell a number, each answering nothing where the bytes are not
/// there — which is how a size read out of a truncated head comes back unmeasured rather than
/// wrong.
fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    let pair: [u8; 2] = bytes.get(at..at + 2)?.try_into().ok()?;
    Some(u32::from(u16::from_be_bytes(pair)))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    let word: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(word))
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    let pair: [u8; 2] = bytes.get(at..at + 2)?.try_into().ok()?;
    Some(u32::from(u16::from_le_bytes(pair)))
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let three = bytes.get(at..at + 3)?;
    Some(u32::from(three[0]) | u32::from(three[1]) << 8 | u32::from(three[2]) << 16)
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    let word: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(word))
}

/// The wire form of what the bytes said about their newlines.
fn line_ending(read: crate::encoding::LineEnding) -> FolderLineEndingDto {
    match read {
        crate::encoding::LineEnding::Lf => FolderLineEndingDto::Lf,
        crate::encoding::LineEnding::Crlf => FolderLineEndingDto::Crlf,
        crate::encoding::LineEnding::Mixed => FolderLineEndingDto::Mixed,
    }
}

/// The hint the encoding guess is given: the top-level domain standing for the language the reader
/// chose to be spoken to in (`crate::encoding::tld_for`).
///
/// `config.json` is a file of its own, read here rather than held, because the only caller is the
/// one file in 645 that is not UTF-8 — holding it would be caching a read that almost never happens
/// against a setting that can change under it.
fn language_tld() -> Option<&'static [u8]> {
    let language = amenbo_core::config::Paths::resolve()
        .ok()
        .and_then(|paths| amenbo_core::config::Config::load(&paths.config_file).language)?;
    crate::encoding::tld_for(Some(&language))
}

/// At most `cap` bytes from the front of a file. A short file comes back short; a long one comes
/// back cut, which is what `truncated` is then read from.
fn read_head(path: &Path, cap: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut buf = Vec::new();
    open_no_follow(path)?.take(cap as u64).read_to_end(&mut buf)?;
    Ok(buf)
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

    /// The last name is not resolved, so a link there passes the fence — and is refused where the
    /// file is opened, in the same call that opens it. That order is what leaves no window between
    /// asking and acting, and it is what lets a name that is not there yet be named at all.
    #[cfg(unix)]
    #[test]
    fn a_link_at_the_last_name_is_refused_at_the_open() {
        let (dir, roots) = folders();
        std::os::unix::fs::symlink(dir.path().join("secret.txt"), roots[0].join("escape"))
            .expect("the link");
        let (_, path) = under(&roots, 0, ["escape"]).expect("the fence lets the name through");
        let refused = open_no_follow(&path).expect_err("the open refuses it");
        assert_eq!(refused.raw_os_error(), Some(libc::ELOOP));
        // And the file it points at is readable through no door of this module.
        assert!(read_head(&path, TEXT_CAP).is_err());
        // A file that is really a file still opens.
        let (_, real) = under(&roots, 0, ["notes", "a.md"]).expect("a real file");
        assert_eq!(read_head(&real, TEXT_CAP).unwrap(), b"hello");
        drop(dir);
    }

    /// What a file is, is read off its bytes and never off its name — the point of the whole
    /// judgement (`AMB-T-3547`).
    #[test]
    fn text_and_binary_are_told_apart_by_a_nul_and_not_by_a_name() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let text = dir.path().join("no-extension-at-all");
        std::fs::write(&text, "日本語もテキスト").expect("a file");
        let binary = dir.path().join("looks-like.md");
        std::fs::write(&binary, [0x00, 0x01, 0x02]).expect("a file");

        let head = read_head(&text, HEAD).unwrap();
        assert!(!head.contains(&0));
        let head = read_head(&binary, HEAD).unwrap();
        assert!(head.contains(&0));
    }

    /// A NUL past the head is not looked for. What matters is that the judgement reads a bounded
    /// piece of the file, so a huge one costs the same as a small one — and the bound is the head,
    /// not the text cap, so raising the cap did not raise what a binary costs to recognise.
    #[test]
    fn only_the_head_is_judged() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("long.txt");
        let mut bytes = vec![b'a'; HEAD + 10];
        bytes[HEAD + 5] = 0;
        std::fs::write(&file, &bytes).expect("a file");
        let head = read_head(&file, HEAD).unwrap();
        assert_eq!(head.len(), HEAD);
        assert!(!head.contains(&0));
    }

    /// A picture is what its first bytes say it is. The name is not asked, so a PNG called `.md`
    /// draws and a text file called `.png` does not.
    #[test]
    fn a_picture_is_recognised_by_its_bytes() {
        assert_eq!(picture(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(picture(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(picture(b"GIF89a"), Some("image/gif"));
        assert_eq!(picture(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(picture(b"RIFF\0\0\0\0WAVEfmt "), None);
        assert_eq!(picture(b"# a heading"), None);
    }

    /// The floor is pruned whether or not anything says so, and what the folder's own ignore file
    /// calls noise is noise here too — the point of taking ripgrep's walker rather than reading the
    /// directory (`AMB-T-3604`). A folder whose name starts with a dot is not noise: it is a folder
    /// somebody made.
    #[test]
    fn the_folder_says_what_of_it_is_the_machines() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("node_modules/left-pad")).expect("the machine's folder");
        std::fs::create_dir_all(root.join("build/out")).expect("this project's own");
        std::fs::create_dir_all(root.join("notes")).expect("somebody's folder");
        std::fs::create_dir_all(root.join(".github")).expect("a dot folder is somebody's too");
        std::fs::write(root.join(".gitignore"), "build/\n").expect("the ignore file");

        let found = scan(root);
        let kept: Vec<String> = found
            .dirs
            .iter()
            .filter_map(|d| d.strip_prefix(root).ok())
            .map(|d| d.to_string_lossy().into_owned())
            .collect();
        // The root is one of them: it is watched like every folder under it.
        assert!(found.dirs.contains(&root.to_path_buf()));
        assert!(kept.iter().any(|d| d == "notes"), "{kept:?}");
        assert!(kept.iter().any(|d| d == ".github"), "a dot folder is somebody's: {kept:?}");
        for gone in ["node_modules", "build"] {
            assert!(
                !kept.iter().any(|d| d.starts_with(gone)),
                "{gone} is the machine's: {kept:?}",
            );
        }
    }

    /// The tree and the walk the watch is laid over part company at the ignore file, and only
    /// there: what a repository ignores is a file somebody wrote, and the tree says so by drawing
    /// it as ignored rather than by leaving it out (`AMB-D-786`). The floor is under both.
    #[test]
    fn the_tree_shows_what_the_repository_ignores_and_says_that_it_does() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("node_modules")).expect("the machine's folder");
        std::fs::create_dir_all(root.join("build")).expect("this project's own");
        std::fs::write(root.join(".gitignore"), "build/
.env
").expect("the ignore file");
        std::fs::write(root.join("node_modules/x.js"), b"built").expect("a file");
        std::fs::write(root.join("build/out.js"), b"built").expect("a file");
        std::fs::write(root.join(".env"), b"SECRET=1").expect("a file");
        std::fs::write(root.join("notes.md"), b"mine").expect("a file");

        // The same two walks the row is built from, and the same difference between them.
        let kept: std::collections::HashSet<String> =
            level(&mut walker(root)).into_iter().map(|(name, _)| name).collect();
        let rows: Vec<(String, bool)> = level(&mut shown(root))
            .into_iter()
            .map(|(name, _)| {
                let ignored = !kept.contains(&name);
                (name, ignored)
            })
            .collect();
        let named = |want: &str| rows.iter().find(|(name, _)| name == want).map(|(_, i)| *i);

        assert_eq!(named("notes.md"), Some(false), "nothing ignores it: {rows:?}");
        assert_eq!(named(".env"), Some(true), "ignored, and on the list all the same: {rows:?}");
        assert_eq!(named("build"), Some(true));
        // The floor is not the ignore file's to overturn: it is off the tree either way.
        assert_eq!(named("node_modules"), None, "the floor is pruned outright: {rows:?}");

        // And the walk the watch is laid over has not moved: what is ignored is still out of it.
        let found = scan(root);
        for gone in ["build", "node_modules"] {
            assert!(
                !found.dirs.iter().any(|d| d.ends_with(gone)),
                "{gone} is not watched: {:?}",
                found.dirs,
            );
        }
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

    /// The floor is read off a path the same way it is walked — and only below the root, since the
    /// folders above it are not the reader's choice to answer for.
    #[test]
    fn the_floor_reads_the_same_off_a_path() {
        let root = Path::new("/home/someone/target/thing");
        assert!(!pruned(root, root));
        assert!(!pruned(root, &root.join("src/lib.rs")));
        assert!(pruned(root, &root.join("target/debug/x.o")));
        assert!(pruned(root, &root.join("node_modules/left-pad/index.js")));
        assert!(pruned(root, &root.join(".git/index")));
        // A name that only starts the same way is somebody's own.
        assert!(!pruned(root, &root.join("targets/plan.md")));
        // Somewhere else entirely says nothing about this folder, so it is not thrown away.
        assert!(!pruned(root, Path::new("/elsewhere/target/x.o")));
    }

    /// A picture with a bare header of the given form, long enough to be measured and nothing more.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn webp(form: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(form);
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(body);
        bytes
    }

    /// A JPEG whose frame header sits behind `ahead` bytes of something else — which is what an
    /// EXIF thumbnail and a colour profile are, and why this form is handed more of the file.
    fn jpeg(width: u16, height: u16, ahead: usize) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xE1]);
        bytes.extend_from_slice(&((ahead + 2) as u16).to_be_bytes());
        bytes.extend(std::iter::repeat(0xAB).take(ahead));
        bytes.extend_from_slice(&[0xFF, 0xC0]);
        bytes.extend_from_slice(&17u16.to_be_bytes());
        bytes.push(8);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes
    }

    /// How large a picture is, read off its front and never by decoding it — the measurement the
    /// pixel cap is applied to (`AMB-D-783`). All four forms the panel draws answer.
    #[test]
    fn a_picture_says_how_large_it_is_in_its_first_bytes() {
        assert_eq!(measure("image/png", &png(1920, 1080)), Some((1920, 1080)));
        assert_eq!(measure("image/gif", b"GIF89a\x40\x01\xf0\x00rest"), Some((320, 240)));
        assert_eq!(measure("image/jpeg", &jpeg(800, 600, 4)), Some((800, 600)));

        // Lossy: fourteen bits each behind the frame tag and the start code.
        let mut lossy = vec![0x00, 0x00, 0x00, 0x9D, 0x01, 0x2A];
        lossy.extend_from_slice(&300u16.to_le_bytes());
        lossy.extend_from_slice(&200u16.to_le_bytes());
        assert_eq!(measure("image/webp", &webp(b"VP8 ", &lossy)), Some((300, 200)));

        // Lossless: the same two numbers packed into one word, each written one short.
        let packed: u32 = (300 - 1) | ((200 - 1) << 14);
        let mut lossless = vec![0x2F];
        lossless.extend_from_slice(&packed.to_le_bytes());
        assert_eq!(measure("image/webp", &webp(b"VP8L", &lossless)), Some((300, 200)));

        // Extended: the canvas behind the feature flags, three bytes each and again one short.
        let mut extended = vec![0x00; 4];
        extended.extend_from_slice(&(300u32 - 1).to_le_bytes()[..3]);
        extended.extend_from_slice(&(200u32 - 1).to_le_bytes()[..3]);
        assert_eq!(measure("image/webp", &webp(b"VP8X", &extended)), Some((300, 200)));
    }

    /// The one form whose size is not already in hand: the walk has to step over whatever was
    /// written in front of the frame, and 8 KB of head is not enough to reach past a thumbnail
    /// (`AMB-T-3769` — 78.9% at 8 KB, 99.3% at 64 KB).
    #[test]
    fn a_jpeg_is_measured_from_behind_what_was_written_in_front_of_it() {
        let fat = jpeg(4000, 3000, 25_000);
        assert!(fat.len() > HEAD && fat.len() <= JPEG_HEAD);
        assert_eq!(measure("image/jpeg", &fat), Some((4000, 3000)));
        // The same file cut at the head the type was sniffed from says nothing — not a wrong number.
        assert_eq!(measure("image/jpeg", &fat[..HEAD]), None);
    }

    /// Bytes that do not say are answered with nothing at all, whatever they are — a truncated
    /// header, a chunk that is not IHDR, a WebP form nobody has seen. Nothing here guesses.
    #[test]
    fn a_front_that_does_not_say_is_not_guessed_at() {
        assert_eq!(measure("image/png", &png(10, 10)[..20]), None);
        assert_eq!(measure("image/png", b"\x89PNG\r\n\x1a\n\0\0\0\rIDATxxxxxxxx"), None);
        assert_eq!(measure("image/webp", &webp(b"VP9 ", &[0; 20])), None);
        // A lossy chunk whose start code is not there is not read at the offsets that follow it.
        assert_eq!(measure("image/webp", &webp(b"VP8 ", &[0; 20])), None);
        // The scan is where the segments stop; a frame that has not appeared by then never will.
        assert_eq!(measure("image/jpeg", &[0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x0C]), None);
        assert_eq!(measure("image/avif", &png(10, 10)), None);
    }

    /// Two caps, and each catches what the other passes (`AMB-D-783`). The bytes stand for what this
    /// process holds, the pixels for what the webview decodes, and their ratio is the author's to
    /// choose — so neither alone is a fence.
    #[test]
    fn both_caps_are_needed_because_each_passes_what_the_other_stops() {
        // A 4.83 MB PNG of sixteen hundred megapixels: under the byte cap, twenty-two seconds of
        // frozen window. Only the pixel cap stops it.
        assert!(!carriable(4_830_000, Some((40_000, 40_000))));
        // A 14 MB JPEG of nine hundred megapixels: decoded almost for free, but held whole in this
        // process. Only the byte cap stops it.
        assert!(!carriable(14_060_000, Some((30_000, 30_000))));
        // What people actually have goes through: the largest of 27,659 pictures measured was 64
        // megapixels at under 2 MB.
        assert!(carriable(1_980_000, Some((9_824, 6_552))));
    }

    /// A size nobody could read is let through on the bytes alone. The forms that are expensive to
    /// decode all answer within thirty bytes, so "unmeasured" never stands in for "dangerous".
    #[test]
    fn a_picture_that_would_not_say_its_size_is_still_judged_on_its_bytes() {
        assert!(carriable(IMAGE_CAP, None));
        assert!(!carriable(IMAGE_CAP + 1, None));
    }

    /// The cap is what a panel is handed, not what the file is: the size travels whole, and the cut
    /// is said out loud.
    #[test]
    fn a_long_file_comes_back_cut() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("long.txt");
        std::fs::write(&file, "x".repeat(TEXT_CAP + 100)).expect("a file");
        let head = read_head(&file, TEXT_CAP).unwrap();
        assert_eq!(head.len(), TEXT_CAP);
        assert!((head.len() as u64) < std::fs::metadata(&file).unwrap().len());
    }
}
