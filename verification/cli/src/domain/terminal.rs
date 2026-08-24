//! The `terminal` domain's one premise: **the machine a pane would be opened on**.
//!
//! Everything else this domain names is a screen's — a pane is what a reader is already typing in,
//! so the moves are the operator's and this driver walks none of them (`crate::Driver::action`).
//! What is here is the world underneath them, and it is here because it is a world: which agents a
//! pane can be opened with is settled before the app comes up, the same as a project already on the
//! board.
//!
//! **The build asks the operator's own machine, so a road has to answer for it.** What a frame
//! offers to open a pane with is every agent the build could find, and it finds them by running the
//! pane's login shell over the `PATH` that shell reads (`app/src-tauri/src/launch.rs`). Left alone,
//! the row is therefore whatever the person running the gate happens to have installed — nothing on
//! it where several were found, one on where a single was, no row at all where none were, all three
//! correct — so a road reading it would pass or fail by the machine and not by the build.
//!
//! **A directory in front of the `PATH` is how it is answered**, and it is the whole of the reach:
//! the programs stand in a directory the session owns, the GUI harness hands it to the app it
//! launches and to nothing else (`amenbo_verify_gui::launch`), and it goes when the session does.
//! Nothing is installed, nothing outside the run can see it, and the build under test is the shipped
//! one being asked the question it always asks.
//!
//! **It can only add.** Taking an install away would mean handing the probe a `PATH` the operator's
//! own profile could not put back, and that profile is read every time the shell starts — so the
//! count a road asks for is a floor, and the shape that can be stood up is "more than one thing to
//! open with". That is the shape worth having: it is the one the first run is read on.

use std::path::Path;

use amenbo_scenario::{Args, Domain};

use crate::{req_i64, unmapped, Driver, Outcome};

/// The commands the build looks for, in the order it lists them (`amenbo_core::harness::HARNESSES`).
///
/// Written down here rather than asked, because no face of the shipped binary hands the catalog out
/// — `agent-hook snippet` answers with a tool's wiring text and never with the program it is
/// started as. What that costs is drift: a command the build renamed leaves the stand-in unfound,
/// and the machine reads as whatever the operator has. The row this premise exists to stand up is
/// then not standing, and the assert that reads it (`opens-with` with `start: none`) is what says
/// so — on any machine with fewer than two agents of its own.
const COMMANDS: &[&str] = &["claude", "copilot", "cursor-agent", "codex", "gemini"];

/// Put `count` of them in `tools`, and say what was stood up.
///
/// The first of the catalog rather than a road's pick: which agents are on the row is nothing this
/// premise is about — what it is about is how many — and a road naming one would be a road about a
/// tool. They are written as programs that say what they are and stop, since nothing here starts
/// one: a pane opened on an agent is another road's, and a stand-in that pretended to be the tool
/// would be a road walking a fake.
fn stand_up(tools: &Path, count: i64) -> Result<String, String> {
    if count < 2 {
        return Err(format!(
            "`can-start` takes a count of 2 or more, not {count} — it puts programs in front of the \
             machine's own `PATH` and can only add to what the operator has installed, so a row with \
             fewer things on it is not a machine this can stand up"
        ));
    }
    let want = usize::try_from(count).unwrap_or(usize::MAX);
    if want > COMMANDS.len() {
        return Err(format!(
            "`can-start` was asked for {want} agents and Amenbo knows how to start {} — a machine \
             with more of them on it than there are is not one a reader could ever be at",
            COMMANDS.len()
        ));
    }
    let named = &COMMANDS[..want];
    for command in named {
        write_program(
            &tools.join(command),
            &format!(
                "#!/bin/sh\necho 'this is the verification harness standing in for {command}'\n"
            ),
        )?;
    }
    Ok(format!(
        "this machine can start {want} of the agents Amenbo knows ({})",
        named.join(", ")
    ))
}

/// Write `body` at `path` as a runnable program, leaving no descriptor on it in this process.
///
/// The writing is handed to a child for the reason the GUI harness's own stand-ins are: a file
/// written here and exec'd a moment later is the ETXTBSY race, and a process forked while this
/// one's descriptor was still open carries it until it execs. A premise runs beside whatever else
/// the harness is doing, so the descriptor is put somewhere nothing here can fork from.
fn write_program(path: &Path, body: &str) -> Result<(), String> {
    use std::io::Write as _;

    let mut writer = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"cat > "$0" && chmod 755 "$0""#)
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    let mut input = writer.stdin.take().ok_or("the writer took no input")?;
    input.write_all(body.as_bytes()).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    drop(input);
    let done = writer.wait().map_err(|e| format!("could not write {}: {e}", path.display()))?;
    match done.success() {
        true => Ok(()),
        false => Err(format!("{} was not left runnable ({done})", path.display())),
    }
}

impl Driver<'_> {
    /// The terminal face's one action here, and it is a premise's: everything else in this domain is
    /// a move on a screen this driver has not got.
    pub(crate) fn terminal_action(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "can-start" => {
                Ok(Outcome::action(stand_up(&self.session.tools, req_i64(with, "count")?)?))
            }
            _ => Err(unmapped(Domain::Terminal, op)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the premise claims: a program per agent asked for, taken off the front of the catalog,
    /// and each one runnable — an unrunnable file is not something `command -v` would answer for.
    #[test]
    fn the_asked_for_agents_are_standing_and_runnable() {
        let session = crate::scratch::session("can-start-test", false).expect("a throwaway session");
        let said = stand_up(&session.tools, 2).expect("two is a machine it can stand up");

        for command in &COMMANDS[..2] {
            let at = session.tools.join(command);
            assert!(at.is_file(), "{command} is standing: {said}");
            let out = std::process::Command::new(&at).output().expect("the stand-in runs");
            assert!(out.status.success(), "{command} is runnable");
        }
        assert!(!session.tools.join(COMMANDS[2]).exists(), "and nothing beyond what was asked for");
    }

    /// A count this cannot honestly deliver is refused rather than half-done. The reach is additive,
    /// so "one" and "none" are machines the operator's own installs decide, and a premise that
    /// shrugged would leave a road reading a row it never stood up.
    #[test]
    fn a_count_below_two_is_refused_because_nothing_here_can_take_an_install_away() {
        let session = crate::scratch::session("can-start-floor-test", false).expect("a session");
        for count in [-1, 0, 1] {
            let err = stand_up(&session.tools, count).expect_err("a floor, never a ceiling");
            assert!(err.contains("2 or more"), "{err}");
        }
    }

    /// And more agents than Amenbo knows how to start, which is a machine nobody could be at.
    #[test]
    fn more_agents_than_there_are_is_refused() {
        let session = crate::scratch::session("can-start-over-test", false).expect("a session");
        let asked = COMMANDS.len() as i64 + 1;
        let err = stand_up(&session.tools, asked).expect_err("there are only so many");
        assert!(err.contains(&COMMANDS.len().to_string()), "{err}");
    }
}
