//! Putting a file's text back where it was read from (`AMB-D-776`).
//!
//! [`crate::folder`] reads a file and [`crate::folder_write`] alters what names a folder holds.
//! This is the third door and the one with the narrowest job: the same file, the same name, other
//! bytes. It is its own module because what it has to get right is nothing either of the others has
//! to think about.
//!
//! **There are two ways to write a file and each of them loses something** (`AMB-T-3739` measured
//! both, `AMB-D-776` chose between them).
//!
//! | | writing into the file itself | writing beside it and moving it over |
//! |---|---|---|
//! | a machine that stops half way | the file is half written | the file is untouched |
//! | the mode, the extended attributes | kept | **lost** |
//! | a hard link's other name | follows | **left on the old bytes** |
//!
//! Losing the mode is the one that costs a reader something they cannot see: a script with its
//! execute bit set came out `644` and stopped running, and nothing said so. So a file is written
//! beside and moved over — and the file written beside it **starts as a copy of the one it will
//! replace**, which is what carries the mode over (and, on macOS, the extended attributes with it).
//!
//! **A hard link is the exception**, and the only one. Its bytes have another name on them, and
//! moving a new file over this one would leave that other name on the old bytes — two names that
//! were one file, quietly becoming two. That file is written into directly, and the half-written
//! window is what it costs.
//!
//! **Nothing here tells the watch anything.** A save wakes it like any other write and the face is
//! told, which is right: what a reader has just saved is a file whose git colour has moved, and
//! that colour is what the tree draws (`AMB-D-785`). The one thing a save owes the walk is that the
//! file it writes beside the real one never appears in it ([`crate::folder::SAVING`]).
//!
//! **A link is not one of the cases.** `AMB-D-776` was written with three, the third being to write
//! through a symbolic link at the file it points at; `AMB-D-782` was settled after it and closed
//! that door on the reading side ([`crate::folder::open_no_follow`]), so a file reached through a
//! link never opens, never reaches an editor, and has no text to be saved. What is left is the two
//! above.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use encoding_rs::Encoding;

use crate::dto::FolderLineEndingDto;
use crate::error::CmdError;
use crate::folder::{gone, open_no_follow, rooted, under, write_no_follow, SAVING};

/// How many saves this process has begun. It is the whole of what the name of a half-written file
/// has to carry: that name only has to be in the folder the file is in, be nobody else's, and be
/// recognisable as this app's ([`SAVING`]).
static SAVES: AtomicU64 = AtomicU64::new(0);

/// Write one file's text back, in the encoding and the newline it was read in.
///
/// **The whole text is what travels, both ways.** The editor holds the changes as a diff and could
/// send that instead — but a diff applied here would mean this side holding the document too, and
/// then there would be two of them to keep the same. Reading already pays for the whole text
/// ([`crate::folder::folder_read`]); saving pays for it once more, on a file of at most five
/// megabytes.
///
/// **What may not be saved is refused before anything is opened**, and the panel already knows all
/// of it: a file cut at the read cap, and one whose bytes and text do not round-trip, are drawn
/// read-only from the start (`AMB-D-773`). What only shows up here is a character the encoding
/// cannot write — a `✓` typed into a Shift_JIS file — which is named back rather than mangled into
/// the file as `&#10003;`.
#[tauri::command]
pub fn folder_save(
    project_id: i64,
    root: String,
    path: Vec<String>,
    text: String,
    encoding: String,
    bom: bool,
    line_ending: FolderLineEndingDto,
) -> Result<(), CmdError> {
    let Some(encoding) = crate::encoding::writable(&encoding) else {
        return Err(not_saved(format_args!("{encoding} is not an encoding this writes back")));
    };
    let text = lines_ending(&text, line_ending)?;
    let bytes = crate::encoding::write(&text, encoding, bom)
        .map_err(|character| unwritable(character, encoding))?;

    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, target) = under(&roots, base, &path).ok_or_else(gone)?;
    // Opened before anything is written, for the two answers only the open can give: that the last
    // name is not a link (the kernel refuses in the same call), and that what is there is a file.
    let opened = open_no_follow(&target).map_err(|_| gone())?;
    let meta = opened.metadata().map_err(|_| gone())?;
    if !meta.is_file() {
        return Err(gone());
    }
    let shared = shares_its_bytes(&opened, &meta);
    drop(opened);

    if shared { in_place(&target, &bytes) } else { replace(&target, &bytes) }
        .map_err(not_saved)?;
    Ok(())
}

