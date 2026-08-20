//! Start at login (`AMB-D-541`): the registration the OS reads when the user signs in, and the switch
//! that writes or removes it.
//!
//! What is registered is a plain launch of this executable — the window comes up the way it does when
//! the user opens the app themselves. There is no tray-resident shape behind this, and none is assumed:
//! the reason to be up early is that the inbox items which arrived while the app was closed are
//! collected on the next open (`AMB-D-310`), so opening more often is noticing sooner.
//!
//! Which door writes it is not the same on every OS (`AMB-D-549`). Windows and Linux go through
//! `tauri-plugin-autostart`, which writes the `HKCU` Run key and a `~/.config/autostart` desktop entry
//! — per-user, and listed by the OS beside the app they name. On macOS the plugin writes a plist under
//! `~/Library/LaunchAgents`, and macOS files such a plist as a *legacy agent*: it belongs to no bundle,
//! so System Settings lists it under the developer's name among background items, where someone looking
//! for this app does not find it. `SMAppService` registers the app itself instead, which is what puts a
//! row under "Open at Login" with a button to take it away. A macOS without that API (before 13) has
//! neither that list nor `SMAppService`, and keeps the plugin.
//!
//! Two states have to agree, and only one of them is ours: `config.autostart` is what the user asked
//! for, and the OS registration is what actually happens. [`crate::autostart::set`] writes the OS half
//! and the command that calls it writes the config half, in that order — a registration that could not
//! be written leaves the config saying "off", never the reverse. Between two runs the two can still
//! drift apart, because the registration is something the user can take away and a path that stops
//! naming this executable, so [`crate::autostart::reconcile`] settles them once as the app comes up
//! (`AMB-D-546`). One absence is not the user's doing and is read as such: an app that has moved since
//! the last pass takes its registration with it, and the pass puts it back where the app is now rather
//! than switching the setting off (`AMB-D-720`).
//!
//! A development build has none of it (`AMB-D-547`): the plugin is not registered, and this refuses.

use crate::error::CmdError;

/// The plugin that owns the per-user registration on Windows and Linux, ready to hand to
/// `app.handle().plugin(…)`. It is registered on macOS as well, where it is what a system too old for
/// `SMAppService` falls back to. No extra arguments are passed: a launch at login is the ordinary
/// launch.
#[cfg(desktop)]
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None)
}

/// The macOS door: the app registers *itself* as a login item, which is what a user can see and undo
/// from System Settings (`AMB-D-549`).
#[cfg(target_os = "macos")]
mod login_item {
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    /// Whether this macOS has `SMAppService` at all — it arrived in macOS 13, and the framework
    /// holding it is old enough that only the class itself can answer.
    pub fn available() -> bool {
        objc2::runtime::AnyClass::get(c"SMAppService").is_some()
    }

    /// Register (`enabled`) or unregister this app as a login item.
    pub fn set(enabled: bool) -> Result<(), String> {
        let service = unsafe { SMAppService::mainAppService() };
        let outcome = unsafe {
            if enabled {
                service.registerAndReturnError()
            } else {
                service.unregisterAndReturnError()
            }
        };
        match outcome {
            Ok(()) => Ok(()),
            // Asking for the state it is already in is answered as a failure by macOS, and is not one:
            // what the caller wanted is what the OS holds. Only a status that disagrees is an error.
            Err(_) if registered() == enabled => Ok(()),
            Err(e) => Err(e.localizedDescription().to_string()),
        }
    }

    /// Whether the OS is holding a login item for this app *and* honouring it. A user who switches the
    /// row off in System Settings leaves a status that is neither enabled nor gone, and the answer for
    /// every one of those is the same as having none: what they can see is off (`AMB-D-546`).
    pub fn registered() -> bool {
        let service = unsafe { SMAppService::mainAppService() };
        let status = unsafe { service.status() };
        status == SMAppServiceStatus::Enabled
    }

