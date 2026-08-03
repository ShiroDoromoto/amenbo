//! Start at login (`AMB-D-541`): the registration the OS reads when the user signs in, and the switch
//! that writes or removes it.
//!
//! What is registered is a plain launch of this executable — the window comes up the way it does when
//! the user opens the app themselves. There is no tray-resident shape behind this, and none is assumed:
//! the reason to be up early is that the inbox items which arrived while the app was closed are
//! collected on the next open (`AMB-D-310`), so opening more often is noticing sooner.
//!
//! Two states have to agree, and only one of them is ours: `config.autostart` is what the user asked
//! for, and the OS registration is what actually happens. [`crate::autostart::set`] writes the OS half
//! and the command that calls it writes the config half, in that order — a registration that could not
//! be written leaves the config saying "off", never the reverse. Between two runs the two can still
//! drift apart, because the registration is a file the user can delete and a path that stops naming
//! this executable, so [`crate::autostart::reconcile`] settles them once as the app comes up
//! (`AMB-D-546`).
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

/// What a startup pass has to do to bring the setting and the registration back into agreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    /// They already agree.
    Nothing,
    /// The user wants it and something is registered: write the registration again, so it names the
    /// executable running now rather than wherever this app used to live.
    Rewrite,
    /// The user wants it and nothing is registered: they removed it from the OS, so the setting goes
    /// off to match what they can see.
    TurnSettingOff,
    /// The user does not want it and something is registered: take the registration away.
    Deregister,
}

/// The whole of the reconciliation rule (`AMB-D-546`), as a truth table over the two states.
///
/// The registered-and-current row is not distinguished from registered-and-stale, and cannot be:
/// the plugin answers whether *something* is registered and offers no way to read what it points at,
/// so telling the two apart would mean parsing a plist, a desktop entry and a registry value that
/// another crate writes and may reformat. Writing the registration again covers both — it is what a
/// stale one needs, and what a current one already says. The user sees the same thing either way; the
/// cost is a few hundred bytes rewritten at launch.
pub fn fix_for(setting_on: bool, registered: bool) -> Fix {
    match (setting_on, registered) {
        (true, true) => Fix::Rewrite,
        (true, false) => Fix::TurnSettingOff,
        (false, true) => Fix::Deregister,
        (false, false) => Fix::Nothing,
    }
}

/// Settle the setting and the OS registration against each other, once, as the app comes up.
///
/// Nothing here is a reason to refuse to start, so every failure is logged and swallowed: an OS that
/// will not say what is registered leaves both halves as they were, which is the state the last run
/// left working. A development build returns without asking anything (`AMB-D-547`).
#[cfg(desktop)]
pub fn reconcile(app: &tauri::AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    if amenbo_core::config::Paths::is_dev_channel() {
        return;
    }
    let Ok(paths) = amenbo_core::config::Paths::resolve() else {
        return;
    };
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    let launcher = app.autolaunch();
    let registered = match launcher.is_enabled() {
        Ok(v) => v,
        Err(e) => {
            log::warn!("autostart: could not read the login registration ({e})");
            return;
        }
    };
    match fix_for(config.autostart, registered) {
        Fix::Nothing => {}
        Fix::Rewrite => {
            if let Err(e) = launcher.enable() {
                log::warn!("autostart: could not point the login registration at this build ({e})");
            }
        }
        Fix::TurnSettingOff => {
            config.autostart = false;
            if let Err(e) = config.save(&paths.config_file) {
                log::warn!("autostart: the login registration is gone but the setting could not follow ({e})");
            }
        }
        Fix::Deregister => {
            if let Err(e) = launcher.disable() {
                log::warn!("autostart: could not take the login registration away ({e})");
            }
        }
    }
}

/// The same pass on a target with no login of its own. There is nothing to settle.
#[cfg(not(desktop))]
pub fn reconcile(_app: &tauri::AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule in full, so a later reading of it has to move a row on purpose.
    #[test]
    fn the_two_states_settle_the_way_the_user_can_see() {
        // Wanted and registered: rewritten, because a registration naming a place this app has moved
        // out of starts nothing and says nothing about having failed.
        assert_eq!(fix_for(true, true), Fix::Rewrite);
        // Wanted but nothing registered: the user took it away from the OS side, and the setting
        // follows so that the switch and the login agree.
        assert_eq!(fix_for(true, false), Fix::TurnSettingOff);
        // Not wanted but registered: left over from a switch that was on, and it goes.
        assert_eq!(fix_for(false, true), Fix::Deregister);
        assert_eq!(fix_for(false, false), Fix::Nothing);
    }
}
