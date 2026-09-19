//! `viewer`: the server this store is read from on a phone, and which phone may read it (`AMB-D-884`).
//!
//! **Every one of these is the device's, not a project's.** The server stands in one Cloudflare account,
//! the key is one key, and the read code is one code — so nothing here is asked which project it is about,
//! and nothing here writes a row a project owns.
//!
//! **Two of them put a secret on the screen and neither of them will put one anywhere else.** The API
//! token setup needs is taken on stdin, never as an argument: a token on the command line is visible in
//! the process list and lands in shell history. The read code carries the encryption key, so it is drawn
//! where a camera can reach it and nowhere a pipe can.

use std::io::IsTerminal as _;

use serde_json::json;

use amenbo_core::config::Paths;
use amenbo_core::viewer::{self, code, pairing, repair::Repaired, send::HeldBack, send::Server};
use amenbo_core::Store;

use crate::cli::ViewerCmd;
use crate::output::{confirm, human, print_json, write_envelope, CliError, Flags};

pub(crate) fn viewer(store: &mut Store, flags: &Flags, sub: ViewerCmd) -> Result<i32, CliError> {
    match sub {
        ViewerCmd::Setup { account, name } => setup(store, flags, account.as_deref(), name.as_deref()),
        ViewerCmd::App { terminal } => app(flags, terminal),
        ViewerCmd::Qr { terminal } => qr(store, flags, terminal),
        ViewerCmd::Phones => phones(store, flags),
        ViewerCmd::Revoke => revoke(store, flags),
        ViewerCmd::Send => send(store, flags),
        ViewerCmd::Repair { send } => mend(store, flags, send),
    }
}

/// Stand the server up, with the token read from wherever the person is.
fn setup(
    store: &mut Store,
    flags: &Flags,
    account: Option<&str>,
    name: Option<&str>,
) -> Result<i32, CliError> {
    let token = the_api_token(flags)?;
    let stood = viewer::setup(store, &token, account, name).map_err(CliError::from)?;
    let kept = matches!(stood.keys, viewer::Keys::Kept);
    let line = format!(
        "The Viewer's server is up at {} (account {}, database {}). {}",
        stood.url,
        stood.account,
        stood.database,
        if kept {
            "The keys already here were kept, so every phone paired to it still reads."
        } else {
            "New keys were drawn, so pair a phone again with `viewer qr`."
        },
    );
    write_envelope(flags, "viewer.setup", "server", json!(stood), None, false, line);
    Ok(0)
}

/// The Cloudflare API token, which is never an argument.
///
/// **A terminal is asked and a pipe is read.** The token is used for the one run and written down
/// nowhere, so where it comes from is only ever this: the person in front of the screen, or whatever fed
/// them in. Under `--json` there is nobody to ask, so only the pipe is left.
fn the_api_token(flags: &Flags) -> Result<String, CliError> {
    let asked = if std::io::stdin().is_terminal() {
        if flags.json {
            return Err(CliError {
                code: "invalid_value",
                message: "the API token has to come in on stdin, and stdin is a terminal".to_string(),
                hint: Some(format!(
                    "Pipe it in (`… | {} viewer setup --json`), or run without `--json` and be asked for it.",
                    Paths::command_name(),
                )),
                exit: 2,
            });
        }
        human(
            flags,
            format!(
                "Create a token with the permissions this needs already ticked, then paste it here:\n  {}",
                viewer::token_link(),
            ),
        );
        rpassword::prompt_password("Cloudflare API token: ").map_err(|err| CliError {
            code: "io_error",
            message: format!("Cannot read the API token: {err}"),
            hint: None,
            exit: 1,
        })?
    } else {
        use std::io::Read as _;
        let mut typed = String::new();
        std::io::stdin().read_to_string(&mut typed).map_err(|err| CliError {
            code: "io_error",
            message: format!("Cannot read the API token from stdin: {err}"),
            hint: None,
            exit: 1,
        })?;
        typed
    };
    let token = asked.trim().to_string();
    if token.is_empty() {
        return Err(CliError {
            code: "invalid_value",
            message: "no API token was given, so there is no account to build in".to_string(),
            hint: Some(format!("Create one at {}", viewer::token_link())),
            exit: 2,
        });
    }
    Ok(token)
}

