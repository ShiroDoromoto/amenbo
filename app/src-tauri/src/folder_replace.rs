//! Putting other text where a folder-wide search found something, over as many files as it found it
//! in (`AMB-D-911`).
//!
//! **What is written here is the file, not a draft of it.** The other shape — leaving every changed
//! file unsaved and letting the reader look through them — is what Zed can do because its results
//! *are* an editor holding all of them at once. The panel here draws one file at a time, so fifty
//! unsaved files would be fifty files nobody can see, and "I replaced it and it did not take" is
//! what that leaves behind. There is no undo of our own either: what a replacement is taken back
//! with is git, and a folder outside it cannot take one back at all.
//!
//! **That is paid for by refusing to start rather than by cleaning up afterwards.** A run that is
//! half applied over a folder is the failure this shape has and the other does not, so the whole
//! set of files is checked for writability before a byte is written, and one file that cannot be
//! written stops all of them (JetBrains' `ensureFilesWritable`, which was the one product of four
//! that had it — `AMB-T-4952`). VS Code has neither that nor a comparison against what the search
//! read, and its missing undo has been open since 2018.
//!
//! **A file that moved since the search is left alone, and said so.** The search hands out the mark
//! of the bytes it read (`crate::folder_search`), and that mark comes back in here: a file that no
//! longer answers to it is a file whose lines and columns mean something else now, and writing at
//! them would put the reader's text in a place nobody chose. It is skipped rather than refused,
//! because one file having moved says nothing about the other forty.

use std::collections::HashSet;
use std::path::Path;

use crate::dto::{
    FolderReplaceAtDto, FolderReplaceFileDto, FolderReplacedDto, FolderReplacedFileDto,
    FolderSkippedDto, FolderSkippedFileDto, FolderStoppedFileDto,
};
use crate::error::CmdError;
use crate::folder_bytes::{TEXT_CAP, digest, language_tld};
use crate::folder_fence::{gone, open_no_follow, rooted, under};

/// Write other text at the places a search found, over one folder's files.
///
/// Every file is named the way the search named it — the path under the root, the mark of what was
/// read, and the lines and columns of the hits to replace — and `with` is what goes in their place,
/// as itself: a pattern's groups are not read out of it (`AMB-T-5061`).
///
/// **Nothing is written until every file has been found writable.** What comes back then is what was
/// done, what was left alone and why, and — where the filesystem stopped it part way — which file it
/// stopped on. A run that stopped leaves what it had already written; there is nothing here that
/// could take those back, and pretending otherwise by trying would be a second pass over files that
/// have just been shown to be failing.
#[tauri::command]
pub async fn folder_replace(
    project_id: i64,
    root: String,
    files: Vec<FolderReplaceFileDto>,
    with: String,
) -> Result<FolderReplacedDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || run(project_id, &root, files, &with))
        .await
        .map_err(|e| -> CmdError { format!("this replacement did not finish: {e}").into() })?
}

/// The whole run, off the thread the webview is drawn on: it reads and writes every file named.
fn run(
    project_id: i64,
    root: &str,
    files: Vec<FolderReplaceFileDto>,
    with: &str,
) -> Result<FolderReplacedDto, CmdError> {
    let (roots, base) = rooted(project_id, root)?;
    // A file named with no hits on it is a file this run has nothing to do to, and it is dropped
    // here so that it cannot fail the writability check on behalf of a replacement it is not in.
    let files: Vec<FolderReplaceFileDto> = files.into_iter().filter(|f| !f.at.is_empty()).collect();

    let mut targets = Vec::with_capacity(files.len());
    let mut seen = HashSet::with_capacity(files.len());
    for file in &files {
        let (_owner, target) = under(&roots, base, &file.path).ok_or_else(gone)?;
        if !seen.insert(target.clone()) {
            // Not a coded refusal: a code is a sentence the face has to hold in every language, and
            // this one says a caller named the same file twice. Nobody reading is owed prose for it.
            return Err(format!("{} is named twice in this replacement", file.path.join("/")).into());
        }
        targets.push(target);
    }

    let refused: Vec<String> = files
        .iter()
        .zip(&targets)
        .filter(|(_, target)| !writable(target))
        .map(|(file, _)| file.path.join("/"))
        .collect();
    if !refused.is_empty() {
        return Err(CmdError::coded(
            "folder_replace_read_only",
            format!("{} of these files cannot be written to, so none of them were", refused.len()),
            serde_json::json!({ "count": refused.len(), "paths": refused }),
        ));
    }

    let tld = language_tld();
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    for (file, target) in files.iter().zip(&targets) {
        match one(target, file, with, tld) {
            Ok(Ok(written)) => done.push(FolderReplacedFileDto {
                path: file.path.clone(),
                hits: file.at.len() as u32,
                digest: written,
            }),
            Ok(Err(why)) => {
                skipped.push(FolderSkippedFileDto { path: file.path.clone(), why })
            }
            // The filesystem refused where it had already agreed the file was writable — a disk
            // that filled up, a folder taken away. What is already written stays written.
            Err(reason) => {
                return Ok(FolderReplacedDto {
                    done,
                    skipped,
                    stopped: Some(FolderStoppedFileDto { path: file.path.clone(), reason }),
                });
            }
        }
    }
    Ok(FolderReplacedDto { done, skipped, stopped: None })
}

