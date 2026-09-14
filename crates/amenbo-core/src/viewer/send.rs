//! **Putting what moved where the Viewer reads it** (`AMB-D-884`, ported from the `viewer` plugin the
//! retreat retired).
//!
//! **A turn is in two halves, and they are not the same speed.** One copies what moved out of the change
//! feed into the queue the carrier keeps ([`super::carried`]); the other empties that queue into the
//! reader's own Worker, oldest first. The feed's window is five thousand rows wide and it turns, so the
//! copying has to keep up with the backlog whatever the network is doing — and the sending has to be free
//! to fall behind without dragging the copying back to where it stands.
//!
//! The ordinary turn copies only what moved. Every write moves the store's version, so one cheap question
//! answers "is there anything to do", and the feed answers "what" — which is why the whole backlog is
//! taken only on a first run, on a server just stood up, and after a gap.
//!
//! **What is placed is what the store holds, one record per row, sealed.** The Worker runs somewhere the
//! reader merely rents, so it is handed ciphertext and an ordering and nothing that says what any of it
//! means ([`super::sealing`]).
//!
//! **What it reads the store through is the roads that were already there** — [`crate::sync_snapshot`]
//! for the whole picture and for reading rows back by id, the change feed for what moved. They are asked
//! through [`Reach::All`], because the Viewer is the device's carrier rather than one project's: it is set
//! up on the device and it carries every project on it.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::error::{Error, Result};
use crate::model::SecretArea;
use crate::reach::Reach;
use crate::store::SyncChanges;
use crate::store::Store;
use crate::store_engine::read::FeedRow;

use super::carried::{Carried, Op, Waiting};
use super::pace::{
    be_quiet_for, out_of_budget, quiet, spend, the_wait_in_words, ROWS_WE_MAY_SPEND_A_DAY,
};
use super::refusal::{one_line_of, Refused};
use super::sealing::Sealer;

/// The version of the contract these records are written to. It is not Amenbo's format version: this one
/// moves when the two ends change what they say to each other, and a Worker refuses a version it does not
/// read rather than guessing.
pub const SPEC_V: i64 = 1;

/// How many records one write to the Worker may carry. It is the Worker's own limit, kept here so that too
/// much is split before it is sent rather than refused at the door — a `413` is nothing the send can do
/// anything with once the bytes are already on the wire.
///
/// **The two ends have to agree on this number**, and they are built and deployed together: the Worker's
/// script is baked into this binary ([`super::WORKER_SCRIPT`]), so
/// `tests::the_worker_takes_the_number_of_records_this_sends` reads the limit out of the script it
/// carries.
pub const RECORDS_PER_WRITE: usize = 500;

/// How many changes one read of the feed takes. It is [`crate::sync_snapshot::RECORDS_PER_READ`] because
/// that is where the ids in it go next: a page that was drained can always be read back in one ask.
const CHANGES_PER_READ: i64 = crate::sync_snapshot::RECORDS_PER_READ as i64;

/// What one call to the Worker gets. A send nobody is waiting on must still end, so a call that is not
/// going to answer gives the run back rather than holding it.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// The most of a refusal's body that is read. What is worth having off one is a sentence or a fault
/// number, and the rest is markup on a page nobody asked for.
const MOST_OF_A_REFUSAL: u64 = 1 << 20;

/// One record as it travels: the key it is filed under, what happened to it, and — for everything but a
/// delete — the envelope it was sealed into.
#[derive(Debug, serde::Serialize)]
struct Travelling {
    k: String,
    op: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    n: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    c: String,
}

/// The body of one request. The version travels with it so the Worker can recognise a repeat of what it
/// already holds, and refuse an ordering that went backwards.
///
/// `part` of `parts` is how the Worker is told which piece of one turn it is holding — **it writes the
/// version and the key's name down with the last part alone**, so a turn that was cut short leaves the
/// Worker standing where it was.
#[derive(Debug, serde::Serialize)]
struct Placement {
    spec_v: i64,
    version: i64,
    part: usize,
    parts: usize,
    /// The key these records were sealed with, named rather than shown, so a phone can find out that its
    /// own key does not fit without fetching a store it cannot open a row of.
    key_fingerprint: String,
    records: Vec<Travelling>,
}

/// What one request answered: where the Worker's ordering stands afterwards, how many rows its database
/// actually wrote, and which build of the Worker answered.
///
/// **The second is measured and not worked out.** An upsert onto a key already there costs a different
/// number of rows from one onto a key that is not, so counting from this side would mean keeping a model
/// of that database here, to go stale the day it changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
struct Written {
    #[serde(default)]
    seq: i64,
    #[serde(default)]
    rows_written: i64,
    #[serde(default)]
    build: i64,
}

/// What the Worker says of itself when it is asked rather than written to.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Standing {
    /// Where its ordering has reached.
    #[serde(default)]
    pub seq: i64,
    /// The backlog version it was last left standing at.
    #[serde(default)]
    pub version: i64,
    /// The key the records in it were sealed with, as it was named on the last turn that landed whole.
    /// Absent on a Worker nothing has been placed in yet.
    #[serde(default)]
    pub key_fingerprint: Option<String>,
}

/// What one turn did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Sent {
    /// How many records reached the Worker.
    pub placed: usize,
    /// How many are still waiting behind them.
    pub waiting: i64,
    /// Why the turn did nothing, where it did nothing on purpose.
    ///
    /// **Neither reason is a failure**, and the queue is where it was under both. `None` is a turn that
    /// ran — which may still have placed nothing, there having been nothing to place.
    pub held_back: Option<HeldBack>,
}

/// The two reasons a turn does nothing and is right to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeldBack {
    /// Another run already held the turn. **It is the hold working** ([`super::lock`]): a write sets a
    /// carrier off and writes come in bursts, so this is the ordinary answer for every run of a burst
    /// after the first — the stretch it would have carried is being carried beside it.
    AnotherTurn,
    /// This device's switch is off ([`super::carried::switched_on`]). Neither the reading nor the placing
    /// happens, and what is already queued keeps.
    SwitchedOff,
}

/// A call that did not give back what was asked for.
#[derive(Debug)]
pub enum Rebuffed {
    /// The Worker answered, and said no. It is kept whole rather than turned into a sentence here,
    /// because a caller with a reading of its own — a turn honouring the moment it was told to come back
    /// at — needs the parts.
    Refused(Refused),
    /// Nothing answered, or what answered could not be read.
    Unreadable(Error),
}

impl Rebuffed {
    /// The moment this asks to be come back at, when it asks at all.
    fn asks_to_be_left_for(&self, now: DateTime<Utc>) -> Option<Duration> {
        match self {
            Rebuffed::Refused(refused) => refused.asks_to_be_left_for(now),
            Rebuffed::Unreadable(_) => None,
        }
    }

