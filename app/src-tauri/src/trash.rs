//! Taking a row out of a folder — into the machine's own bin, and back out of it (`AMB-D-777`).
//!
//! **There is no way from here to delete anything.** What the panel offers is the bin and nothing
//! else, so a press that was a slip costs a trip to the bin rather than the file. That is a promise
//! rather than a preference, and it is why the Windows half asks a question before it acts: on a
//! drive with no bin the shell there deletes and reports success (`AMB-T-3749` measured it on a
//! substituted drive, a mapped network drive and a UNC path), which would break the promise without
//! anything on screen saying so.
//!
//! **Putting it back is Amenbo's own.** The way each OS is asked to bin a row here is the one that
//! says where it put it, and holding that answer is the whole of the undo: the pair "where it was"
//! and "where it is now" is enough on all three (`AMB-T-3747`). The alternative on macOS — asking
//! Finder — costs a permission dialog that can be refused for good, and 650 times the time.
//!
//! **What is remembered lasts as long as the app is up.** The stack lives in [`crate::trash::Bin`]
//! and nothing writes it down: the pair stays good until somebody empties the bin, so it could be
//! kept — but a row deleted last week coming back under an undo is a surprise, not a convenience.
//!
//! Three things differ between the operating systems, and none of them can be papered over:
//!
//! 1. **What identifies a binned row.** macOS and Windows answer with a path inside the bin and are
//!    put back by moving that path; the freedesktop bin is a pair of files, one of which records
//!    where the row came from, so a row there is identified by the record and put back by the crate
//!    that owns the format.
//! 2. **Whether the machine refuses.** macOS says so in a sentence already written for a reader, in
//!    their language, naming the volume; Linux says so as an error naming the directory it could not
//!    make; Windows says nothing at all, which is what the question before the act is for.
//! 3. **Whether a name in the way can be refused without a window.** macOS has a rename that will
//!    not replace, and it answers `ENOTSUP` over SMB (`AMB-T-3749`); the other two have to ask first
//!    and act second. So the asking is written once, for all three, and the atomic form is taken
//!    where it is offered.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::dto::{FolderRestoredDto, FolderStoppedDto, FolderTrashedDto};
use crate::error::CmdError;
use crate::folder::{gone, rooted, under};

/// One row that went to the bin: where it was, and what its machine needs to put it back.
struct Trashed {
    /// The path it came from, which is the path it goes back to. It is also where its name is read
    /// from, so a message about it names what the reader named.
    from: PathBuf,
    /// What the bin gave back, in whatever shape that machine's bin hands one out.
    held: Held,
}

