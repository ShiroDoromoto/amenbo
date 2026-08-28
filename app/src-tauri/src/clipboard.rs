//! The machine's own clipboard, holding files rather than words (`AMB-D-796`).
//!
//! **The keys are the operating system's, so what they do has to be the operating system's too.**
//! A panel that remembered a copied row by itself would be smaller to write and would make `⌘C` a
//! lie: a reader who copied a row here and pressed paste in their file manager would get nothing,
//! and a file copied there would not come in here. The key would look like the one every other
//! application has and mean something only inside this window.
//!
//! **The webview cannot do it.** `navigator.clipboard` carries text and images and has no way to
//! carry a file, so the reading and the writing are the host's on all three machines — which is the
//! whole of why this module exists.
//!
//! Each machine holds files under a name of its own, and none of the three is a format the other two
//! read:
//!
//! | | what is put on it |
//! |---|---|
//! | macOS | `NSURL`s, written as file URLs |
//! | Windows | `CF_HDROP` — a header and a run of wide paths, ended twice |
//! | Linux | `text/uri-list` — `file://` lines |
//!
//! ⚠ **On Linux the clipboard is this process, not the system.** X11 and Wayland both make the
//! copying application the owner of the selection, serving it when somebody asks; the other two hand
//! the bytes to the system and let go. So a copy made here survives Amenbo closing on macOS and
//! Windows and does not on Linux. That is the shape of the clipboard there rather than something
//! missing here, and nothing in this module can paper over it.

use std::path::PathBuf;

// -------------------------------------------------------------------------------------------
// macOS — the pasteboard takes objects, and a file is an NSURL.
// -------------------------------------------------------------------------------------------

/// Put these files on the pasteboard, replacing whatever was on it.
///
/// `writeObjects` is the door that takes a list, so the whole selection goes on in one call and
/// arrives in Finder as the several files it is. Clearing first is not optional: the pasteboard is
/// append-only within a change count, and skipping it would leave the last copy's files on with
/// these.
#[cfg(target_os = "macos")]
pub fn put(paths: &[PathBuf]) -> Result<(), String> {
    put_on(&objc2_app_kit::NSPasteboard::generalPasteboard(), paths)
}

/// The writing itself, told which pasteboard to write on. Named apart from [`put`] so the round trip
/// can be held to itself on a pasteboard of its own: a test that used the machine's would take the
/// clipboard away from whoever was running it.
#[cfg(target_os = "macos")]
fn put_on(pasteboard: &objc2_app_kit::NSPasteboard, paths: &[PathBuf]) -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::NSPasteboardWriting;
    use objc2_foundation::{NSArray, NSString, NSURL};

    let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = paths
        .iter()
        .map(|path| {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
            ProtocolObject::from_retained(url)
        })
        .collect();
    if urls.is_empty() {
        return Err("nothing to copy".to_string());
    }

    pasteboard.clearContents();
    let wrote = pasteboard.writeObjects(&NSArray::from_retained_slice(&urls));
    if wrote {
        Ok(())
    } else {
        Err("the pasteboard would not take them".to_string())
    }
}

/// The files on the pasteboard, or none where what is on it is not files.
///
/// **Each item is asked for the one flavour a file is written under**, rather than the pasteboard
/// being asked to hand back objects. The two are not the same reading: a pasteboard holding several
/// files holds an item apiece, and the flavour is the name every writer of a file agrees on — the
/// panel's own copy, and the reader's file manager.
///
/// An item with no file under it is dropped rather than refused: a pasteboard holding a picture or a
/// line of text is one this folder has nothing to take from, which is not a failure.
#[cfg(target_os = "macos")]
pub fn take() -> Vec<PathBuf> {
    take_from(&objc2_app_kit::NSPasteboard::generalPasteboard())
}