    /// Take away the plist that the versions writing this registration through the plugin left in
    /// `~/Library/LaunchAgents`, and say whether one was there.
    ///
    /// It is removed rather than kept because both would fire: the plist starts the app at login on its
    /// own, and the login item does too. What it says while it is there is that this user had asked to
    /// start at login, which is why the answer is carried into the same pass that removes it.
    ///
    /// Only a plist naming this app's bundle is touched — the file is in the user's own directory, and
    /// the name alone is not enough to be sure it is ours.
    ///
    /// That name is the **old** one, and stays lowercase however the bundle is spelled today: a plist
    /// only exists here because a version that wrote one through the plugin put it there, and every
    /// such version lived in `amenbo.app`. Spelling it the way the app is spelled now would match
    /// nothing, and the plist left behind would go on starting the app beside the login item.
    pub fn take_legacy_plist() -> bool {
        let Some(dirs) = directories::UserDirs::new() else {
            return false;
        };
        let plist = dirs.home_dir().join("Library/LaunchAgents/amenbo.plist");
        let Ok(body) = std::fs::read_to_string(&plist) else {
            return false;
        };
        if !body.contains("/amenbo.app/Contents/MacOS/") {
            return false;
        }
        if let Err(e) = std::fs::remove_file(&plist) {
            log::warn!("autostart: the login registration this app used to write could not be taken away ({e})");
            return false;
        }
        true
    }
}

/// Write (`enabled`) or remove (`!enabled`) this user's login registration, through whichever door
/// this OS uses.
#[cfg(desktop)]
fn write(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if login_item::available() {
        return login_item::set(enabled);
    }
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let written = if enabled { launcher.enable() } else { launcher.disable() };
    written.map_err(|e| e.to_string())
}

/// Whether the OS is holding a registration for this build right now.
#[cfg(desktop)]
fn read(app: &tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    if login_item::available() {
        return Ok(login_item::registered());
    }
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Whether a registration written by an older version of this app was found and taken away, so that
/// this pass can read it as the registration it replaces. Only macOS has ever written one.
#[cfg(desktop)]
fn take_legacy_registration() -> bool {
    #[cfg(target_os = "macos")]
    if login_item::available() {
        return login_item::take_legacy_plist();
    }
    false
}

/// Where this build runs from, as the absolute path the next pass compares against.
///
/// `None` when the OS will not say. A pass that cannot name its own location learns nothing from the
/// comparison and leaves the recorded one alone, so an absence there reads the way `AMB-D-546` had it.
#[cfg(desktop)]
fn running_from() -> Option<String> {
    std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned())
}

/// Write (`enabled`) or remove (`!enabled`) this user's login registration.
///
/// The webview never reaches the plugin itself — that is why no `autostart:*` permission is granted in
/// `capabilities/default.json`. A caller who could enable the registration without also writing
/// `config.autostart` would leave the switch and the OS disagreeing, so the only door in is
/// [`crate::commands::config_set_autostart`], which writes both.
#[cfg(desktop)]
pub fn set(app: &tauri::AppHandle, enabled: bool) -> Result<(), CmdError> {
    if amenbo_core::config::Paths::is_dev_channel() {
        // Nothing is registered on this channel, so there is nothing here to move. Refusing is the
        // half that holds whoever the caller is, the way the front end not drawing the switch is the
        // half that holds for the user (`AMB-D-547`).
        return Err("a development build does not register a login item".into());
    }
    write(app, enabled)
        .map_err(|e| CmdError::from(format!("could not write the login registration: {e}")))
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
    /// The user wants it, nothing is registered, and the app is not where it was: the registration
    /// went with the move rather than being taken away, so it is written again at the place the app
    /// runs from now (`AMB-D-720`).
    Reregister,
    /// The user wants it, nothing is registered, and the app is where it was: they removed it from the
    /// OS, so the setting goes off to match what they can see.
    TurnSettingOff,
    /// The user does not want it and something is registered: take the registration away.
    Deregister,
}

