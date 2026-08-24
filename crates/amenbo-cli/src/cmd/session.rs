//! `session`: the surface layer's face — what an AI says about the terminal it is running in
//! (`AMB-D-749`). The vocabulary and the drop box it writes into are
//! [`amenbo_core::session`]'s; this is the reading of them, and the refusal that meets everywhere else.

use amenbo_core::session::{self, Statement, Surface};

use crate::cli::SessionCmd;
use crate::output::{human, print_json, CliError, Flags};

/// Say one thing about this session, or print the layer's canon when no verb was given.
///
/// The window is resolved first, for every route including the canon: the canon describes a vocabulary
/// that does not exist here, and handing it to a reader who cannot run a word of it is the same
/// misleading answer as accepting a statement nobody will see.
pub(crate) fn session_cmd(flags: &Flags, sub: Option<&SessionCmd>) -> Result<i32, CliError> {
    let surface = session::surface().ok_or_else(CliError::session_outside_surface)?;
    let Some(sub) = sub else { return Ok(canon(flags)) };
    let statement = statement(sub);
    // The bound is the label's, and this is the door it is held at (`amenbo_core::session`): what the
    // row cannot fit is turned away here, where the agent can still write it shorter, rather than
    // taken in and cut where nobody would know it had been.
    if let Some(over) = statement.overlong() {
        return Err(CliError::session_reason_too_long(over));
    }
    say(flags, &surface, statement)
}

/// The clap verb, as the layer's own statement. The two lists are the same list — a verb that parses
/// and a verb that can be said — and this is where that is held.
fn statement(sub: &SessionCmd) -> Statement {
    match sub {
        SessionCmd::Name { text } => Statement::Name(text.clone()),
        SessionCmd::Note { text } => Statement::Note(text.clone()),
        SessionCmd::Waiting { text } => Statement::Waiting(text.clone()),
        SessionCmd::Finished { text } => Statement::Finished(text.clone()),
        SessionCmd::Point { target, why } => {
            Statement::Point { target: target.clone(), why: why.clone() }
        }
    }
}

/// Leave the statement for the window, and report that it was left — not that it was read. Nothing here
/// waits for the pane to redraw, so what is confirmed is the only thing that is known.
fn say(flags: &Flags, surface: &Surface, statement: Statement) -> Result<i32, CliError> {
    session::say(surface, &statement).map_err(CliError::from)?;
    if flags.json {
        print_json(&serde_json::json!({
            "ok": true,
            "action": format!("session.{}", statement.verb()),
            "session": surface.session,
        }));
    } else {
        human(flags, format!("✓ {}", said(&statement)));
    }
    Ok(0)
}

/// One line saying what was just said, in the words the pane will show.
fn said(statement: &Statement) -> String {
    match statement {
        Statement::Name(text) => format!("this pane is now called “{text}”"),
        Statement::Note(text) => format!("note: {text}"),
        Statement::Waiting(text) => format!("waiting for a person: {text}"),
        Statement::Finished(text) => format!("finished: {text}"),
        Statement::Point { target, why } => format!("pointed at {target} — {why}"),
    }
}

/// The layer's canon (`session --json`, and its human reading). It is what `agent --json` deliberately
/// does not carry: this vocabulary exists in the window alone, so it is taught in the window alone.
fn canon(flags: &Flags) -> i32 {
    let spec = session::spec();
    if flags.json {
        print_json(&spec);
        return 0;
    }
    human(flags, spec["what"].as_str().unwrap_or_default());
    human(flags, "");
    human(flags, "Owed:");
    for line in spec["owed"].as_array().into_iter().flatten() {
        human(flags, format!("  • {}", line.as_str().unwrap_or_default()));
    }
    human(flags, "");
    human(flags, "Offered:");
    for line in spec["offered"].as_array().into_iter().flatten() {
        human(flags, format!("  • {}", line.as_str().unwrap_or_default()));
    }
    human(flags, "");
    for c in spec["commands"].as_array().into_iter().flatten() {
        human(
            flags,
            format!(
                "  {} {}\n      {}",
                c["command"].as_str().unwrap_or_default(),
                c["args"].as_str().unwrap_or_default(),
                c["summary"].as_str().unwrap_or_default(),
            ),
        );
    }
    0
}