    /// This, as the sentence whoever has to fix it reads.
    pub(super) fn in_words(self, now: DateTime<Utc>) -> Error {
        match self {
            Rebuffed::Refused(refused) => Error::invalid(refused.in_words(now)),
            Rebuffed::Unreadable(error) => error,
        }
    }
}

/// The reader's own Worker: where it answers, the token that opens its writing door, and the key nothing
/// there is written without.
pub struct Server {
    url: String,
    token: String,
    seal: Sealer,
    agent: ureq::Agent,
}

impl Server {
    /// Where it answers — what a request is addressed to.
    pub(super) fn at(&self) -> &str {
        &self.url
    }

    /// The server this device was set up against, or `None` where setup has not run.
    ///
    /// **Half a route is not a route.** A URL with no token is turned away at the door on every send, and
    /// a token with no key seals nothing — so all three are read together and absence of any of them is
    /// absence of the server. It is not a failure: a device nobody has set the Viewer up on is not failing
    /// at anything.
    pub fn of_device(store: &Store) -> Result<Option<Server>> {
        let url = store.secret_value(None, SecretArea::Viewer, None, super::WORKER_URL)?;
        let token = store.secret_value(None, SecretArea::Viewer, None, super::AUTH_TOKEN)?;
        let (Some(url), Some(token), Some(seal)) = (url, token, Sealer::for_device(store)?) else {
            return Ok(None);
        };
        let (url, token) = (url.trim().trim_end_matches('/').to_string(), token.trim().to_string());
        if url.is_empty() || token.is_empty() {
            return Ok(None);
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(CALL_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Some(Server { url, token, seal, agent }))
    }

    /// What the Worker says of itself — its ordering, and the key its records were sealed with.
    pub fn standing(&self) -> std::result::Result<Standing, Rebuffed> {
        let answered = self.ask("GET", "/meta", None)?;
        serde_json::from_slice(&answered).map_err(|err| {
            Rebuffed::Unreadable(Error::invalid(format!(
                "/meta answered with something this build cannot read: {err}"
            )))
        })
    }

    /// Put one request's worth of a queue, sealed on its way out, and read what the Worker did with it.
    ///
    /// **The answer is read as well as sent.** A Worker that wrote what it was handed stands exactly the
    /// record count further on, so a request that was taken in and dropped is one whose answer did not
    /// move. Saying "sent" of one of those is the quiet way a backlog loses a record for good. Checking
    /// that answer is the caller's, since the caller is what holds the records until it is satisfied.
    fn place(&self, body: &Placement) -> std::result::Result<Written, Rebuffed> {
        let raw = serde_json::to_vec(body).map_err(|err| {
            Rebuffed::Unreadable(Error::invalid(format!(
                "this build could not put the placement together: {err}"
            )))
        })?;
        let answered = self.ask("PUT", "/records", Some(raw))?;
        serde_json::from_slice(&answered).map_err(|err| {
            Rebuffed::Unreadable(Error::invalid(format!(
                "/records answered with something this build cannot read: {err}"
            )))
        })
    }

    /// Send one request to the Worker and hand back the body of a good answer.
    ///
    /// **Every door the Worker has is read the same way**, which is why this is the neighbours' as well
    /// as the sending's ([`super::pairing`]): a refusal has one shape whether what was asked for was a
    /// placement or a read code, and two readings of it would be two answers to "why is my phone not
    /// updating?".
    ///
    /// **No sentence this produces carries the token or anything the body held.** What travels is the
    /// door's name, what it answered, and the Worker's own words — which are written for whoever has to
    /// fix it.
    pub(super) fn ask(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> std::result::Result<Vec<u8>, Rebuffed> {
        let carries_a_body = body.is_some();
        let mut building = ureq::http::Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.url))
            .header("authorization", format!("Bearer {}", self.token))
            .header("accept", "application/json");
        if carries_a_body {
            building = building.header("content-type", "application/json");
        }
        let request = building.body(body.unwrap_or_default()).map_err(|err| {
            Rebuffed::Unreadable(Error::invalid(format!(
                "this build could not put the {path} call together: {err}"
            )))
        })?;
        let mut answer = self.agent.run(request).map_err(|err| {
            Rebuffed::Unreadable(Error::invalid(format!("{path} did not answer: {err}")))
        })?;
        let status = answer.status().as_u16();
        let wait_for = answer
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .trim()
            .to_string();
        let said = answer
            .body_mut()
            .with_config()
            .limit(MOST_OF_A_REFUSAL)
            .read_to_vec()
            .map_err(|err| {
                Rebuffed::Unreadable(Error::invalid(format!(
                    "{path} answered {status}, and the answer could not be read: {err}"
                )))
            })?;
        if (200..=299).contains(&status) {
            return Ok(said);
        }
        Err(Rebuffed::Refused(refused_as(path, status, wait_for, &said)))
    }
}

/// Read one refusal: the Worker's own sentence when it wrote its JSON, and otherwise a line of whatever
/// came back — Cloudflare's own error pages are the reason, since they carry a fault number worth keeping.
fn refused_as(path: &str, status: u16, wait_for: String, said: &[u8]) -> Refused {
    #[derive(serde::Deserialize)]
    struct Said {
        error: String,
    }
    let raw = String::from_utf8_lossy(said);
    match serde_json::from_str::<Said>(&raw) {
        Ok(Said { error }) if !error.is_empty() => Refused {
            path: path.to_string(),
            status,
            said: error,
            page: String::new(),
            wait_for,
        },
        _ => Refused {
            path: path.to_string(),
            status,
            said: String::new(),
            page: one_line_of(&raw),
            wait_for,
        },
    }
}

/// Run one turn: copy what moved into the queue, then empty as much of that queue as the Worker will take.
///
/// A device with no server set up is not failing at anything, and answers that it placed nothing.
pub fn carry(store: &Store) -> Result<Sent> {
    carry_at(store, Utc::now())
}