/// The reading itself, told which pasteboard to read. Apart from [`take`] for the reason [`put_on`]
/// is apart from [`put`].
#[cfg(target_os = "macos")]
fn take_from(pasteboard: &objc2_app_kit::NSPasteboard) -> Vec<PathBuf> {
    use objc2_app_kit::NSPasteboardTypeFileURL;
    use objc2_foundation::NSURL;

    // SAFETY: the flavour's name is a constant AppKit defines and this only reads it.
    let flavour = unsafe { NSPasteboardTypeFileURL };
    let Some(items) = pasteboard.pasteboardItems() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.stringForType(flavour))
        .filter_map(|text| NSURL::URLWithString(&text))
        .filter_map(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .collect()
}

// -------------------------------------------------------------------------------------------
// Windows — CF_HDROP, which is the format a file manager both writes and reads.
// -------------------------------------------------------------------------------------------

/// Put these files on the clipboard as `CF_HDROP`.
///
/// The block is a `DROPFILES` header followed by every path as wide characters, each ended with a
/// NUL and the run ended with a second one. `fWide` says the paths are UTF-16 rather than the code
/// page, which is the only reading under which a name outside it survives the trip.
///
/// The memory is the clipboard's once `SetClipboardData` has taken it, so it is allocated movable
/// and **not** freed here — freeing what was handed over is a double free the shell performs later.
#[cfg(target_os = "windows")]
pub fn put(paths: &[PathBuf]) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::UI::Shell::DROPFILES;

    if paths.is_empty() {
        return Err("nothing to copy".to_string());
    }

    let mut names: Vec<u16> = Vec::new();
    for path in paths {
        names.extend(path.as_os_str().encode_wide());
        names.push(0);
    }
    names.push(0);

    let head = std::mem::size_of::<DROPFILES>();
    let bytes = head + names.len() * std::mem::size_of::<u16>();
    // SAFETY: every call below is on the handle this one answered with, and the block is written
    // once, while locked, before it is handed over.
    unsafe {
        let block = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if block.is_null() {
            return Err("there was no room for the list".to_string());
        }
        let at = GlobalLock(block);
        if at.is_null() {
            return Err("the list could not be written".to_string());
        }
        let header = at.cast::<DROPFILES>();
        (*header).pFiles = head as u32;
        (*header).pt.x = 0;
        (*header).pt.y = 0;
        (*header).fNC = 0;
        (*header).fWide = 1;
        std::ptr::copy_nonoverlapping(
            names.as_ptr(),
            at.cast::<u8>().add(head).cast::<u16>(),
            names.len(),
        );
        GlobalUnlock(block);

        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("the clipboard was busy".to_string());
        }
        EmptyClipboard();
        let taken = SetClipboardData(CF_HDROP, block);
        CloseClipboard();
        if taken.is_null() {
            return Err("the clipboard would not take them".to_string());
        }
    }
    Ok(())
}

/// The number the shell files dropped paths under. `windows-sys` types the clipboard formats as a
/// plain `u32`, and this is the one this module reads and writes.
#[cfg(target_os = "windows")]
const CF_HDROP: u32 = 15;

/// The files on the clipboard, or none where what is on it is not files.
#[cfg(target_os = "windows")]
pub fn take() -> Vec<PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;

    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

    let mut found = Vec::new();
    // SAFETY: the handle is the clipboard's own and is only read, between opening and closing it.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return found;
        }
        let block = GetClipboardData(CF_HDROP);
        if !block.is_null() {
            let drop = block as HDROP;
            // `0xFFFF_FFFF` is the count rather than an index, which is how this call is asked how
            // many there are before it is asked for any of them.
            let many = DragQueryFileW(drop, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
            for one in 0..many {
                let wide = DragQueryFileW(drop, one, std::ptr::null_mut(), 0);
                if wide == 0 {
                    continue;
                }
                // Room for the name and the NUL the call writes after it.
                let mut name = vec![0u16; wide as usize + 1];
                let written = DragQueryFileW(drop, one, name.as_mut_ptr(), name.len() as u32);
                if written == 0 {
                    continue;
                }
                name.truncate(written as usize);
                found.push(PathBuf::from(std::ffi::OsString::from_wide(&name)));
            }
        }
        CloseClipboard();
    }
    found
}