/// The text with the newline the file has, from an editor that hands back only one kind.
///
/// CodeMirror reads `\r\n`, `\r` and `\n` alike and gives every one of them back as `\n`
/// (`app/src/files/editorLoad.ts`), so what the file's newline was is not in the text any more. It
/// is in `line_ending`, which the panel carried out of the read and back in (`AMB-D-773`).
///
/// **A file with both kinds is refused here**, because which one it should become has a reader's
/// answer and no right one: rounding to the commoner kind rewrites every line of the other, and
/// 1,106 files in a real folder are mixed (`AMB-T-3746`). The panel asks and sends what was chosen,
/// so a mixed answer arriving here is a caller that never asked.
fn lines_ending(text: &str, ending: FolderLineEndingDto) -> Result<String, CmdError> {
    if matches!(ending, FolderLineEndingDto::Mixed) {
        return Err(not_saved("a file with both kinds of newline is saved in one of them"));
    }
    let flattened = text.replace("\r\n", "\n").replace('\r', "\n");
    Ok(match ending {
        FolderLineEndingDto::Crlf => flattened.replace('\n', "\r\n"),
        _ => flattened,
    })
}

/// Write the bytes into a file of this app's own beside the one being saved, and move them over it.
///
/// **The half-written file starts as a copy**, and that is the whole of what keeps a replace from
/// being a loss. What a copy carries is the operating system's answer — the mode on all three, and
/// the extended attributes as well on macOS, where `fs::copy` is `fcopyfile`. Written from nothing
/// instead, the new file comes out with whatever the umask says, which is how a script measured
/// `640` before a save and `644` after it (`AMB-T-3739`).
///
/// **What was half written is taken away again.** A disk that filled up part way through left 40 MB
/// of it behind in that measurement, in a folder somebody is looking at.
///
/// The copy is the one step here that would follow a link, and the open above has already refused
/// one — what is between those two answers is a person's own machine, and even there the fence
/// holds: the bytes are written into the folder either way, since `rename` replaces a link rather
/// than writing through it. What such a race could carry over is another file's mode, not another
/// file's contents (`AMB-D-782` draws the same line for `crate::folder_write`).
fn replace(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let folder = target
        .parent()
        .ok_or_else(|| std::io::Error::other("a file with no folder above it"))?;
    let half = folder.join(format!("{SAVING}{}", SAVES.fetch_add(1, Ordering::Relaxed)));

    let saved = std::fs::copy(target, &half)
        .and_then(|_| put(&half, bytes, true))
        .and_then(|()| std::fs::rename(&half, target));
    if saved.is_err() {
        let _ = std::fs::remove_file(&half);
    }
    saved
}

/// Write the bytes into the file itself — what a hard link asks for, and what it costs: a machine
/// that stops half way through leaves the file half written. Every other file goes through
/// [`replace`], so this is the only shape with that window in it (`AMB-D-776`).
fn in_place(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    put(target, bytes, false)
}

/// Empty one file and write these bytes into it, with a link at the last name refused rather than
/// followed ([`write_no_follow`]).
fn put(path: &Path, bytes: &[u8], create: bool) -> std::io::Result<()> {
    use std::io::Write as _;
    write_no_follow(path, create)?.write_all(bytes)
}

/// Whether this file's bytes have more than one name — the one file written into directly.
#[cfg(unix)]
fn shares_its_bytes(_file: &std::fs::File, meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    meta.nlink() > 1
}

/// The same question where the standard library has no settled way to ask it: `MetadataExt`'s
/// `number_of_links` is still unstable, so the handle already in hand is asked directly — the same
/// call the standard library makes underneath. A handle that will not answer is read as one name,
/// which is what all but a vanishingly small number of files have.
#[cfg(windows)]
fn shares_its_bytes(file: &std::fs::File, _meta: &std::fs::Metadata) -> bool {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut about: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is a file this process holds open for the length of the call, and the
    // structure is one the call fills in rather than reads.
    let answered = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut about) };
    answered != 0 && about.nNumberOfLinks > 1
}

/// The one refusal a reader can actually cause: a character that has no place in this encoding.
///
/// **It names the character**, which is the whole reason the encoder used is the one that stops
/// rather than the one that substitutes. `Encoding::encode` would have written a `✓` into a
/// Shift_JIS file as `&#10003;` and said nothing (`AMB-D-773`).
fn unwritable(character: char, encoding: &'static Encoding) -> CmdError {
    CmdError::coded(
        "folder_unwritable_character",
        format!("{character} cannot be written in {}", encoding.name()),
        serde_json::json!({ "character": character.to_string(), "encoding": encoding.name() }),
    )
}

/// Everything else, in the machine's own words. What a disk that filled up or a permission that was
/// not there has to say is its own, and a sentence written here would be a guess at which it was —
/// the same reading `crate::folder_write` takes for a carry that stopped.
fn not_saved(reason: impl std::fmt::Display) -> CmdError {
    let reason = reason.to_string();
    CmdError::coded(
        "folder_not_saved",
        format!("this file could not be saved: {reason}"),
        serde_json::json!({ "reason": reason }),
    )
}

/// What a caller could ask for, and the two answers the filesystem gives that a reader would feel.
#[cfg(test)]
mod tests {
    use super::*;