/// [`carry`], with the moment handed in — what a test drives, and what keeps the day a spend is counted
/// against out of a clock nothing can move.
pub fn carry_at(store: &Store, now: DateTime<Utc>) -> Result<Sent> {
    let Some(server) = Server::of_device(store)? else {
        return Ok(Sent::default());
    };

    // The switch is asked before the hold, because it is the cheaper question and the commoner answer: a
    // device that is switched off has nothing for a turn to hold.
    if !store.viewer_switched_on()? {
        return held_back(store, HeldBack::SwitchedOff);
    }

    // **The turn is taken before the first question, not before the first record.** What must not overlap
    // is the whole of it — the reading, the placing, and the writing down of where it got to — so the hold
    // is taken above all three and let go when the turn is over (see `super::lock`).
    let Some(turn) = super::lock::take_the_turn(&store.paths)? else {
        return held_back(store, HeldBack::AnotherTurn);
    };

    // The version is read before the picture is taken, never after: a write landing in between makes a
    // remembered version one turn stale, which costs a turn that finds nothing. Remembering a version
    // newer than the picture would instead skip whatever landed in that gap, and the phone would never
    // learn of it.
    let version = store.device_sync_version()?;
    let mut left = store.viewer_carried()?;

    // The number to carry is read off what was remembered rather than off what the copying is about to
    // write down: copying a stretch out moves the version field, and the number turns on where the Worker
    // was left standing before any of that.
    let sending = the_number_to_send(version, &left);

    let copied = copy_out(store, &mut left, version);
    // What was copied out is written down whether or not the copying finished, and before anything is
    // sent: a cursor lost with the queue it read is a stretch of the backlog nothing goes back for.
    store.set_viewer_carried(&left)?;
    copied?;

    let placed = drain(store, &server, &mut left, sending, now);
    store.set_viewer_carried(&left)?;
    let placed = placed?;

    // **The moment is written down only where something landed.** A turn that placed nothing left the
    // server exactly as it found it, and dating that would be this device telling a screen it had carried
    // when it had not.
    if placed > 0 {
        left.last_placed_at = Some(crate::time::Timestamp(now).to_rfc3339_z());
        store.set_viewer_carried(&left)?;
    }

    let sent = Sent { placed, waiting: store.viewer_waiting()?, held_back: None };
    drop(turn);
    Ok(sent)
}

/// A turn that did nothing on purpose, with the queue reported as it stands.
fn held_back(store: &Store, why: HeldBack) -> Result<Sent> {
    Ok(Sent { placed: 0, waiting: store.viewer_waiting()?, held_back: Some(why) })
}

/// The version one turn travels under: the backlog's own, except where that is already the number the
/// Worker was left standing at — in which case it is one past it.
///
/// **A Worker drops a turn whose version it is already standing at, and answers as though it took it.**
/// The guard is there to recognise a turn that finished and arrived twice, and it recognises it by that
/// number alone — so two different turns carrying one number are one turn to it. The backlog's version is
/// too coarse a name for a turn: a send asked for by hand, a whole placement sent again, and every send
/// made while the backlog itself has not moved all carry the number the Worker is already standing at.
///
/// **Only a turn that landed is remembered**, so a turn sent again after a refusal carries the same number
/// as it did before and is still recognised as the repeat it is — which is the whole of what the guard was
/// for.
fn the_number_to_send(version: i64, left: &Carried) -> i64 {
    // Nothing placed from here is nothing the Worker can be standing at on our account, and a first turn
    // carries the backlog's own version rather than one past it.
    if left.placed != 0 && version == left.placed {
        version + 1
    } else {
        version
    }
}

/// Copy what the Worker has yet to be told into the back of the queue, and write down how far the feed has
/// been read out.
///
/// **The two go together and that is the whole of the rule.** A cursor written on its own says a stretch
/// was dealt with when it was not, and nothing goes back for it; a queue written on its own costs the same
/// stretch being read a second time, which is a duplicate and not a hole.
fn copy_out(store: &Store, left: &mut Carried, version: i64) -> Result<()> {
    if has_read_nothing_out(left) {
        return the_whole_backlog(store, left, version);
    }
    if left.version == version {
        // Level, and saying so costs nothing. What is already queued is still offered — that is the
        // sending's half, and it answers to nothing here.
        return Ok(());
    }
    loop {
        match store.device_sync_changes(left.cursor, CHANGES_PER_READ)? {
            SyncChanges::Gap => {
                tracing::info!(
                    "the change feed no longer reaches back to where the Viewer's carrier left off — \
                     copying the whole store out again"
                );
                return the_whole_backlog(store, left, version);
            }
            SyncChanges::Changes { rows, cursor, more } => {
                // A stretch that turns out to hold nothing is still copied out: the cursor moves, so the
                // next turn does not read it again.
                let records = the_records_that_moved(store, &rows)?;
                store.enqueue_viewer(&records)?;
                left.cursor = cursor;
                left.version = version;
                if !more {
                    return Ok(());
                }
            }
        }
    }
}

/// Whether this carrier has read nothing out yet — a first run, or a carrier whose memory was thrown away
/// because the server under it was stood up anew ([`super::carried::forget`]).
///
/// **It is the numbers and not the queue that say so.** A queue can be empty because everything in it
/// landed; a cursor at the beginning under a version of nothing is a carrier that has never been anywhere.
fn has_read_nothing_out(left: &Carried) -> bool {
    left.cursor == 0 && left.version == 0
}

/// Copy the whole store out — what a first run does, and what a gap in the feed costs.
///
/// **What was already queued stays in front of it.** The picture says what the store holds now, and a
/// record it does not name is one the store no longer has — but the Worker does, and only a delete already
/// in the queue will say so. Dropping the queue for the picture would leave those behind for good.
fn the_whole_backlog(store: &Store, left: &mut Carried, version: i64) -> Result<()> {
    let (records, cursor) = the_whole_picture(store)?;
    store.enqueue_viewer(&records)?;
    left.cursor = cursor;
    left.version = version;
    Ok(())
}

/// The whole of what this machine holds, as the records that would replace what the Worker has, and the
/// feed position the picture was taken at.
///
/// **Nothing is written down here.** Taking a picture reads nothing out of the feed, so the cursor above it
/// still names the same stretch — which is what lets the repair road ([`super::repair`]) ask what this
/// machine holds without the ordinary send losing its place.
pub(super) fn the_whole_picture(store: &Store) -> Result<(Vec<Waiting>, i64)> {
    let mut whole = Vec::new();
    crate::sync_snapshot::stream_from(&store.paths.store_file, Reach::All, &mut whole)?;
    carried_out_of(&whole)
}

/// Read a whole snapshot into the records that replace what the Worker holds, and the feed position the
/// picture was taken at.
///
/// The tables are walked in name order, and the rows in the order the snapshot wrote them, so two runs
/// over one picture place the same thing.
fn carried_out_of(whole: &[u8]) -> Result<(Vec<Waiting>, i64)> {
    #[derive(serde::Deserialize)]
    struct Picture {
        amenbo_sync: Header,
        tables: BTreeMap<String, Vec<serde_json::Value>>,
    }
    #[derive(serde::Deserialize)]
    struct Header {
        cursor: Option<i64>,
    }
    let picture: Picture = serde_json::from_slice(whole).map_err(|err| {
        Error::invalid(format!("the snapshot came back in a shape this build cannot read: {err}"))
    })?;
    let Some(cursor) = picture.amenbo_sync.cursor else {
        return Err(Error::invalid(
            "the snapshot did not say where in the feed it stands, so there is no position to read on \
             from",
        ));
    };
    let mut records = Vec::new();
    for (dataset, rows) in &picture.tables {
        for row in rows {
            records.push(placed(dataset, row)?);
        }
    }
    Ok((records, cursor))
}