// -------------------------------------------------------------------------------------------
// Linux — `text/uri-list`, served by this process for as long as it is up.
// -------------------------------------------------------------------------------------------

/// Put these files on the clipboard as a `text/uri-list`.
///
/// The list is the format every file manager on this desktop reads, and the lines are `file://`
/// URLs rather than paths: a path with a space in it is a valid line and an invalid URL, and the
/// reader on the other side is parsing URLs.
///
/// ⚠ **What is stored is a promise to serve, not the bytes.** The selection belongs to this process
/// until another application takes it, so what was copied goes when Amenbo goes (`AMB-D-796`).
#[cfg(target_os = "linux")]
pub fn put(paths: &[PathBuf]) -> Result<(), String> {
    use gtk::gdk::Atom;

    if paths.is_empty() {
        return Err("nothing to copy".to_string());
    }
    let list = uri_list(paths);
    let clipboard = gtk::Clipboard::get(&Atom::intern("CLIPBOARD"));
    let targets = [gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 0)];
    clipboard.set_with_data(&targets, move |_, selection, _| {
        selection.set(&Atom::intern("text/uri-list"), 8, list.as_bytes());
    });
    Ok(())
}

/// The files on the clipboard, or none where what is on it is not files.
#[cfg(target_os = "linux")]
pub fn take() -> Vec<PathBuf> {
    use gtk::gdk::Atom;

    let clipboard = gtk::Clipboard::get(&Atom::intern("CLIPBOARD"));
    let Some(selection) = clipboard.wait_for_contents(&Atom::intern("text/uri-list")) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&selection.data()).into_owned();
    from_uri_list(&text)
}

/// The lines a `text/uri-list` is made of. Written apart from the serving so it can be read on its
/// own, and so the pair of it and [`from_uri_list`] can be held to each other in a test.
#[cfg(target_os = "linux")]
fn uri_list(paths: &[PathBuf]) -> String {
    paths.iter().map(|path| as_uri(path)).collect::<Vec<_>>().join("\r\n")
}

/// One path as a `file://` URL, with everything a URL may not carry written as its bytes.
///
/// The unreserved set is the one the URL syntax names, plus the separator and the three punctuation
/// marks a file name ordinarily holds — a name is escaped so the reader gets the name back, not so
/// it looks tidy.
#[cfg(target_os = "linux")]
fn as_uri(path: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt as _;

    let mut url = String::from("file://");
    for byte in path.as_os_str().as_bytes().iter().copied() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                url.push(byte as char);
            }
            other => url.push_str(&format!("%{other:02X}")),
        }
    }
    url
}

/// The paths in a `text/uri-list`, which is the format read back rather than a format of ours: the
/// comment lines it allows are dropped, and a line naming anything but a file is not a path.
#[cfg(target_os = "linux")]
fn from_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("file://"))
        .map(|rest| PathBuf::from(unescape(rest)))
        .collect()
}

