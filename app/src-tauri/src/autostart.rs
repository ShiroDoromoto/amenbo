//! Start at login (`AMB-D-541`): the registration the OS reads when the user signs in, and the switch
//! that writes or removes it.
//!
//! What is registered is a plain launch of this executable — the window comes up the way it does when
//! the user opens the app themselves. There is no tray-resident shape behind this, and none is assumed:
//! the reason to be up early is that the inbox items which arrived while the app was closed are
//! collected on the next open (`AMB-D-310`), so opening more often is noticing sooner.
//!
//! Two states have to agree, and only one of them is ours: `config.autostart` is what the user asked
//! for, and the OS registration is what actually happens. This module writes the OS half and the
//! command that calls it writes the config half, in that order — a registration that could not be
//! written leaves the config saying "off", never the reverse. Nothing here reads the two back to
//! compare them; reconciling a registration the user removed, or one whose path went stale, is the
//! startup pass (`AMB-D-546`), and this module is where it belongs when it arrives.
//!
//! A development build has none of it (`AMB-D-547`): the plugin is not registered, and this refuses.

use crate::error::CmdError;

/// The plugin that owns the per-user registration, ready to hand to `app.handle().plugin(…)`. On
/// macOS it is asked for a LaunchAgent plist rather than a Login Item, so the registration is a file
/// under the user's own `~/Library/LaunchAgents` — visible, removable, and needing no scripting of
/// another application. No extra arguments are passed: a launch at login is the ordinary launch.
#[cfg(desktop)]
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None)
}

/// Write (`enabled`) or remove (`!enabled`) this user's login registration.
///
/// The webview never reaches the plugin itself — that is why no `autostart:*` permission is granted in
/// `capabilities/default.json`. A caller who could enable the registration without also writing
/// `config.autostart` would leave the switch and the OS disagreeing, so the only door in is
/// [`crate::commands::config_set_autostart`], which writes both.
#[cfg(desktop)]
pub fn set(app: &tauri::AppHandle, enabled: bool) -> Result<(), CmdError> {
    use tauri_plugin_autostart::ManagerExt;
    if amenbo_core::config::Paths::is_dev_channel() {
        // The plugin is not registered on this channel, so `autolaunch()` would have nothing to ask.
        // Refusing here is the half that holds whoever the caller is, the way the front end not
        // drawing the switch is the half that holds for the user (`AMB-D-547`).
        return Err("a development build does not register a login item".into());
    }
    let launcher = app.autolaunch();
    let written = if enabled { launcher.enable() } else { launcher.disable() };
    written.map_err(|e| CmdError::from(format!("could not write the login registration: {e}")))
}

/// The same door on a target that has no login of its own to register with. It refuses rather than
/// quietly succeeding, so `config.autostart` is never left claiming a registration that was never
/// written.
#[cfg(not(desktop))]
pub fn set(_app: &tauri::AppHandle, _enabled: bool) -> Result<(), CmdError> {
    Err("this build has no login registration to write".into())
}
