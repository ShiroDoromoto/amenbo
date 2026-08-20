//! `version` and `update`: what this build is, and replacing it — the all-in-one installer,
//! the standalone CLI's in-place self-update, and the rollback that undoes it.

use serde_json::json;

use amenbo_core::config::Paths;

use crate::agent;
use crate::cmd::attach::os_open;
use crate::output::{human, print_json, CliError, Flags};

/// `version` outside a binding: report only what this build knows about itself. Two things are dropped, and
/// both for the same reason — they cannot be answered without opening a store. `format_version` (the version
/// the store records) has nothing to read, and opening one would create it; and whether to query the upstream
/// latest.json is a store setting (`config.update_check`), so fetching it with the default ON while unable to
/// read that setting would trample the user's opt-out. Both are dropped silently, with zero network traffic.
/// Where a binding exists this function is not reached: the `Command::Version` path answers with both fields.
pub(crate) fn version_unbound(flags: &Flags) -> Result<i32, CliError> {
    let channel = amenbo_core::config::Paths::APP_NAME;
    if flags.json {
        print_json(&json!({
            "version": agent::VERSION,
            "schema_version": agent::SCHEMA_VERSION,
            "channel": channel,
            "release_build": amenbo_core::build_stamp::is_release_build(),
            // No store was opened, so claim no store-derived fact — do not pad these with 0 or false.
            "format_version": serde_json::Value::Null,
            "max_supported_format": amenbo_core::model::FORMAT_VERSION,
            "latest_version": serde_json::Value::Null,
            "update_available": serde_json::Value::Null,
        }));
    } else {
        let suffix = if channel == "amenbo" { String::new() } else { format!(" ({channel})") };
        human(flags, format!("amenbo {}{}", agent::VERSION, suffix));
        human(flags, format!("format: this build opens up to store v{}", amenbo_core::model::FORMAT_VERSION));
        if let Some(line) = unstamped_line() {
            human(flags, line);
        }
    }
    Ok(0)
}

/// What a build says about where it came from, and only when that is worth a line: a binary the
/// release workflow did not produce. The stamp is the one thing about a running Amenbo that no
/// version number, channel or path reveals ([`amenbo_core::build_stamp`]) — the number is the same
/// on both sides of a release, and a locally built binary answers to the production channel unless
/// it was built for the dev one. A shipped build stays silent here because it is the ordinary case;
/// `--json` carries `release_build` either way, which is what a machine reads (`AMB-D-540`).
pub(crate) fn unstamped_line() -> Option<&'static str> {
    (!amenbo_core::build_stamp::is_release_build())
        .then_some("build: local — this binary did not come out of the release workflow")
}

