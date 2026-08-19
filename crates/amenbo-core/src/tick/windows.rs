//! The Windows door: one Task Scheduler task, registered from XML because the flags this needs are not
//! on `schtasks` (`AMB-D-707`).
//!
//! **The registration is written as XML, and that is not a preference.** Two of the settings a tick
//! cannot live without have no `schtasks /Create` flag at all, so a task created from the command line's
//! own vocabulary takes their defaults — and their defaults are what this door exists to get past.
//!
//! | Setting | Windows' default | Why it cannot stay |
//! |---|---|---|
//! | `DisallowStartIfOnBatteries` | `true` | A laptop off its charger runs no tick at all |
//! | `StopIfGoingOnBatteries` | `true` | A tick that started is killed the moment the plug comes out |
//! | `StartWhenAvailable` | `false` | A turn the machine slept through is dropped rather than caught up |
//!
//! The first two are the loud ones: on the machine most people have, a plain registration is one that
//! never fires. The third is the same missed-run guarantee the other two doors write a line for.
//!
//! **The task is named for the build that registered it** ([`super::registration_name`]). A user has
//! one Task Scheduler namespace, and `/F` overwrites without asking — so two amenbos under one name
//! is not two tasks but one, pointed at whichever registered last.
//!
//! **The XML file has to be UTF-16.** `schtasks /Create /XML` reads it as Unicode and refuses a UTF-8
//! file, so the bytes are encoded here rather than handed to a writer that would do the ordinary thing.
//! It is a throwaway file next to the registration, not a record of it: what the scheduler holds after
//! the call is the task, and the file is gone.
//!
//! What is held is read back with `/Query` and nothing else — its exit status is the whole answer, so
//! amenbo never parses a task definition back to find out what it wrote, which is the same restraint
//! [`crate::tick::TickFix::Rewrite`] is built on.

use std::path::PathBuf;
use std::process::Stdio;

use crate::error::{Error, Msg, Result};
use crate::sys;
use crate::tmpdir;

/// Windows has a door.
pub(super) const AVAILABLE: bool = true;

/// Deleting the task deletes it: nothing of ours is left for the user to find.
pub(super) const REMOVAL_LEAVES_A_ROW: bool = false;

/// The one row the user sees, named the way the other doors name theirs — this build's name
/// ([`super::registration_name`]), so a dev build asks the scheduler for a task of its own instead of
/// writing over production's. A flat name and not a folder: a folder under `Tasks` is a second thing
/// to create, to find and to leave behind, and there is only ever one task to put in it.
fn task_name() -> String {
    super::registration_name(super::TICK_NAME, '-')
}

/// Nothing here turns on which process is asking: the task is written and read through the scheduler,
/// which answers whoever is signed in as this user.
pub(super) fn reachable_from_here() -> bool {
    AVAILABLE
}

/// There is nothing to launch that would answer differently.
pub(super) fn relaunch_target() -> Option<std::path::PathBuf> {
    None
}

pub(super) fn probe() -> Result<bool> {
    // `/Query /TN` answers in its exit status: `0` the task is there, non-zero it is not. Anything that
    // is not a plain yes is read as "nothing registered" — which is the truth of what is held either
    // way, and keeps a scheduler that would not answer from being told apart from an empty one by a
    // caller that has the same move to make for both.
    Ok(matches!(schtasks(&["/Query", "/TN", &task_name()]), Ok(Some(0))))
}

pub(super) fn register() -> Result<()> {
    let exe = std::env::current_exe()
        .map_err(|e| Error::Invalid(Msg::new(format!("Cannot find amenbo's own path: {e}"))))?;

    // A file for the length of one call. It is written where throwaway files go rather than beside the
    // store: nothing reads it again, and a definition left lying next to a user's work would read as one.
    let path = xml_path();
    std::fs::write(&path, utf16(&definition(&exe.display().to_string())))
        .map_err(|e| wrote(&path, e))?;

    // `/F` is what makes this idempotent: over a task already registered it overwrites without asking,
    // and without it `schtasks` stops on a Y/N nobody is there to answer. Overwriting is also how an
    // upgrade points the task at the build running now.
    let out = run(&["/Create", "/TN", &task_name(), "/XML", &path.display().to_string(), "/F"]);
    let _ = std::fs::remove_file(&path);
    out
}

pub(super) fn unregister() -> Result<()> {
    // Deleting what is not there exits non-zero, and that is the state the caller asked for — so the
    // status is read rather than insisted on, and only a scheduler that could not be reached at all is
    // reported.
    schtasks(&["/Delete", "/TN", &task_name(), "/F"])?;
    Ok(())
}

/// The whole registration, as the scheduler's own vocabulary.
///
/// One trigger, repeating hourly for as long as the day lasts and starting again the next — which is how
/// Task Scheduler writes "every hour, forever", there being no plain hourly trigger. The start boundary
/// is a date in the past on purpose: it says where the hours are counted from, not when the task begins,
/// and one already gone means the first turn is the next hour rather than a day away.
fn definition(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>amenbo hourly tick</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>2000-01-01T00:00:00</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay>
        <DaysInterval>1</DaysInterval>
      </ScheduleByDay>
      <Repetition>
        <Interval>PT1H</Interval>
        <Duration>P1D</Duration>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
    </CalendarTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <ExecutionTimeLimit>PT1H</ExecutionTimeLimit>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{}</Command>
      <Arguments>tick run</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        escape(exe)
    )
}

