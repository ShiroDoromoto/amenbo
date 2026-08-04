//! One process per store: a second launch raises the window that is already open and ends, rather
//! than standing a second app up beside the first.
//!
//! Two of them come up without anyone asking for it. Start at login registers a launch with the OS
//! (`AMB-D-541`), and macOS reopens at the next login whatever was still open at logout — turn the
//! setting on, leave the window open, and the login does both. What is running afterwards is two
//! processes on one store: two watchers, two sets of arrival notifications, and a startup migration
//! that two of them reach at once.
//!
//! What must not be doubled is the **store**, and the key the plugin offers is the bundle
//! identifier. For an ordinary launch the two say the same thing, because a build opens exactly one
//! store — a development build carries its own identifier and its own app-data, so it is guarded on
//! its own rather than against the production app. They part when `AMENBO_HOME` names a store: one
//! identifier then stands for as many stores as are named, and the harness that shoots the shipping
//! GUI is launched that way on purpose — at a scratch store so it touches nothing of the user's
//! (`AMB-D-536`), and expecting to run whatever the user already has open (`AMB-D-539`). Answering
//! for a named store out of the identifier would end that launch inside the user's window, so a run
//! that names its own store is left unguarded.

/// Whether this process claims the guard — every run but one that named its own store.
pub fn guards_this_run() -> bool {
    amenbo_core::env::home().is_none()
}

/// The plugin that holds the claim, ready to hand to `tauri::Builder::plugin(…)`.
///
/// It belongs on the builder rather than inside `setup`, because builder plugins are initialized
/// while the app is still being built — before the window named in `tauri.conf.json` is created. A
/// launch that finds the claim taken therefore ends without ever drawing one.
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_single_instance::init(|app, _args, _cwd| raise(app))
}

/// What the launch that was turned away leaves behind: the window of the process that holds the
/// claim, in front. Without this the second launch would look like nothing happening at all —
/// clicking the app while its window sits minimized behind other work is exactly the case.
///
/// Restoring from minimized and coming to the front are spelled out separately, the same way a
/// notification click does it (`crate::macos_notify`).
fn raise(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}