/// The explicit update route: look up this OS's all-in-one installer URL in latest.json and open it. There
/// is no self-update — opening is all it does. Because the user asked for it, the lookup runs regardless of
/// the config toggle **and regardless of the detection cache's TTL** (`AMB-D-463`): it goes and asks, rather
/// than answering from an entry up to 24 hours old, since what someone who typed this wants to know is the
/// state now. Neither what startup fetched nor a warm cache is reused for the same reason. It still falls
/// back to the latest-release page if the fetch fails or the OS is not listed, and offline behaviour is
/// unchanged — the fresh query shares the fall-back-to-stale contract with the cached one. Callable from
/// outside a binding, so it never touches the store.
///
/// A development build answers differently, and before any of that: the installer this would open is
/// production's, and the manifest it would name a version from is withheld from a dev build
/// (`update_check::is_disabled`), which would leave it saying "no newer version detected" about a
/// build that is normally behind. So it says what is true instead, opens nothing, and sends no traffic.
pub(crate) fn update_cmd(flags: &Flags, print: bool) -> Result<i32, CliError> {
    if Paths::is_dev_channel() {
        if flags.json {
            print_json(&json!({
                "action": "update",
                "current_version": agent::VERSION,
                // Nothing was queried, so claim nothing about upstream — do not pad these.
                "latest_version": serde_json::Value::Null,
                "update_available": false,
                "reason": "dev_channel",
                "url": serde_json::Value::Null,
                "opened": false,
            }));
        } else {
            human(flags, format!("{} is a development build — it does not update itself.", Paths::command_name()));
            human(flags, "Rebuild it from source (`make install-dev`) to move it forward.");
        }
        return Ok(0);
    }
    let latest = amenbo_core::update_check::check_fresh(true);
    let url = latest
        .as_ref()
        .map(|r| r.update_url())
        .unwrap_or_else(|| amenbo_core::update_check::LATEST_RELEASE_PAGE.to_string());
    let newer = latest
        .as_ref()
        .filter(|r| r.is_newer_than(agent::VERSION))
        .map(|r| r.version.clone());
    if flags.json {
        print_json(&json!({
            "action": "update",
            "current_version": agent::VERSION,
            "latest_version": latest.as_ref().map(|r| r.version.clone()),
            "update_available": newer.is_some(),
            "url": url,
            "opened": !print,
        }));
    } else {
        // `--print` is the face that opens nothing (headless / scripts), so it must not say it will.
        match (&newer, print) {
            (Some(v), _) => human(flags, format!("A newer amenbo ({v}) is available (this build is {}).", agent::VERSION)),
            (None, false) => human(flags, format!("This build is {} — no newer version detected (opening the installer anyway).", agent::VERSION)),
            (None, true) => human(flags, format!("This build is {} — no newer version detected.", agent::VERSION)),
        }
        human(flags, format!("Installer: {url}"));
    }
    if !print {
        os_open(&url)?;
    }
    Ok(0)
}

/// `amenbo update --apply`: self-update the standalone CLI in place. Downloads this platform's CLI
/// archive over TLS, checks version monotonicity, and swaps the running binary — no installer, no
/// elevation. Asks upstream afresh, past the detection cache's TTL, for the reason [`update_cmd`] gives
/// (`AMB-D-463`): declining to apply an update is a refusal, and a refusal must not rest on an entry up
/// to 24 hours old. Callable from outside a binding (a CLI-only user updates without a store), so it
/// never touches the store. The
/// three "correctly declined" outcomes (already current / GUI-managed / a development build) are
/// reported as plain messages with a zero exit; genuine failures (download, extract, swap, no archive)
/// are errors.
pub(crate) fn self_update_cmd(flags: &Flags) -> Result<i32, CliError> {
    use amenbo_core::self_update::{self, SelfUpdateError};
    let latest = amenbo_core::update_check::check_fresh(true);
    let Some(latest) = latest else {
        // A development build has no manifest to read by design — it is withheld from the channel
        // (`update_check::is_disabled`), not unreachable — so name the channel rather than the
        // network. `self_update::apply` refuses it as well; this is the half that words it, and it
        // sits ahead of the fetch so the reason reported is the real one.
        if Paths::is_dev_channel() {
            let declined = SelfUpdateError::DevChannel { channel: Paths::APP_NAME.to_string() };
            if flags.json {
                print_json(&json!({
                    "action": "self_update",
                    "updated": false,
                    "reason": "dev_channel",
                    "current_version": agent::VERSION,
                    "latest_version": serde_json::Value::Null,
                    "message": declined.to_string(),
                }));
            } else {
                human(flags, declined.to_string());
                human(flags, "Rebuild it from source (`make install-dev`) to move it forward.");
            }
            return Ok(0);
        }
        return Err(CliError {
            code: "io_error",
            message: "could not reach the release manifest to check for an update".to_string(),
            hint: Some(format!("check your connection, or run `{} update` to open the installer.", Paths::command_name())),
            exit: 1,
        });
    };

    match self_update::apply(&latest) {
        Ok(done) => {
            if flags.json {
                print_json(&json!({
                    "action": "self_update",
                    "updated": true,
                    "from": done.from,
                    "to": done.to,
                    "path": done.path.display().to_string(),
                    "backup": done.backup.display().to_string(),
                }));
            } else {
                human(flags, format!("Updated amenbo: {} → {}.", done.from, done.to));
                human(flags, "Restart amenbo to run the new version.");
                human(flags, format!("The previous binary is kept at {} — undo with `{} update --rollback`.", done.backup.display(), Paths::command_name()));
            }
            Ok(0)
        }
        // Not failures: already current, a GUI-managed CLI that the desktop app updates, or a
        // development build, which is refused whatever manifest it was handed. Report plainly with a
        // zero exit.
        Err(e @ (SelfUpdateError::UpToDate { .. } | SelfUpdateError::GuiManaged { .. } | SelfUpdateError::DevChannel { .. })) => {
            if flags.json {
                let (updated, reason) = match &e {
                    SelfUpdateError::UpToDate { .. } => (false, "up_to_date"),
                    SelfUpdateError::GuiManaged { .. } => (false, "gui_managed"),
                    SelfUpdateError::DevChannel { .. } => (false, "dev_channel"),
                    _ => unreachable!(),
                };
                print_json(&json!({
                    "action": "self_update",
                    "updated": updated,
                    "reason": reason,
                    "current_version": agent::VERSION,
                    "latest_version": latest.version,
                    "message": e.to_string(),
                }));
            } else {
                human(flags, e.to_string());
                if matches!(e, SelfUpdateError::GuiManaged { .. }) {
                    human(flags, format!("Run `{} update` to open the desktop installer instead.", Paths::command_name()));
                }
            }
            Ok(0)
        }
        // Genuine failures — e.g. no CLI archive listed for this platform (fall back to the installer).
        Err(e) => {
            let hint = match e {
                SelfUpdateError::NoArchive { .. } => {
                    Some(format!("run `{} update` to open the installer instead.", Paths::command_name()))
                }
                _ => Some(format!("try again, or run `{} update` to open the installer.", Paths::command_name())),
            };
            Err(CliError { code: "io_error", message: e.to_string(), hint, exit: 1 })
        }
    }
}