/// One file: read it as the search read it, and write it back with the text put in.
///
/// Three answers rather than two. The outer `Err` is the filesystem refusing to write, which stops
/// the run; the inner `Err` is this file being one to leave alone, which the next file knows nothing
/// about.
fn one(
    target: &Path,
    file: &FolderReplaceFileDto,
    with: &str,
    tld: Option<&'static [u8]>,
) -> Result<Result<String, FolderSkippedDto>, String> {
    let Ok(opened) = open_no_follow(target) else {
        return Ok(Err(FolderSkippedDto::Unreadable));
    };
    let Ok(meta) = opened.metadata() else {
        return Ok(Err(FolderSkippedDto::Unreadable));
    };
    if !meta.is_file() {
        return Ok(Err(FolderSkippedDto::Unreadable));
    }
    let shared = crate::folder_save::shares_its_bytes(&opened, &meta);
    drop(opened);

    // Read the way the search read it, through the same door, so that what is compared against the
    // mark is the same stretch of the same file.
    let Ok(Some(bytes)) = crate::folder_search::text(target) else {
        return Ok(Err(FolderSkippedDto::NotText));
    };
    if digest(&bytes) != file.seen {
        return Ok(Err(FolderSkippedDto::Changed));
    }
    let truncated = bytes.len() >= TEXT_CAP;
    let read = crate::encoding::read(&bytes, truncated, tld);
    // The same bar a save is held to: a file whose bytes and text are not the same thing said twice
    // cannot be written back without changing what was not replaced (`AMB-D-773`). A file cut at the
    // read cap is one of those, and so is one in an encoding nothing here promises to write.
    if !read.clean {
        return Ok(Err(FolderSkippedDto::NotClean));
    }
    let Some(text) = applied(&read.text, &file.at, with) else {
        return Ok(Err(FolderSkippedDto::Unplaced));
    };
    let Ok(bytes) = crate::encoding::write(&text, read.encoding, read.bom) else {
        // A character the file's own encoding cannot hold — a tick typed into a Shift_JIS file.
        // It is this file's answer and not the run's, so the rest go on (`AMB-D-773`).
        return Ok(Err(FolderSkippedDto::Unwritable));
    };

    crate::folder_save::written(target, &bytes, shared).map_err(|e| e.to_string())?;
    Ok(Ok(digest(&bytes)))
}

/// The text with `with` put in at every place named, or nothing where one of them does not land on
/// this text.
///
/// **The places are the search's own** — a line counted from one, and a column counted in UTF-16
/// code units on that line — and they are taken as read rather than matched against again. What
/// makes that safe is the mark: the bytes are the bytes the search read, so the columns are where it
/// found something. What is still checked is that they make sense as places at all, because they
/// came in over the seam: a line past the end of the file, a column past the end of its line, or two
/// that overlap are a caller holding a map of another file.
///
/// Newlines are never touched. A line's own stretch stops before its `\r\n`, so a file keeps the
/// endings it had, mixed ones included — which is the one thing a save cannot do
/// (`crate::folder_save`), and it can be done here because nothing is being read back out of an
/// editor that flattened them.
fn applied(text: &str, at: &[FolderReplaceAtDto], with: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for chunk in text.split_inclusive('\n') {
        let body = chunk.strip_suffix('\n').unwrap_or(chunk);
        let body = body.strip_suffix('\r').unwrap_or(body);
        lines.push((start, start + body.len()));
        start += chunk.len();
    }

    let mut places: Vec<&FolderReplaceAtDto> = at.iter().collect();
    places.sort_by_key(|place| (place.line, place.at));

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for place in places {
        let (from, to) = *lines.get((place.line as usize).checked_sub(1)?)?;
        let line = &text[from..to];
        let opens = from + utf16_at(line, place.at)?;
        let closes = from + utf16_at(line, place.at.checked_add(place.length)?)?;
        // In order, inside the line, and not over the one before it. Any of the three failing is a
        // map of something other than this file.
        if opens < cursor || closes < opens || closes > to {
            return None;
        }
        out.push_str(&text[cursor..opens]);
        out.push_str(with);
        cursor = closes;
    }
    out.push_str(&text[cursor..]);
    Some(out)
}

