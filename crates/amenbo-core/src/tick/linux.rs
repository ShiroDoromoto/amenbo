//! The Linux door: a pair of systemd **user** units, and `systemctl --user` to hold them
//! (`AMB-D-707`).
//!
//! User units and not system ones, which is what keeps the whole thing free of an administrator: they
//! are written into the reader's own configuration directory, held by the session's own systemd, and
//! the row they draw in a settings pane is theirs to switch off. The cost is the other half of that
//! symmetry — a user manager exists while the user is logged in, so logging out stops the tick. Turning
//! that off would take `enable-linger`, which is a machine-wide grant nobody asked amenbo for, so the
//! units are left as they are and the tick sleeps with the session.
//!
//! **A timer and a service, because systemd separates when from what.** The timer says once an hour and
//! the service says what to run; neither is usable without the other, so both are written, both are
//! removed, and only the timer is ever enabled — the service is pulled in by it.
//!
//! **The pair is named for the build that wrote it** ([`super::registered_name`]). One user has one
//! `~/.config/systemd/user`, so a dev build sharing production's unit names would not get its own
//! timer — it would overwrite production's and point it at itself.
//!
//! **`Persistent=true` is the one line the default gets wrong.** Without it a machine that was asleep or
//! shut down over the top of the hour simply loses that turn, and the missed-run guarantee
//! `AMB-D-707` set out for all three doors does not hold here.
//!
//! What the scheduler holds is read back with `is-enabled` and nothing else. Its exit status is the
//! whole answer — `0` enabled, `1` disabled, `4` no such unit — so amenbo never parses a unit file back
//! to find out what it wrote, which is the same restraint [`crate::tick::TickFix::Rewrite`] is built on.

use std::path::PathBuf;
use std::process::Stdio;

use crate::error::{Error, Msg, Result};
use crate::sys;

/// Linux has a door.
pub(super) const AVAILABLE: bool = true;

/// The unit that says *when*. It is the only one enabled, and so the only one `is-enabled` is asked
/// about: enabling the service directly would run it once at boot rather than once an hour.
fn timer() -> String {
    format!("{}.timer", super::registered_name())
}

/// The unit that says *what*. Named to match the timer, which is how systemd pairs the two without
/// either naming the other — so both are taken from the same stem, and a dev build's pair lands
/// beside production's in `~/.config/systemd/user` instead of on top of it.
fn service() -> String {
    format!("{}.service", super::registered_name())
}

/// Nothing here turns on which process is asking: a user unit is written and read through
/// `systemctl --user`, which answers whoever can reach the session.
pub(super) fn reachable_from_here() -> bool {
    AVAILABLE
}

/// There is nothing to launch that would answer differently.
pub(super) fn relaunch_target() -> Option<std::path::PathBuf> {
    None
}

pub(super) fn probe() -> Result<bool> {
    // `is-enabled` answers in its exit status and not on stdout, and the three answers that matter are
    // `0` enabled, `1` disabled, `4` no such unit. Anything else — a Linux with no systemd on it, a
    // session with no user manager to ask — is read as "nothing registered", which is the truth of what
    // is held either way. Reading it as an error instead would make a machine that simply has no timer
    // indistinguishable from one that would not say, and it is the writes below that have to speak up
    // about a systemd there is none of, since only they have something to fail at.
    Ok(matches!(systemctl(&["is-enabled", &timer()]).map(|c| c == Some(0)), Ok(true)))
}

pub(super) fn register() -> Result<()> {
    let dir = unit_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| wrote(&format!("create {}", dir.display()), e))?;

    let exe = std::env::current_exe()
        .map_err(|e| Error::Invalid(Msg::new(format!("Cannot find amenbo's own path: {e}"))))?;
    // Written over whatever is there rather than merged into it. These two files are amenbo's own —
    // their names say so — and rewriting them is exactly what points a timer at the build running now
    // after an upgrade, which is the contract `register` is idempotent for.
    let (timer, service) = (timer(), service());
    std::fs::write(dir.join(&service), service_unit(&exe.display().to_string()))
        .map_err(|e| wrote(&service, e))?;
    std::fs::write(dir.join(&timer), timer_unit()).map_err(|e| wrote(&timer, e))?;

    // systemd reads unit files once and caches them, so a file written under a manager already running
    // is not seen until it is told to look again.
    run(&["daemon-reload"])?;
    // `--now` starts the timer as well as enabling it, so the first turn is not waited for across a
    // reboot. Both halves are idempotent: over a timer already enabled and running, this exits 0 and
    // changes nothing.
    run(&["enable", "--now", &timer])
}

