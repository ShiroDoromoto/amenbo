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
///
/// The identifier is passed in rather than left to the plugin's own default (which reads the same
/// one off the config) because Linux needs it in a shape the identifier does not have to be in —
/// see `dbus_name`.
pub fn init(identifier: &str) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_single_instance::Builder::new()
        .callback(|app, _args, _cwd| raise(app))
        .dbus_id(dbus_name(identifier))
        .build()
}

/// The bundle identifier in a shape D-Bus will take as a well-known name, which is what Linux claims
/// the instance with (the plugin appends `.SingleInstance` to it). macOS and Windows claim theirs
/// some other way and never read this, so it is composed on every platform and consumed on one.
///
/// **A name's elements may not begin with a digit**, and Amenbo's do whenever a theme's development
/// preview is built: `AMB-D-732` splits the identifier by the theme's task number, so
/// `work.amenbo.app.dev.3497` is an ordinary identifier and an invalid bus name at once, and the
/// claim panicked on it before a window was ever drawn (`AMB-T-3503`). An offending element takes a
/// `_` in front, which the rule allows and which leaves every element that already complies —
/// production's `work.amenbo.app` and the shared dev build's `work.amenbo.app.dev` among them —
/// exactly as it was, so no build's claim moves to a name other than the one it already held.
fn dbus_name(identifier: &str) -> String {
    identifier
        .split('.')
        .map(|element| match element.starts_with(|c: char| c.is_ascii_digit()) {
            true => format!("_{element}"),
            false => element.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(".")
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

#[cfg(test)]
mod tests {
    use super::dbus_name;

    /// The names the shipped builds claim under, held to the letter: a fix for the theme previews
    /// that moved either of these would move where every running production or shared-dev app looks
    /// for its own claim, and two of them would stand up on one store again.
    #[test]
    fn leaves_a_valid_identifier_untouched() {
        assert_eq!(dbus_name("work.amenbo.app"), "work.amenbo.app");
        assert_eq!(dbus_name("work.amenbo.app.dev"), "work.amenbo.app.dev");
    }

    /// A theme's preview, which is the whole reason this exists (`AMB-T-3503`).
    #[test]
    fn prefixes_an_element_that_begins_with_a_digit() {
        assert_eq!(dbus_name("work.amenbo.app.dev.3497"), "work.amenbo.app.dev._3497");
    }

    /// A digit inside an element is allowed by the rule, so it is left alone — the transformation is
    /// the leading-digit one and nothing besides.
    #[test]
    fn leaves_a_digit_that_is_not_leading() {
        assert_eq!(dbus_name("work.amenbo.app2.dev"), "work.amenbo.app2.dev");
    }
}
