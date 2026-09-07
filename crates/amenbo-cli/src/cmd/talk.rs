//! `talk`: the surface layer's face — what an AI says about the terminal it is running in
//! (`AMB-D-749`), spoken under the name of the window it reaches (`AMB-D-757`). The vocabulary and the
//! drop box it writes into are [`amenbo_core::session`]'s; this is the reading of them, and the refusal
//! that meets everywhere else.

use amenbo_core::session::{self, Statement, Surface};

use crate::cli::TalkCmd;
use crate::output::{human, print_json, CliError, Flags};

/// Say one thing about this session, or print the layer's canon when no verb was given.
///
/// The window is resolved first, for every route including the canon: the canon describes a vocabulary
/// that does not exist here, and handing it to a reader who cannot run a word of it is the same
/// misleading answer as accepting a statement nobody will see.
pub(crate) fn talk_cmd(flags: &Flags, sub: Option<&TalkCmd>) -> Result<i32, CliError> {
    let surface = session::surface().ok_or_else(CliError::talk_outside_surface)?;
    let Some(sub) = sub else { return Ok(canon(flags)) };
    say(flags, &surface, statement(sub))
}

/// The clap verb, as the layer's own statement. The two lists are the same list — a verb that parses
/// and a verb that can be said — and this is where that is held.
fn statement(sub: &TalkCmd) -> Statement {
    match sub {
        TalkCmd::Name { text } => Statement::Name(text.clone()),
    }
}

/// Leave the statement for the window, and report that it was left — not that it was read. Nothing here
/// waits for the pane to redraw, so what is confirmed is the only thing that is known.
fn say(flags: &Flags, surface: &Surface, statement: Statement) -> Result<i32, CliError> {
    session::say(surface, &statement).map_err(CliError::from)?;
    if flags.json {
        print_json(&serde_json::json!({
            "ok": true,
            "action": format!("talk.{}", statement.verb()),
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
        // Not a verb anyone types: `amenbo agent` leaves it on its own (`AMB-D-805`), so no route
        // through `talk` ever reaches this line.
        Statement::Briefed => "read the canon".to_string(),
    }
}

/// The layer's canon (`talk --json`, and its human reading). It is what `agent --json` deliberately
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
    // Printed only when there is something in it. The canon keeps the key either way — an empty list
    // is the statement that nothing here may be left out (`AMB-D-859`) — but a heading with no lines
    // under it reads on a terminal as an answer that got cut off.
    if let Some(lines) = spec["offered"].as_array().filter(|o| !o.is_empty()) {
        human(flags, "");
        human(flags, "Offered:");
        for line in lines {
            human(flags, format!("  • {}", line.as_str().unwrap_or_default()));
        }
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
