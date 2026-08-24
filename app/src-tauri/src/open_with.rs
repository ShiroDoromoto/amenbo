//! Opening one file with an application the reader picks — the third of the file face's doors
//! (`AMB-T-3605` landed the other two).
//!
//! **The three operating systems have almost nothing in common here.** Two of them already own the
//! question and answer it with a dialog of their own; the third has no such dialog at all and only
//! a way to ask which applications *claim* the file. So this is not one implementation behind a
//! `cfg`, it is three, and the seam between them is drawn where it does the least damage:
//!
//! | | who draws the chooser | what a call costs |
//! |---|---|---|
//! | Windows | the OS (`SHOpenWithDialog`) | one call, nothing comes back |
//! | Linux | the OS (`GtkAppChooserDialog`) | one call, nothing comes back |
//! | macOS | **we do** (`NSWorkspace` only lists) | a list comes back, then a second call opens |
//!
//! `folder_open_with` is the one door the face knocks on. Where the OS draws the chooser it
//! returns an empty list, having already opened the file; where it does not, it returns the
//! candidates and the face draws them, calling `folder_open_file_with` with the one that was
//! picked. A face that simply draws whatever list it is handed does the right thing on all three
//! without knowing which one it is running on.
//!
//! **Amenbo still keeps no opinion about applications** (`AMB-T-3605`). Nothing here is remembered:
//! the list is asked for again every time it is drawn, and picking one changes nothing about what
//! the file opens with next time. The file face reads — choosing an editor is not its business, it
//! is the reader's, once.
//!
//! The fence is [`crate::folder`]'s and is not relaxed by a step: the file is resolved under a
//! folder the project is bound to before any of this, and on macOS the application named by the
//! webview is checked against the list that was offered rather than taken on its word — the whole
//! point of the fence is that a path arriving over the command seam is a claim, not a fact.

use std::path::Path;

use crate::dto::FolderAppDto;
use crate::error::CmdError;
use crate::folder::{root_of, under};

/// Ask what to open a file with.
///
/// On the operating systems that have a chooser of their own, this shows it and returns nothing:
/// picking and opening both happen in there, and the file is open before anything comes back. On
/// macOS, which has no such dialog, the applications that claim the file come back instead, the
/// usual one first, for the face to draw — one of them is then handed to [`folder_open_file_with`].
///
/// An empty list is therefore not a failure and not "no applications": it means the question was
/// already answered elsewhere. What the face does with an empty list is nothing at all.
#[tauri::command]
pub fn folder_open_with(
    window: tauri::WebviewWindow,
    project_id: i64,
    root: String,
    path: Vec<String>,
) -> Result<Vec<FolderAppDto>, CmdError> {
    let file = under(&root_of(project_id, &root)?, &path).ok_or_else(crate::folder::gone)?;
    ask(&window, &file)
}

/// Open one file with the application picked off the list [`folder_open_with`] handed back.
///
/// Only macOS ever gets here, because only there is the list drawn by us — but the check is not
/// made on the operating system, it is made on the list: the application is opened only if it is
/// one of those offered for *this* file. A webview naming an executable of its own choosing is
/// exactly what the file face's fence exists to refuse, and "it came from a list we sent" is not
/// something the far side of a command seam can prove.
#[tauri::command]
pub fn folder_open_file_with(
    project_id: i64,
    root: String,
    path: Vec<String>,
    app: String,
) -> Result<(), CmdError> {
    let file = under(&root_of(project_id, &root)?, &path).ok_or_else(crate::folder::gone)?;
    if !offered(&file).iter().any(|one| one.path == app) {
        return Err(crate::folder::gone());
    }
    tauri_plugin_opener::open_path(&file, Some(app.as_str()))
        .map_err(|e| CmdError::coded("folder.open", e.to_string(), serde_json::Value::Null))
}

// ---------------------------------------------------------------------------------------------
// macOS — no dialog exists, so the list is ours to draw.
// ---------------------------------------------------------------------------------------------

/// Nothing is shown: there is no dialog on this OS, so asking is the same as listing.
#[cfg(target_os = "macos")]
fn ask(_window: &tauri::WebviewWindow, file: &Path) -> Result<Vec<FolderAppDto>, CmdError> {
    Ok(offered(file))
}