impl Trashed {
    /// The name a message about this row uses — the last segment of where it came from.
    fn name(&self) -> String {
        self.from
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Where a binned row is on macOS and Windows: a path inside the bin, which is what those two answer
/// with and what putting it back moves.
///
/// **The path is never rebuilt from the name.** macOS renames a row whose name is already in the bin
/// to a spelling of its own — `twice.txt 22-09-28-656.txt`, which is not the spelling Finder shows
/// for the same thing — and Windows names every one of them `$R` and six random characters
/// (`AMB-T-3747`). What comes back is the only path that is true.
#[cfg(not(target_os = "linux"))]
type Held = PathBuf;

/// The same, on the operating system whose bin is a format rather than a folder: the record the
/// freedesktop bin keeps beside the row, which names where the row came from and is deleted with it
/// when the row is put back.
#[cfg(target_os = "linux")]
type Held = trash::TrashItem;

/// What this run of the app has put in the bin, newest last.
///
/// One press is one entry however many rows it took, so undoing puts back exactly what the press
/// took away. Nothing here is written down: it is the app's memory of what it did, and it goes when
/// the app does (`AMB-D-777`).
///
/// **One stack for the app, not one per window or per project.** Two windows of Amenbo are two
/// faces onto the same machine, and the bin they send a row to is the machine's; a reader who binned
/// something and then walked to the other window would find undo saying there was nothing to undo.
#[derive(Default)]
pub struct Bin(Mutex<Vec<Vec<Trashed>>>);

impl Bin {
    /// Remember one press. An empty one is not remembered — an undo that put nothing back would
    /// still have consumed a press of the undo.
    fn keep(&self, went: Vec<Trashed>) {
        if went.is_empty() {
            return;
        }
        if let Ok(mut stack) = self.0.lock() {
            stack.push(went);
        }
    }

    /// Take the newest press back off, or nothing where none is left.
    fn take(&self) -> Option<Vec<Trashed>> {
        self.0.lock().ok()?.pop()
    }
}

/// Put rows of one of the project's folders into the machine's bin.
///
/// The rows go in the order they were given and the first refusal ends it: what came before is in
/// the bin, what comes after was never touched, and the answer says which is which — the same shape
/// a carry answers with (`crate::folder_write`), and for the same reason. A refusal cannot be the
/// whole answer when some of the list is already gone.
#[tauri::command]
pub fn folder_trash(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
    bin: tauri::State<'_, Bin>,
) -> Result<FolderTrashedDto, CmdError> {
    let (roots, base) = rooted(project_id, &root)?;

    let mut went = Vec::new();
    let mut names = Vec::new();
    for path in &paths {
        let name = path.last().ok_or_else(gone)?.clone();
        let (_, target) = under(&roots, base, path).ok_or_else(gone)?;
        match take(&target) {
            Ok(held) => {
                went.push(Trashed { from: target, held });
                names.push(name);
            }
            Err(why) => {
                bin.keep(went);
                return Ok(FolderTrashedDto {
                    gone: names,
                    stopped: Some(FolderStoppedDto { name, why }),
                });
            }
        }
    }

    bin.keep(went);
    Ok(FolderTrashedDto { gone: names, stopped: None })
}

/// Put back what the last press took away, newest first.
///
/// `None` is the answer when there is nothing left to undo, and it is an answer rather than a
/// refusal: a person pressing undo one more time than they deleted has done nothing wrong.
///
/// A press that stops part way leaves the rest of it on the stack, so pressing undo again carries on
/// where this one got to rather than skipping past what is still in the bin.
#[tauri::command]
pub fn folder_untrash(bin: tauri::State<'_, Bin>) -> Result<Option<FolderRestoredDto>, CmdError> {
    let Some(mut group) = bin.take() else {
        return Ok(None);
    };

    let mut back = Vec::new();
    while let Some(one) = group.pop() {
        if let Err(why) = put_back(&one) {
            let name = one.name();
            group.push(one);
            bin.keep(group);
            return Ok(Some(FolderRestoredDto {
                back,
                stopped: Some(FolderStoppedDto { name, why }),
            }));
        }
        back.push(one.name());
    }
    Ok(Some(FolderRestoredDto { back, stopped: None }))
}

/// Put one row back where it came from: the half every machine agrees on, then the half it does not.
fn put_back(one: &Trashed) -> Result<(), String> {
    returnable(&one.from)?;
    lift(one)
}

/// What has to be true before anything is moved, whatever moves it.
///
/// **A name that is in use is never written over.** The row being put back is not the only thing
/// that could be called that — something else has been made under the name since, or the same name
/// was taken by a different row — and replacing it would lose a file to an undo, which is the one
/// thing an undo must not do. Windows would do exactly that if left to itself, and so would a plain
/// rename on the other two (`AMB-T-3747`).
///
/// **The folder it came from is made again where it has gone.** The three machines disagree —
/// freedesktop's bin makes it, the other two refuse — and one answer is better than three: a person
/// who deleted a folder and then a file inside it presses undo twice, and the second press has
/// nowhere to put anything unless the first one made the folder.
fn returnable(from: &Path) -> Result<(), String> {
    if from.symlink_metadata().is_ok() {
        return Err(format!("{} is already there", named(from)));
    }
    if let Some(parent) = from.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The name to put in a sentence about this path.
fn named(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// macOS — the bin says where it put it, and a rename that will not replace puts it back.
// ---------------------------------------------------------------------------------------------

/// Ask the file manager to bin the row, and keep the URL it answers with.
///
/// The answer is the point. `trashItemAtURL:resultingItemURL:` is the only door to the bin on this
/// OS that gives one, and the crate that wraps it passes `None` where the answer would go — so it is
/// called directly, at no cost in dependencies or time (239µs against the crate's 233µs, measured in
/// `AMB-T-3747`). What comes back is always on the same volume as the row, which is what makes
/// putting it back a rename and never a copy.
///
/// A refusal is handed on in the words the OS wrote it in. They are already a sentence for a reader,
/// in the reader's language, and they name the volume that has no bin — which is more than a
/// sentence written here could say (`AMB-T-3749` read one over SMB).
#[cfg(target_os = "macos")]
fn take(path: &Path) -> Result<Held, String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let mut landed: Option<objc2::rc::Retained<NSURL>> = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut landed))
        .map_err(|e| e.localizedDescription().to_string())?;
    landed
        .and_then(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .ok_or_else(|| "the bin did not say where it put it".to_string())
}

/// Move it back, refusing a name in the way in the same call that would have taken it.
///
/// `renamex_np` with `RENAME_EXCL` is that call, and it is the only one of the three machines where
/// the question and the act are one — everywhere else there is a window between them. It is not
/// everywhere on this machine either: a network volume answers `ENOTSUP` (`AMB-T-3749`), and the
/// plain rename is what is left there. The check in [`returnable`] has already run, so the fallback
/// is the ordinary road with a window in it rather than no check at all.
#[cfg(target_os = "macos")]
fn lift(one: &Trashed) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt as _;

    if one.held.symlink_metadata().is_err() {
        return Err(format!("{} is no longer in the bin", one.name()));
    }
    let from = std::ffi::CString::new(one.held.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let to = std::ffi::CString::new(one.from.as_os_str().as_bytes()).map_err(|e| e.to_string())?;

    let done = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
    if done == 0 {
        return Ok(());
    }
    let refused = std::io::Error::last_os_error();
    if refused.raw_os_error() == Some(libc::ENOTSUP) {
        return std::fs::rename(&one.held, &one.from).map_err(|e| e.to_string());
    }
    Err(refused.to_string())
}

// ---------------------------------------------------------------------------------------------
// Linux — the bin is a format, and the crate that knows it owns both halves.
// ---------------------------------------------------------------------------------------------

/// Bin the row, then find the record the bin wrote for it.
///
/// The door in is `trash::delete`, which says nothing about where the row went; the record is read
/// back out of the bin afterwards. What identifies it is that it was not there a moment ago: a name
/// and a time cannot, because the bin holds a second `l6.txt` under a name of its own while calling
/// them both `l6.txt`, and the deletion time it records has a second's resolution (`AMB-T-3747`).
#[cfg(target_os = "linux")]
fn take(path: &Path) -> Result<Held, String> {
    let before = records(path);
    trash::delete(path).map_err(|e| e.to_string())?;
    records(path)
        .into_iter()
        .find(|one| !before.iter().any(|was| was.id == one.id))
        .ok_or_else(|| "the bin kept no record of it".to_string())
}

/// The bin's records for one path, which is every row it holds that came from there.
///
/// A bin that cannot be read is an empty one here rather than a refusal: it is read to tell one
/// record from another, and the answer that matters — whether the row went in — is the deletion's.
#[cfg(target_os = "linux")]
fn records(path: &Path) -> Vec<trash::TrashItem> {
    let Some(name) = path.file_name() else {
        return Vec::new();
    };
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    trash::os_limited::list()
        .unwrap_or_default()
        .into_iter()
        .filter(|one| one.name == name && one.original_parent == parent)
        .collect()
}

/// Put it back the way the format says, which takes the record away with it.
///
/// Moving the row out by hand would leave the record behind, and a bin with a record pointing at
/// nothing is a bin that lists a row nobody can restore. One record at a time: two rows the bin
/// calls by the same name are refused as a pair (`AMB-T-3747`).
#[cfg(target_os = "linux")]
fn lift(one: &Trashed) -> Result<(), String> {
    trash::os_limited::restore_all([one.held.clone()]).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// Windows — ask whether the drive has a bin at all, then watch where the row lands.
// ---------------------------------------------------------------------------------------------

/// Bin the row, having first made sure the drive it is on has a bin.
///
/// **The question is not a formality.** On a substituted drive, a mapped network drive and a UNC
/// path, the shell deletes the row outright and reports success: the flag that asks for the bin is
/// dropped, no warning is drawn under `FOF_NO_UI`, and nothing afterwards can tell that it happened
/// (`AMB-T-3749`). `SHQueryRecycleBinW` answers for the volume rather than for the path, so the
/// row's own path is what it is asked about, and every answer but `S_OK` is read as "no bin here" —
/// a drive that is not there and a drive with no bin are both drives this must not delete on. It
/// costs about 9ms on a full bin because it counts what is in it, which is why it is asked once,
/// here, rather than anywhere a screen could poll it.
#[cfg(target_os = "windows")]
fn take(path: &Path) -> Result<Held, String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        i64Size: 0,
        i64NumItems: 0,
    };
    let has_bin = unsafe { SHQueryRecycleBinW(&HSTRING::from(path.as_os_str()), &mut info) };
    if has_bin.is_err() {
        return Err(format!("{} is on a drive with no recycle bin", named(path)));
    }

    // The shell's file operation is a COM object, and so is the sink it reports through, so the
    // thread has to be in an apartment before either exists. One that is already in one says so,
    // and the pairing is owed either way.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let landed = recycle(path);
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    landed
}

/// The operation itself, once the thread has its apartment.
///
/// The sink is the only door to where the row landed: the operation reports whether it worked and
/// nothing more, and the name the shell gives a binned row is `$R` and six characters it drew at
/// random (`AMB-T-3747`).
#[cfg(target_os = "windows")]
fn recycle(path: &Path) -> Result<Held, String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    use windows::Win32::UI::Shell::{
        FileOperation, IFileOperation, IFileOperationProgressSink, IShellItem,
        SHCreateItemFromParsingName, FOF_NOCONFIRMATION, FOF_NO_UI, FOFX_RECYCLEONDELETE,
    };

