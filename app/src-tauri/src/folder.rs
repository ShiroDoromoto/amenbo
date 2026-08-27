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

use amenbo_core::binding::canonical_dir;
use base64::Engine as _;

use crate::dto::{
    FolderChangedDto, FolderEntryDto, FolderFileDto, FolderImageDto, FolderOversizeDto,
};
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
///
/// The old quarter of a megabyte was a number for looking, not for working: four files in this
/// repository alone are over it, and a cut one written back drops its tail without saying so
/// (`AMB-D-783`). What is cut is what `truncated` is for — a face that lets a file be edited reads
/// it and refuses to save.
const TEXT_CAP: usize = 5 * 1024 * 1024;

/// The largest picture carried whole over the command seam. Past it the reader is told there is a
/// picture and not made to wait for it.
///
/// This is the cap on what the **host** holds: the file is read whole into this process before any
/// of it reaches the webview.
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
///
/// **Canonical here is the reader's spelling** ([`canonical_dir`], `AMB-D-703`), not
/// `std::fs::canonicalize`'s. On Windows that call answers in the verbatim `\\?\C:\…` form, and a
/// path in that form is not a path every Win32 entry point takes: `SHOpenWithDialog` rejects it
/// outright with `E_INVALIDARG` and draws nothing (`AMB-T-3651` measured it on a real machine).
/// What leaves this fence is handed to the shell, so it leaves in the form the shell accepts.
pub fn under(root: &Path, segments: impl IntoIterator<Item = impl AsRef<str>>) -> Option<PathBuf> {
    let root = canonical_dir(root).ok()?;

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
    let path = canonical_dir(&path).ok()?;
    path.starts_with(&root).then_some(path)
}

/// The folder this call is rooted at, having established that the project really is bound to it.
///
/// The caller names a root, and a webview is not trusted to name one: the registry is asked whether
/// this project claims that folder. Everything else in this module resolves under what comes back.
pub fn root_of(project_id: i64, root: &str) -> Result<PathBuf, CmdError> {
    let asked = canonical_dir(root).map_err(|_| gone())?;
    let store = crate::commands::open_store_read()?;
    let bound = store
        .bindings()
        .dirs_for_project(project_id)
        .into_iter()
        .any(|dir| canonical_dir(dir).is_ok_and(|dir| dir == asked));
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

/// What one file has to show: its text, or its picture, or why the picture is not here, or none of
/// those.
///
/// The head is read once and answers both questions — whether there is a NUL in it, and what the
/// first bytes say the file is — so a name is never consulted about either. Only what that head
/// settles on is then read further: the text up to its cap, or a JPEG's front far enough to reach
/// the frame header. A file that is neither is never read past the head at all.
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
    let head = read_head(&file, HEAD).map_err(|_| gone())?;

    // The one judgement, made on bytes: text is what has no NUL in its head. Reading it as UTF-8 is
    // a separate matter and never a verdict — a file cut at the cap can end inside a character, and
    // a page of text in another encoding is still text to a person looking for what they wrote.
    if !head.contains(&0) {
        let bytes = if size > HEAD as u64 {
            read_head(&file, TEXT_CAP).map_err(|_| gone())?
        } else {
            head
        };
        return Ok(FolderFileDto {
            truncated: (bytes.len() as u64) < size,
            text: Some(String::from_utf8_lossy(&bytes).into_owned()),
            image: None,
            oversize: None,
        });
    }

    let Some(mime) = picture(&head) else {
        return Ok(FolderFileDto { text: None, truncated: false, image: None, oversize: None });
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
        let image = std::fs::read(&file).ok().map(|whole| FolderImageDto {
            mime: mime.to_string(),
            base64: base64::engine::general_purpose::STANDARD.encode(whole),
        });
        return Ok(FolderFileDto { text: None, truncated: false, image, oversize: None });
    }
    Ok(FolderFileDto {
        text: None,
        truncated: false,
        image: None,
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
        assert_eq!(under(&root, ["notes"]), Some(canonical_dir(root.join("notes")).unwrap()));
        assert_eq!(under(&root, Vec::<String>::new()), Some(canonical_dir(&root).unwrap()));
        drop(dir);
    }

    /// What the fence hands back is handed on to the shell, so it comes back in the spelling the
    /// shell takes and not in Win32's internal one (`AMB-D-703`). A verbatim `\\?\C:\…` path is
    /// what `std::fs::canonicalize` answers with on Windows, and `SHOpenWithDialog` refuses one
    /// with `E_INVALIDARG` and draws nothing at all — measured on a real machine in `AMB-T-3651`.
    #[test]
    fn what_the_fence_hands_back_is_spelled_the_way_the_shell_takes_it() {
        let (dir, root) = folders();
        let file = under(&root, ["notes", "a.md"]).expect("a file inside the folder");
        assert!(
            !file.to_string_lossy().starts_with(r"\\?\"),
            "no verbatim prefix leaves the fence: {}",
            file.display(),
        );
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