/// The whole of the reconciliation rule (`AMB-D-546`, amended by `AMB-D-720`), as a truth table over
/// the two states and whether the app itself has moved since the last pass.
///
/// The registered-and-current row is not distinguished from registered-and-stale, and cannot be:
/// what is registered can be read as a yes or no and offers no way to read what it points at, so
/// telling the two apart would mean parsing a plist, a desktop entry and a registry value that
/// another crate writes and may reformat. Writing the registration again covers both — it is what a
/// stale one needs, and what a current one already says. The user sees the same thing either way; the
/// cost is one write at launch. That is also why `moved` is read only where nothing is registered:
/// where something is, the answer is already to write it again.
///
/// Nothing registered is the row the move splits. The OS says the same thing whether the user switched
/// the row off or the app was replaced underneath it, and only one of those is an answer. An app that
/// is not where the last pass left it was replaced — a macOS `.pkg` swaps the bundle whole — so its
/// registration is put back rather than read as a no. Where the app has not moved, the absence means
/// what `AMB-D-546` always took it to mean, and the setting follows what the user can see.
///
/// The cost is the one order this cannot recover: a user who moves the app and switches the row off
/// between the same two starts is read as having only moved it, and gets the registration back.
/// Whether the app is somewhere other than where the last pass recorded it.
///
/// Two answers are needed to compare, and either being absent is the same as agreement: a first pass
/// has nothing to have moved from, and a pass that cannot read its own location has nothing to compare
/// with. Both leave the absence of a registration meaning what `AMB-D-546` had it mean, which is the
/// conservative half — it never writes a registration back on a guess.
pub fn has_moved(here: Option<&str>, recorded: Option<&str>) -> bool {
    match (here, recorded) {
        (Some(now), Some(before)) => now != before,
        _ => false,
    }
}

pub fn fix_for(setting_on: bool, registered: bool, moved: bool) -> Fix {
    match (setting_on, registered, moved) {
        (true, true, _) => Fix::Rewrite,
        (true, false, true) => Fix::Reregister,
        (true, false, false) => Fix::TurnSettingOff,
        (false, true, _) => Fix::Deregister,
        (false, false, _) => Fix::Nothing,
    }
}

