//! What a folder holds — the question [`crate::fileproto`] refuses, answered here.
//!
//! That door hands out the bytes of one file and says so plainly: a directory is not listed there,
//! because listing is not what a door that streams bytes is for. The file face asks the other half
//! of the question — what is in this folder, what changed in it lately, and what does this file
//! say — and it asks over the command seam, where an answer can be a list.
//!
//! **The fence is the project's folder, not a session's.** The face's rows belong to the project:
//! the tree and what changed in it do not move when the pane beside them is switched (`AMB-T-3602`).
//! So the root a caller may name is a folder the project is bound to, checked against the store
//! rather than taken on the caller's word, and everything under it is judged the way `fileproto`
//! judges a path — segment by segment as text, and then again against the real filesystem, links
//! followed (see [`crate::folder::under`], which both doors share).
//!
//! **What a file is, is read off its bytes.** A name says nothing reliable: the extension table this
//! replaces could not answer for 19% of this repository's files (`AMB-T-3547`). A NUL byte in the
//! head is what separates text from everything else, and a picture is recognised by the bytes it
//! starts with — so a `.md` that is really a PNG draws as one, and a text file with no extension at
//! all still reads.

use std::path::{Component, Path, PathBuf};

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

/// The path `segments` name inside `root`, or nothing at all — the fence both doors are built on.
///
/// Nothing is resolved before it is judged and nothing is judged before it is resolved: each segment
/// is checked on its own as text, and what they add up to is then checked again against the real
/// filesystem. A path that passes the first check can still leave the folder through a symbolic
/// link, and a path that would pass the second could still have been written as `..` — neither
/// check subsumes the other. What comes back is canonical and inside `root`; whether it may be a
/// directory is the caller's to say, since one door hands out bytes and the other lists names.
pub fn under(root: &Path, segments: impl IntoIterator<Item = impl AsRef<str>>) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;

    let mut path = root.clone();
    for segment in segments {
        // One ordinary name and nothing else. `..`, `.`, an embedded separator, a root and a drive
        // letter all come back as some other kind of component, or as more than one — none of which
        // is a file name.
        let mut parts = Path::new(segment.as_ref()).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(name)), None) => path.push(name),
            _ => return None,
        }
    }

    // Now the filesystem's own answer, links followed. A link inside the folder that points out of
    // it is only caught here, which is why the text check above is not the end of it.
    let path = path.canonicalize().ok()?;
    path.starts_with(&root).then_some(path)
}

/// The folder this call is rooted at, having established that the project really is bound to it.
///
/// The caller names a root, and a webview is not trusted to name one: the registry is asked whether
/// this project claims that folder. Everything else in this module resolves under what comes back.
pub fn root_of(project_id: i64, root: &str) -> Result<PathBuf, CmdError> {
    let asked = Path::new(root).canonicalize().map_err(|_| gone())?;
    let store = crate::commands::open_store_read()?;
    let bound = store
        .bindings()
        .dirs_for_project(project_id)
        .into_iter()
        .any(|dir| Path::new(dir).canonicalize().is_ok_and(|dir| dir == asked));
    if bound { Ok(asked) } else { Err(gone()) }
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
    let dir = under(&root_of(project_id, &root)?, &path).ok_or_else(gone)?;
    if !dir.is_dir() {
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
    let file = under(&root_of(project_id, &root)?, &path).ok_or_else(gone)?;
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
    let file = under(&root_of(project_id, &root)?, &path).ok_or_else(gone)?;
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
    let file = under(&root_of(project_id, &root)?, &path).ok_or_else(gone)?;
    let meta = std::fs::metadata(&file).map_err(|_| gone())?;
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
        Some(mime) if size <= IMAGE_CAP => std::fs::read(&file).ok().map(|whole| FolderImageDto {
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
    std::fs::File::open(path)?
        .take(cap as u64)
        .read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with something in it, and a sibling holding a secret that must stay out of reach.
    fn folders() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("work");
        std::fs::create_dir_all(root.join("notes")).expect("the folder");
        std::fs::create_dir_all(root.join("node_modules")).expect("the machine's folder");
        std::fs::write(root.join("notes/a.md"), b"hello").expect("a file");
        std::fs::write(root.join("node_modules/x.js"), b"built").expect("the machine's file");
        std::fs::write(dir.path().join("secret.txt"), b"no").expect("the secret");
        (dir, root)
    }

    /// The fence, which is the whole of what this module has to get right: every spelling of
    /// "somewhere else" is refused, including the ones that would land back inside after a detour.
    #[test]
    fn nothing_outside_the_folder_can_be_named() {
        let (dir, root) = folders();
        assert!(under(&root, ["notes", "a.md"]).is_some());
        for segments in [
            vec![".."],
            vec!["notes", "..", "..", "secret.txt"],
            vec!["notes/a.md"],
            vec!["/etc/passwd"],
            vec!["."],
        ] {
            assert!(under(&root, &segments).is_none(), "reached out with {segments:?}");
        }
        drop(dir);
    }

    /// Unlike the door that hands out bytes, this one answers for a folder too — it is what a tree
    /// is opened with — and for the root itself, named by no segments at all.
    #[test]
    fn a_folder_is_an_answer_here() {
        let (dir, root) = folders();
        assert_eq!(under(&root, ["notes"]), Some(root.join("notes").canonicalize().unwrap()));
        assert_eq!(under(&root, Vec::<String>::new()), Some(root.canonicalize().unwrap()));
        drop(dir);
    }

    /// A link is followed and *then* judged, which is the only order that catches one pointing out
    /// of the folder. Judging the text alone would pass it: nothing in the spelling says where it
    /// goes.
    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folder_is_refused() {
        let (dir, root) = folders();
        std::os::unix::fs::symlink(dir.path().join("secret.txt"), root.join("escape"))
            .expect("the link");
        assert!(under(&root, ["escape"]).is_none());
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