/// A URL's bytes back as they were written. A `%` that is not the start of a pair is left as it
/// stands: it is a byte in a name that somebody else did not escape, and dropping it would hand
/// back a different name.
#[cfg(target_os = "linux")]
fn unescape(text: &str) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt as _;

    let raw = text.as_bytes();
    let mut out = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < raw.len() {
        if raw[at] == b'%' && at + 2 < raw.len() {
            if let Ok(byte) = u8::from_str_radix(&text[at + 1..at + 3], 16) {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(raw[at]);
        at += 1;
    }
    std::ffi::OsString::from_vec(out)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    /// A name a URL cannot carry as it stands comes back as itself, which is the whole of what the
    /// escaping is for: the reader on the other side is opening the file, not reading the line.
    #[test]
    fn a_name_survives_the_trip_through_a_uri_list() {
        let paths = vec![
            PathBuf::from("/home/alice/notes/a plain one.md"),
            PathBuf::from("/home/alice/notes/100% done.md"),
            PathBuf::from("/home/alice/notes/新しいファイル.md"),
        ];
        assert_eq!(from_uri_list(&uri_list(&paths)), paths);
    }

    /// The format's own comment lines, and a line that is not a file at all.
    #[test]
    fn only_the_lines_that_name_a_file_come_back() {
        let list = "# a comment\r\nfile:///tmp/one.md\r\nhttps://example.com/two.md\r\n\r\n";
        assert_eq!(from_uri_list(list), vec![PathBuf::from("/tmp/one.md")]);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod mac_tests {
    use super::*;

    /// The pair held to each other on a pasteboard of this test's own, which is the only way to ask
    /// the question without taking the clipboard away from whoever is running the suite.
    ///
    /// It is worth asking on the real thing rather than a stand-in: what a pasteboard hands back for
    /// a file is AppKit's answer, and the shape this was written in first wrote a file the machine
    /// could read and read back nothing at all.
    #[test]
    fn a_file_written_to_a_pasteboard_is_the_file_read_back_off_it() {
        use objc2_app_kit::NSPasteboard;

        let dir = tempfile::tempdir().expect("a temp dir");
        let one = dir.path().join("a plain one.md");
        let two = dir.path().join("100% done.md");
        std::fs::write(&one, b"mine").expect("a file");
        std::fs::write(&two, b"mine too").expect("a file");

        let pasteboard = NSPasteboard::pasteboardWithUniqueName();
        put_on(&pasteboard, &[one.clone(), two.clone()]).expect("the pasteboard takes them");

        // The names come back as they were written, punctuation and all: what a URL cannot carry as
        // it stands is escaped on the way on, and undoing that is not this side's to get right.
        let read = take_from(&pasteboard);
        assert_eq!(read.len(), 2, "both files come back: {read:?}");
        assert!(read.iter().all(|path| path.is_file()), "and they are the files: {read:?}");
        assert_eq!(
            read.iter().filter_map(|path| path.file_name()).collect::<Vec<_>>(),
            vec![one.file_name().expect("a name"), two.file_name().expect("a name")],
        );
    }

    /// ⚠ **A command runs on a worker thread, and this is the reading it does there.** AppKit is
    /// mostly the main thread's, so the question is whether the pasteboard is one of the parts that
    /// insists — a read that answered on one thread and not the other would be a feature that worked
    /// in a test and never once in the app.
    #[test]
    fn the_reading_answers_off_the_main_thread_too() {
        use objc2_app_kit::NSPasteboard;

        let dir = tempfile::tempdir().expect("a temp dir");
        let one = dir.path().join("away.md");
        std::fs::write(&one, b"mine").expect("a file");

        let pasteboard = NSPasteboard::pasteboardWithUniqueName();
        let name = pasteboard.name().to_string();
        put_on(&pasteboard, std::slice::from_ref(&one)).expect("the pasteboard takes it");

        let read = std::thread::spawn(move || {
            use objc2_foundation::NSString;
            let same = NSPasteboard::pasteboardWithName(&NSString::from_str(&name));
            take_from(&same)
        })
        .join()
        .expect("the thread");
        assert_eq!(read, vec![one], "the same pasteboard, read from another thread");
    }

    /// A pasteboard holding something that is not a file is not a failure — it is a paste this
    /// folder has nothing to take from.
    #[test]
    fn a_pasteboard_holding_no_files_hands_back_none() {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;

        let pasteboard = NSPasteboard::pasteboardWithUniqueName();
        pasteboard.clearContents();
        // SAFETY: the flavour's name is a constant AppKit defines and this only reads it.
        let flavour = unsafe { NSPasteboardTypeString };
        pasteboard.setString_forType(&NSString::from_str("just some words"), flavour);

        assert_eq!(take_from(&pasteboard), Vec::<PathBuf>::new());
    }
}