    let landed: Landed = std::sync::Arc::new(Mutex::new(None));
    unsafe {
        let op: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message())?;
        op.SetOperationFlags(FOF_NO_UI | FOF_NOCONFIRMATION | FOFX_RECYCLEONDELETE)
            .map_err(|e| e.message())?;

        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None)
            .map_err(|e| e.message())?;
        let sink: IFileOperationProgressSink = Watch(landed.clone()).into();
        op.DeleteItem(&item, &sink).map_err(|e| e.message())?;
        op.PerformOperations().map_err(|e| e.message())?;
    }

    let held = landed.lock().ok().and_then(|held| held.clone());
    held.ok_or_else(|| format!("{} was not put in the recycle bin", named(path)))
}

/// Where the sink writes what it was told, for the call that made it to read.
#[cfg(target_os = "windows")]
type Landed = std::sync::Arc<Mutex<Option<PathBuf>>>;

/// What the shell says about each row as it goes. One of the calls carries the answer this is here
/// for; the rest are the price of the interface and say nothing.
#[cfg(target_os = "windows")]
#[windows_core::implement(windows::Win32::UI::Shell::IFileOperationProgressSink)]
struct Watch(Landed);

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
impl windows::Win32::UI::Shell::IFileOperationProgressSink_Impl for Watch_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _: windows::core::HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
        _: windows::core::HRESULT,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
        _: windows::core::HRESULT,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreCopyItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
        _: windows::core::HRESULT,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreDeleteItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    /// The one call this sink exists for. What is handed in is the row's path inside the bin, and it
    /// is absent exactly when the shell deleted the row instead of binning it.
    fn PostDeleteItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: windows::core::HRESULT,
        newly: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::SIGDN_FILESYSPATH;

        let Some(item) = newly.as_ref() else {
            return Ok(());
        };
        // The shell allocates the string and this side owns it from here, so it is read once and
        // given back — the sink is called for every row of an operation.
        let raw = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
        let path = unsafe { raw.to_string() }.map(PathBuf::from);
        unsafe { CoTaskMemFree(Some(raw.as_ptr() as *const std::ffi::c_void)) };
        if let (Ok(path), Ok(mut held)) = (path, self.0.lock()) {
            *held = Some(path);
        }
        Ok(())
    }
    fn PreNewItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _: u32,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
        _: &windows::core::PCWSTR,
        _: &windows::core::PCWSTR,
        _: u32,
        _: windows::core::HRESULT,
        _: windows::core::Ref<'_, windows::Win32::UI::Shell::IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Move it back out of the bin.
