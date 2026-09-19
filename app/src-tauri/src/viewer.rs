//! The doors the Viewer is worked through from the screen (`AMB-D-884`).
//!
//! What the settings screen needs is all in core — the setup, the pairing, the carrying and the repair —
//! and until this there was no way to reach any of it from the GUI. These are that way, and they are
//! nothing more: each one is a call into `amenbo_core::viewer` and a shape to answer in.
//!
//! **Every one of them is the device's, not a project's.** The server stands in one Cloudflare account,
//! the key is one key and the read code is one code, so nothing here takes a project and nothing here
//! writes a row a project owns.
//!
//! **The three secrets setup leaves behind never come back out.** `worker_url`, `auth_token` and
//! `encryption_key` stay in the table no road out of the store walks; what a screen gets is whether a
//! server is up ([`crate::dto::ViewerStateDto`]). The one exception is a read code, which *is* the key
//! and goes to the webview because the webview is what draws it for a camera.
//!
//! **A device with no server is answered, not refused.** Core says "no server" rather than failing — a
//! device nobody has set the Viewer up on is not failing at anything — so the doors that need one answer
//! `null`, or an outcome that names it, and the screen draws the state it is in.
//!
//! Everything that goes over the network is off the main thread, for the reason a dead host is only found
//! out by waiting for its timeout.

use amenbo_core::viewer::{self, pairing, repair::Repaired, send::HeldBack, send::Sent, send::Server};

use crate::commands::{open_store, open_store_read};
use crate::dto::{
    ViewerAppDto, ViewerCodeDto, ViewerDriftDto, ViewerPairingDto, ViewerRepairedDto, ViewerSentDto,
    ViewerStateDto, ViewerStoodDto,
};
use crate::error::CmdError;

/// **What this device holds, without asking the network** — the read the settings screen's first paint
/// is drawn from.
///
/// Whether a phone may read is deliberately not here: only the server can answer that, and a screen that
/// waited for it would show nothing at all until a round trip came back ([`viewer_pairing`]).
#[tauri::command]
pub fn viewer_state() -> Result<ViewerStateDto, CmdError> {
    let store = open_store_read()?;
    let carried = store.viewer_carried()?;
    Ok(ViewerStateDto {
        set_up: Server::of_device(&store)?.is_some(),
        carrying: store.viewer_switched_on()?,
        waiting: store.viewer_waiting()?,
        last_placed_at: carried.last_placed_at,
        server_build: carried.build,
        worker_build: viewer::WORKER_BUILD.into(),
        token_link: viewer::token_link(),
    })
}

/// Where the phone's half of this is got. It asks nothing of the store: the two addresses are the same on
/// every device, set up or not.
#[tauri::command]
pub fn viewer_app() -> Vec<ViewerAppDto> {
    pairing::THE_APP.iter().map(|app| ViewerAppDto { phone: app.phone, link: app.link }).collect()
}

/// Throw the switch this device carries under. Off, neither the reading nor the placing happens, and what
/// is already queued keeps — turning the carrying off is not throwing away what has been read out.
#[tauri::command]
pub fn viewer_set_carrying(carrying: bool) -> Result<(), CmdError> {
    open_store()?.set_viewer_switched_on(carrying)?;
    Ok(())
}