/// Where a count of UTF-16 code units lands in these bytes, or nothing where it lands past the end
/// or inside a character.
///
/// The count is the webview's, because the columns came from one: JavaScript measures a string in
/// those units, and the search wrote them that way for the face to slice with
/// (`crate::folder_search`).
fn utf16_at(line: &str, units: u32) -> Option<usize> {
    let mut counted = 0u32;
    for (at, character) in line.char_indices() {
        if counted == units {
            return Some(at);
        }
        counted += character.len_utf16() as u32;
    }
    (counted == units).then_some(line.len())
}

/// Whether this file can be written to, asked by opening it for writing and nothing more.
///
/// **Asked of every file before any of them is written** (`AMB-D-911`). Neither the mode nor the
/// metadata answers it: a file's mode says what its owner may do and the process asking is not
/// always the owner, an access control list can say otherwise on both operating systems, and a
/// read-only volume says nothing in either place. Opening for writing is the question itself.
///
/// **It opens without emptying.** [`crate::folder_fence::write_no_follow`] truncates as it opens,
/// which is what a save wants and the opposite of what a check wants.
///
/// A link at the last name is refused here as everywhere else (`AMB-D-782`), so a replacement never
/// reaches a file through one.
#[cfg(unix)]
fn writable(path: &Path) -> bool {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .is_ok()
}

