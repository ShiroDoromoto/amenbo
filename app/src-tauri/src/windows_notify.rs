//! Delivers Windows OS notifications through notify-rust ourselves, so that clicking a toast can
//! navigate to the inbox. The desktop path of the stock `tauri-plugin-notification` fires
//! notify-rust's `.show()` and forgets it — it exposes no click or action callback (the action
//! listener lives only in its `mobile.rs`). Symmetrically to macOS, where we drive
//! UNUserNotificationCenter ourselves to get the click response, Windows uses notify-rust directly:
//! WinRT's ToastActivated/Dismissed arrive via `NotificationHandle::wait_for_response`, and a click
//! on the toast body (`NotificationResponse::Default`) is wired to the frontend's
//! `notification-activated` subscription. A WinRT toast demands a sender AUMID: in an installed
//! build NSIS has already registered one under the app's identifier (`config.identifier`), so we use
//! it; in a dev build (launched out of `target\debug|release`) no AUMID is registered, so `app_id`
//! is left unset and notify-rust falls back to its default PowerShell AUMID.

#![cfg(target_os = "windows")]

use std::path::MAIN_SEPARATOR;

use notify_rust::{Notification, NotificationResponse};
use tauri::{Emitter, Manager};

/// Name of the event emitted to the frontend on a click; the webview (AppShell) opens the inbox on
/// it. The macOS path (`macos_notify`) emits the same name, so the frontend subscribes once.
const ACTIVATED_EVENT: &str = "notification-activated";

/// Shows an arrival toast and, if it is clicked, raises the window and asks the frontend to
/// navigate to the inbox. Called from `notify_os`. `wait_for_response` blocks on `recv()` until
/// WinRT answers, so it waits on a thread of its own and never holds up the fire-and-forget send.
/// A failure to show (an unregistered AUMID, say) is not fatal and is only logged. `sound_name`
/// names WinRT's default toast sound explicitly because, left unset, notify-rust sends
/// `<audio silent="true"/>` and the toast is silent.
pub fn send(app: &tauri::AppHandle, title: String, body: String) {
    let mut notification = Notification::new();
    notification.summary(&title).body(&body).sound_name("Default");

    // Only an installed build has an AUMID under our identifier; dev falls back to the default one.
    if is_installed_app() {
        notification.app_id(&app.config().identifier);
    }

    let app = app.clone();
    std::thread::spawn(move || match notification.show() {
        Ok(handle) => {
            // A failed recv (the toast died before it answered, say) is not fatal — it did display.
            let _ = handle.wait_for_response(|response: &NotificationResponse| match response {
                NotificationResponse::Default | NotificationResponse::Action(_) => on_activated(&app),
                // Dismissed, expired or replied to (Windows never sends this): no navigation.
                NotificationResponse::Closed(_) | NotificationResponse::Reply(_) => {}
            });
        }
        Err(e) => log::warn!("failed to show windows toast: {e}"),
    });
}

/// What a toast click does: raise the window and ask the frontend to navigate to the inbox — the
/// same as `macos_notify`. An arrival notification aggregates a count (`notifyArrival(n)`), so no
/// single task can be identified and the inbox is the right destination.
fn on_activated(app: &tauri::AppHandle) {
    // The OS activates the app itself through the AUMID, but unminimizing and raising is explicit.
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    // Opening the inbox is the frontend's navigation call to make, so ask it with an event.
    if let Err(e) = app.emit(ACTIVATED_EVENT, ()) {
        log::warn!("failed to emit {ACTIVATED_EVENT}: {e}");
    }
}

/// Whether this is an installed build — i.e. whether an AUMID is registered. Excludes a dev build,
/// whose exe sits in a directory ending in `…\target\debug` or `…\target\release`; the plugin's own
/// desktop path decides it the same way.
fn is_installed_app() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(dir) = exe.parent().map(|p| p.display().to_string()) else {
        return false;
    };
    let sep = MAIN_SEPARATOR;
    !(dir.ends_with(&format!("{sep}target{sep}debug")) || dir.ends_with(&format!("{sep}target{sep}release")))
}