/// **Stand the server up in the reader's own Cloudflare account**, or stand it up again over the one
/// already there.
///
/// `api_token` is held for this one run and written down nowhere: what is left in the store afterwards is
/// the address, the write token and the key, none of which can create anything in that account. It comes
/// from the box the reader pasted it into and goes no further than this call — so nothing above logs it,
/// and nothing below keeps it.
///
/// `account` picks between accounts where the token can reach more than one. Absent, core picks the only
/// one there is and refuses where that is not a question it can answer for them.
///
/// **Which server of that account is meant is not asked here yet.** Naming one is how a second server is
/// stood up, and how a store says that a server already standing under the usual name is its own
/// (`AMB-D-930`) — this screen has no box for it, so a reader who meets that refusal is pointed at the
/// flag the CLI beside the app carries. The box is `AMB-T-5136`.
///
/// Off the main thread: it talks to Cloudflare, uploads the Worker and waits for the name to answer.
#[tauri::command]
pub async fn viewer_setup(
    api_token: String,
    account: Option<String>,
) -> Result<ViewerStoodDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<ViewerStoodDto, CmdError> {
        let mut store = open_store()?;
        let stood = viewer::setup(&mut store, &api_token, account.as_deref(), None)?;
        Ok(ViewerStoodDto {
            url: stood.url,
            account: stood.account,
            database: stood.database,
            keys: match stood.keys {
                viewer::Keys::Kept => "kept",
                viewer::Keys::Generated => "generated",
            },
        })
    })
    .await
    .map_err(|e| -> CmdError { format!("standing the Viewer's server up did not finish: {e}").into() })?
}

/// **Whether a phone may read, as the server answers it.** `null` where no server has been stood up.
///
/// It asks rather than answering from here: the one thing worth knowing is whether the code that was
/// issued is still the code the server holds, and that is a question only the server can answer.
///
/// Off the main thread, and slow enough that the screen draws the rest of itself without it.
#[tauri::command]
pub async fn viewer_pairing() -> Result<Option<ViewerPairingDto>, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<ViewerPairingDto>, CmdError> {
        let store = open_store_read()?;
        Ok(pairing::who_may_read(&store)?
            .map(|reading| ViewerPairingDto { paired: reading.paired, issued_at: reading.issued_at }))
    })
    .await
    .map_err(|e| -> CmdError { format!("asking who may read did not finish: {e}").into() })?
}

/// **Draw a new read code**, for the screen to hold up to a camera. `null` where no server has been stood
/// up.
///
/// **It replaces whatever the server was holding**, so the phone that had the code before stops reading.
/// That is what makes pairing a second phone one press, and re-pairing after a lost phone one press as
/// well — and it is what the screen owes the reader a sentence about before they press it.
///
/// Off the main thread: it tells the server the hash of the code it drew.
#[tauri::command]
pub async fn viewer_pair_code() -> Result<Option<ViewerCodeDto>, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<ViewerCodeDto>, CmdError> {
        let store = open_store()?;
        Ok(pairing::issue(&store)?
            .map(|issued| ViewerCodeDto { carried: issued.carried, issued_at: issued.issued_at }))
    })
    .await
    .map_err(|e| -> CmdError { format!("drawing a read code did not finish: {e}").into() })?
}

/// **Take the read code away**, so whatever was holding it stops reading.
///
/// There is one code, so this takes every phone off at once and there is nothing to name. `false` where
/// the server was holding no code — that is an answer and not a refusal — and `null` where there is no
/// server.
///
/// Off the main thread: it is the server that holds the code.
#[tauri::command]
pub async fn viewer_cut_off() -> Result<Option<bool>, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<bool>, CmdError> {
        let store = open_store()?;
        Ok(pairing::cut_off(&store)?)
    })
    .await
    .map_err(|e| -> CmdError { format!("taking the read code away did not finish: {e}").into() })?
}

/// **Carry what has moved, now** — one turn, asked for by hand rather than waited for.
///
/// A device with no server placed nothing and says so; so does one whose switch is off, and one whose
/// turn another run is already taking. None of the three is a failure, and the queue is where it was
/// under all of them.
///
/// Off the main thread: a turn is a conversation with the server.
#[tauri::command]
pub async fn viewer_send() -> Result<ViewerSentDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<ViewerSentDto, CmdError> {
        let store = open_store()?;
        Ok(sent_row(viewer::send::carry(&store)?))
    })
    .await
    .map_err(|e| -> CmdError { format!("carrying to the Viewer did not finish: {e}").into() })?
}

