//! What a folder holds — the question [`crate::fileproto`] refuses, answered here.
//!
//! That door hands out the bytes of one file and says so plainly: a directory is not listed there,
//! because listing is not what a door that streams bytes is for. The file face asks the other half
//! of the question — what is in this folder, what changed in it lately, and what does this file
//! say — and it asks over the command seam, where an answer can be a list.
//!
//! **The fence is the project's folders, not a session's.** The face's rows belong to the project:
//! the tree and what changed in it do not move when the pane beside them is switched (`AMB-T-3602`).
//! So the root a caller may name is a folder the project is bound to, checked against the store
//! rather than taken on the caller's word, and everything under it is judged the way `fileproto`
//! judges a path — segment by segment as text, the folders then resolved against the real filesystem,
//! and the last name left as text for the open to refuse a link at ([`under`], [`open_no_follow`],
//! which both doors share).
//!
//! **What a file is, is read off its bytes.** A name says nothing reliable: the extension table this
//! replaces could not answer for 19% of this repository's files (`AMB-T-3547`). A NUL byte in the
//! head is what separates text from everything else, and a picture is recognised by the bytes it
//! starts with — so a `.md` that is really a PNG draws as one, and a text file with no extension at
//! all still reads.

use std::path::{Component, Path, PathBuf};

use amenbo_core::binding::canonical_dir;
use base64::Engine as _;

use crate::dto::{FolderChangedDto, FolderEntryDto, FolderFileDto, FolderImageDto};
use crate::error::CmdError;

/// The floor of the pruning: folders whose contents are the machine's rather than the person's,
/// pruned whether or not anything says to. A tree that lists them buries what somebody wrote under
/// what a build wrote, and the walk behind "what changed lately" would return almost nothing else —
/// a build touches thousands of files in seconds (`AMB-T-3566`).
///
/// Above this floor the folder speaks for itself: what its `.gitignore` calls noise is noise here
/// too ([`walker`]). A floor is still needed under that, because `.git` is not in anybody's ignore
/// file, and a folder that is not a repository has no ignore file at all.
const PRUNED: [&str; 4] = [".git", "node_modules", "target", "dist"];

/// How much of a file is read to decide whether it is text (`AMB-T-3547`).
const HEAD: usize = 8000;

/// The most text a panel is handed. A file longer than this is drawn as far as this goes and said
/// to be cut — the face reads, it does not page.
const TEXT_CAP: usize = 256 * 1024;

/// The largest picture carried whole over the command seam. Past it the reader is told there is a
/// picture and not made to wait for it.
const IMAGE_CAP: u64 = 4 * 1024 * 1024;

/// How many files "what changed lately" names.
const RECENT: usize = 30;

/// How many names the walk behind it will look at before it stops. A folder someone points the app
/// at can be anything, and a list of the thirty newest files is not worth an unbounded walk.
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
/// What leaves this fence is handed to the shell, so it leaves in the form the shell accepts.
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
    let owner = roots
        .iter()
        .enumerate()
        .filter(|(_, root)| path.starts_with(root))
        .max_by_key(|(_, root)| root.as_os_str().len())?
        .0;
    Some((owner, path))
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