/// Read back what one stretch of the feed says moved, alongside the deletes that need no read.
///
/// **A record deleted after being written is a delete, and one written after being deleted is a write** —
/// the phone needs where a record ended up, not how it got there, so the last word about it is the only
/// one kept.
///
/// An id that comes back absent is one that went away between the feed naming it and this read. It is left
/// out rather than guessed at: the change that says so is still ahead of the cursor and carries the delete
/// on the next turn.
fn the_records_that_moved(store: &Store, rows: &[FeedRow]) -> Result<Vec<Waiting>> {
    let (read, dropped) = collapse(rows);

    let mut placed_rows = Vec::new();
    let mut drops = Vec::new();
    for (dataset, ids) in &read {
        for page in ids.chunks(crate::sync_snapshot::RECORDS_PER_READ) {
            // A dataset the feed names is one this road carries: both lines are drawn by
            // `export::WITHHELD_ON_THE_WAY_OUT`, the feed's and the read-back's alike. A refusal here is
            // therefore a build that has grown two registries, which is worth stopping over.
            let mut back = Vec::new();
            crate::sync_snapshot::records_from(
                &store.paths.store_file,
                Reach::All,
                dataset,
                page,
                &mut back,
            )?;
            placed_rows.extend(read_back_out_of(dataset, &back)?);
        }
    }
    for (dataset, ids) in &dropped {
        for id in ids {
            drops.push(Waiting {
                record_key: record_key(dataset, *id),
                op: Op::Deleted,
                body: None,
            });
        }
    }
    // The deletes go behind the writes, in key order, so one stretch always produces one queue.
    drops.sort_by(|a, b| a.record_key.cmp(&b.record_key));
    placed_rows.extend(drops);
    Ok(placed_rows)
}

/// Reduce a stretch of the feed to what has to be carried: the ids to read back and the ids to drop, both
/// gathered by dataset and both in a settled order.
fn collapse(rows: &[FeedRow]) -> (BTreeMap<String, Vec<i64>>, BTreeMap<String, Vec<i64>>) {
    let mut last: BTreeMap<(&str, i64), &str> = BTreeMap::new();
    for moved in rows {
        last.insert((&moved.dataset, moved.row_id), &moved.op);
    }
    let (mut read, mut dropped) = (BTreeMap::new(), BTreeMap::new());
    for ((dataset, row_id), op) in last {
        let gathered = if op == "delete" { &mut dropped } else { &mut read };
        gathered.entry(dataset.to_string()).or_insert_with(Vec::new).push(row_id);
    }
    for gathered in [&mut read, &mut dropped] {
        for ids in gathered.values_mut() {
            ids.sort_unstable();
        }
    }
    (read, dropped)
}

/// Read a by-id answer into the records it holds.
fn read_back_out_of(dataset: &str, back: &[u8]) -> Result<Vec<Waiting>> {
    #[derive(serde::Deserialize)]
    struct Answered {
        tables: BTreeMap<String, Vec<serde_json::Value>>,
    }
    let answered: Answered = serde_json::from_slice(back).map_err(|err| {
        Error::invalid(format!("`{dataset}` came back in a shape this build cannot read: {err}"))
    })?;
    let mut records = Vec::new();
    for (table, rows) in &answered.tables {
        for row in rows {
            records.push(placed(table, row)?);
        }
    }
    Ok(records)
}

/// One row on its way out: the key it is filed under, and the row as it stands.
fn placed(dataset: &str, row: &serde_json::Value) -> Result<Waiting> {
    let Some(id) = row.get("id").and_then(serde_json::Value::as_i64) else {
        return Err(Error::invalid(format!(
            "a `{dataset}` row came back with no id to file it under"
        )));
    };
    Ok(Waiting {
        record_key: record_key(dataset, id),
        op: Op::Placed,
        body: Some(row.to_string()),
    })
}

/// What a row is filed under: its dataset and its id, as one string. The Worker is told nothing about what
/// either half means — to it this is a key and no more, which is what keeps Amenbo's schema out of a place
/// the reader merely rents.
fn record_key(dataset: &str, id: i64) -> String {
    format!("{dataset}/{id}")
}

/// Send the front of the queue, in as many requests as it takes, and answer how many landed.
///
/// **What the Worker took is dropped, and what it did not is kept.** A request that fails stops the rest
/// and leaves everything from it onwards where it is, so nothing is offered as sent that was not — a
/// refusal costs the turn and no records. This is why there is no send position: the queue is the
/// position, and there is nothing for a second number to disagree with.
///
/// **The version is settled by the last request alone.** The Worker writes it down with the part that says
/// it is the last of its turn, so a queue emptied to the end is the only thing that leaves the Worker
/// standing at the number this turn carried — and only then is that number remembered.
fn drain(
    store: &Store,
    server: &Server,
    left: &mut Carried,
    sending: i64,
    now: DateTime<Utc>,
) -> Result<usize> {
    let waiting = store.viewer_waiting()?;
    if waiting == 0 {
        return Ok(0);
    }
    // **Being asked to wait is not a failure, and neither is being out of budget.** Both leave the queue
    // where it is and cost nothing, and both are answered by doing nothing until the moment comes — so
    // neither is reported as a refusal, which would put a red line in the log on every write for as long
    // as it lasted.
    if let Some(wait) = quiet(left, now) {
        tracing::info!(
            "the Viewer's server asked to be left for a while — {waiting} record(s) wait another {}",
            the_wait_in_words(wait)
        );
        return Ok(0);
    }
    if out_of_budget(left, now) {
        tracing::info!(
            "today's {ROWS_WE_MAY_SPEND_A_DAY} rows for the Viewer's server are spent — \
             {waiting} record(s) go on after midnight UTC"
        );
        return Ok(0);
    }

    let per_write = RECORDS_PER_WRITE as i64;
    let parts = ((waiting + per_write - 1) / per_write) as usize;
    let (mut landed, mut emptied) = (0usize, true);
    for at in 1..=parts {
        // The budget is read between requests rather than inside one: what a write costs is the Worker's
        // answer, so the count only moves once the write has happened. What that buys is an overshoot of
        // at most one request, against a ceiling that already holds a tenth of the day back.
        if out_of_budget(left, now) {
            tracing::info!(
                "today's {ROWS_WE_MAY_SPEND_A_DAY} rows for the Viewer's server are spent — \
                 the rest go on after midnight UTC"
            );
            emptied = false;
            break;
        }
        let records = store.viewer_front(per_write)?;
        if records.is_empty() {
            break;
        }
        let body = Placement {
            spec_v: SPEC_V,
            version: sending,
            part: at,
            parts,
            key_fingerprint: server.seal.fingerprint().to_string(),
            records: records
                .iter()
                .map(|record| sealed(&server.seal, record))
                .collect::<Result<Vec<_>>>()?,
        };
        let answered = match server.place(&body) {
            Ok(answered) => answered,
            Err(rebuffed) => {
                // A refusal that names a moment to come back at is honoured, and written down: the
                // process ends with this run, so a wait held anywhere else lasts no time at all.
                if let Some(wait) = rebuffed.asks_to_be_left_for(now) {
                    *left = be_quiet_for(left, wait, now);
                }
                return Err(rebuffed.in_words(now));
            }
        };
        *left = spend(left, answered.rows_written, now);
        // Which Worker took it, remembered because a screen has no other way to ask: it reads nothing over
        // the network, and this is a write that already happened.
        left.build = the_build_that_answered(answered.build);
        // **Zero is "not known"**, which is what a carrier that has never been told stands at, so the
        // check begins with the answer to the first write rather than against a number nothing gave.
        let expected = left.seq + records.len() as i64;
        if left.seq != 0 && answered.seq != expected {
            return Err(Error::invalid(format!(
                "the Viewer's server took part {at} of {parts} and did not write it: {} records should \
                 have carried the ordering to {expected}, and it answered {} — what it did not write is \
                 still queued",
                records.len(),
                answered.seq,
            )));
        }
        left.seq = answered.seq;
        store.drop_viewer_front(records.len() as i64)?;
        landed += records.len();
        // A Worker that took something is one that is not asking to be left alone any more.
        left.quiet_until = None;
    }
    if emptied {
        left.placed = sending;
    }
    Ok(landed)
}