pub(super) fn unregister() -> Result<()> {
    // Disabling first, while the units are still on disk: systemd needs to read the timer's `[Install]`
    // section to undo what enabling it wrote. It is allowed to fail, and the ordinary reason it does is
    // that there was nothing registered — which is the state the caller asked for, so the removal
    // carries on to the files rather than stopping on a no-op.
    let _ = systemctl(&["disable", "--now", &timer()]);

    let dir = unit_dir()?;
    for unit in [timer(), service()] {
        match std::fs::remove_file(dir.join(&unit)) {
            Ok(()) => {}
            // Already gone is the state being asked for.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(wrote(&unit, e)),
        }
    }
    run(&["daemon-reload"])
}

/// Where a user's own units live — `$XDG_CONFIG_HOME/systemd/user`, which is `~/.config/systemd/user`
/// unless the reader moved their configuration. It is asked for the same way every other per-user path
/// in amenbo is, so a machine that has moved it is followed rather than written past.
fn unit_dir() -> Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("systemd").join("user"))
        .ok_or_else(|| Error::Invalid(Msg::new("Cannot find this user's configuration directory")))
}

/// One `oneshot` run of the entry the scheduler is here to call. It is not a daemon and holds nothing
/// open between turns, which is why the timer rather than the service is what stays registered.
fn service_unit(exe: &str) -> String {
    format!(
        "[Unit]\n\
         Description=amenbo hourly tick\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={exe} tick run\n"
    )
}

/// Once an hour, on the hour, and **carrying the turns it slept through**. `Persistent=true` is what
/// makes the second half true: without it a machine that was off over the top of the hour loses that
/// turn outright, and the tick's whole promise is that a day owed something eventually gets it.
fn timer_unit() -> String {
    "[Unit]\n\
     Description=amenbo hourly tick\n\
     \n\
     [Timer]\n\
     OnCalendar=hourly\n\
     Persistent=true\n\
     \n\
     [Install]\n\
     WantedBy=timers.target\n"
        .to_string()
}

/// Run a `systemctl --user` line and insist it succeeded — for the writes, where a failure the caller
/// cannot see would leave amenbo claiming a timer nobody holds.
fn run(args: &[&str]) -> Result<()> {
    match systemctl(args)? {
        Some(0) => Ok(()),
        other => Err(Error::Invalid(Msg::new(format!(
            "`systemctl --user {}` did not succeed{}",
            args.join(" "),
            match other {
                Some(code) => format!(" (exit {code})"),
                None => " (it was killed)".to_string(),
            }
        )))),
    }
}

/// Run a `systemctl --user` line and hand back its exit code, saying nothing on either stream: every
/// caller here reads the status and none of them reads the output, and a user's terminal is not
/// systemd's to write on in the middle of an unrelated command.
fn systemctl(args: &[&str]) -> Result<Option<i32>> {
    let status = sys::command("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| {
            Error::Invalid(Msg::new(format!(
                "Cannot reach systemd on this machine (`systemctl --user`): {e}"
            )))
        })?;
    Ok(status.code())
}

fn wrote(what: &str, e: std::io::Error) -> Error {
    Error::Invalid(Msg::new(format!("Cannot write the hourly tick's units ({what}): {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two lines a default would get wrong, and the one that pairs the units. They are read off the
    /// text because that is the whole of what amenbo writes — systemd is what acts on it, and a machine
    /// to act on is not something a build-time test has.
    #[test]
    fn the_units_carry_what_the_defaults_leave_out() {
        let timer = timer_unit();
        // Without this, every turn the machine slept through is simply lost.
        assert!(timer.contains("Persistent=true"), "got: {timer}");
        assert!(timer.contains("OnCalendar=hourly"), "got: {timer}");
        // What makes enabling the timer mean anything: with no `[Install]` there is nothing to enable.
        assert!(timer.contains("WantedBy=timers.target"), "got: {timer}");

        let service = service_unit("/opt/amenbo/bin/amenbo");
        assert!(service.contains("Type=oneshot"), "nothing is held open between turns: {service}");
        // The registration names the build that wrote it, which is what `Rewrite` is for.
        assert!(service.contains("ExecStart=/opt/amenbo/bin/amenbo tick run"), "got: {service}");
    }

    /// The pair systemd pairs is one stem with the two endings, and the stem is this build's — so a
    /// dev build writes two files of its own rather than over production's two.
    #[test]
    fn the_two_units_share_the_stem_this_build_registers_under() {
        let stem = super::super::registered_name();
        assert_eq!(timer(), format!("{stem}.timer"));
        assert_eq!(service(), format!("{stem}.service"));
    }

    /// The units are the user's own, so they are written where that user's configuration lives —
    /// `~/.config` unless the machine says otherwise, never a path amenbo made up.
    #[test]
    fn the_units_are_written_under_this_users_own_configuration() {
        let dir = unit_dir().expect("a user has a configuration directory");
        assert!(dir.ends_with("systemd/user"), "got: {}", dir.display());
    }
}