    /// The reason a file is replaced rather than written from nothing: a script keeps the bit that
    /// makes it a script. Measured going the other way first — `640` became `644` and the file
    /// stopped running (`AMB-T-3739`).
    #[cfg(unix)]
    #[test]
    fn a_saved_script_is_still_a_script() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("a temp dir");
        let script = dir.path().join("run.sh");
        std::fs::write(&script, b"#!/bin/sh\necho one\n").expect("a file");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("the mode");

        replace(&script, b"#!/bin/sh\necho two\n").expect("the save");

        assert_eq!(std::fs::read(&script).expect("the file"), b"#!/bin/sh\necho two\n");
        let mode = std::fs::metadata(&script).expect("the file").permissions().mode();
        assert_eq!(mode & 0o777, 0o755, "the execute bit is still on: {mode:o}");
    }

    /// A Japanese document in a legacy encoding goes back into the file as the bytes it came out
    /// of, which is the whole of what carrying the encoding out and back in is for (`AMB-D-773`).
    #[test]
    fn a_shift_jis_document_is_saved_as_the_bytes_it_was_read_as() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("メモ.txt");
        let written = "日本語のなかに English が混ざった文書。\r\n二行目もある。\r\n";
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(written);
        std::fs::write(&file, &bytes).expect("a file");

        // What the panel hands back: one kind of newline, the file's own encoding beside it.
        let typed = lines_ending(
            "日本語のなかに English が混ざった文書。\n二行目もある。\n",
            FolderLineEndingDto::Crlf,
        )
        .expect("the newline the file had");
        let out = crate::encoding::write(&typed, encoding_rs::SHIFT_JIS, false).expect("the bytes");
        replace(&file, &out).expect("the save");

        assert_eq!(std::fs::read(&file).expect("the file"), bytes.to_vec());
    }

    /// A hard link is written into rather than replaced, so the file's other name is still the same
    /// file afterwards. Replacing it would have left that name on the old bytes (`AMB-D-776`).
    #[cfg(unix)]
    #[test]
    fn a_hard_link_stays_one_file() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let one = dir.path().join("one.md");
        let other = dir.path().join("other.md");
        std::fs::write(&one, b"before").expect("a file");
        std::fs::hard_link(&one, &other).expect("the second name");

        let meta = std::fs::symlink_metadata(&one).expect("the file");
        let opened = open_no_follow(&one).expect("the file");
        assert!(shares_its_bytes(&opened, &meta), "two names, one file");

        in_place(&one, b"after").expect("the save");
        assert_eq!(std::fs::read(&other).expect("the other name"), b"after");
    }

    /// Nothing of the save is left in the folder when it could not be made. The measurement that
    /// asked for this filled a disk and found 40 MB of a half-written file still there.
    #[test]
    fn a_save_that_could_not_be_made_leaves_nothing_behind() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("note.md");
        std::fs::write(&file, b"mine").expect("a file");
        // A folder where the move cannot land: the name is taken by a directory, which `rename`
        // refuses on every one of the three.
        let blocked = dir.path().join("taken.md");
        std::fs::create_dir(&blocked).expect("a folder");

        replace(&blocked, b"nothing doing").expect_err("a folder is not a file to save over");
        let left: Vec<String> = std::fs::read_dir(dir.path())
            .expect("the folder")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(SAVING))
            .collect();
        assert!(left.is_empty(), "the half-written file was cleaned up: {left:?}");
    }

    /// The newline the file had is put back, from a text that has only one kind in it — and a file
    /// with both is not something to guess at.
    #[test]
    fn the_newline_comes_back_the_way_the_file_had_it() {
        let typed = "one\ntwo\n";
        assert_eq!(lines_ending(typed, FolderLineEndingDto::Lf).expect("lf"), "one\ntwo\n");
        assert_eq!(lines_ending(typed, FolderLineEndingDto::Crlf).expect("crlf"), "one\r\ntwo\r\n");
        // Whatever the editor hands back, saying it once is what it comes out as.
        assert_eq!(
            lines_ending("one\r\ntwo\r", FolderLineEndingDto::Crlf).expect("crlf"),
            "one\r\ntwo\r\n",
        );
        let refused = lines_ending(typed, FolderLineEndingDto::Mixed).expect_err("nobody asked");
        assert_eq!(refused.code, "folder_not_saved");
    }

    /// A character the encoding has no room for stops the save and comes back by name, rather than
    /// reaching the file as `&#10003;` with nobody told (`AMB-D-773`).
    #[test]
    fn a_character_that_cannot_be_written_is_named_and_nothing_is_saved() {
        let refused = unwritable('✓', encoding_rs::SHIFT_JIS);
        assert_eq!(refused.code, "folder_unwritable_character");
        assert_eq!(refused.fields["character"], "✓");
        assert_eq!(refused.fields["encoding"], "Shift_JIS");
    }
}