/// One record in its envelope, ready to be written as it is. A delete carries no row, so there is nothing
/// to seal for it — what it is filed under is the whole of it.
///
/// **A write with no row is refused rather than sent as a delete.** The two are one character apart on the
/// wire and opposites on the phone, so reading a queue this build cannot explain as "remove it" is how a
/// record would be dropped from every reader at once.
fn sealed(seal: &Sealer, record: &Waiting) -> Result<Travelling> {
    match (record.op, record.body.as_deref()) {
        (Op::Placed, Some(body)) => {
            let envelope = seal.seal(&record.record_key, body.as_bytes());
            Ok(Travelling {
                k: record.record_key.clone(),
                op: Op::Placed.as_str(),
                n: envelope.nonce,
                c: envelope.ciphertext,
            })
        }
        (Op::Placed, None) => Err(Error::invalid(format!(
            "`{}` is queued as a write and carries no row — there is nothing to seal for it",
            record.record_key
        ))),
        (Op::Deleted, _) => Ok(Travelling {
            k: record.record_key.clone(),
            op: Op::Deleted.as_str(),
            n: String::new(),
            c: String::new(),
        }),
    }
}

/// Read the build off an answer.
///
/// **A Worker that names none is build 1**, which is the promise the field was added under: every Worker
/// deployed before it existed answers nothing here, and one of those is precisely what a sender needs to
/// recognise. Reading the silence as "unknown" would let the oldest Worker of all be the one nobody is
/// ever told about.
fn the_build_that_answered(named: i64) -> i64 {
    if named == 0 {
        1
    } else {
        named
    }
}

/// How a face sets a carrier off — a **process**, and one it never waits for.
///
/// **The network is never on the write's path.** Placing a burst against a server somewhere else takes as
/// long as that server takes, and whoever was typing is owed none of it. So this seam answers one question
/// and returns — *carry what has moved, and do not make me wait* — exactly as a notification sender is
/// handed its messages ([`crate::notify_dispatch::Dispatcher`]).
///
/// **Nothing is handed over with it.** What a carrier is to carry is in the store, so there is no batch to
/// pass and nothing to go stale between the handing and the reading.
///
/// An `Err` means no carrier started. It costs the turn and no records: what has moved is still ahead of
/// the cursor, and the next write sets another one off.
pub trait Carrier {
    fn set_off(&self) -> std::io::Result<()>;
}

/// The carrier a real face hands a write seam: **this same executable, re-run** as a carrier.
///
/// Every face ships as a single binary — the CLI is one, and so is the app — so a carrier needs no second
/// one; what differs between faces is only how each names its own entry point, which is what `argv`
/// carries. The store's base directory follows it, because a carrier must carry the store its parent wrote
/// to and not whichever one its own working directory would resolve to.
pub struct SelfCarrier {
    argv: Vec<String>,
    base_dir: std::path::PathBuf,
}

impl SelfCarrier {
    pub fn new(argv: &[&str], base_dir: std::path::PathBuf) -> Self {
        Self { argv: argv.iter().map(|a| (*a).to_string()).collect(), base_dir }
    }
}