/// The same question on the operating system whose open does not refuse a link: the handle comes
/// back naming the link itself, and saying no to one is ours to do.
#[cfg(windows)]
fn writable(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .is_ok_and(|file| !file.metadata().is_ok_and(|meta| meta.file_type().is_symlink()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(line: u32, at: u32, length: u32) -> FolderReplaceAtDto {
        FolderReplaceAtDto { line, at, length }
    }

    /// The places are put in where they were found, and everything between them is left exactly as
    /// it was — the newlines included, whichever kind each of them is.
    #[test]
    fn the_places_are_written_and_nothing_between_them_is() {
        let text = "one needle two\r\nneedle\nno\n";
        let out = applied(text, &[place(1, 4, 6), place(2, 0, 6)], "pin").expect("all three land");
        assert_eq!(out, "one pin two\r\npin\nno\n");
    }

    /// Two matches on one line, and the second one's column is still counted from the start of the
    /// line rather than from where the first replacement left off.
    #[test]
    fn two_on_one_line_are_each_where_the_search_said() {
        let text = "aa bb aa\n";
        let out = applied(text, &[place(1, 6, 2), place(1, 0, 2)], "zzz").expect("both land");
        assert_eq!(out, "zzz bb zzz\n");
    }

    /// A column is the webview's count, so a line with Japanese or an emoji in front of the match
    /// lands where the face would have drawn it.
    #[test]
    fn a_column_is_counted_in_what_a_webview_counts_in() {
        let out = applied("日本語 needle\n", &[place(1, 4, 6)], "pin").expect("it lands");
        assert_eq!(out, "日本語 pin\n");
        let out = applied("🍣 needle\n", &[place(1, 3, 6)], "pin").expect("it lands");
        assert_eq!(out, "🍣 pin\n");
    }

    /// A map of another file does not land, and nothing is written from it: a line past the end, a
    /// column past the end of its line, a column inside a character, and two that overlap.
    #[test]
    fn a_place_that_does_not_land_writes_nothing() {
        let text = "日本語 needle\nsecond\n";
        assert!(applied(text, &[place(9, 0, 1)], "pin").is_none(), "no ninth line");
        assert!(applied(text, &[place(0, 0, 1)], "pin").is_none(), "no line counted from zero");
        assert!(applied(text, &[place(2, 4, 9)], "pin").is_none(), "past the end of its line");
        // One emoji is two of the units a column is counted in, and one is not a place at all.
        assert!(applied("\u{1F363} needle\n", &[place(1, 1, 2)], "pin").is_none(), "inside a character");
        assert!(
            applied(text, &[place(1, 4, 6), place(1, 6, 4)], "pin").is_none(),
            "the second starts inside the first",
        );
    }

    /// A file that ends without a newline is a line all the same, and the file goes on ending
    /// without one.
    #[test]
    fn the_last_line_needs_no_newline_to_be_one() {
        let out = applied("a\nneedle", &[place(2, 0, 6)], "pin").expect("it lands");
        assert_eq!(out, "a\npin");
    }

    /// A replacement is written into the file the search read, with the encoding and the newlines
    /// it had, and the mark of what was written comes back.
    #[test]
    fn a_file_is_written_back_as_itself() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let target = dir.path().join("a.txt");
        let (was, _, _) = encoding_rs::SHIFT_JIS.encode("これはタスクです\r\n");
        std::fs::write(&target, &was[..]).expect("a file");

        let file = FolderReplaceFileDto {
            path: vec!["a.txt".to_string()],
            seen: digest(&was),
            at: vec![place(1, 3, 3)],
        };
        let mark = one(&target, &file, "作業", None).expect("the filesystem agreed").expect("it lands");

        let now = std::fs::read(&target).expect("the file is there");
        let (text, _, bad) = encoding_rs::SHIFT_JIS.decode(&now);
        assert!(!bad, "written back in the encoding it was read in");
        assert_eq!(text, "これは作業です\r\n", "and with the newline it had");
        assert_eq!(mark, digest(&now));
    }

    /// A file whose mark the replacement no longer answers to is left alone: its lines and columns
    /// are a map of other bytes.
    #[test]
    fn a_file_that_moved_is_left_alone() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let target = dir.path().join("a.txt");
        std::fs::write(&target, "needle\n").expect("a file");

        let file = FolderReplaceFileDto {
            path: vec!["a.txt".to_string()],
            seen: digest(b"something else entirely"),
            at: vec![place(1, 0, 6)],
        };
        let why = one(&target, &file, "pin", None).expect("the filesystem agreed").expect_err("skipped");
        assert!(matches!(why, FolderSkippedDto::Changed));
        assert_eq!(std::fs::read_to_string(&target).expect("still there"), "needle\n");
    }

    /// A picture is not a file to replace text in, and neither is one in an encoding this cannot
    /// write back.
    #[test]
    fn what_cannot_be_written_back_is_left_alone() {
        let dir = tempfile::tempdir().expect("a temp dir");

        let picture = dir.path().join("p.png");
        let bytes = b"\x89PNG\r\n\x1a\n\0needle".to_vec();
        std::fs::write(&picture, &bytes).expect("a file");
        let file = FolderReplaceFileDto {
            path: vec!["p.png".to_string()],
            seen: digest(&bytes),
            at: vec![place(1, 0, 6)],
        };
        let why = one(&picture, &file, "pin", None).expect("the filesystem agreed").expect_err("skipped");
        assert!(matches!(why, FolderSkippedDto::NotText));

        // Big5 is read and shown; it is not one of the encodings a save is promised in.
        let other = dir.path().join("b.txt");
        let (bytes, _, _) = encoding_rs::BIG5.encode("一二三四五六七八九十 needle\n");
        std::fs::write(&other, &bytes[..]).expect("a file");
        let file = FolderReplaceFileDto {
            path: vec!["b.txt".to_string()],
            seen: digest(&bytes),
            at: vec![place(1, 11, 6)],
        };
        let why = one(&other, &file, "pin", None).expect("the filesystem agreed").expect_err("skipped");
        assert!(matches!(why, FolderSkippedDto::NotClean), "{why:?}");
    }

    /// A file nothing may write to is found before anything is written, which is what a whole run
    /// being refused rests on.
    #[test]
    #[cfg(unix)]
    fn a_file_that_cannot_be_written_to_is_known_before_anything_is() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("a temp dir");
        let open = dir.path().join("open.txt");
        let shut = dir.path().join("shut.txt");
        std::fs::write(&open, "needle\n").expect("a file");
        std::fs::write(&shut, "needle\n").expect("a file");
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o444)).expect("read-only");

        assert!(writable(&open));
        assert!(!writable(&shut));
        // The check does not empty what it opens.
        assert_eq!(std::fs::read_to_string(&open).expect("still there"), "needle\n");
    }
}