/// The five characters that would otherwise end an element early. A path is the only thing interpolated
/// here, and `&` is legal in one — so the escape is what keeps a folder somebody named from producing a
/// document the scheduler refuses.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The bytes `schtasks` will read: UTF-16 little-endian, behind the byte-order mark that says so. A
/// UTF-8 file is refused outright, whatever the declaration inside it says.
fn utf16(s: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in s.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn xml_path() -> PathBuf {
    std::env::temp_dir().join(format!("amenbo-tick-{}.xml", tmpdir::suffix()))
}

/// Run a `schtasks` line and insist it succeeded — for the write, where a failure the caller cannot see
/// would leave amenbo claiming a task nobody holds.
fn run(args: &[&str]) -> Result<()> {
    match schtasks(args)? {
        Some(0) => Ok(()),
        other => Err(Error::Invalid(Msg::new(format!(
            "`schtasks {}` did not succeed{}",
            args.join(" "),
            match other {
                Some(code) => format!(" (exit {code})"),
                None => " (it was killed)".to_string(),
            }
        )))),
    }
}

/// Run a `schtasks` line and hand back its exit code, saying nothing on either stream: every caller here
/// reads the status and none of them reads the output, and `/Query` prints a table no one asked for.
fn schtasks(args: &[&str]) -> Result<Option<i32>> {
    let status = sys::command("schtasks")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| {
            Error::Invalid(Msg::new(format!("Cannot reach the Task Scheduler (`schtasks`): {e}")))
        })?;
    Ok(status.code())
}

fn wrote(path: &std::path::Path, e: std::io::Error) -> Error {
    Error::Invalid(Msg::new(format!(
        "Cannot write the hourly tick's definition ({}): {e}",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three settings a default gets wrong, read off the document amenbo hands the scheduler —
    /// which is the whole of what amenbo writes, the scheduler being what acts on it.
    #[test]
    fn the_definition_turns_off_both_battery_gates_and_catches_up_missed_turns() {
        let xml = definition(r"C:\Program Files\amenbo\amenbo.exe");
        // On the machine most people have, leaving either of these at its default is a tick that never
        // fires — the first refuses to start off the charger, the second kills one that had started.
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"));
        // And the missed-run line the other two doors write their own version of.
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        // Hourly, and the hours counted from a boundary already gone so the first turn is this hour.
        assert!(xml.contains("<Interval>PT1H</Interval>"));
        assert!(xml.contains("<StartBoundary>2000-01-01T00:00:00</StartBoundary>"));
        // The registration names the build that wrote it, which is what `Rewrite` is for.
        assert!(xml.contains(r"<Command>C:\Program Files\amenbo\amenbo.exe</Command>"));
        assert!(xml.contains("<Arguments>tick run</Arguments>"));
    }

    /// A path is the one thing interpolated into the document, and `&` is legal in a Windows folder
    /// name — so a reader who has one gets a registration rather than a parse error.
    #[test]
    fn a_path_with_xml_in_it_does_not_end_the_element_early() {
        let xml = definition(r"C:\R&D\amenbo.exe");
        assert!(xml.contains(r"<Command>C:\R&amp;D\amenbo.exe</Command>"), "got: {xml}");
    }

    /// The whole round trip against the machine's own scheduler: register, read it back, register again
    /// over it, take it away, and read the absence. It is the one thing the tests above cannot answer —
    /// whether Task Scheduler *accepts* this document — and no build-time reading of the text can stand
    /// in for it.
    ///
    /// `#[ignore]`d because it writes into the scheduler of whatever machine runs it, which is not
    /// something a plain `cargo test` should do to a reader's laptop. Run it deliberately on a Windows
    /// box: `cargo test -p amenbo-core tick::windows -- --ignored`. It leaves nothing behind — the task
    /// it registers is the real one, and the last thing it does is delete it — so a machine that had the
    /// tick registered before will not afterwards.
    #[test]
    #[ignore = "registers a real task in this machine's Task Scheduler"]
    fn the_scheduler_takes_the_definition_and_gives_it_back() {
        assert!(register().is_ok(), "the scheduler accepts the document");
        assert!(probe().expect("the scheduler has a state to report"), "and holds it afterwards");
        // Over one already there: `/F` overwrites rather than stopping on a question.
        assert!(register().is_ok(), "registering twice is one task and no error");
        assert!(probe().expect("still readable"));

        assert!(unregister().is_ok());
        assert!(!probe().expect("still readable"), "and nothing is held once it is taken away");
        // Taking away what is not there is the state the caller asked for, not a failure.
        assert!(unregister().is_ok());
    }

    /// The encoding `schtasks` insists on: little-endian UTF-16 behind the mark that says so. A file
    /// written as UTF-8 is refused whatever its declaration claims.
    #[test]
    fn the_definition_is_handed_over_as_utf16_behind_its_mark() {
        let bytes = utf16("<Task/>");
        assert_eq!(&bytes[..2], &[0xFF, 0xFE], "the byte-order mark comes first");
        assert_eq!(&bytes[2..6], &[b'<', 0, b'T', 0], "and every unit after it is two bytes");
        assert_eq!(bytes.len(), 2 + "<Task/>".len() * 2);
    }
}