/// Where the app is got. It asks nothing of the store: the two addresses are the same on every device,
/// set up or not.
fn app(flags: &Flags, forced: bool) -> Result<i32, CliError> {
    if flags.json {
        let rows: Vec<serde_json::Value> = pairing::THE_APP
            .iter()
            .map(|store| json!({ "phone": store.phone, "link": store.link }))
            .collect();
        print_json(&json!({ "app": rows }));
        return Ok(0);
    }
    if drawn_here(flags, forced) {
        for store in pairing::THE_APP {
            human(flags, format!("\n{}", store.phone));
            human(flags, drawn(store.link)?);
        }
    }
    // The addresses go out in words as well, so a terminal that cannot draw — or somebody reading this
    // back out of a log — still has the one thing they came for.
    human(flags, "\nThe Viewer app is at:");
    for store in pairing::THE_APP {
        human(flags, format!("  {} ({})", store.link, store.phone));
    }
    Ok(0)
}

/// Draw a new read code.
///
/// **What it is drawable on is settled before it is issued.** Issuing replaces whatever code the server
/// was holding, so a run that would end with nothing on the screen has already stopped the phone that was
/// reading — which is why the refusals below come first.
fn qr(store: &Store, flags: &Flags, forced: bool) -> Result<i32, CliError> {
    if flags.json {
        return Err(CliError {
            code: "invalid_value",
            message: "a read code is for a camera, and JSON is not one".to_string(),
            hint: Some(format!(
                "Run `{} viewer qr` without `--json`. It carries the encryption key, so it is drawn on a screen and nowhere else.",
                Paths::command_name(),
            )),
            exit: 2,
        });
    }
    if !drawn_here(flags, forced) {
        return Err(CliError {
            code: "invalid_value",
            message: "stdout is not a terminal, so there is nowhere to draw the code".to_string(),
            hint: Some("Run it where you are looking, or pass `--terminal` to draw it anyway.".to_string()),
            exit: 2,
        });
    }
    the_server_is_up(store)?;

    // The moment the server wrote it down is not drawn: what a person needs here is the code, and the
    // only reading of that timestamp anybody acts on is `viewer phones`, which asks the server for it.
    let Some(pairing::Issued { carried, .. }) = pairing::issue(store).map_err(CliError::from)? else {
        return Err(no_server_yet());
    };
    human(flags, format!("\n{}", drawn(&carried)?));
    human(flags, "Read this with the Viewer app's camera.");
    human(flags, "That code carries the key, and it stays in this terminal's scrollback.");
    human(
        flags,
        "Whatever phone held the code before this one has stopped reading — pairing again is this same command.",
    );
    Ok(0)
}

/// Whether a phone may read, as the server answers it.
fn phones(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    the_server_is_up(store)?;
    let Some(reading) = pairing::who_may_read(store).map_err(CliError::from)? else {
        return Err(no_server_yet());
    };
    if flags.json {
        print_json(&json!({
            "paired": reading.paired,
            "issued_at": reading.issued_at,
        }));
        return Ok(0);
    }
    if !reading.paired {
        human(flags, "No phone may read this store.");
        human(flags, format!("Pair one with `{} viewer qr`.", Paths::command_name()));
        return Ok(0);
    }
    match reading.issued_at.as_deref() {
        Some(when) => human(flags, format!("A phone may read, with a code issued {when}.")),
        None => human(flags, "A phone may read."),
    }
    // There is one code and the server never learns which phone offered it, so the count is not a number
    // anybody has. Saying so is better than a screen that looks like it is answering "how many".
    human(flags, "There is one read code, so this says whether any phone may read, not how many do.");
    Ok(0)
}