/// **Put right what the server holds, where it has drifted** (`AMB-T-4763`).
///
/// `place` is the consent to spend the count, and the two presses are deliberately two: the first counts
/// the difference and writes it down, the second places it. A screen shows the reader the number before
/// asking for the second press, which is what the count is for.
///
/// Off the main thread: it reads every key the server holds before it compares anything.
#[tauri::command]
pub async fn viewer_repair(place: bool) -> Result<ViewerRepairedDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<ViewerRepairedDto, CmdError> {
        let store = open_store()?;
        Ok(repaired_row(viewer::repair::repair(&store, place)?))
    })
    .await
    .map_err(|e| -> CmdError { format!("putting the Viewer's server right did not finish: {e}").into() })?
}

/// One turn, in the shape the screen reads. The two held-back reasons keep the words core gives them, so
/// a front end branching on them is branching on the same value the CLI prints.
fn sent_row(sent: Sent) -> ViewerSentDto {
    ViewerSentDto {
        placed: sent.placed,
        waiting: sent.waiting,
        held_back: sent.held_back.map(|why| match why {
            HeldBack::AnotherTurn => "another_turn",
            HeldBack::SwitchedOff => "switched_off",
        }),
    }
}

/// One press of the repair, flattened into the outcome and the two things an outcome can carry. Core's
/// enum is a tagged union and TypeScript would take it as one, but every field would then be reached
/// through a narrowing the screen has to write before it can read a count — so the branch is named once,
/// here, and the fields hang off it.
fn repaired_row(done: Repaired) -> ViewerRepairedDto {
    let drift_row = |drift: amenbo_core::viewer::repair::Drift| ViewerDriftDto {
        to_place: drift.to_place,
        to_drop: drift.to_drop,
    };
    match done {
        Repaired::NotSetUp => ViewerRepairedDto { outcome: "not_set_up", drift: None, sent: None },
        Repaired::Level => ViewerRepairedDto { outcome: "level", drift: None, sent: None },
        Repaired::SendingElsewhere => {
            ViewerRepairedDto { outcome: "sending_elsewhere", drift: None, sent: None }
        }
        Repaired::Counted(drift) => {
            ViewerRepairedDto { outcome: "counted", drift: Some(drift_row(drift)), sent: None }
        }
        Repaired::Placed { drift, sent } => ViewerRepairedDto {
            outcome: "placed",
            drift: Some(drift_row(drift)),
            sent: Some(sent_row(sent)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::env_guard;
    use amenbo_core::viewer::repair::Drift;

    /// **What the settings screen draws on a device nobody has set the Viewer up on** — which is every
    /// device the first time it is opened, and the state this read has to answer in rather than fail in.
    ///
    /// It goes through the whole read: the three secrets are looked for and not found, the switch has
    /// never been thrown, and the queue and the carrier's row are empty. A store whose tables this build
    /// expects but does not have would come apart here.
    #[test]
    fn a_device_nobody_has_set_up_is_answered_rather_than_refused() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("viewer-state-fresh");
        std::env::set_var("AMENBO_HOME", &tmp);
        amenbo_core::Store::open().unwrap();

        let state = viewer_state().unwrap();

        assert!(!state.set_up, "nothing has been stood up");
        assert!(state.carrying, "a switch nobody has thrown is on");
        assert_eq!(state.waiting, 0);
        assert_eq!(state.last_placed_at, None, "nothing has ever landed");
        assert_eq!(state.server_build, 0, "zero is 'nothing has been written yet'");
        assert_eq!(state.worker_build, i64::from(viewer::WORKER_BUILD));

        // The switch is the one thing on this screen that works before a server exists, and it keeps.
        viewer_set_carrying(false).unwrap();
        assert!(!viewer_state().unwrap().carrying);
    }

    /// **The three secrets setup leaves behind never reach the webview.** What a screen is told about
    /// them is `setUp`, and this is the one place that promise can be held to: the shape is what goes on
    /// the wire, so a field added to it later is a field a reader's browser gets.
    ///
    /// The read code is the deliberate exception and is not in this shape — it is [`ViewerCodeDto`],
    /// asked for by a press and drawn for a camera.
    #[test]
    fn what_a_screen_is_told_about_the_server_holds_no_secret() {
        let state = ViewerStateDto {
            set_up: true,
            carrying: true,
            waiting: 3,
            last_placed_at: Some("2026-09-14T10:00:00Z".to_string()),
            server_build: 2,
            worker_build: 3,
            token_link: viewer::token_link(),
        };
        let wire = serde_json::to_value(&state).unwrap();
        // Sorted rather than in declaration order: what is being held to is the set of fields, and the
        // order they are serialised in is the map's business.
        let mut keys: Vec<&str> = wire.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            ["carrying", "lastPlacedAt", "serverBuild", "setUp", "tokenLink", "waiting", "workerBuild"],
            "a field added here is a field the webview gets — the address, the write token and the \
             encryption key are not among them"
        );
        // The link goes to Cloudflare's own token screen, so it carries permissions and no credential.
        assert!(wire["tokenLink"].as_str().unwrap().starts_with("https://dash.cloudflare.com/"));
    }

    /// A turn that did nothing on purpose says which of the two reasons it was, in core's own words — so
    /// a screen branching on the value branches on what the CLI prints. A turn that ran carries none,
    /// and that is not the same as having placed nothing.
    #[test]
    fn a_turn_held_back_says_which_of_the_two_reasons_it_was() {
        let held = |why| sent_row(Sent { placed: 0, waiting: 7, held_back: Some(why) });

        assert_eq!(held(HeldBack::AnotherTurn).held_back, Some("another_turn"));
        assert_eq!(held(HeldBack::SwitchedOff).held_back, Some("switched_off"));
        assert_eq!(held(HeldBack::SwitchedOff).waiting, 7, "the queue is where it was");

        let ran = sent_row(Sent { placed: 0, waiting: 0, held_back: None });
        assert_eq!(ran.held_back, None, "a turn with nothing to place is not a turn held back");
    }

    /// Each outcome of the repair carries exactly what that outcome has: a count where one was taken, a
    /// turn where one was run, and neither where the press did nothing. A count sent under `level` would
    /// have a screen offering to place a difference nobody found.
    #[test]
    fn each_repair_outcome_carries_only_what_it_has() {
        let drift = Drift { to_place: 4, to_drop: 1 };

        let counted = repaired_row(Repaired::Counted(drift));
        assert_eq!(counted.outcome, "counted");
        assert_eq!(counted.drift.as_ref().map(|d| (d.to_place, d.to_drop)), Some((4, 1)));
        assert!(counted.sent.is_none(), "counting places nothing");

        let placed = repaired_row(Repaired::Placed {
            drift,
            sent: Sent { placed: 4, waiting: 0, held_back: None },
        });
        assert_eq!(placed.outcome, "placed");
        assert_eq!(placed.sent.as_ref().map(|s| s.placed), Some(4));

        for (done, outcome) in [
            (Repaired::NotSetUp, "not_set_up"),
            (Repaired::Level, "level"),
            (Repaired::SendingElsewhere, "sending_elsewhere"),
        ] {
            let row = repaired_row(done);
            assert_eq!(row.outcome, outcome);
            assert!(row.drift.is_none() && row.sent.is_none(), "{outcome} counted nothing");
        }
    }

    /// Where the app is got is core's list and not a copy of it — a link edited there reaches the screen
    /// without the screen being told, which is the whole reason this door exists rather than two
    /// addresses written into the front end.
    #[test]
    fn where_the_app_is_got_is_cores_own_list() {
        let rows = viewer_app();

        assert_eq!(rows.len(), pairing::THE_APP.len());
        for (row, app) in rows.iter().zip(pairing::THE_APP) {
            assert_eq!(row.phone, app.phone);
            assert_eq!(row.link, app.link);
        }
    }
}