impl Carrier for SelfCarrier {
    fn set_off(&self) -> std::io::Result<()> {
        use std::process::Stdio;
        let child = std::process::Command::new(std::env::current_exe()?)
            .args(&self.argv)
            .arg(&self.base_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        crate::plugin_runner::reap(child);
        Ok(())
    }
}

/// **The carrier process's whole life** — open the store it was handed, take one turn, exit.
///
/// The store is named rather than resolved, for the reason a notification sender's is
/// ([`crate::notify_dispatch::send_process`]): it must carry the store its parent wrote to.
///
/// Nothing here is reported to a caller — there is none. A store that will not open, a server that refuses,
/// a turn another run is already taking: each is a line in the log and a queue that keeps.
pub fn carry_process(base_dir: std::path::PathBuf) {
    let store = match Store::open_at(crate::config::Paths::at(base_dir)) {
        Ok(store) => store,
        Err(err) => {
            tracing::warn!(error = %err, "a Viewer carrier could not open the store; nothing is carried");
            return;
        }
    };
    match carry(&store) {
        // A turn that did nothing on purpose has nothing to say: the stretch it would have carried is
        // being carried beside it, or this device is not carrying at all.
        Ok(sent) if sent.held_back.is_some() => {}
        Ok(sent) => {
            tracing::info!(
                placed = sent.placed,
                waiting = sent.waiting,
                "a Viewer carrier took its turn"
            );
        }
        Err(err) => {
            tracing::warn!(error = %err, "a Viewer carrier could not place what has moved");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_static_host::{Reply, StaticHost};
    use base64::Engine as _;

    /// A store on a scratch base, so what a turn reads and writes lands somewhere.
    fn store_at(tag: &str) -> Store {
        let dir = amenbo_scratch::scratch(&format!("viewer-send-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open_at(crate::config::Paths::at(dir)).unwrap()
    }

    /// The key a test device seals with, written the way setup writes one.
    fn the_key() -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7u8; super::super::sealing::KEY_SIZE])
    }

    /// Leave behind what setup leaves behind, pointed at a stand-in.
    fn set_up(store: &mut Store, host: &StaticHost) {
        for (field, value) in [
            (super::super::WORKER_URL, host.url("")),
            (super::super::AUTH_TOKEN, "write-token".to_string()),
            (super::super::ENCRYPTION_KEY, the_key()),
        ] {
            store.set_secret(None, SecretArea::Viewer, None, field, Some(&value)).unwrap();
        }
    }

    /// A project and a task in it, so the store has a backlog to carry.
    fn seed(store: &mut Store) -> i64 {
        let project = store
            .project_add(crate::ops::project::NewProject {
                name: "carried".into(),
                view: crate::model::View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        store
            .add_task(crate::ops::task::NewTask {
                title: "something to read".into(),
                project_id: Some(project),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
                at_binding_id: None,
            })
            .unwrap();
        project
    }

    /// What this device's key is called, which is what a turn writes on every part.
    fn our_fingerprint() -> String {
        super::super::sealing::Sealer::new(&the_key()).unwrap().fingerprint().to_string()
    }

    /// A Worker taking a write and answering where its ordering now stands.
    fn took(seq: i64, rows: i64) -> Reply {
        Reply::ok(
            serde_json::json!({ "seq": seq, "rows_written": rows, "build": 3 }).to_string(),
        )
    }

    /// The placements a host was sent, oldest first.
    fn placements(host: &StaticHost) -> Vec<serde_json::Value> {
        host.heard()
            .iter()
            .filter(|heard| heard.method == "PUT" && heard.target == "/records")
            .map(|heard| serde_json::from_slice(&heard.body).expect("a placement is JSON"))
            .collect()
    }

    /// A moment, spelled the way the store spells one.
    fn at(when: &str) -> DateTime<Utc> {
        crate::time::Timestamp::parse_rfc3339(when).unwrap().0
    }

    /// One record waiting, written the way the copying writes it.
    fn waiting(key: &str) -> Waiting {
        Waiting { record_key: key.into(), op: Op::Placed, body: Some(format!("{{\"id\":1,\"k\":\"{key}\"}}")) }
    }

    /// The number in the baked Worker's own source, so the two ends cannot drift apart on paper.
    fn declared_in_the_script(name: &str) -> Option<i64> {
        let at = super::super::WORKER_SCRIPT.find(&format!("var {name} = "))?;
        let rest = &super::super::WORKER_SCRIPT[at + format!("var {name} = ").len()..];
        let end = rest.find(|c: char| !c.is_ascii_digit())?;
        rest[..end].parse().ok()
    }

    /// **The two ends have to agree on how much one write carries, and on which contract it is.** They
    /// are built and deployed together — the Worker's script is baked into this binary — so the place to
    /// keep them level is here, against the script this build actually carries.
    #[test]
    fn the_worker_takes_what_this_build_sends() {
        assert_eq!(declared_in_the_script("PER_WRITE"), Some(RECORDS_PER_WRITE as i64));
        assert_eq!(declared_in_the_script("SPEC_V"), Some(SPEC_V));
    }

    /// One record can move several times inside one stretch, and the phone needs where it ended up rather
    /// than how it got there. A record deleted after being written is a delete; one written after being
    /// deleted is a write.
    #[test]
    fn the_last_word_about_a_record_is_the_one_carried() {
        let moved = |dataset: &str, row_id: i64, op: &str| FeedRow {
            id: 0,
            dataset: dataset.into(),
            row_id,
            op: op.into(),
        };
        let (read, dropped) = collapse(&[
            moved("task", 2, "insert"),
            moved("task", 2, "delete"),
            moved("task", 1, "delete"),
            moved("task", 1, "update"),
            moved("decision", 9, "update"),
        ]);
        assert_eq!(read.get("task"), Some(&vec![1]));
        assert_eq!(read.get("decision"), Some(&vec![9]));
        assert_eq!(dropped.get("task"), Some(&vec![2]));
    }

    /// A Worker drops a turn whose version it is already standing at and answers as though it took it, so
    /// a second turn under one version has to carry a number of its own. A first turn, and one after a
    /// refusal, carry the backlog's own.
    #[test]
    fn a_repeat_of_one_version_carries_one_past_it() {
        let nothing_placed = Carried::default();
        assert_eq!(the_number_to_send(12, &nothing_placed), 12);

        let standing_at_twelve = Carried { placed: 12, ..Carried::default() };
        assert_eq!(the_number_to_send(12, &standing_at_twelve), 13);
        assert_eq!(the_number_to_send(13, &standing_at_twelve), 13);
    }

    /// A row travels under its dataset and its id, and nothing else about Amenbo's schema goes to a place
    /// the reader merely rents.
    #[test]
    fn a_record_is_filed_under_its_dataset_and_its_id() {
        assert_eq!(record_key("task", 41), "task/41");
    }

    /// A delete is the key and the word; a write carries the envelope. A write queued with no row is
    /// refused rather than sent as a delete — the two are one character apart on the wire and opposites on
    /// the phone.
    #[test]
    fn a_write_with_no_row_is_refused_rather_than_sent_as_a_delete() {
        let seal = super::super::sealing::Sealer::new(&the_key()).unwrap();

        let gone = Waiting { record_key: "task/1".into(), op: Op::Deleted, body: None };
        let travelling = sealed(&seal, &gone).unwrap();
        assert_eq!(travelling.op, "del");
        assert!(travelling.n.is_empty() && travelling.c.is_empty());

        let written = sealed(&seal, &waiting("task/2")).unwrap();
        assert_eq!(written.op, "put");
        assert!(!written.n.is_empty() && !written.c.is_empty());

        let half = Waiting { record_key: "task/3".into(), op: Op::Placed, body: None };
        assert!(sealed(&seal, &half).is_err());
    }

    /// A snapshot becomes the records that replace what the Worker holds, filed under table and id, and it
    /// says where in the feed it stands — without which there is no position to read on from.
    #[test]
    fn a_snapshot_becomes_the_records_that_replace_what_is_there() {
        let whole = serde_json::json!({
            "amenbo_sync": { "format": "amenbo-sync-json", "cursor": 77 },
            "tables": { "task": [{ "id": 2, "title": "b" }, { "id": 1, "title": "a" }] },
        })
        .to_string();
        let (records, cursor) = carried_out_of(whole.as_bytes()).unwrap();
        assert_eq!(cursor, 77);
        assert_eq!(
            records.iter().map(|r| r.record_key.as_str()).collect::<Vec<_>>(),
            ["task/2", "task/1"]
        );
        assert!(records.iter().all(|r| r.op == Op::Placed && r.body.is_some()));

        let unpositioned =
            serde_json::json!({ "amenbo_sync": {}, "tables": {} }).to_string();
        assert!(carried_out_of(unpositioned.as_bytes()).is_err());
    }

    /// A device nobody has set the Viewer up on is not failing at anything: it places nothing and says so.
    #[test]
    fn a_device_with_no_server_places_nothing() {
        let store = store_at("no-server");
        assert_eq!(carry(&store).unwrap(), Sent::default());
    }

    /// **A device that is switched off carries nothing**, and does not read either: what the switch stops
    /// is the whole turn. The queue keeps, so throwing it back on places what was already read out.
    #[test]
    fn a_device_that_is_switched_off_neither_reads_nor_places() {
        let mut store = store_at("switched-off");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        store.enqueue_viewer(&[waiting("task/1")]).unwrap();
        store.set_viewer_switched_on(false).unwrap();

        let sent = carry(&store).unwrap();

        assert_eq!(sent, Sent { placed: 0, waiting: 1, held_back: Some(HeldBack::SwitchedOff) });
        assert!(placements(&host).is_empty());
        assert_eq!(store.viewer_carried().unwrap(), Carried::default(), "and nothing was read out");

        // Back on, what was already read out goes. A window that turned in the meantime is the copying's
        // own answer — it finds the gap and takes the whole store again.
        store.set_viewer_switched_on(true).unwrap();
        host.set_reply("/records", took(1, 1));
        let sent = carry(&store).unwrap();
        assert_eq!(sent.held_back, None);
        assert!(sent.placed > 0, "{sent:?}");
    }

    /// **A device that has never touched the switch carries.** Standing a server up is the act of asking
    /// for this, so an absent row is not "off" — it is nobody having said otherwise.
    #[test]
    fn a_switch_nobody_has_touched_is_on() {
        let store = store_at("switch-untouched");
        assert!(store.viewer_switched_on().unwrap());

        store.set_viewer_switched_on(false).unwrap();
        assert!(!store.viewer_switched_on().unwrap());
        store.set_viewer_switched_on(true).unwrap();
        assert!(store.viewer_switched_on().unwrap());
    }

    /// **When this device last placed anything is dated only where something landed.** A turn that placed
    /// nothing left the server as it found it, and dating that would tell a screen it had carried when it
    /// had not.
    #[test]
    fn the_moment_it_last_placed_is_written_only_where_something_landed() {
        let mut store = store_at("last-placed");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);

        // Nothing queued and nothing in the store to read out: the turn runs and places nothing.
        let quiet = at("2026-09-14T10:00:00Z");
        assert_eq!(carry_at(&store, quiet).unwrap().placed, 0);
        assert_eq!(store.viewer_carried().unwrap().last_placed_at, None);

        seed(&mut store);
        host.set_reply("/records", took(9, 9));
        let carried = at("2026-09-14T11:00:00Z");
        assert!(carry_at(&store, carried).unwrap().placed > 0);
        assert_eq!(
            store.viewer_carried().unwrap().last_placed_at.as_deref(),
            Some("2026-09-14T11:00:00Z"),
        );
    }

    /// **A turn somebody else is taking is not this run's to take.** Two turns at once put an older
    /// picture of a record on top of a newer one, so a run that cannot take the hold does nothing at all —
    /// and says so, rather than answering "nothing to send" and leaving the queue looking dealt with.
    #[test]
    fn a_turn_another_run_is_taking_is_left_alone() {
        let mut store = store_at("turn-taken");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        store.enqueue_viewer(&[waiting("task/1")]).unwrap();

        let held = super::super::lock::take_the_turn(&store.paths).unwrap().expect("nobody holds it");
        let sent = carry(&store).unwrap();

        assert_eq!(sent, Sent { placed: 0, waiting: 1, held_back: Some(HeldBack::AnotherTurn) });
        assert!(placements(&host).is_empty(), "nothing is placed under somebody else's turn");
        assert_eq!(store.viewer_waiting().unwrap(), 1, "and the queue is where it was");

        // The moment that run lets go, the turn is this one's.
        drop(held);
        host.set_reply("/records", took(1, 1));
        let sent = carry(&store).unwrap();
        assert_eq!(sent.held_back, None);
        assert!(sent.placed > 0, "{sent:?}");
    }

    /// **A first run places the whole backlog**, and every part of it carries the contract, the version and
    /// the name of the key it was sealed with — the Worker writing the last two down with the last part
    /// alone.
    #[test]
    fn a_first_run_places_the_whole_backlog() {
        let mut store = store_at("first-run");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply("/records", took(4, 4));

        let sent = carry(&store).unwrap();
        assert!(sent.placed > 0, "a store with a project in it has something to place");
        assert_eq!(sent.waiting, 0, "what the Worker took is dropped from the queue");

        let placed = placements(&host);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0]["spec_v"], SPEC_V);
        assert_eq!(placed[0]["part"], 1);
        assert_eq!(placed[0]["parts"], 1);
        assert_eq!(placed[0]["key_fingerprint"], our_fingerprint());
        assert_eq!(placed[0]["records"].as_array().unwrap().len(), sent.placed);
        // Nothing readable goes out: a record on the wire is a key, a word and an envelope.
        let first = &placed[0]["records"][0];
        assert!(first["n"].is_string() && first["c"].is_string());
        assert!(first.get("r").is_none(), "a row never travels in the clear");

        let left = store.viewer_carried().unwrap();
        assert_eq!(left.seq, 4);
        assert_eq!(left.build, 3);
        assert_eq!(left.placed, left.version, "a turn that emptied the queue settles the version");
    }

    /// A queue longer than one write goes in parts, each naming which of the turn it is, and the version
    /// and the key's name travel on every one of them.
    #[test]
    fn a_queue_longer_than_one_write_goes_in_parts() {
        let mut store = store_at("parts");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_replies("/records", [took(500, 500), took(501, 1)]);

        let queued: Vec<Waiting> =
            (1..=RECORDS_PER_WRITE + 1).map(|n| waiting(&format!("task/{n}"))).collect();
        store.enqueue_viewer(&queued).unwrap();

        let server = Server::of_device(&store).unwrap().unwrap();
        let mut left = Carried::default();
        let placed = drain(&store, &server, &mut left, 9, Utc::now()).unwrap();

        assert_eq!(placed, RECORDS_PER_WRITE + 1);
        let sent = placements(&host);
        assert_eq!(sent.len(), 2);
        assert_eq!((sent[0]["part"].as_i64(), sent[0]["parts"].as_i64()), (Some(1), Some(2)));
        assert_eq!((sent[1]["part"].as_i64(), sent[1]["parts"].as_i64()), (Some(2), Some(2)));
        assert_eq!(sent[0]["records"].as_array().unwrap().len(), RECORDS_PER_WRITE);
        assert_eq!(sent[1]["records"].as_array().unwrap().len(), 1);
        for part in &sent {
            assert_eq!(part["version"], 9);
            assert_eq!(part["key_fingerprint"], our_fingerprint());
        }
        assert_eq!(store.viewer_waiting().unwrap(), 0);
        assert_eq!(left.placed, 9, "the queue emptied, so the Worker stands at this turn's number");
    }

    /// **A Worker that took a write and did not write it is caught.** It stands exactly the record count
    /// further on when it wrote what it was handed, so an answer that did not move that far is a request
    /// that was dropped — and the records stay queued rather than being called sent.
    #[test]
    fn a_server_that_did_not_write_what_it_took_keeps_the_records() {
        let mut store = store_at("seq");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        // Two records were handed over, and the ordering moved by one.
        host.set_reply("/records", took(11, 1));
        store.enqueue_viewer(&[waiting("task/1"), waiting("task/2")]).unwrap();

        let server = Server::of_device(&store).unwrap().unwrap();
        // A carrier that has been told where this Worker stands. Zero is "not known", and a turn with
        // nothing to check against trusts the first answer and checks from the next.
        let mut left = Carried { seq: 10, ..Carried::default() };
        let refused = drain(&store, &server, &mut left, 3, Utc::now()).unwrap_err();

        assert!(refused.to_string().contains("did not write it"), "{refused}");
        assert_eq!(store.viewer_waiting().unwrap(), 2, "nothing is dropped from a turn that failed");
        assert_ne!(left.placed, 3, "a turn that did not empty settles no version");
    }

    /// **A carrier that has not been told where the Worker stands trusts the first answer**, and checks
    /// from the next — which is what makes the turn after a first run checkable without a call that asks.
    #[test]
    fn a_carrier_that_knows_nothing_checks_from_the_second_request_on() {
        let mut store = store_at("seq-unknown");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        // A Worker standing somewhere this device was never told about takes the first part, and the
        // second answer is the one that has to add up.
        host.set_replies("/records", [took(900, 500), took(900, 0)]);
        let queued: Vec<Waiting> =
            (1..=RECORDS_PER_WRITE + 1).map(|n| waiting(&format!("task/{n}"))).collect();
        store.enqueue_viewer(&queued).unwrap();

        let server = Server::of_device(&store).unwrap().unwrap();
        let mut left = Carried::default();
        let refused = drain(&store, &server, &mut left, 3, Utc::now()).unwrap_err();

        assert!(refused.to_string().contains("part 2 of 2"), "{refused}");
        assert_eq!(left.seq, 900, "the first answer is taken as where it stands");
        assert_eq!(store.viewer_waiting().unwrap(), 1, "and what it did not write is still queued");
    }

    /// A refusal that names a moment to come back at is honoured, and written down: the process ends with
    /// this run, so a wait held anywhere else would last no time at all. The next turn sends nothing and
    /// keeps the queue.
    #[test]
    fn a_refusal_that_names_a_moment_is_honoured() {
        let mut store = store_at("retry-after");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply(
            "/records",
            Reply::status(503, serde_json::json!({ "error": "busy" }).to_string())
                .and_header("Retry-After", "90"),
        );
        store.enqueue_viewer(&[waiting("task/1")]).unwrap();

        let server = Server::of_device(&store).unwrap().unwrap();
        let now = Utc::now();
        let mut left = Carried::default();
        assert!(drain(&store, &server, &mut left, 3, now).is_err());
        assert_eq!(
            quiet(&left, now).map(|wait| wait.as_secs()),
            Some(89),
            "the moment the server named is what is waited for — read a whole second after it was set"
        );

        // The next turn does nothing at all — no request goes out, and the record is still queued.
        let sent_before = placements(&host).len();
        assert_eq!(drain(&store, &server, &mut left, 3, now).unwrap(), 0);
        assert_eq!(placements(&host).len(), sent_before);
        assert_eq!(store.viewer_waiting().unwrap(), 1);
    }

    /// Today's allowance being gone leaves the queue where it is and costs nothing. It is not a refusal —
    /// what answers it is doing nothing until midnight UTC.
    #[test]
    fn nothing_is_sent_when_todays_rows_are_spent() {
        let mut store = store_at("budget");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        store.enqueue_viewer(&[waiting("task/1")]).unwrap();

        let server = Server::of_device(&store).unwrap().unwrap();
        let now = Utc::now();
        let mut left = spend(&Carried::default(), ROWS_WE_MAY_SPEND_A_DAY, now);
        assert_eq!(drain(&store, &server, &mut left, 3, now).unwrap(), 0);
        assert!(placements(&host).is_empty());
        assert_eq!(store.viewer_waiting().unwrap(), 1);
    }

    /// **The ordinary turn copies only what moved.** The first one takes the whole backlog; the next one
    /// reads the feed on from where it left off, and carries the one record that moved and nothing else.
    /// A turn with nothing behind it reads no feed at all.
    #[test]
    fn a_later_turn_carries_only_what_moved() {
        let mut store = store_at("what-moved");
        let project = seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_replies("/records", [took(9, 9), took(10, 1)]);

        let whole = carry(&store).unwrap();
        assert!(whole.placed > 1);

        // Nothing has moved, so there is nothing to read out and nothing to send.
        assert_eq!(carry(&store).unwrap(), Sent::default());
        assert_eq!(placements(&host).len(), 1);

        store
            .add_task(crate::ops::task::NewTask {
                title: "one more".into(),
                project_id: Some(project),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
                at_binding_id: None,
            })
            .unwrap();
        let moved = carry(&store).unwrap();

        assert_eq!(moved.placed, 1, "only the record that moved travels");
        let sent = placements(&host);
        assert_eq!(sent.len(), 2);
        let keys: Vec<&str> =
            sent[1]["records"].as_array().unwrap().iter().map(|r| r["k"].as_str().unwrap()).collect();
        assert_eq!(keys.len(), 1);
        assert!(keys[0].starts_with("task/"), "{keys:?}");
    }

    /// **The copying does not wait on the sending.** The feed's window turns, so what is not read out in
    /// time is not read out at all — a server refusing everything must therefore not hold the reading
    /// where it stands. The queue grows, the cursor moves, and nothing is lost.
    #[test]
    fn a_server_that_takes_nothing_does_not_hold_the_reading_back() {
        let mut store = store_at("two-halves");
        let project = seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply(
            "/records",
            Reply::status(500, serde_json::json!({ "error": "no" }).to_string()),
        );

        assert!(carry(&store).is_err());
        let read_out = store.viewer_waiting().unwrap();
        let cursor = store.viewer_carried().unwrap().cursor;
        assert!(read_out > 0 && cursor > 0);

        store
            .add_task(crate::ops::task::NewTask {
                title: "written while nothing lands".into(),
                project_id: Some(project),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
                at_binding_id: None,
            })
            .unwrap();
        assert!(carry(&store).is_err());

        assert!(
            store.viewer_waiting().unwrap() > read_out,
            "what moved is read out whether or not anything can be sent"
        );
        assert!(store.viewer_carried().unwrap().cursor > cursor, "and the cursor moves with it");
    }

    /// A cursor the feed can no longer reach is answered with the whole store rather than an empty page:
    /// saying nothing would be indistinguishable from nothing having happened, and the phone would sit
    /// stale believing it was current.
    #[test]
    fn a_cursor_the_feed_cannot_reach_takes_the_whole_store_again() {
        let mut store = store_at("gap");
        seed(&mut store);
        let mut left = Carried { cursor: 9_000_000, version: 1, ..Carried::default() };
        copy_out(&store, &mut left, 2).unwrap();

        assert!(store.viewer_waiting().unwrap() > 0, "the whole store is copied out again");
        assert_eq!(left.version, 2);
        assert!(left.cursor < 9_000_000, "the cursor comes back to where the picture was taken");
    }
}