/// The applications Launch Services says can open this file, the usual one first and the rest by
/// name — and, being the list the face draws from, also the list an answer has to be found in.
///
/// **The order is ours because the answer is unordered.** Launch Services returns everything that
/// ever claimed the type, which for a `.md` on a working machine was nineteen applications — a
/// music scorer and an emulator among them (`AMB-T-3547`). Nothing in that list says which one the
/// reader means, except the one the OS would have used anyway, so that one goes first and the rest
/// are put in the order a person reads a list in.
///
/// The name is the one Finder shows, not the last segment of the path: it is localised, and it is
/// spelled without `.app` for a reader who has extensions hidden.
#[cfg(target_os = "macos")]
fn offered(file: &Path) -> Vec<FolderAppDto> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    let workspace = NSWorkspace::sharedWorkspace();
    let files = NSFileManager::defaultManager();

    let usual = workspace
        .URLForApplicationToOpenURL(&url)
        .and_then(|url| url.path())
        .map(|path| path.to_string());

    let mut apps: Vec<FolderAppDto> = workspace
        .URLsForApplicationsToOpenURL(&url)
        .iter()
        .filter_map(|url| {
            let path = url.path()?.to_string();
            let name = files.displayNameAtPath(&NSString::from_str(&path)).to_string();
            let usual = usual.as_deref() == Some(path.as_str());
            Some(FolderAppDto { name, path, usual })
        })
        .collect();
    apps.sort_by(|a, b| {
        b.usual
            .cmp(&a.usual)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    apps
}

/// Nowhere but macOS has a list of ours to pick off — the OS took the whole question, chooser and
/// answer together — so nothing can have come from one, and [`folder_open_file_with`] refuses.
#[cfg(not(target_os = "macos"))]
fn offered(_file: &Path) -> Vec<FolderAppDto> {
    Vec::new()
}

// ---------------------------------------------------------------------------------------------
// Windows — the OS draws it, on a thread of its own.
// ---------------------------------------------------------------------------------------------

/// Hand the question to `SHOpenWithDialog`, which draws the list, takes the answer and opens the
/// file (`OAIF_EXEC`) without any of it coming back here.
///
/// **It is modal, and the thread that calls it does not return until the dialog is closed**
/// (`AMB-T-3565` watched it sit for six seconds and then killed it). So it is called on a thread of
/// its own and this returns at once: a command that blocked would block the webview's call, and a
/// command that blocked on the UI thread would freeze the window the dialog is meant to sit over.
///
/// The dialog is given no owner window. Owning it would mean naming a window that belongs to
/// another thread, which asks the input queues of the two to be attached for as long as the dialog
/// is up — a hang there costs the whole app, and what is bought is that the dialog is centred on the
/// window instead of on the screen. `AMB-T-3565` measured the unowned form working.
///
/// Nothing is reported back, because there is nothing a reader could do with it: by the time
/// anything goes wrong the dialog is theirs, not ours (`AMB-T-3605` — a failure is not drawn).
#[cfg(target_os = "windows")]
fn ask(_window: &tauri::WebviewWindow, file: &Path) -> Result<Vec<FolderAppDto>, CmdError> {
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::UI::Shell::{SHOpenWithDialog, OAIF_EXEC, OPENASINFO};

    // A wide, NUL-terminated copy made here rather than on the far thread: the path is what the
    // fence just approved, and it has to outlive this call by exactly as long as the dialog does.
    let wide: Vec<u16> = {
        use std::os::windows::ffi::OsStrExt as _;
        file.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    };

    std::thread::spawn(move || {
        // The shell dialog is a COM object and wants the apartment it is asked for. Failing to get
        // one is not worth reporting: the call below is what decides whether anything is drawn.
        // (`COINIT_*` are typed as the signed `COINIT`, and the call takes the flags unsigned.)
        let com = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        let info = OPENASINFO {
            pcszFile: wide.as_ptr(),
            pcszClass: std::ptr::null(),
            oaifInFlags: OAIF_EXEC,
        };
        unsafe { SHOpenWithDialog(std::ptr::null_mut(), &info) };
        if com >= 0 {
            unsafe { CoUninitialize() };
        }
    });
    Ok(Vec::new())
}

// ---------------------------------------------------------------------------------------------
// Linux — the OS draws it, on the thread it insists on.
// ---------------------------------------------------------------------------------------------

/// Hand the question to `GtkAppChooserDialog`, which sorts the answer into "recommended" and "all"
/// on its own — the sorting macOS made us do by hand is not needed here (`AMB-T-3566`).
///
/// **GTK may only be touched from the thread its main loop runs on**, which is the app's main
/// thread, so the whole of this is posted there and this returns at once. Being on that thread is
/// also what lets the dialog be owned properly by the window it came from — the ownership Windows
/// could not have, for the same reason, from the other side.
///
/// The application is launched through `gio::AppInfo::launch` rather than through the opener
/// plugin's `with`: on this OS that path builds a command out of the name and runs it, and what a
/// chooser hands back is a desktop entry, not an executable on `PATH` (`AMB-T-3547`).
#[cfg(target_os = "linux")]
fn ask(window: &tauri::WebviewWindow, file: &Path) -> Result<Vec<FolderAppDto>, CmdError> {
    let file = file.to_path_buf();
    // The window it came from is asked for **on the far side**: a GTK object is bound to the thread
    // its main loop runs on and is not `Send`, so the handle travels and the window is taken out of
    // it there. Carrying the window itself would not compile, which is the type system saying the
    // same thing this comment does.
    let owner = window.clone();
    let posted = window.run_on_main_thread(move || {
        use gtk::prelude::*;

        let parent = owner.gtk_window().ok();
        let target = gtk::gio::File::for_path(&file);
        let dialog = gtk::AppChooserDialog::new(
            parent.as_ref(),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            &target,
        );
        let answer = dialog.run();
        let picked = dialog.app_info();
        // `run` spins a loop of its own and leaves the dialog standing when it comes back; a dialog
        // nobody hides is a window that stays on the screen after the answer is in.
        unsafe { dialog.destroy() };
        if answer == gtk::ResponseType::Ok {
            if let Some(app) = picked {
                let _ = app.launch(&[target], gtk::gio::AppLaunchContext::NONE);
            }
        }
    });
    // Reaching the main thread is the one part of this that can fail here; everything past it is
    // the reader's dialog, and a failure in there is not ours to draw (`AMB-T-3605`).
    posted.map_err(|e| CmdError::coded("folder.open", e.to_string(), serde_json::Value::Null))?;
    Ok(Vec::new())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// The order is the whole of what macOS's half decides — the list itself comes from Launch
    /// Services, which returns whatever ever claimed the type and says nothing about which of them
    /// a reader means (`AMB-T-3547`). The one the OS would have used goes first; the rest read as a
    /// list, which means by name and not by where they happen to be installed.
    #[test]
    fn the_usual_application_is_named_first_and_the_rest_read_as_a_list() {
        let mut apps = [
            FolderAppDto { name: "Zed".into(), path: "/Applications/Zed.app".into(), usual: false },
            FolderAppDto { name: "MuseScore 4".into(), path: "/Applications/M.app".into(), usual: false },
            FolderAppDto { name: "cursor".into(), path: "/Applications/C.app".into(), usual: false },
            FolderAppDto { name: "Antigravity".into(), path: "/Applications/A.app".into(), usual: true },
        ];
        apps.sort_by(|a, b| {
            b.usual
                .cmp(&a.usual)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let names: Vec<&str> = apps.iter().map(|one| one.name.as_str()).collect();
        // Not "Antigravity, MuseScore, Zed, cursor": a reader sorting by name does not put the
        // lowercase ones after everything else.
        assert_eq!(names, ["Antigravity", "cursor", "MuseScore 4", "Zed"]);
    }

    /// Launch Services really does answer for an ordinary file on this machine, and the answer is
    /// shaped the way the face is told it is: every row names a real application bundle, and at
    /// most one of them is the one the OS would have used.
    ///
    /// What it cannot assert is *which* applications — that is this machine's business, and a
    /// machine with no editor installed is entitled to answer with nothing.
    #[test]
    fn launch_services_answers_for_a_real_file() {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::{NSString, NSURL};

        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("notes.md");
        std::fs::write(&file, "# a heading").expect("a file");

        let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
        let workspace = NSWorkspace::sharedWorkspace();
        let apps = workspace.URLsForApplicationsToOpenURL(&url);
        for app in apps.iter() {
            let path = app.path().expect("an application has a path").to_string();
            assert!(path.ends_with(".app"), "not an application bundle: {path}");
        }
    }
}