/// The walk both doors take, pruned the one way (`AMB-T-3604`).
///
/// **What a folder holds is what somebody wrote in it.** The floor above is pruned outright, and
/// over it the repository's own answer is taken: `.gitignore`, the global one and the parents' are
/// all read, so a build directory this project happens to call `.next` or `__pycache__` drops out
/// without anybody listing it here. A folder that is no repository loses nothing — there is simply
/// nothing to read, and the floor is all that applies.
///
/// Hidden files are **not** skipped, which is where this parts company with the ignore crate's
/// default: a dotfile is a file somebody wrote, and `.amenbo` and `.env` are exactly the ones a
/// reader goes looking for after an agent has been at work.
fn walker(root: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .require_git(false)
        .filter_entry(|entry| {
            !PRUNED.contains(&entry.file_name().to_string_lossy().as_ref())
        });
    builder
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

/// Everything under `root` that a reader would call theirs: the files with when they were last
/// written, and the folders they are in — which is also the list a watch is installed over
/// (`crate::folder_watch`).
///
/// The walk is capped rather than trusted to end: a folder somebody points the app at can be
/// anything, and neither a list of the thirty newest files nor a set of watches is worth an
/// unbounded one. `capped` is true when the cap is what stopped it, which is the one thing the
/// caller cannot work out from the answer.
pub struct Scan {
    /// Every file, most recently written first, as segments from the root.
    pub files: Vec<(std::time::SystemTime, Vec<String>)>,
    /// Every folder walked, `root` included — one watch each.
    pub dirs: Vec<PathBuf>,
    /// Whether the walk stopped at the cap rather than at the end of the tree.
    pub capped: bool,
}

pub fn scan(root: &Path) -> Scan {
    let mut files = Vec::new();
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
        let path = entry.path();
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            dirs.push(path.to_path_buf());
            continue;
        }
        let Ok(segments) = path.strip_prefix(root) else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        files.push((
            modified,
            segments.iter().map(|s| s.to_string_lossy().into_owned()).collect(),
        ));
    }

    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    Scan { files, dirs, capped }
}