///
/// The name in the way was refused before this ran, which is the whole of the care taken here:
/// `rename` on this OS replaces what it finds without a word (`crate::folder_write`), and a window
/// between the question and the act is what is left once no call offers to do both (`AMB-T-3747`).
#[cfg(target_os = "windows")]
fn lift(one: &Trashed) -> Result<(), String> {
    if one.held.symlink_metadata().is_err() {
        return Err(format!("{} is no longer in the recycle bin", one.name()));
    }
    std::fs::rename(&one.held, &one.from).map_err(|e| e.to_string())
}

/// Only what a caller could ask for, and the halves that do not need a real bin to answer. The bin
/// itself is asked in the round trip at the end, which puts back what it took so the machine is left
/// as it was found.
#[cfg(test)]
mod tests {
    use super::*;

    /// A press is one entry however many rows it took, and the newest press is the one that comes
    /// back — undo goes backwards through what was done, not forwards through what is in the bin.
    #[test]
    fn the_newest_press_is_the_one_that_comes_back() {
        let bin = Bin::default();
        bin.keep(vec![Trashed {
            from: PathBuf::from("/one/first.md"),
            held: held_for("/bin/first.md"),
        }]);
        bin.keep(vec![
            Trashed { from: PathBuf::from("/one/second.md"), held: held_for("/bin/second.md") },
            Trashed { from: PathBuf::from("/one/third.md"), held: held_for("/bin/third.md") },
        ]);

        let newest = bin.take().expect("the second press");
        assert_eq!(newest.len(), 2, "one press is one entry, whatever it took");
        assert_eq!(newest[0].name(), "second.md");
        assert_eq!(bin.take().expect("the first press")[0].name(), "first.md");
        assert!(bin.take().is_none(), "and then there is nothing left to undo");
    }