/// Take the read code away.
fn revoke(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    the_server_is_up(store)?;
    if !confirm(flags, "take the read code away — every phone paired to this store stops reading")? {
        return Ok(1);
    }
    let Some(cut) = pairing::cut_off(store).map_err(CliError::from)? else {
        return Err(no_server_yet());
    };
    let line = if cut {
        "The read code is gone, and every phone that held it has stopped reading."
    } else {
        "The server was holding no read code, so no phone was reading."
    };
    write_envelope(flags, "viewer.revoke", "pairing", json!({ "cut": cut }), None, !cut, line);
    Ok(0)
}

/// Carry what has moved.
fn send(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    the_server_is_up(store)?;
    let sent = viewer::send::carry(store).map_err(CliError::from)?;
    // Neither reason a turn does nothing is a failure, and the queue is where it was under both — so each
    // is said in its own words rather than reported as "nothing to send".
    let line = match sent.held_back {
        Some(HeldBack::AnotherTurn) => {
            format!("Another carrier is taking its turn. {} record(s) are on the queue.", sent.waiting)
        }
        Some(HeldBack::SwitchedOff) => format!(
            "This device is not carrying to the Viewer. {} record(s) are on the queue, and they keep.",
            sent.waiting,
        ),
        None => format!(
            "{} record(s) reached the server, and {} wait behind them.",
            sent.placed, sent.waiting,
        ),
    };
    write_envelope(
        flags,
        "viewer.send",
        "sent",
        json!(sent),
        None,
        sent.placed == 0,
        line,
    );
    Ok(0)
}

/// Compare the two ends, and — on the press that spends it — place the difference.
fn mend(store: &Store, flags: &Flags, place: bool) -> Result<i32, CliError> {
    the_server_is_up(store)?;
    let done = viewer::repair::repair(store, place).map_err(CliError::from)?;
    let line = match &done {
        Repaired::NotSetUp => return Err(no_server_yet()),
        Repaired::SendingElsewhere => {
            "Another carrier is taking its turn, so nothing was compared. Try again once it is done."
                .to_string()
        }
        Repaired::Level => "The server holds what this machine holds.".to_string(),
        Repaired::Counted(drift) => format!(
            "{} record(s) to place and {} to drop. `{} viewer repair --send` carries them, and so does \
             running this again within ten minutes.",
            drift.to_place,
            drift.to_drop,
            Paths::command_name(),
        ),
        Repaired::Placed { drift, sent } => format!(
            "{} record(s) to place and {} to drop went on the queue; {} reached the server, and {} wait \
             behind them.",
            drift.to_place, drift.to_drop, sent.placed, sent.waiting,
        ),
    };
    write_envelope(
        flags,
        "viewer.repair",
        "repair",
        json!(done),
        None,
        matches!(done, Repaired::Level | Repaired::SendingElsewhere),
        line,
    );
    Ok(0)
}

/// Refuse before anything else where setup has never run. Core answers "no server" rather than failing —
/// a device nobody has set the Viewer up on is not failing at anything — and at this face that answer is
/// the whole of what went wrong, so it is said once, here.
fn the_server_is_up(store: &Store) -> Result<(), CliError> {
    match Server::of_device(store).map_err(CliError::from)? {
        Some(_) => Ok(()),
        None => Err(no_server_yet()),
    }
}

fn no_server_yet() -> CliError {
    CliError {
        code: "viewer_not_set_up",
        message: "the Viewer's server has not been stood up on this device".to_string(),
        hint: Some(format!(
            "Run `{} viewer setup` first. It builds a Worker and a database in your own Cloudflare account.",
            Paths::command_name(),
        )),
        exit: 2,
    }
}

/// Whether a code goes on this screen: wherever stdout is a terminal, and wherever `--terminal` says so
/// — the shell that is a window onto somebody else's machine.
fn drawn_here(flags: &Flags, forced: bool) -> bool {
    forced || (!flags.json && std::io::stdout().is_terminal())
}

fn drawn(carried: &str) -> Result<String, CliError> {
    code::in_blocks(carried).map_err(CliError::from)
}