/// The rows "what changed lately" is drawn from: the newest of a scan, in the shape the panel reads.
pub fn recent(scan: &Scan) -> Vec<FolderChangedDto> {
    scan.files
        .iter()
        .take(RECENT)
        .map(|(modified, path)| FolderChangedDto {
            path: path.clone(),
            modified: chrono::DateTime::<chrono::Utc>::from(*modified).to_rfc3339(),
        })
        .collect()
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
    // One level of the shared walk. Going through it rather than reading the directory outright is
    // what keeps the tree and the changed list saying the same thing about what is in this folder:
    // a name the repository ignores is absent from both, not from one of them.
    let mut rows: Vec<FolderEntryDto> = walker(&dir)
        .max_depth(Some(1))
        .build()
        .filter_map(Result::ok)
        // The first entry of a walk is the folder it started in.
        .filter(|entry| entry.depth() > 0)
        .map(|entry| FolderEntryDto {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: entry.file_type().is_some_and(|t| t.is_dir()),
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
/// The face reads; it does not edit (`AMB-T-3602`). What it can do is hand the file to whatever the
/// person already opens that kind of file with, which is the whole of this: the OS decides what that
/// is, and Amenbo does not keep an opinion about it.
#[tauri::command]
pub fn folder_open_file(project_id: i64, root: String, path: Vec<String>) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    tauri_plugin_opener::open_path(&file, None::<&str>)
        .map_err(|e| CmdError::coded("folder.open", e.to_string(), serde_json::Value::Null))
}

/// Show one file where it lives, in the machine's file manager.
///
/// It is the other half of opening: what a person wants of a file is as often "where is this" as
/// "what is in it", and a panel that could only read would leave them hunting for a path they can
/// already see.
#[tauri::command]
pub fn folder_reveal_file(project_id: i64, root: String, path: Vec<String>) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    tauri_plugin_opener::reveal_item_in_dir(&file)
        .map_err(|e| CmdError::coded("folder.reveal", e.to_string(), serde_json::Value::Null))
}

/// What one file has to show: its text, or its picture, or neither.
///
/// The head is read once and answers both questions — whether there is a NUL in it, and what the
/// first bytes say the file is — so a name is never consulted about either.
#[tauri::command]
pub fn folder_read(
    project_id: i64,
    root: String,
    path: Vec<String>,
) -> Result<FolderFileDto, CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    // The name's own answer, not the one it leads to: a link is not a file to read here.
    let meta = std::fs::symlink_metadata(&file).map_err(|_| gone())?;
    if !meta.is_file() {
        return Err(gone());
    }
    let size = meta.len();
    let bytes = read_head(&file, TEXT_CAP).map_err(|_| gone())?;

    // The one judgement, made on bytes: text is what has no NUL in its head. Reading it as UTF-8 is
    // a separate matter and never a verdict — a file cut at the cap can end inside a character, and
    // a page of text in another encoding is still text to a person looking for what they wrote.
    if !bytes.iter().take(HEAD).any(|b| *b == 0) {
        return Ok(FolderFileDto {
            truncated: (bytes.len() as u64) < size,
            text: Some(String::from_utf8_lossy(&bytes).into_owned()),
            image: None,
        });
    }

    let image = match picture(&bytes) {
        Some(mime) if size <= IMAGE_CAP => read_whole(&file).ok().map(|whole| FolderImageDto {
            mime: mime.to_string(),
            base64: base64::engine::general_purpose::STANDARD.encode(whole),
        }),
        _ => None,
    };
    Ok(FolderFileDto { text: None, truncated: false, image })
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

/// At most `cap` bytes from the front of a file. A short file comes back short; a long one comes
/// back cut, which is what `truncated` is then read from.
fn read_head(path: &Path, cap: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut buf = Vec::new();
    open_no_follow(path)?.take(cap as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

/// The whole of a file, opened the way its head was — a picture is carried entire or not at all.
fn read_whole(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut buf = Vec::new();
    open_no_follow(path)?.read_to_end(&mut buf)?;
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

        let head = read_head(&text, TEXT_CAP).unwrap();
        assert!(!head.iter().take(HEAD).any(|b| *b == 0));
        let head = read_head(&binary, TEXT_CAP).unwrap();
        assert!(head.iter().take(HEAD).any(|b| *b == 0));
    }

    /// A NUL past the head is not looked for. What matters is that the judgement reads a bounded
    /// piece of the file, so a huge one costs the same as a small one.
    #[test]
    fn only_the_head_is_judged() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("long.txt");
        let mut bytes = vec![b'a'; HEAD + 10];
        bytes[HEAD + 5] = 0;
        std::fs::write(&file, &bytes).expect("a file");
        let head = read_head(&file, TEXT_CAP).unwrap();
        assert!(!head.iter().take(HEAD).any(|b| *b == 0));
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
    /// directory (`AMB-T-3604`). A dotfile is not noise: it is a file somebody wrote.
    #[test]
    fn the_folder_says_what_of_it_is_the_machines() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("node_modules")).expect("the machine's folder");
        std::fs::create_dir_all(root.join("build")).expect("this project's own");
        std::fs::write(root.join(".gitignore"), "build/\n*.log\n").expect("the ignore file");
        std::fs::write(root.join("node_modules/x.js"), b"built").expect("a file");
        std::fs::write(root.join("build/out.js"), b"built").expect("a file");
        std::fs::write(root.join("run.log"), b"noise").expect("a file");
        std::fs::write(root.join(".amenbo"), b"pointer").expect("a file");
        std::fs::write(root.join("notes.md"), b"mine").expect("a file");

        let found = scan(root);
        let names: Vec<String> = found.files.iter().map(|(_, p)| p.join("/")).collect();
        assert!(names.contains(&"notes.md".to_string()));
        assert!(names.contains(&".amenbo".to_string()), "a dotfile is somebody's: {names:?}");
        assert!(names.contains(&".gitignore".to_string()));
        for gone in ["node_modules/x.js", "build/out.js", "run.log"] {
            assert!(!names.contains(&gone.to_string()), "{gone} is the machine's: {names:?}");
        }
        // Every folder the walk kept is one the watch is laid over, the root included.
        assert!(found.dirs.contains(&root.to_path_buf()));
        assert!(!found.dirs.iter().any(|d| d.ends_with("node_modules")));
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

    /// Newest first, because that is the whole of what the row says: what was written last.
    #[test]
    fn the_newest_file_is_named_first() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::write(root.join("older.txt"), b"a").expect("a file");
        // Written one after the other, and the clock has to have moved between them for the order
        // to mean anything.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(root.join("newer.txt"), b"b").expect("a file");

        let rows = recent(&scan(root));
        let names: Vec<&str> = rows.iter().map(|r| r.path[0].as_str()).collect();
        assert_eq!(names, ["newer.txt", "older.txt"]);
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