    /// An empty press is not remembered: undoing it would put nothing back while spending the press
    /// that would have undone the deletion before it.
    #[test]
    fn a_press_that_took_nothing_is_not_remembered() {
        let bin = Bin::default();
        bin.keep(Vec::new());
        assert!(bin.take().is_none());
    }

    /// A name that is in use is never written over, whatever is under it now.
    #[test]
    fn a_name_taken_since_stops_the_return() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("note.md");
        std::fs::write(&from, b"written since").expect("a file");

        let refused = returnable(&from).expect_err("the name is in use");
        assert!(refused.contains("note.md"), "the refusal names it: {refused}");
        assert_eq!(std::fs::read(&from).expect("the file"), b"written since");
    }

    /// The folder it came from is made again where it has gone, so undoing a folder and then a file
    /// inside it puts both back.
    #[test]
    fn the_folder_it_came_from_is_made_again() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("deep/deeper/note.md");

        returnable(&from).expect("nothing is in the way");
        assert!(dir.path().join("deep/deeper").is_dir(), "the folder is there to put it back into");
    }

    /// A row the bin no longer holds cannot be put back, and says so rather than reporting a move
    /// that never happened. The freedesktop bin answers this for itself, in the crate that owns the
    /// format.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn a_row_the_bin_no_longer_holds_says_so() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let one = Trashed {
            from: dir.path().join("note.md"),
            held: dir.path().join("bin/note.md"),
        };

        let refused = put_back(&one).expect_err("nothing is in the bin");
        assert!(refused.contains("note.md"), "the refusal names it: {refused}");
    }

    /// Moving it back is a move: the bin no longer holds it, and the folder does. The bin here is
    /// another folder, which is what the two machines that answer with a path treat it as.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn what_is_put_back_leaves_the_bin() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).expect("a folder");
        let held = bin.join("kept-under-another-name");
        std::fs::write(&held, b"mine").expect("a file");

        let one = Trashed { from: dir.path().join("gone/note.md"), held };
        put_back(&one).expect("the return");
        assert_eq!(std::fs::read(&one.from).expect("the file"), b"mine");
        assert!(!one.held.exists(), "and the bin does not hold it twice");
    }

    /// The whole of it, against the machine's own bin: a file goes in, comes back, and is where it
    /// was with what it held. It leaves nothing behind — what it put in the bin it took out again.
    #[test]
    fn a_file_goes_to_the_bin_and_comes_back() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let from = dir.path().join("round-trip.md");
        std::fs::write(&from, b"mine").expect("a file");

        let held = take(&from).expect("the machine's bin");
        assert!(!from.exists(), "it is not in the folder any more");

        let one = Trashed { from: from.clone(), held };
        put_back(&one).expect("the return");
        assert_eq!(std::fs::read(&from).expect("the file"), b"mine");
    }

    /// A stand-in for what the bin hands back, in the shape this machine's bin hands one out in.
    #[cfg(not(target_os = "linux"))]
    fn held_for(path: &str) -> Held {
        PathBuf::from(path)
    }

    /// The same, where a record rather than a path is what identifies a binned row. Only the fields
    /// the stack itself reads are filled: nothing in these tests puts one back.
    #[cfg(target_os = "linux")]
    fn held_for(path: &str) -> Held {
        trash::TrashItem {
            id: std::ffi::OsString::from(path),
            name: std::ffi::OsString::from(
                Path::new(path).file_name().unwrap_or_default(),
            ),
            original_parent: Path::new(path).parent().unwrap_or(Path::new("/")).to_path_buf(),
            time_deleted: 0,
        }
    }
}