/// `amenbo update --rollback`: undo the last `--apply` by restoring the binary it retained beside the
/// running one — offline and instant, no download and no version check (a rollback is a deliberate
/// downgrade). Touches no store, like `--apply`. `NoBackup` (nothing was retained) and `GuiManaged` (the
/// desktop app owns a bundled CLI, so self-replace does not apply) are reported plainly with a zero exit;
/// a failed restore is a genuine error.
pub(crate) fn self_rollback_cmd(flags: &Flags) -> Result<i32, CliError> {
    use amenbo_core::self_update::{self, SelfUpdateError};
    match self_update::rollback() {
        Ok(done) => {
            let restored = done.restored.clone();
            if flags.json {
                print_json(&json!({
                    "action": "self_rollback",
                    "rolled_back": true,
                    "from": done.from,
                    "restored": restored,
                    "path": done.path.display().to_string(),
                }));
            } else {
                match &restored {
                    Some(v) => human(flags, format!("Rolled back amenbo: {} → {}.", done.from, v)),
                    None => human(flags, format!("Rolled back amenbo from {} to the previous version.", done.from)),
                }
                human(flags, "Restart amenbo to run the restored version.");
            }
            Ok(0)
        }
        // Not failures: nothing retained to roll back to, or a GUI-managed CLI that does not self-replace
        // here. Report plainly with a zero exit.
        Err(e @ (SelfUpdateError::NoBackup { .. } | SelfUpdateError::GuiManaged { .. })) => {
            if flags.json {
                let reason = match &e {
                    SelfUpdateError::NoBackup { .. } => "no_backup",
                    SelfUpdateError::GuiManaged { .. } => "gui_managed",
                    _ => unreachable!(),
                };
                print_json(&json!({
                    "action": "self_rollback",
                    "rolled_back": false,
                    "reason": reason,
                    "current_version": agent::VERSION,
                    "message": e.to_string(),
                }));
            } else {
                human(flags, e.to_string());
                if matches!(e, SelfUpdateError::GuiManaged { .. }) {
                    human(flags, "The desktop app owns updates for this CLI — use its own version history.");
                }
            }
            Ok(0)
        }
        // A genuine failed restore.
        Err(e) => Err(CliError {
            code: "io_error",
            message: e.to_string(),
            hint: Some(format!("try again, or run `{} update` to reinstall from the installer.", Paths::command_name())),
            exit: 1,
        }),
    }
}