/// Settle the setting and the OS registration against each other, once, as the app comes up.
///
/// Nothing here is a reason to refuse to start, so every failure is logged and swallowed: an OS that
/// will not say what is registered leaves both halves as they were, which is the state the last run
/// left working. A development build returns without asking anything (`AMB-D-547`).
#[cfg(desktop)]
pub fn reconcile(app: &tauri::AppHandle) {
    if amenbo_core::config::Paths::is_dev_channel() {
        return;
    }
    let Ok(paths) = amenbo_core::config::Paths::resolve() else {
        return;
    };
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    // A registration this app wrote through an older door is swept away here and counted in — for the
    // user it is the same answer, given once, and it goes on holding through the pass that moves it to
    // the door this build uses.
    let carried_over = take_legacy_registration();
    let registered = match read(app) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("autostart: could not read the login registration ({e})");
            return;
        }
    };
    // Where the app is now, against where the last pass left it (`AMB-D-720`).
    let here = running_from();
    let moved = has_moved(here.as_deref(), config.autostart_exe.as_deref());
    let mut settled = false;
    // A move that was found but could not be answered leaves the recorded location alone, so the next
    // pass finds the same move and tries again. Recording it would turn the retry into a
    // `TurnSettingOff` — the very thing this row exists to prevent — over a write that failed once.
    let mut record_here = true;
    match fix_for(config.autostart, registered || carried_over, moved) {
        Fix::Nothing => {}
        Fix::Rewrite => {
            if let Err(e) = write(app, true) {
                log::warn!("autostart: could not point the login registration at this build ({e})");
            }
        }
        Fix::Reregister => {
            if let Err(e) = write(app, true) {
                log::warn!("autostart: this app moved and its login registration did not survive it, and could not be written again here ({e})");
                record_here = false;
            }
        }
        Fix::TurnSettingOff => {
            config.autostart = false;
            settled = true;
        }
        Fix::Deregister => {
            if let Err(e) = write(app, false) {
                log::warn!("autostart: could not take the login registration away ({e})");
            }
        }
    }
    // Record where this pass ran, so the next one can tell a move from the user's own hand. Written
    // whatever the setting says: it is what the setting will be read against once it is on.
    if record_here && here.is_some() && here != config.autostart_exe {
        config.autostart_exe = here;
        settled = true;
    }
    if settled {
        if let Err(e) = config.save(&paths.config_file) {
            log::warn!("autostart: what this pass settled could not be written down, so the next one reads the state it left ({e})");
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
        // out of starts nothing and says nothing about having failed. Whether the app moved does not
        // enter it — writing it again is already the answer to both.
        assert_eq!(fix_for(true, true, false), Fix::Rewrite);
        assert_eq!(fix_for(true, true, true), Fix::Rewrite);
        // Wanted, nothing registered, and the app is where it was: the user took it away from the OS
        // side, and the setting follows so that the switch and the login agree.
        assert_eq!(fix_for(true, false, false), Fix::TurnSettingOff);
        // Not wanted but registered: left over from a switch that was on, and it goes.
        assert_eq!(fix_for(false, true, false), Fix::Deregister);
        assert_eq!(fix_for(false, true, true), Fix::Deregister);
        assert_eq!(fix_for(false, false, false), Fix::Nothing);
        assert_eq!(fix_for(false, false, true), Fix::Nothing);
    }

    /// The row `AMB-D-720` added, and the one it is told apart from. Both look the same to the OS —
    /// the setting is on and nothing is registered — and only the app's own location separates them.
    #[test]
    fn a_registration_the_move_took_is_not_a_registration_the_user_took() {
        // The app is not where the last pass left it, so what removed the registration was the move.
        // Switching the setting off here is what would silently stop every updated mac.
        assert_eq!(fix_for(true, false, true), Fix::Reregister);
        // The app has not moved, so the only hand that could have removed it is the user's.
        assert_eq!(fix_for(true, false, false), Fix::TurnSettingOff);
    }

    /// The comparison the new row rests on, and the two ways it has nothing to compare.
    #[test]
    fn a_location_is_only_a_move_against_one_that_was_recorded() {
        assert!(has_moved(Some("/Applications/Amenbo.app/Contents/MacOS/amenbo-app"), Some("/Applications/amenbo.app/Contents/MacOS/amenbo-app")));
        assert!(!has_moved(Some("/Applications/amenbo.app/Contents/MacOS/amenbo-app"), Some("/Applications/amenbo.app/Contents/MacOS/amenbo-app")));
        // No pass has recorded one yet: a first start is not a move, whatever it finds registered.
        assert!(!has_moved(Some("/Applications/amenbo.app/Contents/MacOS/amenbo-app"), None));
        // The OS would not say where this build runs from, so there is nothing to compare and the
        // recorded one is left standing for the next pass.
        assert!(!has_moved(None, Some("/Applications/amenbo.app/Contents/MacOS/amenbo-app")));
        assert!(!has_moved(None, None));
    }

    /// The registration a machine carries in from an older door is a registration: the pass that
    /// sweeps it away writes the new one, instead of reading the sweep as the user having said no.
    #[test]
    fn a_registration_carried_in_from_the_older_door_keeps_the_answer() {
        let registered_now = false;
        let carried_over = true;
        assert_eq!(fix_for(true, registered_now || carried_over, false), Fix::Rewrite);
        // Nothing carried in, and the absence means what it has always meant.
        assert_eq!(fix_for(true, false, false), Fix::TurnSettingOff);
    }
}
